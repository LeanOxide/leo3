// Automated version-drift issue tracker for the Lean/Rust compat CI tiers.
//
// Invoked from .github/workflows/ci.yml (compat-failure-report job) via
// actions/github-script:
//
//   const script = require('./.github/scripts/compat-watch.js')
//   await script({github, context, core})
//
// Behavior:
// - Only acts on `main` pushes, scheduled sweeps and `workflow_dispatch`
//   runs on `main` — never for pull_request events or feature branches, so
//   PR experiments cannot file false positives.
// - For every tracked job that FAILED: find the open issue carrying the
//   per-leg marker; create one if missing, otherwise append a "still
//   failing" comment (skipped when the same run was already reported).
// - For every tracked job that SUCCEEDED: close its open issue (if any)
//   with a recovery note.
// - Cancelled/skipped conclusions are ignored entirely.
//
// Tracked jobs: `Compat / Full Matrix` legs on the `beta`/`nightly` Lean
// channels (version-drift canaries) plus the Heavy tier jobs. Stable and
// pinned-version matrix legs are ordinary regressions already surfaced by
// the CI conclusion job, so they are not tracked here.

'use strict'

const LABEL = 'ci-compat-failure'
const MARKER_PREFIX = 'leo3-compat-watch:'
const MAX_ANNOTATIONS = 8
const MAX_ANNOTATION_MESSAGE = 300

// Matrix legs are matched by the trailing display name; reusable-workflow
// jobs carry a `<caller> (<matrix>) / ` prefix which is ignored.
const MATRIX_JOB_RE =
  /Compat \/ Full Matrix \((?<os>[^,]+), Lean (?<lean>[^)]+)\)$/

// Heavy-tier jobs, matched by display-name prefix, with stable issue keys.
const HEAVY_JOBS = [
  {prefix: 'Heavy / Careful (UB detection)', key: 'heavy:careful'},
  {prefix: 'Heavy / AddressSanitizer', key: 'heavy:asan'},
  {prefix: 'Heavy / Coverage', key: 'heavy:coverage'},
  {prefix: 'Bench / Criterion Suite', key: 'heavy:bench'},
]

// Only these Lean channels are version-drift canaries.
const DRIFT_LEANS = new Set(['beta', 'nightly'])

function classifyJob(name) {
  const m = name.match(MATRIX_JOB_RE)
  if (m) {
    const {os, lean} = m.groups
    return {
      key: `compat:${os}:${lean}`,
      title: `Compat / Full Matrix (${os}, Lean ${lean})`,
      lean,
    }
  }
  for (const {prefix, key} of HEAVY_JOBS) {
    if (name.startsWith(prefix)) return {key, title: prefix, lean: null}
  }
  return null
}

function marker(key) {
  return `<!-- ${MARKER_PREFIX} ${key} -->`
}

function truncate(text, max) {
  const flat = (text || '').replace(/\s+/g, ' ').trim()
  return flat.length > max ? `${flat.slice(0, max - 1)}…` : flat
}

async function errorSummary(github, owner, repo, job) {
  try {
    const annotations = await github.paginate(github.rest.checks.listAnnotations, {
      owner,
      repo,
      check_run_id: job.id,
      per_page: 100,
    })
    return annotations
      .filter((a) => a.annotation_level === 'failure')
      .slice(0, MAX_ANNOTATIONS)
      .map((a) => `- \`${a.path}:${a.start_line}\` — ${truncate(a.message, MAX_ANNOTATION_MESSAGE)}`)
  } catch {
    return []
  }
}

function summaryBlock(lines) {
  if (lines.length === 0) {
    return '_No failure annotations were captured — see the job log via the run link above._'
  }
  return lines.join('\n')
}

async function commitSubject(github, context) {
  const fromPayload = context.payload.head_commit && context.payload.head_commit.message
  if (fromPayload) return fromPayload.split('\n')[0]
  try {
    const {data} = await github.rest.repos.getCommit({
      owner: context.repo.owner,
      repo: context.repo.repo,
      ref: context.sha,
    })
    return (data.commit.message || '').split('\n')[0]
  } catch {
    return ''
  }
}

async function ensureLabel(github, owner, repo, dryRun) {
  try {
    await github.rest.issues.getLabel({owner, repo, name: LABEL})
  } catch (error) {
    if (error.status !== 404) throw error
    if (!dryRun) {
      await github.rest.issues.createLabel({
        owner,
        repo,
        name: LABEL,
        color: 'd93f0b',
        description: 'Automated Lean/Rust version-drift CI failure tracker',
      })
    }
  }
}

async function findTrackedIssue(github, owner, repo, key) {
  const issues = await github.paginate(github.rest.issues.listForRepo, {
    owner,
    repo,
    state: 'open',
    labels: LABEL,
    per_page: 100,
  })
  const needle = marker(key)
  return issues.find((issue) => !issue.pull_request && (issue.body || '').includes(needle)) || null
}

async function alreadyReportedRun(github, owner, repo, issue, runId) {
  const comments = await github.paginate(github.rest.issues.listComments, {
    owner,
    repo,
    issue_number: issue.number,
    per_page: 100,
  })
  const needle = `actions/runs/${runId}`
  return (
    (issue.body || '').includes(needle) ||
    comments.some((comment) => (comment.body || '').includes(needle))
  )
}

