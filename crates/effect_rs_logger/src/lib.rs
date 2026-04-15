//! Injectable [`EffectLogger`] service for the effect system.
//!
//! # Service/Tag pattern
//!
//! Extract the logger from the environment once with `~EffectLogger`, then call
//! its methods as regular effectful steps:
//!
//! ```ignore
//! effect!(|_r: &mut R| {
//!     let logger = ~EffectLogger;
//!     ~logger.warn("something suspicious");
//!     ~logger.info("all good");
//!     result
//! })
//! ```
//!
//! The environment `R` only needs to satisfy
//! `R: Get<EffectLogKey, Here, Target = EffectLogger>`.  The caller composes
//! layers at the top of the program. For a minimal stack that only provides
//! [`EffectLogger`], build `Context::new(Cons(layer_effect_logger().build().expect(\"…\"), Nil))`
//! or `Context::new(Cons(Service::<EffectLogKey, _>::new(EffectLogger), Nil))` at the program edge.
//!
//! Log methods accept `impl Into<Cow<'static, str>>`: literals stay zero-copy;
//! runtime text passes as `String` or `format!(...)`.

#![deny(missing_docs)]

use core::convert::Infallible;
use std::borrow::Cow;
use std::cell::RefCell;
use std::future::ready;
use std::sync::Arc;

use ::effect_rs::{BoxFuture, Effect, EffectHashMap, FiberRef, Get, Here, IntoBind, box_future};

mod pipeline;

pub use pipeline::{
  CompositeLogBackend, JsonLogBackend, LogBackend, LogRecord, Logger, StructuredLogBackend,
  TracingLogBackend,
};

use pipeline::TracingLogBackend as TracingSink;

/// Supertrait alias for `Get<EffectLogKey, Here, Target = EffectLogger>`.
///
/// Use `R: NeedsEffectLogger` in `where` clauses instead of the full bound:
///
/// ```ignore
/// fn my_fn<R: NeedsEffectLogger + 'static>(...) -> Effect<..., R> { ... }
/// ```
pub trait NeedsEffectLogger: Get<EffectLogKey, Here, Target = EffectLogger> {}
impl<R: Get<EffectLogKey, Here, Target = EffectLogger>> NeedsEffectLogger for R {}

effect_rs::service_key!(
  /// Tag for [`EffectLogger`] in an [`effect_rs::Context`] stack.
  pub struct EffectLogKey
);

effect_rs::service_key!(
  /// Tag for the fiber-local minimum [`LogLevel`] used by [`EffectLogger::log`].
  pub struct EffectLogMinLevelKey
);

thread_local! {
  static MIN_LOG_LEVEL_FIBER_REF: RefCell<Option<FiberRef<LogLevel>>> = const { RefCell::new(None) };
}

thread_local! {
  static COMPOSITE_LOG_BACKEND: RefCell<Option<Arc<CompositeLogBackend>>> = const { RefCell::new(None) };
}

thread_local! {
  static LOG_ANNOTATIONS_FIBER_REF: RefCell<Option<FiberRef<EffectHashMap<String, String>>>> =
    const { RefCell::new(None) };
}

thread_local! {
  static LOG_SPAN_STACK_FIBER_REF: RefCell<Option<FiberRef<Vec<String>>>> = const { RefCell::new(None) };
}

fn install_min_log_level_fiber_ref(fr: FiberRef<LogLevel>) {
  MIN_LOG_LEVEL_FIBER_REF.with(|c| {
    *c.borrow_mut() = Some(fr);
  });
}

fn install_composite_log_backend(c: Arc<CompositeLogBackend>) {
  COMPOSITE_LOG_BACKEND.with(|cell| {
    *cell.borrow_mut() = Some(c);
  });
}

fn install_log_annotations_fiber_ref(fr: FiberRef<EffectHashMap<String, String>>) {
  LOG_ANNOTATIONS_FIBER_REF.with(|c| {
    *c.borrow_mut() = Some(fr);
  });
}

fn install_log_spans_fiber_ref(fr: FiberRef<Vec<String>>) {
  LOG_SPAN_STACK_FIBER_REF.with(|c| {
    *c.borrow_mut() = Some(fr);
  });
}

#[cfg(test)]
fn test_clear_min_log_level_fiber_ref() {
  MIN_LOG_LEVEL_FIBER_REF.with(|c| {
    *c.borrow_mut() = None;
  });
}

#[cfg(test)]
fn test_clear_composite_log_backend() {
  COMPOSITE_LOG_BACKEND.with(|c| {
    *c.borrow_mut() = None;
  });
}

