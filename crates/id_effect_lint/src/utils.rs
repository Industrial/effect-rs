use rustc_hir::intravisit::FnKind;
use rustc_hir::{FnDecl, FnRetTy, IsAsync, QPath, TyKind, WherePredicateKind};
use rustc_lint::LateContext;
use rustc_span::Symbol;

// ──────────────────────────────────────────────────────────
// Return-type helpers
// ──────────────────────────────────────────────────────────

/// `true` iff the HIR type's outermost named segment is `name`.
pub fn ty_last_segment_is(ty: &rustc_hir::Ty<'_>, name: &str) -> bool {
  match &ty.kind {
    TyKind::Path(QPath::Resolved(_, path)) => path
      .segments
      .last()
      .map_or(false, |s| s.ident.name.as_str() == name),
    TyKind::Path(QPath::TypeRelative(_, seg)) => seg.ident.name.as_str() == name,
    _ => false,
  }
}

pub fn returns_effect(decl: &FnDecl<'_>) -> bool {
  matches!(&decl.output, FnRetTy::Return(ty) if ty_last_segment_is(ty, "Effect"))
}

pub fn returns_result(decl: &FnDecl<'_>) -> bool {
  matches!(&decl.output, FnRetTy::Return(ty) if ty_last_segment_is(ty, "Result"))
}

// ──────────────────────────────────────────────────────────
// Async / coroutine helpers
// ──────────────────────────────────────────────────────────

pub fn fn_header_is_async(header: &rustc_hir::FnHeader) -> bool {
  matches!(header.asyncness, IsAsync::Async(_))
}

pub fn is_async_fn_kind(kind: &FnKind<'_>) -> bool {
  matches!(kind, FnKind::ItemFn(_, _, h) if fn_header_is_async(h))
}

// ──────────────────────────────────────────────────────────
// Test-context helpers
// ──────────────────────────────────────────────────────────

#[allow(deprecated)] // TyCtxt::get_attrs: replace with find_attr! / hir attrs when MSRV allows
pub fn in_test_context(cx: &LateContext<'_>, def_id: rustc_hir::def_id::LocalDefId) -> bool {
  use rustc_span::sym;
  let mut hir_id = cx.tcx.local_def_id_to_hir_id(def_id);
  loop {
    if cx
      .tcx
      .get_attrs(hir_id.owner.def_id.to_def_id(), sym::test)
      .next()
      .is_some()
    {
      return true;
    }
    match cx.tcx.hir_parent_id_iter(hir_id).next() {
      Some(parent) => hir_id = parent,
      None => break,
    }
  }
  false
}

// ──────────────────────────────────────────────────────────
// Where-clause helpers
// ──────────────────────────────────────────────────────────

pub fn has_where_clause(generics: &rustc_hir::Generics<'_>) -> bool {
  !generics.predicates.is_empty()
}

pub fn generic_param_names(generics: &rustc_hir::Generics<'_>) -> Vec<Symbol> {
  generics
    .params
    .iter()
    .filter_map(|p| match p.kind {
      rustc_hir::GenericParamKind::Type { .. } => Some(p.name.ident().name),
      _ => None,
    })
    .collect()
}

/// `true` iff the where clause contains an `E: From<_>` bound for `e_name`.
pub fn e_has_from_bound(generics: &rustc_hir::Generics<'_>, e_name: Symbol) -> bool {
  use rustc_hir::GenericBound;
  for pred in generics.predicates {
    if let WherePredicateKind::BoundPredicate(bp) = pred.kind {
      if ty_last_segment_is(bp.bounded_ty, e_name.as_str()) {
        for bound in bp.bounds {
          if let GenericBound::Trait(tref) = bound {
            if tref
              .trait_ref
              .path
              .segments
              .last()
              .map_or(false, |s| s.ident.name.as_str() == "From")
            {
              return true;
            }
          }
        }
      }
    }
  }
  false
}

/// `true` iff any where predicate uses a raw `Get<…>` bound.
pub fn has_raw_get_bound(generics: &rustc_hir::Generics<'_>) -> bool {
  use rustc_hir::GenericBound;
  for pred in generics.predicates {
    if let WherePredicateKind::BoundPredicate(bp) = pred.kind {
      for bound in bp.bounds {
        if let GenericBound::Trait(tref) = bound {
          if tref
            .trait_ref
            .path
            .segments
            .last()
            .map_or(false, |s| s.ident.name.as_str() == "Get")
          {
            return true;
          }
        }
      }
    }
  }
  false
}

// ──────────────────────────────────────────────────────────
// Expression / macro helpers
// ──────────────────────────────────────────────────────────

/// `true` iff the expression is a macro call whose name contains `name`.
pub fn is_macro_call_named(expr: &rustc_hir::Expr<'_>, name: &str) -> bool {
  if expr.span.from_expansion() {
    let data = expr.span.ctxt().outer_expn_data();
    if let rustc_span::hygiene::ExpnKind::Macro(_, sym) = data.kind {
      return sym.as_str().contains(name);
    }
  }
  false
}

pub fn contains_rc_refcell(ty: &rustc_hir::Ty<'_>) -> bool {
  if !ty_last_segment_is(ty, "Rc") {
    return false;
  }
  if let TyKind::Path(QPath::Resolved(_, path)) = &ty.kind {
    if let Some(last) = path.segments.last() {
      if let Some(args) = last.args {
        for arg in args.args {
          if let rustc_hir::GenericArg::Type(inner) = arg {
            if ty_last_segment_is(inner.as_unambig_ty(), "RefCell") {
              return true;
            }
          }
        }
      }
    }
  }
  false
}
