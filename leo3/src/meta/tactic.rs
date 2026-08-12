//! Tactic state and goal management types.
//!
//! This module provides the foundation for tactic-style proof construction,
//! where proofs are built by transforming a list of goals (metavariables)
//! rather than constructing proof terms directly.
//!
//! # Core Types
//!
//! - [`TacticState`] — A snapshot of goals (metavariable expressions) at a point in a tactic proof
//! - [`TacticResult`] — The outcome of applying a tactic: success with new state, or failure
//!
//! # Example
//!
//! ```ignore
//! use leo3::prelude::*;
//! use leo3::meta::*;
//! use leo3::meta::tactic::*;
//!
//! leo3::with_lean(|lean| {
//!     let env = LeanEnvironment::empty(lean, 0)?;
//!     let mut metam = MetaMContext::new(lean, env)?;
//!
//!     // Create a goal (metavariable)
//!     let goal_name = LeanName::from_str(lean, "?goal.1")?;
//!     let goal = LeanExpr::mvar(lean, goal_name)?;
//!
//!     let state = TacticState::new(vec![goal]);
//!     assert_eq!(state.num_goals(), 1);
//!     assert!(!state.is_solved());
//!     Ok(())
//! })?;
//! ```

use crate::err::{LeanError, LeanResult};
use crate::instance::LeanBound;
use crate::meta::expr::LeanExpr;
use crate::meta::metam::MetaMContext;
use crate::meta::name::LeanName;
use leo3_ffi as ffi;

/// A tactic state: a list of goals (metavariable expressions) to be solved.
///
/// Each goal is a metavariable expression (`LeanExpr` with kind `MVar`).
/// Tactics transform a `TacticState` by solving, splitting, or replacing goals.
///
/// `TacticState` does not own a `MetaMContext` — it only holds the goal list.
/// The `MetaMContext` is passed separately when goal inspection is needed.
pub struct TacticState<'l> {
    goals: Vec<LeanBound<'l, LeanExpr>>,
}

impl<'l> TacticState<'l> {
    /// Create a new tactic state from a list of goals.
    ///
    /// Goals are metavariable expressions (created with `LeanExpr::mvar`).
    pub fn new(goals: Vec<LeanBound<'l, LeanExpr>>) -> Self {
        Self { goals }
    }

    /// All remaining goals.
    pub fn goals(&self) -> &[LeanBound<'l, LeanExpr>] {
        &self.goals
    }

    /// The main (first) goal, if any.
    pub fn main_goal(&self) -> Option<&LeanBound<'l, LeanExpr>> {
        self.goals.first()
    }

    /// Whether all goals have been solved.
    pub fn is_solved(&self) -> bool {
        self.goals.is_empty()
    }

    /// Number of remaining goals.
    pub fn num_goals(&self) -> usize {
        self.goals.len()
    }

    /// Drop the first goal and return the remaining state.
    ///
    /// If there are no goals, returns an empty state.
    pub fn remaining_goals(mut self) -> Self {
        if !self.goals.is_empty() {
            self.goals.remove(0);
        }
        self
    }

    /// Consume the tactic state and return the goals vector.
    pub fn into_goals(self) -> Vec<LeanBound<'l, LeanExpr>> {
        self.goals
    }

    /// Apply a tactic function to the current state.
    ///
    /// The tactic function receives the full state and MetaMContext,
    /// and returns a `TacticResult`.
    pub fn apply_tactic<F>(self, metam: &mut MetaMContext<'l>, tactic: F) -> TacticResult<'l>
    where
        F: FnOnce(&mut MetaMContext<'l>, TacticState<'l>) -> TacticResult<'l>,
    {
        tactic(metam, self)
    }

    /// Focus on the main goal and run a tactic.
    ///
    /// The tactic receives a state with only the main goal. After the tactic
    /// completes, any new goals are prepended to the remaining goals from
    /// the original state.
    pub fn focus<F>(self, metam: &mut MetaMContext<'l>, tactic: F) -> TacticResult<'l>
    where
        F: FnOnce(&mut MetaMContext<'l>, TacticState<'l>) -> TacticResult<'l>,
    {
        if self.goals.is_empty() {
            return TacticResult::Failure(LeanError::other("focus: no goals"));
        }

        let mut goals = self.goals;
        let main = goals.remove(0);
        let rest = goals;

        let focused = TacticState::new(vec![main]);
        match tactic(metam, focused) {
            TacticResult::Success(mut result_state) => {
                result_state.goals.extend(rest);
                TacticResult::Success(result_state)
            }
            failure => failure,
        }
    }
}

/// The result of applying a tactic.
pub enum TacticResult<'l> {
    /// Tactic succeeded, producing a new state.
    Success(TacticState<'l>),
    /// Tactic failed with an error.
    Failure(LeanError),
}

/// Get the type of a goal (metavariable) by inferring its type via MetaM.
///
/// The goal must be a metavariable expression. This calls `infer_type` on
/// the metavariable expression to retrieve the proposition/type it represents.
///
/// # Errors
///
/// Returns an error if `infer_type` fails (e.g., the metavariable is unknown
/// in the current context).
pub fn goal_type<'l>(
    metam: &mut MetaMContext<'l>,
    goal: &LeanBound<'l, LeanExpr>,
) -> LeanResult<LeanBound<'l, LeanExpr>> {
    if LeanExpr::is_mvar(goal) {
        Ok(metam.get_mvar_decl(goal)?.type_)
    } else {
        metam.infer_type(goal)
    }
}

