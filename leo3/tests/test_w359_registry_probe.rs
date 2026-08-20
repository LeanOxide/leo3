//! W-359 experimental probe: per-`run_command` frontend registry growth on
//! 4.26+ (measured on 4.33.0-rc1).
//!
//! Diagnostic, not a regression assertion. Two probes:
//!
//! `probe_per_call_env_retention` — for `run_command("axiom X_N : Nat")`
//! calls that all start from the same base environment, measures the
//! refcount (`m_rc`) of each per-call output `Environment`, the state of
//! the 4.33 `Environment.checked` task (object field 2), whether draining
//! `checked` via `lean_task_get` releases references, and RSS growth.
//!
//! `probe_command_kind_bisection` — RSS slope per command kind
//! (`#check` / `axiom` / `def` / `set_option`) to localize the growth to
//! the `addDecl` path, then finalizes the Lean task manager and re-reads
//! the per-call env refcounts to test whether the task manager's global
//! task registry is the retainer.
//!
//! Run with `--nocapture` to see the diagnostics.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows")
))]

use leo3::meta::*;
use leo3::prelude::*;

#[cfg(target_os = "linux")]
fn rss_bytes() -> u64 {
    let s = std::fs::read_to_string("/proc/self/statm").expect("statm");
    let resident_pages: u64 = s.split_whitespace().nth(1).unwrap().parse().unwrap();
    resident_pages * 4096
}

#[cfg(not(target_os = "linux"))]
fn rss_bytes() -> u64 {
    0
}

/// 4.33 frontend `Environment` object field 2 is
/// `checked : Task Kernel.Environment` (field order: `base`,
/// `serverBaseExts`, `checked`, `asyncConstsMap`, `asyncCtx?`,
/// `importRealizationCtx?`, `localRealizationCtxMap`,
/// `allRealizations`; scalar `isExporting`).
fn checked_task(env: *const leo3::ffi::object::lean_object) -> *mut leo3::ffi::object::lean_object {
    unsafe { leo3::ffi::lean_ctor_get(env, 2) as *mut leo3::ffi::object::lean_object }
}

fn task_state(t: *const leo3::ffi::object::lean_object) -> u8 {
    unsafe { leo3::ffi::closure::lean_io_get_task_state_core(t) }
}

type Env<'l> = LeanBound<'l, LeanEnvironment>;

#[test]
fn probe_per_call_env_retention() {
    const ITERS: u32 = 50;

    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        let base_ptr = metam.env().as_ptr();
        let run = |cmd: &str| -> LeanResult<Env<'_>> {
            let stx = leo3::meta::repl::parse_command(lean, &metam.env(), cmd)?;
            leo3::meta::repl::run_command(lean, &metam, &stx)
        };

        // Warm up (one-time costs must not count).
        for i in 0..5 {
            run(&format!("axiom W{i} : Nat"))?;
        }

        let rss_before = leo3::run_worker(|| rss_bytes());
        let mut envs: Vec<Env<'_>> = Vec::with_capacity(ITERS as usize);
        for i in 0..ITERS {
            let e = run(&format!("axiom X{i} : Nat"))?;
            if i == 0 {
                let rc0 = leo3::run_worker(|| unsafe { (*e.as_ptr()).m_rc });
                eprintln!("[w359] env_0 rc right after call 1: {rc0}");
            }
            envs.push(e);
        }
        let rss_after = leo3::run_worker(|| rss_bytes());

        let (rcs, states): (Vec<i32>, Vec<u8>) = leo3::run_worker(|| unsafe {
            let mut rcs = Vec::with_capacity(envs.len());
            let mut states = Vec::with_capacity(envs.len());
            for e in &envs {
                let p = e.as_ptr();
                states.push(task_state(checked_task(p as *const _) as *const _));
                rcs.push((*p).m_rc);
            }
            (rcs, states)
        });
        let base_rc = leo3::run_worker(|| unsafe { (*base_ptr).m_rc });
        let (min_rc, max_rc) = (rcs.iter().min(), rcs.iter().max());
        let distinct_states: std::collections::BTreeSet<u8> = states.iter().copied().collect();
        eprintln!(
            "[w359] after {ITERS} axioms: base rc={base_rc}; per-call env rc \
             min={min_rc:?} max={max_rc:?}; distinct checked-task states: {distinct_states:?}"
        );
        let rss_growth = rss_after.saturating_sub(rss_before);
        eprintln!(
            "[w359] RSS growth over {ITERS} axioms: {rss_growth} bytes \
             (~{} KiB/call)",
            rss_growth / ITERS as u64 / 1024
        );

        // Phase B: drain each env's `checked` task and re-read the rc.
        let pumped_rcs: Vec<i32> = leo3::run_worker(|| unsafe {
            let mut out = Vec::with_capacity(envs.len());
            for e in &envs {
                let p = e.as_ptr();
                let t = checked_task(p as *const _);
                let v = leo3::ffi::closure::lean_task_get(t as leo3::ffi::b_lean_obj_arg);
                leo3::ffi::lean_dec(v as *mut leo3::ffi::object::lean_object);
                out.push((*p).m_rc);
            }
            out
        });
        let rss_pumped = leo3::run_worker(|| rss_bytes());
        let (min2, max2) = (pumped_rcs.iter().min(), pumped_rcs.iter().max());
        eprintln!(
            "[w359] after draining checked tasks: per-call env rc min={min2:?} \
             max={max2:?}; RSS delta since pre-pump: {} bytes",
            rss_pumped.saturating_sub(rss_after)
        );
        Ok(())
    });
    result.expect("probe failed");
}

