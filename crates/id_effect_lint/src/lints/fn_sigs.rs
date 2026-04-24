use crate::diag::span_lint_and_help;
use crate::utils::*;
use rustc_hir::Generics;
use rustc_hir::intravisit::FnKind;
use rustc_lint::LateContext;
use rustc_span::Span;

use crate::{
  BLOCKING_SUFFIX_RESERVED_FOR_RUNNERS, EFFECT_ERROR_BOUND_MUST_USE_FROM,
  EFFECT_FN_REQUIRES_WHERE_CLAUSE, MULTI_STEP_MUST_USE_EFFECT_MACRO, NO_ASYNC_FN_IN_EFFECT_CRATE,
  NO_CONCRETE_CONTEXT_IN_PUB_API, NO_EFFECT_SUFFIX_ON_GRAPH_BUILDERS, NO_RESULT_RETURN_IN_PUB_FN,
  PREFER_NEEDS_X_OVER_RAW_GET_BOUNDS,
};

// ─── A-01 ─────────────────────────────────────────────────────────────────────

/// Flags `async fn` at the application level.
pub fn check_no_async_fn<'tcx>(
  cx: &LateContext<'tcx>,
  kind: FnKind<'tcx>,
  span: Span,
  def_id: rustc_hir::def_id::LocalDefId,
) {
  if !is_async_fn_kind(&kind) {
    return;
  }
  // Allow async in #[test] / #[tokio::test] functions.
  if in_test_context(cx, def_id) {
    return;
  }
  span_lint_and_help(
    cx,
    NO_ASYNC_FN_IN_EFFECT_CRATE,
    span,
    "`async fn` is forbidden in Effect.rs code",
    None,
    "convert to `fn ... -> Effect<A, E, R>` and wrap async calls with `from_async`",
  );
}

// ─── A-02 ─────────────────────────────────────────────────────────────────────

/// Flags public functions that return `Result<T, E>`.
pub fn check_no_result_return_pub<'tcx>(
  cx: &LateContext<'tcx>,
  kind: FnKind<'tcx>,
  decl: &'tcx rustc_hir::FnDecl<'tcx>,
  is_pub: bool,
  span: Span,
  def_id: rustc_hir::def_id::LocalDefId,
) {
  // Skip non-public and test functions.
  if in_test_context(cx, def_id) || !is_pub {
    return;
  }
  // Allow `fn main` – returning Result<(), _> is fine as a binary exit code.
  if let FnKind::ItemFn(ident, _, _) = kind {
    if ident.name.as_str() == "main" {
      return;
    }
  }
  if !returns_result(decl) {
    return;
  }
  span_lint_and_help(
    cx,
    NO_RESULT_RETURN_IN_PUB_FN,
    span,
    "public functions must not return `Result<T, E>`",
    None,
    "return `Effect<A, E, R>` instead; run the effect from `main` or test harnesses",
  );
}

// ─── A-03 ─────────────────────────────────────────────────────────────────────

/// Flags Effect-returning functions whose generic params lack a `where` clause.
pub fn check_effect_fn_where_clause<'tcx>(
  cx: &LateContext<'tcx>,
  decl: &'tcx rustc_hir::FnDecl<'tcx>,
  generics: &'tcx Generics<'tcx>,
  span: Span,
) {
  if !returns_effect(decl) {
    return;
  }
  let has_generics = generics
    .params
    .iter()
    .any(|p| matches!(p.kind, rustc_hir::GenericParamKind::Type { .. }));
  if has_generics && !has_where_clause(generics) {
    span_lint_and_help(
      cx,
      EFFECT_FN_REQUIRES_WHERE_CLAUSE,
      span,
      "Effect-returning function has generic parameters but no `where` clause",
      None,
      "add `where A: …, E: From<…> + 'static, R: NeedsX + 'static`",
    );
  }
}

// ─── A-04 ─────────────────────────────────────────────────────────────────────

/// Flags Effect-returning functions where `E` lacks a `From<_>` bound.
pub fn check_effect_error_from_bound<'tcx>(
  cx: &LateContext<'tcx>,
  decl: &'tcx rustc_hir::FnDecl<'tcx>,
  generics: &'tcx Generics<'tcx>,
  span: Span,
) {
  if !returns_effect(decl) {
    return;
  }
  if !has_where_clause(generics) {
    return; // A-03 already fires; avoid duplicate noise
  }
  let names = generic_param_names(generics);
  // Look for a parameter named "E" (conventional).
  let Some(e_name) = names.iter().find(|n| n.as_str() == "E").copied() else {
    return;
  };
  if !e_has_from_bound(generics, e_name) {
    span_lint_and_help(
      cx,
      EFFECT_ERROR_BOUND_MUST_USE_FROM,
      span,
      "`E` parameter in Effect-returning function has no `E: From<_>` bound",
      None,
      "add `E: From<SpecificError> + 'static` to the `where` clause for each service used",
    );
  }
}

// ─── A-05 ─────────────────────────────────────────────────────────────────────