#[cfg(test)]
fn test_clear_log_metadata_fiber_refs() {
  LOG_ANNOTATIONS_FIBER_REF.with(|c| *c.borrow_mut() = None);
  LOG_SPAN_STACK_FIBER_REF.with(|c| *c.borrow_mut() = None);
}

#[cfg(test)]
fn test_clear_all_logger_tls() {
  test_clear_min_log_level_fiber_ref();
  test_clear_composite_log_backend();
  test_clear_log_metadata_fiber_refs();
}

/// Log sink for use as [`effect_rs::Service<EffectLogKey, Self>`](effect_rs::Service); forwards to [`tracing`].
///
/// Extracted from the environment with `~EffectLogger` inside [`effect_rs::effect!`].
/// After extraction its methods return `Effect<(), EffectLoggerError, R>` and
/// are themselves awaited with `~`.
#[derive(Clone, Copy, Debug, Default)]
pub struct EffectLogger;

/// Errors that a log sink may produce.
///
/// Currently the only backend is [`tracing`], which is infallible, so no
/// variant is ever constructed at runtime.  The type exists so that callers
/// can compose it into their `E` bound and gain compile-time proof that the
/// logger's error channel is handled, without changing the API when a
/// fallible backend (e.g. a network sink) is added later.
#[derive(Debug, Clone, ::effect_rs::EffectData)]
pub enum EffectLoggerError {
  /// The underlying log sink returned an error.
  Sink(String),
}

impl std::fmt::Display for EffectLoggerError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      EffectLoggerError::Sink(msg) => write!(f, "log sink error: {msg}"),
    }
  }
}

impl std::error::Error for EffectLoggerError {}

impl From<Infallible> for EffectLoggerError {
  fn from(e: Infallible) -> Self {
    match e {}
  }
}

/// Metadata attached to log lines (wall-clock UTC, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogContext {
  /// Wall-clock instant in UTC when the log context was created or captured.
  pub timestamp: ::effect_rs::UtcDateTime,
}

impl LogContext {
  /// Build a context with an explicit UTC timestamp.
  #[inline]
  pub const fn new(timestamp: ::effect_rs::UtcDateTime) -> Self {
    Self { timestamp }
  }

  /// Capture the current system time as UTC.
  #[inline]
  pub fn with_now_timestamp() -> Self {
    Self {
      timestamp: ::effect_rs::UtcDateTime::from_std(std::time::SystemTime::now())
        .expect("system time should be representable as UtcDateTime"),
    }
  }
}

/// Logging level for [`EffectLogger`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LogLevel {
  /// Most verbose; diagnostic detail.
  Trace = 0,
  /// Development diagnostics.
  Debug = 1,
  /// Normal operational messages.
  Info = 2,
  /// Something unexpected but recoverable.
  Warn = 3,
  /// Failure or serious problem.
  Error = 4,
  /// Highest severity; mapped to `tracing::error!` with a distinct target in composite pipelines.
  Fatal = 5,
  /// Use as a **minimum** level only: no messages pass the filter. Do not emit log lines at this level.
  None = 255,
}

impl LogLevel {
  /// Numeric severity (higher = more severe). Used for minimum-level filtering.
  #[inline]
  pub const fn severity(self) -> u8 {
    match self {
      LogLevel::None => 255,
      _ => self as u8,
    }
  }

  /// Whether a log at `message_level` should be emitted when `self` is the configured minimum.
  #[inline]
  pub const fn allows(self, message_level: LogLevel) -> bool {
    match (self, message_level) {
      (LogLevel::None, _) | (_, LogLevel::None) => false,
      _ => message_level.severity() >= self.severity(),
    }
  }

  /// Stable uppercase name for structured / JSON backends.
  #[inline]
  pub const fn as_str(self) -> &'static str {
    match self {
      LogLevel::Trace => "TRACE",
      LogLevel::Debug => "DEBUG",
      LogLevel::Info => "INFO",
      LogLevel::Warn => "WARN",
      LogLevel::Error => "ERROR",
      LogLevel::Fatal => "FATAL",
      LogLevel::None => "NONE",
    }
  }
}

impl std::str::FromStr for LogLevel {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.trim().to_ascii_lowercase().as_str() {
      "trace" => Ok(LogLevel::Trace),
      "debug" => Ok(LogLevel::Debug),
      "info" => Ok(LogLevel::Info),
      "warn" | "warning" => Ok(LogLevel::Warn),
      "error" => Ok(LogLevel::Error),
      "fatal" => Ok(LogLevel::Fatal),
      "none" => Ok(LogLevel::None),
      other => Err(format!("unknown log level: {other:?}")),
    }
  }
}