// ============================================================================
// Helper: checked assignment via MetaM
// ============================================================================

/// Assign a metavariable to a value using `lean_checked_assign` (MetaM).
///
/// Returns `true` if the assignment succeeded (type-checked), `false` otherwise.
fn checked_assign<'l>(
    metam: &mut MetaMContext<'l>,
    mvar_id: &LeanBound<'l, LeanName>,
    val: &LeanBound<'l, LeanExpr>,
) -> LeanResult<bool> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        // lean_checked_assign has arity 7:
        // (mvarId, val, meta_ctx, meta_state, core_ctx, core_state, world)
        // Fix 2 args (mvarId, val), leaving 5 for run().
        ffi::lean_inc(mvar_id.as_ptr());
        ffi::lean_inc(val.as_ptr());
        let closure = ffi::inline::lean_alloc_closure(
            ffi::meta::lean_checked_assign as *mut std::ffi::c_void,
            7,
            2,
        );
        ffi::inline::lean_closure_set(closure, 0, mvar_id.as_ptr());
        ffi::inline::lean_closure_set(closure, 1, val.as_ptr());

        let computation = LeanBound::from_owned_ptr(metam.lean(), closure);
        let result = metam.run_persistent(computation)?;

        // Result is a Lean Bool: lean_box(0) = false, lean_box(1) = true
        let bool_val = ffi::lean_unbox(result.as_ptr());
        Ok(bool_val != 0)
    }
}

// ============================================================================
// Tactic: exact
// ============================================================================