#[test]
fn probe_command_kind_bisection() {
    leo3::test_with_lean(probe_bisection_inner).expect("probe failed");
}

fn probe_bisection_inner<'l>(lean: Lean<'l>) -> LeanResult<()> {
    const ITERS: u32 = 50;

    let env = import_modules(lean, &["Lean"], 0)?;
    let metam = MetaMContext::new(lean, env)?;
    let base_ptr = metam.env().as_ptr();
    let run = |cmd: &str| -> LeanResult<Env<'l>> {
        let stx = leo3::meta::repl::parse_command(lean, &metam.env(), cmd)?;
        leo3::meta::repl::run_command(lean, &metam, &stx)
    };

    // Session warm-up.
    for i in 0..5 {
        run(&format!("axiom W{i} : Nat"))?;
        run("#check 1")?;
    }

    let phase =
        |name: &str, make: &dyn Fn(u32) -> String, keep_envs: bool| -> (u64, Vec<Env<'l>>) {
            let rss0 = leo3::run_worker(|| rss_bytes());
            let mut envs = Vec::new();
            let mut ptrs: Vec<*mut leo3::ffi::object::lean_object> = Vec::new();
            for i in 0..ITERS {
                let e = run(&make(i)).unwrap();
                let p = e.as_ptr();
                if keep_envs {
                    if i == 0 {
                        let rc0 = leo3::run_worker(|| unsafe { (*p).m_rc });
                        eprintln!("[w359] {name}: first per-call env rc: {rc0}");
                    }
                    envs.push(e);
                }
                ptrs.push(p);
            }
            let distinct = ptrs.iter().copied().collect::<std::collections::BTreeSet<_>>();
            let rss1 = leo3::run_worker(|| rss_bytes());
            let growth = rss1.saturating_sub(rss0);
            eprintln!(
                "[w359] {name}: {ITERS} calls, RSS growth {growth} bytes (~{} KiB/call), \
                 distinct env objects: {}",
                growth / ITERS as u64 / 1024,
                distinct.len()
            );
            (growth, envs)
        };

    let make_check = |_: u32| "#check 1".to_string();
    let make_axiom = |i: u32| format!("axiom A{i} : Nat");
    let make_def = |i: u32| format!("def D{i} : Nat := 1");

    phase("#check 1 (control, no addDecl)", &make_check, true);
    let (_, ax_envs) = phase("axiom (addDecl)", &make_axiom, true);
    let (_, def_envs) = phase("def (addDecl + kernel check)", &make_def, true);

    let base_rc = leo3::run_worker(|| unsafe { (*base_ptr).m_rc });
    eprintln!("[w359] base env rc after all phases: {base_rc}");

    let rc_of = |envs: &Vec<Env<'l>>| -> (Option<i32>, Option<i32>) {
        leo3::run_worker(|| unsafe {
            let rcs: Vec<i32> = envs.iter().map(|e| (*e.as_ptr()).m_rc).collect();
            (rcs.iter().min().copied(), rcs.iter().max().copied())
        })
    };
    let (amin, amax) = rc_of(&ax_envs);
    let (dmin, dmax) = rc_of(&def_envs);
    eprintln!(
        "[w359] per-call env rc: axiom min={amin:?} max={amax:?}; \
         def min={dmin:?} max={dmax:?}"
    );

    // Experiment: finalize the Lean task manager and re-read.
    eprintln!("[w359] finalizing task manager ...");
    leo3::run_worker(|| unsafe {
        leo3::ffi::closure::lean_finalize_task_manager();
    });
    let (amin2, amax2) = rc_of(&ax_envs);
    let (dmin2, dmax2) = rc_of(&def_envs);
    let rss_final = leo3::run_worker(|| rss_bytes());
    eprintln!(
        "[w359] after task-manager finalization: axiom rc min={amin2:?} max={amax2:?}; \
         def rc min={dmin2:?} max={dmax2:?}; RSS now {rss_final}"
    );
    Ok(())
}

