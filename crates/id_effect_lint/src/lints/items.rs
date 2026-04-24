use crate::diag::span_lint_and_help;
use crate::utils::*;
use rustc_hir::ItemKind;
use rustc_lint::LateContext;

use crate::{
  NO_NESTED_RESULT_IN_PUB_API, NO_STATIC_SERVICE_GLOBALS, NO_THREAD_LOCAL_FOR_REQUEST_CONTEXT,
};

// ─── E-02: No static service globals ─────────────────────────────────────────

/// Flags `static` items whose types look like service types.
pub fn check_no_static_service_globals(cx: &LateContext<'_>, item: &rustc_hir::Item<'_>) {
  // ItemKind::Static(mutbl, ident, ty, body) in nightly-2026+
  if let ItemKind::Static(_, _, ty, _) = &item.kind {
    if !is_primitive_ty(ty) {
      span_lint_and_help(
        cx,
        NO_STATIC_SERVICE_GLOBALS,
        item.span,
        "`static` item found; services must not live in global statics",
        None,
        "declare services with `service_key!` + `NeedsX` and access via `~Tag` in `effect!`",
      );
    }
  }
}

// ─── E-03: No thread_local! for request context ───────────────────────────────

#[allow(deprecated)] // TyCtxt::get_attrs: replace with find_attr! / hir attrs when MSRV allows
pub fn check_no_thread_local(cx: &LateContext<'_>, item: &rustc_hir::Item<'_>) {
  if matches!(item.kind, ItemKind::Static(..)) {
    let has_thread_local_attr = cx
      .tcx
      .get_attrs(item.owner_id.to_def_id(), rustc_span::sym::thread_local)
      .next()
      .is_some();
    if has_thread_local_attr {
      span_lint_and_help(
        cx,
        NO_THREAD_LOCAL_FOR_REQUEST_CONTEXT,
        item.span,
        "`thread_local!` storage found in Effect code",
        None,
        "use `FiberRef::new(initial)` for fiber-scoped dynamic state",
      );
    }
  }
}

// ─── F-01: No Result<Result<…>> in public APIs ───────────────────────────────

pub fn check_no_nested_result_type(cx: &LateContext<'_>, item: &rustc_hir::Item<'_>) {
  let is_pub = cx.tcx.visibility(item.owner_id.def_id).is_public();
  if !is_pub {
    return;
  }
  // ItemKind::TyAlias(ident, generics, ty) in nightly-2026+
  if let ItemKind::TyAlias(_, _, ty) = &item.kind {
    if is_nested_result(ty) {
      span_lint_and_help(
        cx,
        NO_NESTED_RESULT_IN_PUB_API,
        item.span,
        "public type alias is `Result<Result<_, _>, _>`",
        None,
        "use `Or<E1, E2>` or a flat `E: From<E1> + From<E2>` channel",
      );
    }
  }
}

/// Also check function return types (called from check_fn in the pass).
pub fn check_no_nested_result_fn<'tcx>(
  cx: &LateContext<'tcx>,
  decl: &'tcx rustc_hir::FnDecl<'tcx>,
  is_pub: bool,
  span: rustc_span::Span,
) {
  if !is_pub {
    return;
  }
  if let rustc_hir::FnRetTy::Return(ty) = &decl.output {
    if is_nested_result(ty) {
      span_lint_and_help(
        cx,
        NO_NESTED_RESULT_IN_PUB_API,
        span,
        "public function returns `Result<Result<_, _>, _>`",
        None,
        "use `Or<E1, E2>` or flatten errors via `E: From<E1> + From<E2>`",
      );
    }
  }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn is_nested_result(ty: &rustc_hir::Ty<'_>) -> bool {
  if !ty_last_segment_is(ty, "Result") {
    return false;
  }
  if let rustc_hir::TyKind::Path(rustc_hir::QPath::Resolved(_, path)) = &ty.kind {
    if let Some(last) = path.segments.last() {
      if let Some(args) = last.args {
        if let Some(rustc_hir::GenericArg::Type(ok_ty)) = args.args.first() {
          return ty_last_segment_is(ok_ty.as_unambig_ty(), "Result");
        }
      }
    }
  }
  false
}

fn is_primitive_ty(ty: &rustc_hir::Ty<'_>) -> bool {
  matches!(
      &ty.kind,
      rustc_hir::TyKind::Path(rustc_hir::QPath::Resolved(_, path))
          if path.segments.last().map_or(false, |s| matches!(
              s.ident.name.as_str(),
              "u8" | "u16" | "u32" | "u64" | "u128" | "usize"
              | "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
              | "f32" | "f64" | "bool" | "char" | "str" | "String"
          ))
  ) || matches!(
    ty.kind,
    rustc_hir::TyKind::Tup(_) | rustc_hir::TyKind::Never
  )
}