/// Flags public functions whose return type contains a concrete `Context<Cons<…>>`.
pub fn check_no_concrete_context<'tcx>(
  cx: &LateContext<'tcx>,
  decl: &'tcx rustc_hir::FnDecl<'tcx>,
  is_pub: bool,
  span: Span,
) {
  if !is_pub {
    return;
  }
  if let rustc_hir::FnRetTy::Return(ty) = &decl.output {
    if ty_last_segment_is(ty, "Effect") {
      // Walk generic args of Effect to find R type.
      if let rustc_hir::TyKind::Path(rustc_hir::QPath::Resolved(_, path)) = &ty.kind {
        if let Some(last) = path.segments.last() {
          if let Some(args) = last.args {
            // Third generic arg is R.
            if let Some(rustc_hir::GenericArg::Type(r_ty)) = args.args.get(2) {
              if ty_last_segment_is(r_ty.as_unambig_ty(), "Context")
                || ty_last_segment_is(r_ty.as_unambig_ty(), "Cons")
              {
                span_lint_and_help(
                  cx,
                  NO_CONCRETE_CONTEXT_IN_PUB_API,
                  span,
                  "public API exposes concrete `Context<Cons<…>>` as `R` type",
                  None,
                  "use `impl NeedsX` or a generic `R: NeedsX + 'static` bound",
                );
              }
            }
          }
        }
      }
    }
  }
}

// ─── A-06 ─────────────────────────────────────────────────────────────────────

/// Flags raw `Get<Key, Here, Target = T>` bounds; prefer `NeedsX` supertraits.
pub fn check_prefer_needs_x<'tcx>(
  cx: &LateContext<'tcx>,
  decl: &'tcx rustc_hir::FnDecl<'tcx>,
  generics: &'tcx Generics<'tcx>,
  span: Span,
) {
  if !returns_effect(decl) {
    return;
  }
  if has_raw_get_bound(generics) {
    span_lint_and_help(
      cx,
      PREFER_NEEDS_X_OVER_RAW_GET_BOUNDS,
      span,
      "Effect-returning function uses raw `Get<Key, …>` bounds",
      None,
      "define a `NeedsX: Get<XKey>` supertrait and use `R: NeedsX + 'static` instead",
    );
  }
}

// ─── A-07 ─────────────────────────────────────────────────────────────────────

/// Flags Effect-returning functions with a `_effect` suffix in their name.
pub fn check_no_effect_suffix<'tcx>(
  cx: &LateContext<'tcx>,
  kind: FnKind<'tcx>,
  decl: &'tcx rustc_hir::FnDecl<'tcx>,
  _span: Span,
) {
  if !returns_effect(decl) {
    return;
  }
  if let FnKind::ItemFn(ident, _, _) | FnKind::Method(ident, _) = kind {
    if ident.name.as_str().ends_with("_effect") {
      span_lint_and_help(
        cx,
        NO_EFFECT_SUFFIX_ON_GRAPH_BUILDERS,
        ident.span,
        "Effect-returning function name ends with `_effect`",
        None,
        "remove the `_effect` suffix; the return type already communicates effectfulness",
      );
    }
  }
}

// ─── A-08 ─────────────────────────────────────────────────────────────────────

/// Flags functions named `*_blocking` whose return type is `Effect<…>` (misleading).
pub fn check_blocking_suffix_misuse<'tcx>(
  cx: &LateContext<'tcx>,
  kind: FnKind<'tcx>,
  decl: &'tcx rustc_hir::FnDecl<'tcx>,
  _span: Span,
) {
  if !returns_effect(decl) {
    return;
  }
  if let FnKind::ItemFn(ident, _, _) | FnKind::Method(ident, _) = kind {
    if ident.name.as_str().ends_with("_blocking") {
      span_lint_and_help(
        cx,
        BLOCKING_SUFFIX_RESERVED_FOR_RUNNERS,
        ident.span,
        "function named `*_blocking` returns `Effect`; the name implies it runs the effect",
        None,
        "rename, or call `run_blocking(…)` inside and return `Result<_, _>`",
      );
    }
  }
}

// ─── B-01 ─────────────────────────────────────────────────────────────────────

/// Flags chained `.flat_map(…).flat_map(…)` chains (≥2) that should use `effect!`.
pub fn check_multi_step_flat_map(cx: &LateContext<'_>, expr: &rustc_hir::Expr<'_>) {
  use rustc_hir::ExprKind;
  if let ExprKind::MethodCall(seg, recv, _, _) = &expr.kind {
    if seg.ident.name.as_str() != "flat_map" {
      return;
    }
    // Check if the receiver is also a flat_map call.
    if let ExprKind::MethodCall(inner_seg, _, _, _) = &recv.kind {
      if inner_seg.ident.name.as_str() == "flat_map" {
        span_lint_and_help(
          cx,
          MULTI_STEP_MUST_USE_EFFECT_MACRO,
          expr.span,
          "chained `.flat_map(…).flat_map(…)` found",
          None,
          "use `effect! { let a = ~ step_a(); let b = ~ step_b(a); … }` instead",
        );
      }
    }
  }
}