/// Close the main goal by providing an exact proof term.
///
/// Checks that the type of `expr` is definitionally equal to the goal type,
/// then assigns `expr` to the goal's metavariable.
///
/// # Errors
///
/// Returns an error if:
/// - There are no goals
/// - Type inference fails
/// - The expression's type doesn't match the goal type
/// - The checked assignment fails
pub fn exact<'l>(
    metam: &mut MetaMContext<'l>,
    state: TacticState<'l>,
    expr: &LeanBound<'l, LeanExpr>,
) -> TacticResult<'l> {
    let goal = match state.main_goal() {
        Some(g) => g,
        None => return TacticResult::Failure(LeanError::other("exact: no goals")),
    };

    let goal_decl = match metam.get_mvar_decl(goal) {
        Ok(decl) => decl,
        Err(e) => return TacticResult::Failure(e),
    };
    let goal_ty = goal_decl.type_;

    let expr_ty = match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
        m.infer_type(expr)
    }) {
        Ok(ty) => ty,
        Err(e) => return TacticResult::Failure(e),
    };

    // Check definitional equality
    match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
        m.is_def_eq(&expr_ty, &goal_ty)
    }) {
        Ok(true) => {}
        Ok(false) => {
            return TacticResult::Failure(LeanError::other(
                "exact: type mismatch — expression type is not definitionally equal to goal type",
            ));
        }
        Err(e) => return TacticResult::Failure(e),
    }

    // Assign the expression to the goal metavariable
    let mvar_id = match LeanExpr::mvar_id(goal) {
        Ok(id) => id,
        Err(e) => return TacticResult::Failure(e),
    };

    match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
        checked_assign(m, &mvar_id, expr)
    }) {
        Ok(true) => {}
        Ok(false) => {
            return TacticResult::Failure(LeanError::other("exact: checked assignment failed"));
        }
        Err(e) => return TacticResult::Failure(e),
    }

    TacticResult::Success(state.remaining_goals())
}

// ============================================================================
// Tactic: rfl
// ============================================================================

/// Close a reflexivity goal (`a = a`).
///
/// The goal type must be of the form `@Eq α a a`. Builds `@Eq.refl α a`
/// and assigns it to the goal.
///
/// # Errors
///
/// Returns an error if:
/// - There are no goals
/// - The goal type is not an equality
/// - The two sides of the equality are not definitionally equal
pub fn rfl<'l>(metam: &mut MetaMContext<'l>, state: TacticState<'l>) -> TacticResult<'l> {
    let goal = match state.main_goal() {
        Some(g) => g,
        None => return TacticResult::Failure(LeanError::other("rfl: no goals")),
    };

    let goal_decl = match metam.get_mvar_decl(goal) {
        Ok(decl) => decl,
        Err(e) => return TacticResult::Failure(e),
    };
    let goal_ty = goal_decl.type_;

    // The goal type should be `@Eq α lhs rhs` which is `App (App (App (Const Eq [u]) α) lhs) rhs`
    // We need to extract α and lhs, then build @Eq.refl α lhs
    // Peel off: rhs = app_arg(goal_ty), inner = app_fn(goal_ty)
    //           lhs = app_arg(inner), inner2 = app_fn(inner)
    //           α = app_arg(inner2), eq_const = app_fn(inner2)
    let lean = metam.lean();

    let rhs = match LeanExpr::app_arg(&goal_ty) {
        Ok(r) => r,
        Err(_) => {
            return TacticResult::Failure(LeanError::other(
                "rfl: goal type is not an application (expected equality)",
            ));
        }
    };
    let inner = match LeanExpr::app_fn(&goal_ty) {
        Ok(f) => f,
        Err(_) => {
            return TacticResult::Failure(LeanError::other("rfl: goal type is not an equality"));
        }
    };
    let lhs = match LeanExpr::app_arg(&inner) {
        Ok(l) => l,
        Err(_) => {
            return TacticResult::Failure(LeanError::other("rfl: goal type is not an equality"));
        }
    };
    let inner2 = match LeanExpr::app_fn(&inner) {
        Ok(f) => f,
        Err(_) => {
            return TacticResult::Failure(LeanError::other("rfl: goal type is not an equality"));
        }
    };
    let alpha = match LeanExpr::app_arg(&inner2) {
        Ok(a) => a,
        Err(_) => {
            return TacticResult::Failure(LeanError::other("rfl: goal type is not an equality"));
        }
    };
    let eq_const = match LeanExpr::app_fn(&inner2) {
        Ok(c) => c,
        Err(_) => {
            return TacticResult::Failure(LeanError::other("rfl: goal type is not an equality"));
        }
    };

    // Extract universe levels from the Eq constant
    let eq_levels = match LeanExpr::const_levels(&eq_const) {
        Ok(l) => l,
        Err(_) => {
            return TacticResult::Failure(LeanError::other("rfl: Eq head is not a constant"));
        }
    };

    // Check that lhs and rhs are definitionally equal
    match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
        m.is_def_eq(&lhs, &rhs)
    }) {
        Ok(true) => {}
        Ok(false) => {
            return TacticResult::Failure(LeanError::other(
                "rfl: left-hand side and right-hand side are not definitionally equal",
            ));
        }
        Err(e) => return TacticResult::Failure(e),
    }

    // Build @Eq.refl α lhs
    let refl_proof = match LeanExpr::mk_eq_refl(lean, eq_levels, &alpha, &lhs) {
        Ok(p) => p,
        Err(e) => return TacticResult::Failure(e),
    };

    // Assign to the goal
    let mvar_id = match LeanExpr::mvar_id(goal) {
        Ok(id) => id,
        Err(e) => return TacticResult::Failure(e),
    };

    match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
        checked_assign(m, &mvar_id, &refl_proof)
    }) {
        Ok(true) => {}
        Ok(false) => {
            return TacticResult::Failure(LeanError::other("rfl: checked assignment failed"));
        }
        Err(e) => return TacticResult::Failure(e),
    }

    TacticResult::Success(state.remaining_goals())
}