impl EffectLogger {
  /// Run `inner` with this fiber’s minimum log level overridden to `level` ([`effect_rs::FiberRef::locally`]).
  pub fn with_minimum_log_level<B, E, R>(
    fiber_ref: FiberRef<LogLevel>,
    level: LogLevel,
    inner: Effect<B, E, R>,
  ) -> Effect<B, E, R>
  where
    B: 'static,
    E: 'static,
    R: 'static,
  {
    fiber_ref.locally(level, inner)
  }

  /// Emit a log line at `level`.  Returns an effect that, when run, forwards
  /// to [`tracing`].  The environment `R` is ignored — the logger is
  /// self-contained after extraction.
  ///
  /// When [`layer_minimum_log_level`] has been built on this thread, messages below the current
  /// fiber’s minimum [`LogLevel`] (from that [`FiberRef`]) are dropped without calling [`tracing`].
  ///
  /// `msg` may be a `&'static str`, `String`, or other `Into<Cow<'static, str>>`.
  pub fn log<R: 'static>(
    &self,
    level: LogLevel,
    msg: impl Into<Cow<'static, str>>,
  ) -> Effect<(), EffectLoggerError, R> {
    let msg = msg.into();
    if level == LogLevel::None {
      return Effect::new(|_r: &mut R| Ok(()));
    }
    Effect::new(move |_r: &mut R| {
      let emit = MIN_LOG_LEVEL_FIBER_REF.with(|c| match c.borrow().as_ref() {
        None => true,
        Some(fr) => ::effect_rs::run_blocking(fr.get(), ())
          .map(|min| min.allows(level))
          .unwrap_or(true),
      });
      if !emit {
        return Ok(());
      }

      let annotations = LOG_ANNOTATIONS_FIBER_REF
        .with(|c| {
          c.borrow()
            .as_ref()
            .and_then(|fr| ::effect_rs::run_blocking(fr.get(), ()).ok())
        })
        .unwrap_or_default();

      let spans = LOG_SPAN_STACK_FIBER_REF
        .with(|c| {
          c.borrow()
            .as_ref()
            .and_then(|fr| ::effect_rs::run_blocking(fr.get(), ()).ok())
        })
        .unwrap_or_default();

      let rec = LogRecord {
        level,
        message: msg.clone(),
        annotations,
        spans,
      };

      COMPOSITE_LOG_BACKEND.with(|c| {
        if let Some(comp) = c.borrow().as_ref() {
          comp.emit_all(&rec)?;
        } else {
          LogBackend::emit(&TracingSink, &rec)?;
        }
        Ok::<(), EffectLoggerError>(())
      })?;
      Ok(())
    })
  }

  /// Same as [`Self::log`]; kept for call sites that already hold a [`String`].
  #[inline]
  pub fn log_string<R: 'static>(
    &self,
    level: LogLevel,
    msg: String,
  ) -> Effect<(), EffectLoggerError, R> {
    self.log(level, msg)
  }

  /// Shorthand for [`Self::log`] at [`LogLevel::Trace`].
  pub fn trace<R: 'static>(
    &self,
    msg: impl Into<Cow<'static, str>>,
  ) -> Effect<(), EffectLoggerError, R> {
    self.log(LogLevel::Trace, msg)
  }

  /// Shorthand for [`Self::log`] at [`LogLevel::Debug`].
  pub fn debug<R: 'static>(
    &self,
    msg: impl Into<Cow<'static, str>>,
  ) -> Effect<(), EffectLoggerError, R> {
    self.log(LogLevel::Debug, msg)
  }

  /// Shorthand for [`Self::log`] at [`LogLevel::Info`].
  pub fn info<R: 'static>(
    &self,
    msg: impl Into<Cow<'static, str>>,
  ) -> Effect<(), EffectLoggerError, R> {
    self.log(LogLevel::Info, msg)
  }

  /// Shorthand for [`Self::log`] at [`LogLevel::Warn`].
  pub fn warn<R: 'static>(
    &self,
    msg: impl Into<Cow<'static, str>>,
  ) -> Effect<(), EffectLoggerError, R> {
    self.log(LogLevel::Warn, msg)
  }

  /// Shorthand for [`Self::log`] at [`LogLevel::Error`].
  pub fn error<R: 'static>(
    &self,
    msg: impl Into<Cow<'static, str>>,
  ) -> Effect<(), EffectLoggerError, R> {
    self.log(LogLevel::Error, msg)
  }

  /// Shorthand for [`Self::log`] at [`LogLevel::Fatal`].
  pub fn fatal<R: 'static>(
    &self,
    msg: impl Into<Cow<'static, str>>,
  ) -> Effect<(), EffectLoggerError, R> {
    self.log(LogLevel::Fatal, msg)
  }
}

