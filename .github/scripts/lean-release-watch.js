// Lean release sentinel: detects new leanprover/lean4 releases (stable or
// beta/RC prereleases) and immediately dispatches the full CI matrix on
// `main`, moving the discovery window from "next daily sweep" to "release
// day".
//
// Invoked from .github/workflows/lean-release-watch.yml via
// actions/github-script:
//
//   const script = require('./.github/scripts/lean-release-watch.js')
//   await script({github, context, core})
//
// The job is stateless: instead of persisting "last seen" state it treats
// any release published within the look-back window (default 26h, slightly
// wider than the daily cron period) as new. A release can therefore trigger
// at most one extra matrix run if the sentinel is retried, which the CI
// concurrency group absorbs.

'use strict'

const WINDOW_HOURS = 26
const LEAN_TAG_RE = /^v\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/
const TARGET_WORKFLOW = 'ci.yml'

module.exports = async function main({github, context, core}, opts = {}) {
  const dryRun = opts.dryRun === true
  const windowHours = opts.windowHours ?? WINDOW_HOURS

  const {owner, repo} = context.repo
  if (owner !== 'LeanOxide' || repo !== 'leo3') {
    core.info(`lean-release-watch: skipping in fork ${owner}/${repo}`)
    return {skipped: true}
  }

  const releases = await github.paginate(github.rest.repos.listReleases, {
    owner: 'leanprover',
    repo: 'lean4',
    per_page: 100,
  })

  const cutoff = Date.now() - windowHours * 3600 * 1000
  const fresh = releases.filter(
    (release) =>
      !release.draft &&
      LEAN_TAG_RE.test(release.tag_name) &&
      Date.parse(release.published_at) >= cutoff,
  )

  if (fresh.length === 0) {
    core.info(`lean-release-watch: no new lean4 releases in the last ${windowHours}h`)
    return {skipped: false, dispatched: false}
  }

  for (const release of fresh) {
    core.info(
      `lean-release-watch: new release ${release.tag_name} ` +
        `(${release.prerelease ? 'prerelease' : 'stable'}) published at ${release.published_at}`,
    )
  }

  if (dryRun) {
    core.info(`lean-release-watch: [dry-run] would dispatch ${TARGET_WORKFLOW} on main`)
    return {skipped: false, dispatched: false, fresh: fresh.map((r) => r.tag_name)}
  }

  await github.rest.actions.createWorkflowDispatch({
    owner,
    repo,
    workflow_id: TARGET_WORKFLOW,
    ref: 'main',
  })
  core.info(`lean-release-watch: dispatched ${TARGET_WORKFLOW} on main`)
  return {skipped: false, dispatched: true, fresh: fresh.map((r) => r.tag_name)}
}

module.exports._internals = {LEAN_TAG_RE, WINDOW_HOURS}