// ============================================================================
// Tactic: intro
// ============================================================================

/// Introduce a hypothesis from a forall/pi goal.
///
/// The goal type must be a forall `∀ (x : A), B`. This tactic:
/// 1. Creates a fresh free variable for the bound variable
/// 2. Substitutes it into the body to get the new goal type
/// 3. Creates a new metavariable goal with the substituted type
/// 4. Assigns `λ (x : A), ?new_goal` to the original goal
///
/// # Errors
///
/// Returns an error if:
/// - There are no goals
/// - The goal type is not a forall
///
/// # Limitations
///
/// Instance-implicit binders are added to the local context, but local instance
/// search metadata is not populated yet.
pub fn intro<'l>(
    metam: &mut MetaMContext<'l>,
    state: TacticState<'l>,
    name: &LeanBound<'l, LeanName>,
) -> TacticResult<'l> {
    let goal = match state.main_goal() {
        Some(g) => g,
        None => return TacticResult::Failure(LeanError::other("intro: no goals")),
    };

    let mvar_id = match LeanExpr::mvar_id(goal) {
        Ok(id) => id,
        Err(e) => return TacticResult::Failure(e),
    };
    let new_goal_mvar = unsafe {
        ffi::lean_inc(mvar_id.as_ptr());
        ffi::lean_inc(name.as_ptr());
        let closure = ffi::inline::lean_alloc_closure(
            ffi::meta::l_Lean_MVarId_intro as *mut std::ffi::c_void,
            7,
            2,
        );
        ffi::inline::lean_closure_set(closure, 0, mvar_id.into_ptr());
        ffi::inline::lean_closure_set(closure, 1, name.clone().into_ptr());
        let computation = LeanBound::from_owned_ptr(metam.lean(), closure);
        let result = match metam.run_persistent(computation) {
            Ok(r) => r,
            Err(e) => return TacticResult::Failure(e),
        };
        // The result pair is owned by `result`; take an owned reference to
        // the fresh metavariable id before the pair is dropped.
        let new_mvar_id_ptr = ffi::lean_ctor_get(result.as_ptr(), 1) as *mut ffi::lean_object;
        ffi::lean_inc(new_mvar_id_ptr);
        let new_mvar_id =
            LeanBound::<crate::instance::LeanAny>::from_owned_ptr(metam.lean(), new_mvar_id_ptr)
                .cast();
        match LeanExpr::mvar(metam.lean(), new_mvar_id) {
            Ok(m) => m,
            Err(e) => return TacticResult::Failure(e),
        }
    };

    let remaining = state.remaining_goals();
    let mut new_goals = vec![new_goal_mvar];
    new_goals.extend(remaining.into_goals());
    TacticResult::Success(TacticState::new(new_goals))
}

// ============================================================================
// Tactic: apply
// ============================================================================