function issueBody({key, title, lean, trigger, runId, runUrl, jobUrl, sha, subject, summary}) {
  const leanRow = lean ? `| Lean channel | \`${lean}\` |\n` : ''
  const subjectRow = subject ? ` — ${subject}` : ''
  return `${marker(key)}
Automated version-drift report from the leo3 compat CI. **Do not edit the marker
line above** — the CI reporter uses it to deduplicate and auto-close this issue.

| | |
| --- | --- |
| Job | \`${title}\` |
${leanRow}| Trigger | ${trigger} |
| Commit | \`${sha}\`${subjectRow} |
| First failing run | [run ${runId}](${runUrl}) |
| Job log | [${title}](${jobUrl}) |

## Latest failure summary

${summaryBlock(summary)}

---
_This issue is managed automatically by \`.github/scripts/compat-watch.js\` (see
\`TESTING.md\`). New failures append a comment; it closes automatically once the
job is green again on \`main\`. If this is a runner/infrastructure flake rather
than a real compatibility break, close it manually._`
}

function failureComment({runId, runUrl, trigger, sha, summary}) {
  return `Still failing as of [run ${runId}](${runUrl}) (${trigger}, commit \`${sha}\`).

## Latest failure summary

${summaryBlock(summary)}`
}

function recoveryComment({runId, runUrl, sha}) {
  return `Recovered: green again in [run ${runId}](${runUrl}) (commit \`${sha}\`). Closing automatically.`
}

module.exports = async function main({github, context, core}, opts = {}) {
  const dryRun = opts.dryRun === true
  const force = opts.force === true || process.env.COMPAT_WATCH_FORCE === '1'

  const onMain = context.ref === 'refs/heads/main'
  const allowedEvent = ['schedule'].includes(context.eventName) ||
    (['push', 'workflow_dispatch'].includes(context.eventName) && onMain)
  if (!force && !allowedEvent) {
    core.info(
      `compat-watch: skipping (event '${context.eventName}', ref '${context.ref}') — ` +
        'only main pushes, schedules and main workflow_dispatch are tracked',
    )
    return {skipped: true}
  }

  const {owner, repo} = context.repo
  if (!force && (owner !== 'LeanOxide' || repo !== 'leo3')) {
    core.info(`compat-watch: skipping in fork ${owner}/${repo}`)
    return {skipped: true}
  }

  const runId = context.runId
  const runUrl = `${context.payload.repository.html_url}/actions/runs/${runId}`
  const trigger = context.eventName === 'schedule'
    ? `scheduled sweep (\`${context.payload.schedule || 'cron'}\`)`
    : context.eventName
  const sha = context.sha.slice(0, 12)
  const subject = await commitSubject(github, context)

  const jobs = await github.paginate(github.rest.actions.listJobsForWorkflowRun, {
    owner,
    repo,
    run_id: runId,
    per_page: 100,
  })

  const tracked = []
  for (const job of jobs) {
    const info = classifyJob(job.name)
    if (!info) continue
    if (info.lean && !DRIFT_LEANS.has(info.lean)) continue
    if (job.conclusion === 'failure') tracked.push({...info, state: 'failed', job})
    else if (job.conclusion === 'success') tracked.push({...info, state: 'passed', job})
  }
  if (tracked.length === 0) {
    core.info('compat-watch: no tracked beta/nightly or Heavy jobs in this run')
    return {skipped: false, created: 0, commented: 0, closed: 0}
  }

  await ensureLabel(github, owner, repo, dryRun)

  let created = 0
  let commented = 0
  let closed = 0

  for (const entry of tracked) {
    const {key, title, state, job} = entry
    const existing = await findTrackedIssue(github, owner, repo, key)

    if (state === 'failed') {
      const summary = await errorSummary(github, owner, repo, job)
      if (!existing) {
        const body = issueBody({
          key, title, lean: entry.lean, trigger, runId, runUrl, jobUrl: job.html_url, sha, subject, summary,
        })
        if (dryRun) {
          core.info(`compat-watch: [dry-run] would create issue '${title}'\n${body}`)
        } else {
          const {data} = await github.rest.issues.create({
            owner,
            repo,
            title: `[CI drift] ${title} failing on main`,
            body,
            labels: [LABEL],
          })
          core.info(`compat-watch: created #${data.number} for '${title}'`)
        }
        created += 1
      } else if (await alreadyReportedRun(github, owner, repo, existing, runId)) {
        core.info(`compat-watch: run ${runId} already reported on #${existing.number}, skipping`)
      } else {
        const body = failureComment({runId, runUrl, trigger, sha, summary})
        if (dryRun) {
          core.info(`compat-watch: [dry-run] would comment on #${existing.number}\n${body}`)
        } else {
          await github.rest.issues.createComment({
            owner,
            repo,
            issue_number: existing.number,
            body,
          })
          core.info(`compat-watch: commented on #${existing.number} ('${title}')`)
        }
        commented += 1
      }
    } else if (existing) {
      const body = recoveryComment({runId, runUrl, sha})
      if (dryRun) {
        core.info(`compat-watch: [dry-run] would close #${existing.number}\n${body}`)
      } else {
        await github.rest.issues.createComment({
          owner,
          repo,
          issue_number: existing.number,
          body,
        })
        await github.rest.issues.update({
          owner,
          repo,
          issue_number: existing.number,
          state: 'closed',
          state_reason: 'completed',
        })
        core.info(`compat-watch: closed #${existing.number} ('${title}' recovered)`)
      }
      closed += 1
    }
  }

  core.info(`compat-watch: done (created=${created}, commented=${commented}, closed=${closed})`)
  return {skipped: false, created, commented, closed}
}

// Exported for unit testing.
module.exports._internals = {classifyJob, marker, truncate, issueBody, LABEL}