/// Field accessor for the 4.33 frontend `Environment` object.
fn env_field(env: *const leo3::ffi::object::lean_object, i: u32) -> *mut leo3::ffi::object::lean_object {
    unsafe { leo3::ffi::lean_ctor_get(env, i) as *mut leo3::ffi::object::lean_object }
}

/// `lean_task` layout (4.33 lean.h): header (8) + m_value (8) + m_imp (8).
fn task_finished(t: *const leo3::ffi::object::lean_object) -> bool {
    unsafe { !(* (t as *const *mut leo3::ffi::object::lean_object).add(1) ).is_null() }
}

/// Decisive retention test: every per-call `Environment` `env_i` produced by
/// `addDecl` shares `base` (object field 0) with the imported env's kernel
/// environment `K0` (`addConstAsync` updates `checked`/`allRealizations`/
/// `asyncConstsMap` but not `base`). `K0` itself is never freed while the
/// imported env is alive, so its refcount is a safe canary for whether the
/// per-call envs survive after we drop our Rust references.
#[test]
fn probe_drop_canary() {
    const N: u32 = 100;
    leo3::test_with_lean(|lean: Lean<'_>| -> LeanResult<()> {
        let env0 = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env0)?;
        let (k0, header, fields, k0_rc_init) = leo3::run_worker(|| unsafe {
            let e0 = metam.env().as_ptr();
            let h = e0 as *const u64;
            let header = (
                (*e0).m_rc,
                u16::from_le_bytes([(*e0).m_cs_sz as u8, 0]),
                (*e0).m_other,
                (*e0).m_tag,
            );
            let mut fields = Vec::new();
            for i in 0..10usize {
                fields.push(h.add(1 + i).read_unaligned());
            }
            let vm0 = env_field(e0, 0); // VisibilityMap shared by base
            (vm0, header, fields, (*vm0).m_rc)
        });
        eprintln!(
            "[w359-drop] E0 header (rc,cs,other,tag)={header:?}; raw fields={fields:?}; VM-rc={k0_rc_init}"
        );
        let run = |cmd: &str| -> LeanResult<Env<'_>> {
            let stx = leo3::meta::repl::parse_command(lean, &metam.env(), cmd)?;
            leo3::meta::repl::run_command(lean, &metam, &stx)
        };

        for i in 0..5 {
            run(&format!("axiom W{i} : Nat"))?;
        }

        let rc_of = |p: *const leo3::ffi::object::lean_object| {
            leo3::run_worker(|| unsafe { (*p).m_rc })
        };
        let rss = || leo3::run_worker(|| rss_bytes());

        let k0_rc_warm = rc_of(k0);
        let rss0 = rss();
        let mut envs: Vec<Env<'_>> = Vec::with_capacity(N as usize);
        for i in 0..N {
            envs.push(run(&format!("axiom A{i} : Nat"))?);
        }
        let k0_rc_held = rc_of(k0);
        let rss1 = rss();
        // Audit one per-call env's task fields before the drop.
        let e = &envs[10];
        let e0p = metam.env().as_ptr();
        let audit = leo3::run_worker(|| unsafe {
            let p = e.as_ptr();
            let h = p as *const u64;
            let fields: Vec<u64> = (0..10usize).map(|i| h.add(1 + i).read_unaligned()).collect();
            let header = ((*p).m_rc, (*p).m_other, (*p).m_tag);
            let t_checked = env_field(p, 2);
            let t_reals = env_field(p, 7);
            let words = |t: *const leo3::ffi::object::lean_object| -> (i32, i32, i32) {
                let b = t as *const u64;
                (
                    b.read_unaligned() as i32, // m_rc
                    b.add(1).read_unaligned() as i32, // m_value as i32 (0/1 = null/non-null)
                    b.add(2).read_unaligned() as i32, // m_imp as i32
                )
            };
            let vm_same = fields[0] == (e0p as *const u64).add(1).read_unaligned();
            (
                header,
                fields,
                vm_same,
                (*env_field(p, 0)).m_rc,
                words(t_checked),
                words(t_reals),
            )
        });
        eprintln!(
            "[w359-drop] env_10 header (rc,other,tag)={:?}; fields={:?}; base-VM-same-as-E0={} VM-rc={}; checked-words={:?} reals-words={:?}",
            audit.0, audit.1, audit.2, audit.3, audit.4, audit.5
        );

        eprintln!(
            "[w359-drop] before drop: E0-base-VM rc {k0_rc_warm} -> {k0_rc_held}; \
             RSS {rss0} -> {rss1} (+{} bytes over {N} calls)",
            rss1.saturating_sub(rss0)
        );

        // Safe drop: keep observer refs to env_10's per-call subtree —
        // the env object itself, its `checked` task (f2), that task's
        // `m_value` (a pure task wrapping the per-call kernel env), the
        // kernel env, the per-call base `VisibilityMap` (f0) and the
        // kernel env in its `private` slot — so their refcounts can be
        // read after all `envs` are dropped. This mirrors the REPL
        // scenario where per-call envs are discarded by the caller.
        let held = leo3::run_worker(|| unsafe {
            let p = e.as_ptr();
            let t10 = env_field(p, 2);
            let v10 = *(t10 as *const *mut leo3::ffi::object::lean_object).add(1);
            // `m_value` is a Finished pure task (tag 252) wrapping the
            // per-call kernel env; fall back to the raw value otherwise.
            let k10 = if !v10.is_null() && (*v10).m_tag == 252 {
                *(v10 as *const *mut leo3::ffi::object::lean_object).add(1)
            } else {
                v10
            };
            let vm10 = env_field(p, 0);
            let k0p = if vm10.is_null() {
                vm10
            } else {
                env_field(vm10, 0)
            };
            for q in [p, t10, v10, k10, vm10, k0p] {
                if !q.is_null() {
                    leo3::ffi::lean_inc(q);
                }
            }
            (p, t10, v10, k10, vm10, k0p)
        });

        let rss_held = rss();
        drop(envs);
        std::thread::yield_now();

        let dropped = leo3::run_worker(|| unsafe {
            let rc = |q: *const leo3::ffi::object::lean_object| {
                if q.is_null() {
                    0
                } else {
                    (*q).m_rc
                }
            };
            (
                rc(held.0),
                rc(held.1),
                rc(held.2),
                rc(held.3),
                rc(held.4),
                rc(held.5),
                rss_bytes(),
            )
        });
        eprintln!(
            "[w359-drop] after dropping all {N} envs: \
             env_10 rc={} (1=observer ref only -> no global retention), \
             checked-task rc={} (2=env_10+us clean, 3=retained), \
             task m_value rc={} (2=clean, 3=retained), \
             kernel-env rc={} (2=clean, 3=retained), \
             base-VM rc={} (2=clean, 3=retained), \
             VM.private kernel-env rc={} (2=clean, 3=retained)",
            dropped.0, dropped.1, dropped.2, dropped.3, dropped.4, dropped.5
        );
        eprintln!(
            "[w359-drop] RSS held={} -> dropped={} (freed {} bytes on drop; \
             ~{} KiB/call retained by a global afterwards)",
            rss_held,
            dropped.6,
            rss_held as i64 - dropped.6 as i64,
            (N as i64 * 20480).saturating_sub(rss_held as i64 - dropped.6 as i64) / N as i64 / 1024
        );
        leo3::run_worker(|| unsafe {
            for q in [held.0, held.1, held.2, held.3, held.4, held.5] {
                if !q.is_null() {
                    leo3::ffi::lean_dec(q);
                }
            }
        });
        Ok(())
    })
    .expect("probe failed");
}