/// Apply a function/lemma to the main goal.
///
/// Infers the type of `expr`, exposes its forall/pi structure via WHNF,
/// creates fresh metavariables for each argument, builds the application,
/// and assigns it to the goal. The fresh metavariables become new goals.
///
/// # Errors
///
/// Returns an error if:
/// - There are no goals
/// - Type inference fails
/// - The result type doesn't match the goal
///
/// # Limitations
///
/// This is a simplified apply that peels off forall binders one at a time.
/// It doesn't synthesize implicit arguments beyond the metavariables it creates.
pub fn apply<'l>(
    metam: &mut MetaMContext<'l>,
    state: TacticState<'l>,
    expr: &LeanBound<'l, LeanExpr>,
) -> TacticResult<'l> {
    let goal = match state.main_goal() {
        Some(g) => g,
        None => return TacticResult::Failure(LeanError::other("apply: no goals")),
    };

    let lean = metam.lean();

    let goal_decl = match metam.get_mvar_decl(goal) {
        Ok(decl) => decl,
        Err(e) => return TacticResult::Failure(e),
    };
    // Infer the type of the expression being applied
    let mut expr_ty =
        match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
            m.infer_type(expr)
        }) {
            Ok(ty) => ty,
            Err(e) => return TacticResult::Failure(e),
        };

    // Peel off forall binders, creating fresh mvars for each argument
    let mut app_expr = expr.clone();
    let mut new_mvars: Vec<LeanBound<'l, LeanExpr>> = Vec::new();
    let mut arg_idx = 0u64;

    loop {
        // WHNF to expose forall structure
        expr_ty = match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
            m.whnf(&expr_ty)
        }) {
            Ok(ty) => ty,
            Err(e) => return TacticResult::Failure(e),
        };

        if !LeanExpr::is_forall(&expr_ty) {
            break;
        }

        let body = match LeanExpr::forall_body(&expr_ty) {
            Ok(b) => b,
            Err(e) => return TacticResult::Failure(e),
        };
        let domain = match LeanExpr::forall_domain(&expr_ty) {
            Ok(d) => d,
            Err(e) => return TacticResult::Failure(e),
        };

        // Create a fresh mvar for this argument
        let mvar_name = match LeanName::from_str(lean, &format!("apply_arg.{arg_idx}")) {
            Ok(n) => n,
            Err(e) => return TacticResult::Failure(e),
        };
        let mvar = match metam.mk_fresh_expr_mvar_at(
            &goal_decl.lctx,
            &goal_decl.local_instances,
            &domain,
            &mvar_name,
        ) {
            Ok(m) => m,
            Err(e) => return TacticResult::Failure(e),
        };

        // Apply the argument: app_expr = app_expr mvar
        app_expr = match LeanExpr::app(&app_expr, &mvar) {
            Ok(a) => a,
            Err(e) => return TacticResult::Failure(e),
        };

        // Substitute the mvar into the body to get the remaining type
        // (instantiate bvar(0) with the mvar)
        expr_ty = match LeanExpr::instantiate1(&body, &mvar) {
            Ok(t) => t,
            Err(e) => return TacticResult::Failure(e),
        };

        new_mvars.push(mvar);
        arg_idx += 1;
    }

    // Assign the application to the goal
    let mvar_id = match LeanExpr::mvar_id(goal) {
        Ok(id) => id,
        Err(e) => return TacticResult::Failure(e),
    };

    match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
        checked_assign(m, &mvar_id, &app_expr)
    }) {
        Ok(true) => {}
        Ok(false) => {
            return TacticResult::Failure(LeanError::other("apply: checked assignment failed"));
        }
        Err(e) => return TacticResult::Failure(e),
    }

    // New goals: the fresh mvars (unassigned arguments) + remaining old goals
    let remaining = state.remaining_goals();
    new_mvars.extend(remaining.into_goals());
    TacticResult::Success(TacticState::new(new_mvars))
}

// ============================================================================
// Tactic: assumption
// ============================================================================

