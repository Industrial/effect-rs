/// Thin wrappers around `LintContext::emit_span_lint` so the rest of the crate
/// doesn't need to import `rustc_errors` directly.
use rustc_errors::{Diag, DiagDecorator, DiagMessage};
use rustc_lint::{LateContext, Lint, LintContext};
use rustc_span::Span;

/// Emit a lint at `span` with an optional help note.
pub fn span_lint_and_help(
  cx: &LateContext<'_>,
  lint: &'static Lint,
  span: Span,
  msg: impl Into<DiagMessage>,
  _help_span: Option<Span>,
  help: impl Into<DiagMessage>,
) {
  let msg = msg.into();
  let help = help.into();
  cx.emit_span_lint(
    lint,
    span,
    DiagDecorator(move |diag: &mut Diag<'_, ()>| {
      diag.primary_message(msg.clone());
      diag.help(help.clone());
    }),
  );
}

/// Emit a plain lint at `span` (available for lints that do not need a help line).
#[allow(dead_code)]
pub fn span_lint(
  cx: &LateContext<'_>,
  lint: &'static Lint,
  span: Span,
  msg: impl Into<DiagMessage>,
) {
  let msg = msg.into();
  cx.emit_span_lint(
    lint,
    span,
    DiagDecorator(move |diag: &mut Diag<'_, ()>| {
      diag.primary_message(msg.clone());
    }),
  );
}