// ============================================================================
// Task-manager retention canaries (no frontend involved).
//
// The 4.33 runtime's task-state docs (lean.h) say a Finished task is freed
// when its RC reaches 0, and `BaseIO.mapTask`/`chainTask` docs say such
// tasks "will run even if the last reference to the task is dropped" —
// i.e. the task manager owns them while pending. Whether the manager also
// retains them (and their results/closures) after completion is the open
// question behind W-359. These canaries isolate the runtime behavior:
//
// `probe_task_canary` — `lean_task_map_core` over an already-completed
// pure task, with an identity closure capturing an unused `dummy`
// upvalue; the pure task's value is `canary`. After the mapped task
// completes and all our references are dropped, a clean runtime frees
// the task (`canary` back to the observer ref only) and the closure
// (`dummy` back to the observer ref only).
//
// `probe_promise_canary` — a resolved `IO.Promise`; same question for the
// promise's result task, whose "Promised" state never enters the task
// manager's queue.
// ============================================================================

extern "C" fn w359_identity(
    self_: *mut leo3::ffi::object::lean_object,
    arg: *mut leo3::ffi::object::lean_object,
) -> *mut leo3::ffi::object::lean_object {
    // Lean closure convention: the callee CONSUMES every argument,
    // including `self` (the upvalue array head). Consume both: drop
    // the upvalue, return the dependency result as the mapped value.
    unsafe {
        leo3::ffi::lean_dec(self_);
    }
    arg
}