// ---------------------------------------------------------------------------
// Service extraction: `~EffectLogger` inside `effect!`
// ---------------------------------------------------------------------------

/// Implementing [`IntoBind`] for [`EffectLogger`] makes `~EffectLogger` valid
/// inside any `effect!` whose environment `R` holds an `EffectLogger` under
/// [`EffectLogKey`].  The zero-sized struct acts as its own "request token":
/// passing it to `~` copies the concrete value out of `R` and binds it as a
/// local variable.
impl<'a, R> IntoBind<'a, R, EffectLogger, EffectLoggerError> for EffectLogger
where
  R: Get<EffectLogKey, Here, Target = EffectLogger> + 'a,
{
  fn into_bind(self, r: &'a mut R) -> BoxFuture<'a, Result<EffectLogger, EffectLoggerError>> {
    Box::pin(ready(Ok(*Get::<EffectLogKey, Here>::get(r))))
  }
}

/// [`effect_rs::layer_service`] constructor for [`EffectLogger`].
#[inline]
pub fn layer_effect_logger() -> effect_rs::layer::LayerFn<
  impl Fn() -> Result<effect_rs::Service<EffectLogKey, EffectLogger>, Infallible>,
> {
  effect_rs::layer_service(EffectLogger)
}

/// Layer that allocates a [`FiberRef`]`<`[`LogLevel`]`>` (default `initial`) and registers it in a
/// thread-local slot consulted by [`EffectLogger::log`].
#[inline]
pub fn layer_minimum_log_level(
  initial: LogLevel,
) -> effect_rs::layer::LayerEffect<
  effect_rs::Service<EffectLogMinLevelKey, FiberRef<LogLevel>>,
  (),
  (),
> {
  effect_rs::layer::effect(FiberRef::make(move || initial).flat_map(|fr| {
    Effect::new(move |_r: &mut ()| {
      install_min_log_level_fiber_ref(fr.clone());
      Ok(effect_rs::service::<EffectLogMinLevelKey, _>(fr))
    })
  }))
}

/// Layer: install a [`CompositeLogBackend`] on this thread so [`EffectLogger::log`] fans out to all
/// registered [`LogBackend`]s (see [`Logger::add`], [`Logger::replace`], [`Logger::remove`]).
#[inline]
pub fn layer_composite_logger(
  composite: Arc<CompositeLogBackend>,
) -> effect_rs::layer::LayerEffect<(), (), ()> {
  let c = composite.clone();
  effect_rs::layer::effect(Effect::new(move |_r: &mut ()| {
    install_composite_log_backend(c.clone());
    Ok(())
  }))
}

/// Layer: allocate fiber-local annotation and span-stack [`FiberRef`]s used by [`annotate_logs`] and
/// [`with_log_span`].
#[inline]
pub fn layer_log_metadata() -> effect_rs::layer::LayerEffect<(), (), ()> {
  use ::effect_rs::collections::hash_map;
  let eff = FiberRef::make_with(
    hash_map::empty::<String, String>,
    |m| m.clone(),
    |p, _c| p.clone(),
  )
  .flat_map(|ann| {
    FiberRef::make_with(Vec::<String>::new, |v| v.clone(), |p, _c| p.clone()).flat_map(move |sp| {
      Effect::new(move |_r: &mut ()| {
        install_log_annotations_fiber_ref(ann.clone());
        install_log_spans_fiber_ref(sp.clone());
        Ok(())
      })
    })
  });
  effect_rs::layer::effect(eff)
}

/// Run `inner` with `key=value` merged into the fiber-local annotation map (restored afterward).
pub fn annotate_logs<A, E, R>(
  key: impl Into<String> + Send + 'static,
  value: impl Into<String> + Send + 'static,
  inner: Effect<A, E, R>,
) -> Effect<A, E, R>
where
  A: Send + 'static,
  E: Send + 'static,
  R: Send + 'static,
{
  let key = key.into();
  let value = value.into();
  Effect::new_async(move |r| {
    let tls = LOG_ANNOTATIONS_FIBER_REF.with(|c| c.borrow().clone());
    let Some(fr) = tls else {
      return box_future(async move { inner.run(r).await });
    };
    let cur = match ::effect_rs::run_blocking(fr.get(), ()) {
      Ok(m) => m,
      Err(_) => return box_future(async move { inner.run(r).await }),
    };
    let next = ::effect_rs::collections::hash_map::set(&cur, key, value);
    let fr = fr.clone();
    box_future(async move { fr.locally(next, inner).run(r).await })
  })
}

/// Run `inner` while a span label is pushed on the fiber-local span stack (restored afterward).
pub fn with_log_span<A, E, R>(
  label: impl Into<String> + Send + 'static,
  inner: Effect<A, E, R>,
) -> Effect<A, E, R>
where
  A: Send + 'static,
  E: Send + 'static,
  R: Send + 'static,
{
  let label = label.into();
  Effect::new_async(move |r| {
    let tls = LOG_SPAN_STACK_FIBER_REF.with(|c| c.borrow().clone());
    let Some(fr) = tls else {
      return box_future(async move { inner.run(r).await });
    };
    let cur = ::effect_rs::run_blocking(fr.get(), ()).unwrap_or_default();
    let mut next = cur.clone();
    next.push(label);
    let fr = fr.clone();
    box_future(async move { fr.locally(next, inner).run(r).await })
  })
}

#[cfg(test)]
mod tests {
  use rstest::rstest;

  use super::*;
  use ::effect_rs::{Cons, Context, Layer, Nil, Service, run_blocking};

  // ========== Fixtures ==========

  type LogCtx = Context<Cons<Service<EffectLogKey, EffectLogger>, Nil>>;

  type LogCtxMin = Context<
    Cons<
      Service<EffectLogKey, EffectLogger>,
      Cons<Service<EffectLogMinLevelKey, FiberRef<LogLevel>>, Nil>,
    >,
  >;

  fn test_ctx() -> LogCtx {
    Context::new(Cons(Service::<EffectLogKey, _>::new(EffectLogger), Nil))
  }

  fn test_ctx_with_min(initial: LogLevel) -> LogCtxMin {
    test_clear_all_logger_tls();
    let logger = layer_effect_logger().build().expect("logger layer");
    let min = layer_minimum_log_level(initial).build().expect("min layer");
    Context::new(Cons(logger, Cons(min, Nil)))
  }

  fn init_subscriber() {
    test_clear_all_logger_tls();
    let _ = tracing_subscriber::fmt()
      .with_env_filter(tracing_subscriber::EnvFilter::new("trace"))
      .with_test_writer()
      .try_init();
  }

  // ========== fiber_min_log_level ==========

  mod fiber_min_log_level {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::Subscriber;
    use tracing_subscriber::Registry;
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::layer::{Context, Layer};

    struct Capture(Arc<Mutex<Vec<tracing::Level>>>);

    impl<S: Subscriber> Layer<S> for Capture {
      fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        self
          .0
          .lock()
          .expect("capture mutex")
          .push(*event.metadata().level());
      }
    }

    fn subscriber_with_capture(levels: Arc<Mutex<Vec<tracing::Level>>>) -> impl Subscriber {
      Registry::default().with(Capture(levels))
    }

    #[test]
    fn logger_filters_below_minimum_level() {
      test_clear_all_logger_tls();
      let levels = Arc::new(Mutex::new(Vec::new()));
      let _g = tracing::subscriber::set_default(subscriber_with_capture(levels.clone()));

      let ctx = test_ctx_with_min(LogLevel::Trace);
      let fr = ctx.0.1.0.value.clone();
      run_blocking(fr.set(LogLevel::Warn), ()).expect("set min");
      run_blocking(EffectLogger.info::<LogCtxMin>("filtered-info"), ctx).expect("log");

      let got = levels.lock().expect("capture");
      assert!(
        !got.contains(&tracing::Level::INFO),
        "expected INFO suppressed, got {got:?}"
      );
    }

    #[test]
    fn logger_with_minimum_log_level_overrides_globally() {
      test_clear_all_logger_tls();
      let levels = Arc::new(Mutex::new(Vec::new()));
      let _g = tracing::subscriber::set_default(subscriber_with_capture(levels.clone()));

      let ctx = test_ctx_with_min(LogLevel::Trace);
      let fr = ctx.0.1.0.value.clone();
      let inner = EffectLogger.info::<LogCtxMin>("inside-scope");
      run_blocking(
        EffectLogger::with_minimum_log_level(fr.clone(), LogLevel::Warn, inner),
        ctx,
      )
      .expect("scoped");

      assert!(
        !levels
          .lock()
          .expect("capture")
          .contains(&tracing::Level::INFO),
        "expected INFO suppressed inside locally"
      );

      levels.lock().expect("capture").clear();
      let ctx = test_ctx_with_min(LogLevel::Trace);
      run_blocking(EffectLogger.info::<LogCtxMin>("outside-scope"), ctx).expect("outer");

      assert!(
        levels
          .lock()
          .expect("capture")
          .contains(&tracing::Level::INFO),
        "expected INFO after new stack without Warn override"
      );
    }

    #[test]
    fn logger_restores_level_after_locally_scope() {
      test_clear_all_logger_tls();
      let _g =
        tracing::subscriber::set_default(subscriber_with_capture(Arc::new(Mutex::new(Vec::new()))));

      let ctx = test_ctx_with_min(LogLevel::Trace);
      let fr = ctx.0.1.0.value.clone();
      run_blocking(fr.set(LogLevel::Debug), ()).expect("set");
      assert_eq!(run_blocking(fr.get::<()>(), ()), Ok(LogLevel::Debug));

      let scoped =
        EffectLogger::with_minimum_log_level(fr.clone(), LogLevel::Warn, fr.get::<LogCtxMin>());
      assert_eq!(run_blocking(scoped, ctx), Ok(LogLevel::Warn));
      assert_eq!(run_blocking(fr.get::<()>(), ()), Ok(LogLevel::Debug));
    }
  }

  // ========== log_context ==========

  mod log_context {
    use super::*;
    use ::effect_rs::UtcDateTime;

    #[test]
    fn log_context_timestamp_format_iso() {
      let ctx = LogContext::new(UtcDateTime::unsafe_make(1_700_000_000_000));
      let s = ctx.timestamp.format_iso();
      assert!(
        s.ends_with('Z'),
        "format_iso should be UTC / RFC 3339 style: {s}"
      );
      assert!(s.contains('T'), "expected date-time separator: {s}");
    }

    #[test]
    fn log_context_with_now_timestamp_is_valid_iso() {
      let ctx = LogContext::with_now_timestamp();
      let s = ctx.timestamp.format_iso();
      assert!(s.ends_with('Z'), "{s}");
      assert!(s.contains('T'), "{s}");
    }
  }

  // ========== effect_logger_log ==========

  mod effect_logger_log {
    use super::*;

    mod with_unit_env {
      use super::*;

      #[rstest]
      #[case::trace(LogLevel::Trace)]
      #[case::debug(LogLevel::Debug)]
      #[case::info(LogLevel::Info)]
      #[case::warn(LogLevel::Warn)]
      #[case::error(LogLevel::Error)]
      #[case::fatal(LogLevel::Fatal)]
      fn returns_ok_for_every_level(#[case] level: LogLevel) {
        init_subscriber();
        let result = run_blocking(EffectLogger.log::<()>(level, "msg"), ());
        assert_eq!(result, Ok(()));
      }
    }

    mod with_context_env {
      use super::*;

      #[rstest]
      #[case::trace(LogLevel::Trace)]
      #[case::debug(LogLevel::Debug)]
      #[case::info(LogLevel::Info)]
      #[case::warn(LogLevel::Warn)]
      #[case::error(LogLevel::Error)]
      #[case::fatal(LogLevel::Fatal)]
      fn returns_ok_for_every_level(#[case] level: LogLevel) {
        init_subscriber();
        let result = run_blocking(EffectLogger.log::<LogCtx>(level, "msg"), test_ctx());
        assert_eq!(result, Ok(()));
      }
    }
  }

  // ========== effect_logger_level_methods ==========

  mod no_tls_paths {
    use super::*;
    use ::effect_rs::run_blocking;

    #[test]
    fn annotate_logs_without_tls_still_runs_inner() {
      crate::test_clear_all_logger_tls();
      let result = run_blocking(
        annotate_logs("k", "v", effect_rs::succeed::<i32, (), ()>(42)),
        (),
      );
      assert_eq!(result, Ok(42));
    }

    #[test]
    fn with_log_span_without_tls_still_runs_inner() {
      crate::test_clear_all_logger_tls();
      let result = run_blocking(
        with_log_span("span", effect_rs::succeed::<i32, (), ()>(99)),
        (),
      );
      assert_eq!(result, Ok(99));
    }
  }

  mod effect_logger_level_methods {
    use super::*;

    #[test]
    fn trace_delegates_to_log_at_trace_level() {
      init_subscriber();
      assert_eq!(run_blocking(EffectLogger.trace::<()>("t"), ()), Ok(()));
    }

    #[test]
    fn debug_delegates_to_log_at_debug_level() {
      init_subscriber();
      assert_eq!(run_blocking(EffectLogger.debug::<()>("d"), ()), Ok(()));
    }

    #[test]
    fn info_delegates_to_log_at_info_level() {
      init_subscriber();
      assert_eq!(run_blocking(EffectLogger.info::<()>("i"), ()), Ok(()));
    }

    #[test]
    fn warn_delegates_to_log_at_warn_level() {
      init_subscriber();
      assert_eq!(run_blocking(EffectLogger.warn::<()>("w"), ()), Ok(()));
    }

    #[test]
    fn error_delegates_to_log_at_error_level() {
      init_subscriber();
      assert_eq!(run_blocking(EffectLogger.error::<()>("e"), ()), Ok(()));
    }

    #[test]
    fn log_string_delegates_at_info_level() {
      init_subscriber();
      assert_eq!(
        run_blocking(
          EffectLogger.log_string::<()>(LogLevel::Info, "owned msg".to_string()),
          (),
        ),
        Ok(()),
      );
    }

    #[test]
    fn info_accepts_formatted_string() {
      init_subscriber();
      let n = 42u32;
      assert_eq!(
        run_blocking(EffectLogger.info::<()>(format!("n={n}")), ()),
        Ok(()),
      );
    }

    #[test]
    fn fatal_delegates_to_log_at_fatal_level() {
      init_subscriber();
      assert_eq!(run_blocking(EffectLogger.fatal::<()>("f"), ()), Ok(()));
    }

    #[test]
    fn log_none_level_returns_ok_without_side_effects() {
      init_subscriber();
      assert_eq!(
        run_blocking(EffectLogger.log::<()>(LogLevel::None, "silenced"), ()),
        Ok(())
      );
    }
  }

  // ========== into_bind_extraction ==========

  mod into_bind_extraction {
    use super::*;

    #[test]
    fn extracts_logger_copy_from_context() {
      let effect: ::effect_rs::Effect<EffectLogger, EffectLoggerError, LogCtx> =
        ::effect_rs::Effect::new_async(move |r| {
          Box::pin(async move { IntoBind::into_bind(EffectLogger, r).await })
        });
      let result = run_blocking(effect, test_ctx());
      assert!(result.is_ok());
    }

    #[test]
    fn extracted_logger_can_emit_log_via_run_blocking() {
      init_subscriber();
      let effect: ::effect_rs::Effect<EffectLogger, EffectLoggerError, LogCtx> =
        ::effect_rs::Effect::new_async(move |r| {
          Box::pin(async move { IntoBind::into_bind(EffectLogger, r).await })
        });
      let logger = run_blocking(effect, test_ctx()).expect("extraction is infallible");
      assert_eq!(run_blocking(logger.info::<()>("extracted"), ()), Ok(()));
    }
  }

  // ========== layer_effect_logger ==========

  mod layer_effect_logger_fn {
    use super::*;

    #[test]
    fn builds_without_error() {
      let result = layer_effect_logger().build();
      assert!(result.is_ok());
    }

    #[test]
    fn produced_service_can_be_placed_in_context() {
      let cell = layer_effect_logger().build().expect("infallible");
      let ctx: LogCtx = Context::new(Cons(cell, Nil));
      let result = run_blocking(EffectLogger.info::<LogCtx>("layer build ok"), ctx);
      assert_eq!(result, Ok(()));
    }
  }

  // ========== effect_logger_error ==========

  mod effect_logger_error {
    use super::*;

    #[test]
    fn sink_variant_display_contains_message() {
      let err = EffectLoggerError::Sink("oops".to_owned());
      assert!(err.to_string().contains("oops"));
    }

    #[test]
    fn sink_variant_display_has_prefix() {
      let err = EffectLoggerError::Sink("x".to_owned());
      assert!(err.to_string().starts_with("log sink error:"));
    }

    #[test]
    fn sink_variant_implements_error_trait() {
      let err: Box<dyn std::error::Error> = Box::new(EffectLoggerError::Sink("e".to_owned()));
      assert!(err.to_string().contains("e"));
    }

    #[test]
    fn two_equal_sink_errors_are_eq() {
      assert_eq!(
        EffectLoggerError::Sink("a".to_owned()),
        EffectLoggerError::Sink("a".to_owned())
      );
    }

    #[test]
    fn two_different_sink_errors_are_ne() {
      assert_ne!(
        EffectLoggerError::Sink("a".to_owned()),
        EffectLoggerError::Sink("b".to_owned())
      );
    }
  }

  // ========== beads 263w: Logger pipeline + backends ==========

  mod wave5_full_logger {
    use std::sync::{Arc, Mutex};

    use ::effect_rs::{Layer, run_blocking};

    use crate::{
      CompositeLogBackend, EffectLogger, EffectLoggerError, JsonLogBackend, LogBackend, LogLevel,
      LogRecord, Logger, StructuredLogBackend, annotate_logs, with_log_span,
    };

    struct MsgCap(Arc<Mutex<Vec<String>>>);

    impl LogBackend for MsgCap {
      fn emit(&self, rec: &LogRecord<'_>) -> Result<(), EffectLoggerError> {
        self.0.lock().expect("cap").push(rec.message.to_string());
        Ok(())
      }
    }

    fn setup_json() -> Arc<Mutex<Vec<u8>>> {
      crate::test_clear_all_logger_tls();
      let jb = JsonLogBackend::new(Vec::<u8>::new());
      let buf = jb.writer_arc();
      let comp = Arc::new(CompositeLogBackend::new());
      comp.add(Arc::new(jb)).expect("add json");
      crate::layer_log_metadata().build().expect("metadata layer");
      crate::layer_composite_logger(comp)
        .build()
        .expect("composite layer");
      buf
    }

    #[test]
    fn logger_json_backend_produces_valid_json() {
      let buf = setup_json();
      run_blocking(
        annotate_logs("k", "v", EffectLogger.info::<()>("hello")),
        (),
      )
      .expect("log");
      let bytes = buf.lock().expect("buf");
      let line = std::str::from_utf8(bytes.as_slice()).expect("utf8");
      let line = line.trim();
      let v: serde_json::Value = serde_json::from_str(line).expect("valid json");
      assert_eq!(v["level"], "INFO");
      assert_eq!(v["message"], "hello");
      assert_eq!(v["fields"]["k"], "v");
    }

    #[test]
    fn logger_add_replaces_layer() {
      crate::test_clear_all_logger_tls();
      let a = Arc::new(Mutex::new(Vec::new()));
      let b = Arc::new(Mutex::new(Vec::new()));
      let comp = Arc::new(CompositeLogBackend::new());
      comp.add(Arc::new(MsgCap(a.clone()))).unwrap();
      comp.add(Arc::new(MsgCap(b.clone()))).unwrap();
      crate::layer_composite_logger(comp.clone()).build().unwrap();
      run_blocking(EffectLogger.info::<()>("m1"), ()).unwrap();
      assert_eq!(*a.lock().unwrap(), vec!["m1".to_string()]);
      assert_eq!(*b.lock().unwrap(), vec!["m1".to_string()]);

      let c = Arc::new(Mutex::new(Vec::new()));
      comp.replace(0, Arc::new(MsgCap(c.clone()))).unwrap();
      a.lock().unwrap().clear();
      b.lock().unwrap().clear();
      run_blocking(EffectLogger.info::<()>("m2"), ()).unwrap();
      assert!(a.lock().unwrap().is_empty());
      assert_eq!(*b.lock().unwrap(), vec!["m2".to_string()]);
      assert_eq!(*c.lock().unwrap(), vec!["m2".to_string()]);
    }

    #[test]
    fn logger_fatal_is_highest_level() {
      assert!(LogLevel::Fatal.severity() > LogLevel::Error.severity());
      assert!(LogLevel::Trace.allows(LogLevel::Fatal));
      assert!(!LogLevel::Fatal.allows(LogLevel::Error));
      assert!(LogLevel::Fatal.allows(LogLevel::Fatal));
      assert!(!LogLevel::None.allows(LogLevel::Info));
    }

    #[test]
    fn annotate_logs_visible_in_structured_output() {
      crate::test_clear_all_logger_tls();
      let structured = StructuredLogBackend::new(Vec::<u8>::new());
      let buf = structured.writer_arc();
      let comp = Arc::new(CompositeLogBackend::new());
      comp.add(Arc::new(structured)).unwrap();
      crate::layer_log_metadata().build().unwrap();
      crate::layer_composite_logger(comp).build().unwrap();
      run_blocking(
        annotate_logs("trace_id", "abc", EffectLogger.info::<()>("done")),
        (),
      )
      .unwrap();
      let s = String::from_utf8(buf.lock().unwrap().clone()).unwrap();
      assert!(
        s.contains("trace_id") && s.contains("abc"),
        "expected annotation in output: {s:?}"
      );
    }

    #[test]
    fn with_log_span_visible_in_json() {
      let buf = setup_json();
      run_blocking(
        with_log_span("outer", EffectLogger.warn::<()>("inside")),
        (),
      )
      .unwrap();
      let bytes = buf.lock().unwrap().clone();
      let line = std::str::from_utf8(&bytes).unwrap().trim();
      let v: serde_json::Value = serde_json::from_str(line).unwrap();
      assert_eq!(v["spans"], serde_json::json!(["outer"]));
    }
  }
}