/// Search the local context for a hypothesis whose type matches the goal.
///
/// For each hypothesis in the provided list, checks if its type is
/// definitionally equal to the goal type. If found, assigns the hypothesis
/// (as an fvar expression) to the goal.
///
/// # Arguments
///
/// * `metam` - MetaM context
/// * `state` - Current tactic state
/// * `hypotheses` - List of (fvar_expr, type) pairs representing the local context
///
/// # Errors
///
/// Returns an error if:
/// - There are no goals
/// - No hypothesis matches the goal type
///
/// # Note
///
/// Since we don't yet have full local context iteration via FFI, the caller
/// must provide the list of hypotheses explicitly. A future version could
/// extract these from the MetaM local context automatically.
pub fn assumption<'l>(
    metam: &mut MetaMContext<'l>,
    state: TacticState<'l>,
    hypotheses: &[(&LeanBound<'l, LeanExpr>, &LeanBound<'l, LeanExpr>)],
) -> TacticResult<'l> {
    let goal = match state.main_goal() {
        Some(g) => g,
        None => return TacticResult::Failure(LeanError::other("assumption: no goals")),
    };

    // Get the goal type
    let goal_decl = match metam.get_mvar_decl(goal) {
        Ok(decl) => decl,
        Err(e) => return TacticResult::Failure(e),
    };
    let goal_ty = goal_decl.type_.clone();

    // Search hypotheses for one whose type matches
    for (hyp_expr, hyp_type) in hypotheses {
        match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
            m.is_def_eq(hyp_type, &goal_ty)
        }) {
            Ok(true) => {
                // Found a match — assign the hypothesis to the goal
                let mvar_id = match LeanExpr::mvar_id(goal) {
                    Ok(id) => id,
                    Err(e) => return TacticResult::Failure(e),
                };

                match metam.with_local_context(&goal_decl.lctx, &goal_decl.local_instances, |m| {
                    checked_assign(m, &mvar_id, hyp_expr)
                }) {
                    Ok(true) => return TacticResult::Success(state.remaining_goals()),
                    Ok(false) => {
                        // Assignment failed despite type match — try next hypothesis
                        continue;
                    }
                    Err(_) => continue,
                }
            }
            Ok(false) => continue,
            Err(_) => continue,
        }
    }

    TacticResult::Failure(LeanError::other(
        "assumption: no hypothesis matches the goal type",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tactic_state() {
        let state: TacticState<'_> = TacticState::new(vec![]);
        assert!(state.is_solved());
        assert_eq!(state.num_goals(), 0);
        assert!(state.main_goal().is_none());
    }

    #[test]
    fn test_tactic_state_with_goals() {
        let result: LeanResult<()> = crate::test_with_lean(|lean| {
            use crate::meta::name::LeanName;

            let name1 = LeanName::from_str(lean, "?g.1")?;
            let goal1 = LeanExpr::mvar(lean, name1)?;
            let name2 = LeanName::from_str(lean, "?g.2")?;
            let goal2 = LeanExpr::mvar(lean, name2)?;

            let state = TacticState::new(vec![goal1, goal2]);
            assert!(!state.is_solved());
            assert_eq!(state.num_goals(), 2);
            assert!(state.main_goal().is_some());

            let state = state.remaining_goals();
            assert_eq!(state.num_goals(), 1);

            let state = state.remaining_goals();
            assert!(state.is_solved());

            Ok(())
        });
        assert!(result.is_ok(), "test failed: {:?}", result.err());
    }

    #[test]
    fn test_tactic_result_variants() {
        let success: TacticResult<'_> = TacticResult::Success(TacticState::new(vec![]));
        assert!(matches!(success, TacticResult::Success(_)));

        let failure: TacticResult<'_> = TacticResult::Failure(LeanError::other("tactic failed"));
        assert!(matches!(failure, TacticResult::Failure(_)));
    }

    #[test]
    fn test_remaining_goals_on_empty() {
        let state: TacticState<'_> = TacticState::new(vec![]);
        let state = state.remaining_goals();
        assert!(state.is_solved());
    }
}