/// A fresh 0-field ctor object with a multi-threaded header (`m_rc = -1`),
/// so refcount updates from the task manager's worker threads are legal.
unsafe fn fresh_mt() -> *mut leo3::ffi::object::lean_object {
    let o = leo3::ffi::lean_alloc_ctor(0, 0, 0);
    (*o).m_rc = -1;
    o
}

/// Builds a 4.33 Lean closure object (header + `m_fun` + `m_arity` +
/// `m_num_fixed` + `m_objs[]`), arity 2 with one fixed upvalue, pointing
/// at [`w359_identity`].
unsafe fn mk_identity_closure(
    upvalue: *mut leo3::ffi::object::lean_object,
) -> *mut leo3::ffi::object::lean_object {
    let c = leo3::ffi::lean_alloc_ctor(0, 0, 24); // 8 + 24 = 32 bytes
    (*c).m_tag = 245; // LeanClosure
    (*c).m_rc = -1;
    (c as *mut *mut std::os::raw::c_void)
        .add(1)
        .write(w359_identity as *mut std::os::raw::c_void); // m_fun @8
    (c as *mut u16).add(8).write(2); // m_arity @16
    (c as *mut u16).add(9).write(1); // m_num_fixed @18
    (c as *mut *mut leo3::ffi::object::lean_object).add(3).write(upvalue); // m_objs[0] @24
    leo3::ffi::lean_inc(upvalue);
    c
}

#[test]
fn probe_task_canary() {
    const N: u32 = 100;
    leo3::test_with_lean(|_lean: Lean<'_>| -> LeanResult<()> {
        leo3::run_worker(|| -> LeanResult<()> {
            unsafe {
                let mut s_vals = Vec::with_capacity(N as usize);
                let mut d_vals = Vec::with_capacity(N as usize);
                for sync in [true, false] {
                    let base = s_vals.len();
                    for _ in 0..N {
                        let s = fresh_mt(); // task-value canary (observer ref, held to the end)
                        leo3::ffi::lean_inc(s); // hand off a second ref to `lean_task_pure`
                        let d = fresh_mt(); // upvalue canary (observer ref, held to the end)
                        let cl = mk_identity_closure(d); // closure takes its own ref on `d`
                        let t_pure = leo3::ffi::closure::lean_task_pure(s);
                        let _t_map = leo3::ffi::closure::lean_task_map_core(
                            cl,
                            t_pure,
                            0,
                            sync,
                            false,
                        );
                        // Block until Finished. `lean_task_get` borrows both the
                        // task and the result (`b_obj_arg`/`b_obj_res`), so our
                        // reference to the mapped task must be released explicitly:
                        // dropping it is what triggers `deactivate_task` ->
                        // `lean_dec(m_value)` -> `free_task` for a clean runtime.
                        leo3::ffi::closure::lean_task_get(_t_map);
                        leo3::ffi::lean_dec(_t_map);
                        s_vals.push(s);
                        d_vals.push(d);
                    }
                    // Count survivors in this batch right away, then after
                    // forcing worker-frame reuse (more tasks) and a short
                    // sleep — to tell permanent task-manager retention apart
                    // from transient worker-frame retention.
                    let end = s_vals.len();
                    let count = |s: &[*mut leo3::ffi::object::lean_object],
                                d: &[*mut leo3::ffi::object::lean_object]| -> (u32, u32) {
                        (
                            s.iter().filter(|q| (*(**q)).m_rc.abs() > 1).count() as u32,
                            d.iter().filter(|q| (*(**q)).m_rc.abs() > 1).count() as u32,
                        )
                    };
                    let (kt1, kc1) = count(
                        &s_vals[base..end],
                        &d_vals[base..end],
                    );
                    for _ in 0..10 {
                        let s = fresh_mt();
                        leo3::ffi::lean_inc(s);
                        let d = fresh_mt();
                        let cl = mk_identity_closure(d);
                        let t_pure = leo3::ffi::closure::lean_task_pure(s);
                        let t_map =
                            leo3::ffi::closure::lean_task_map_core(cl, t_pure, 0, true, false);
                        leo3::ffi::closure::lean_task_get(t_map);
                        leo3::ffi::lean_dec(t_map);
                        leo3::ffi::lean_dec(s);
                        leo3::ffi::lean_dec(d);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    let (kt2, kc2) = count(
                        &s_vals[base..end],
                        &d_vals[base..end],
                    );
                    eprintln!(
                        "[w359-task-canary] sync={sync} {N} mapTasks (no task refs kept): \
                         task-value retained: immediate={kt1} after-flush={kt2} \
                         (0=clean, {N}=all retained); closure retained: \
                         immediate={kc1} after-flush={kc2} (0=clean, {N}=all retained)"
                    );
                }
                for v in s_vals.iter().chain(d_vals.iter()) {
                    leo3::ffi::lean_dec(*v);
                }
            }
            Ok(())
        })
        .expect("task canary failed");
        Ok(())
    })
    .expect("probe failed");
}

