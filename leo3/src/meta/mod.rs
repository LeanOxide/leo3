//! Meta-programming module for Leo3
//!
//! This module provides high-level Rust APIs for Lean's meta-programming capabilities,
//! enabling programmatic manipulation of Lean environments and expressions.
//!
//! # Core Types
//!
//! - [`LeanEnvironment`] - Immutable collection of declarations (axioms, definitions, theorems)
//! - [`LeanExpr`] - Lean expressions (the core term language)
//! - [`LeanDeclaration`] - Declaration builders
//! - [`MetaMContext`] - MetaM monad runner for type inference, checking, and proof validation
//! - [`LeanLevel`] - Universe levels
//! - [`LeanLiteral`] - Literal values (Nat, String)
//!
//! # Use Cases
//!
//! - **Code generation**: Build Lean code programmatically from Rust
//! - **DSL compilation**: Translate domain-specific languages to Lean
//! - **Proof automation**: Generate proof terms programmatically
//! - **Static analysis**: Inspect and analyze Lean code
//! - **IDE support**: Power language servers and refactoring tools
//!
//! # Proof Construction
//!
//! Leo3 supports building and validating proof terms from Rust. The typical workflow is:
//!
//! 1. Build proof terms using [`LeanExpr`] constructors
//! 2. Validate proofs using [`MetaMContext`] (type inference, type checking)
//! 3. Add validated proofs to the environment as theorems via [`LeanEnvironment::add_decl`]
//!
//! ## Equality Proof Helpers
//!
//! [`LeanExpr`] provides helpers for common equality proof patterns:
//!
//! - [`LeanExpr::mk_eq`] — Build `@Eq α lhs rhs` (equality proposition)
//! - [`LeanExpr::mk_eq_refl`] — Build `@Eq.refl α a` (reflexivity proof)
//! - [`LeanExpr::mk_eq_symm`] — Build `@Eq.symm α a b h` (symmetry proof)
//! - [`LeanExpr::mk_eq_trans`] — Build `@Eq.trans α a b c h1 h2` (transitivity proof)
//!
//! These are pure expression constructors (AST only). For type checking to succeed,
//! `Eq`/`Eq.refl`/`Eq.symm`/`Eq.trans` must exist in the environment.
//!
//! ## Proof Validation
//!
//! [`MetaMContext`] provides proof-oriented utilities:
//!
//! - [`MetaMContext::get_proof_type`] — Infer the proposition a proof proves
//! - [`MetaMContext::is_proof_of`] — Check if a proof proves a given proposition
//! - [`MetaMContext::is_prop`] — Check if an expression is a proposition (type is `Prop`)
//! - [`MetaMContext::check`] — Verify an expression is well-typed
//! - [`MetaMContext::infer_type`] — Infer the type of any expression
//!
//! ## Example: Proving ∀ P : Prop, P → P
//!
//! ```ignore
//! use leo3::prelude::*;
//! use leo3::meta::*;
//!
//! leo3::with_lean(|lean| {
//!     let env = LeanEnvironment::empty(lean, 0)?;
//!
//!     // Type: ∀ (P : Prop), ∀ (h : P), P
//!     let p_name = LeanName::from_str(lean, "P")?;
//!     let h_name = LeanName::from_str(lean, "h")?;
//!     let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
//!     let bvar0 = LeanExpr::bvar(lean, 0)?;
//!     let bvar1 = LeanExpr::bvar(lean, 1)?;
//!     let inner = LeanExpr::forall(h_name.clone(), bvar0, bvar1, BinderInfo::Default)?;
//!     let thm_type = LeanExpr::forall(p_name.clone(), prop.clone(), inner, BinderInfo::Default)?;
//!
//!     // Proof: λ (P : Prop) (h : P), h
//!     let prop2 = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
//!     let bvar0a = LeanExpr::bvar(lean, 0)?;
//!     let bvar0b = LeanExpr::bvar(lean, 0)?;
//!     let inner_lam = LeanExpr::lambda(h_name, bvar0a, bvar0b, BinderInfo::Default)?;
//!     let proof = LeanExpr::lambda(p_name, prop2, inner_lam, BinderInfo::Default)?;
//!
//!     // Validate and add to environment
//!     let mut ctx = MetaMContext::new(lean, env.clone())?;
//!     ctx.check(&proof)?;
//!     assert!(ctx.is_proof_of(&proof, &thm_type)?);
//!
//!     let name = LeanName::from_str(lean, "my_id")?;
//!     let decl = LeanDeclaration::theorem(lean, name, LeanList::nil(lean)?, thm_type, proof)?;
//!     let _new_env = LeanEnvironment::add_decl(&env, &decl)?;
//!     Ok(())
//! })?;
//! ```
//!
//! # Expression Construction Example
//!
//! ```ignore
//! use leo3::prelude::*;
//! use leo3::meta::*;
//!
//! leo3::with_lean(|lean| {
//!     let env = LeanEnvironment::empty(lean, 0)?;
//!
//!     // Build a simple expression: λ x : Nat, x
//!     let x_name = LeanName::from_str(lean, "x")?;
//!     let nat_const = LeanExpr::const_(
//!         lean,
//!         LeanName::from_str(lean, "Nat")?,
//!         LeanList::nil(lean)?
//!     )?;
//!     let body = LeanExpr::bvar(lean, 0)?;
//!     let lambda = LeanExpr::lambda(
//!         x_name,
//!         nat_const,
//!         body,
//!         BinderInfo::Default
//!     )?;
//!
//!     println!("Created lambda: {:?}", LeanExpr::dbg_to_string(&lambda)?);
//!     Ok(())
//! })?;
//! ```

pub mod context;
pub mod declaration;
pub mod environment;
pub mod expr;
#[cfg(all(lean_4_25, target_os = "linux"))]
pub mod keepalive;
pub mod level;
pub mod literal;
pub mod metam;
pub mod name;
#[cfg(lean_4_25)]
pub mod repl;
pub mod tactic;

// Re-export main types
pub use context::{CoreContext, CoreState, MetaContext, MetaState};
pub use declaration::{DefinitionSafety, LeanDeclaration};
pub use environment::{ConstantKind, LeanConstantInfo, LeanEnvironment};
pub use expr::{BinderInfo, ExprKind, LeanExpr};
pub use level::LeanLevel;
pub use literal::LeanLiteral;
pub use metam::MetaMContext;
pub use name::{LeanName, NameKind};
pub use tactic::{apply, assumption, exact, goal_type, intro, rfl, TacticResult, TacticState};

#[cfg(all(lean_4_25, target_os = "linux"))]
pub use keepalive::{
    diff_freed_vmras, record_freed_set, remap_cross_set_bases, snapshot_lean_vmras, LeanVma,
    RemapError,
};
#[cfg(lean_4_25)]
pub use repl::{
    default_term_context, default_term_state, ensure_search_path, import_modules,
    import_modules_with_exts, init_search_path, parse_tactic, parse_term, run_tactic,
    RunTacticOutcome,
};