#[test]
fn probe_promise_canary() {
    leo3::test_with_lean(|_lean: Lean<'_>| -> LeanResult<()> {
        leo3::run_worker(|| -> LeanResult<()> {
            unsafe {
                let canary = fresh_mt();
                leo3::ffi::lean_inc(canary); // observer ref, kept to the end
                // 4.33: `Promise α` IS the runtime `lean_promise_object`
                // ({ m_header, m_result : Task α }) — not a ctor wrapper.
                let p = leo3::ffi::closure::lean_io_promise_new(leo3::ffi::io::lean_io_mk_world());
                let tag_p = (*p).m_tag;
                let m_result = *(p as *const *mut leo3::ffi::object::lean_object).add(1);
                let t_p = leo3::ffi::closure::lean_io_promise_result_opt(p);
                leo3::ffi::lean_inc(t_p); // observer ref
                // Consumes `canary`; may or may not consume `p` (4.33 ABI).
                let r = leo3::ffi::closure::lean_io_promise_resolve(
                    canary,
                    p,
                    leo3::ffi::io::lean_io_mk_world(),
                );
                // Result is `BaseIO Unit`: dec only when it is a heap object.
                if (r as usize) & 1 == 0 {
                    leo3::ffi::lean_dec(r);
                }
                // The runtime zeroes `m_rc` before freeing, so 0 here means
                // the C resolve consumed `p` (we must not dec it again).
                let rc_p = (*p).m_rc;
                // Blocks until Finished; the returned value is borrowed.
                let _res = leo3::ffi::closure::lean_task_get(t_p);
                let rc_t = (*t_p).m_rc;
                let rc_c = (*canary).m_rc;
                eprintln!(
                    "[w359-promise-canary] held: p tag={tag_p} rc={rc_p} \
                     (0=C consumed it, 1=we still own it) m_result same-as-t_p={}\
                     t_p rc={rc_t} (3=p+us+observer clean) canary={rc_c} \
                     (2=us + task result clean)",
                    std::ptr::eq(m_result, t_p)
                );
                // Drop every ref we own: t_p (2x) and p when we own it.
                leo3::ffi::lean_dec(t_p);
                leo3::ffi::lean_dec(t_p);
                if rc_p == 1 {
                    leo3::ffi::lean_dec(p);
                }
                std::thread::yield_now();
                let rc_c2 = (*canary).m_rc;
                eprintln!(
                    "[w359-promise-canary] after dropping all our refs: canary={rc_c2} \
                     (1=clean, 2=resolved promise task retained by a global)"
                );
                leo3::ffi::lean_dec(canary);
            }
            Ok(())
        })
        .expect("promise canary failed");
        Ok(())
    })
    .expect("probe failed");
}
