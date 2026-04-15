//! **EffectInterface** — typed operation-signature protocol (Koka/Eff/Frank analogue).
//!
//! In Koka an *effect* is a named set of typed *operations*:
//!
//! ```koka
//! effect state<s>
//!   fun get()     : s
//!   fun put(x: s) : ()
//! ```
//!
//! and a *handler* provides the concrete implementation:
//!
//! ```koka
//! handler { fun get() ... fun put(x) ... }
//! ```
//!
//! Our system already models this via `service_key!` + `Layer`, but with no
//! formal vocabulary linking "effect interface" (trait with operations) to
//! "handler" (implementation injected via Layer). This module adds that link.
//!
//! ## Pattern
//!
//! 1. **Declare an interface** with the `effect_interface!` macro.  
//!    Each operation becomes a field on a struct that holds `Effect`-returning
//!    closures (like a vtable, but explicit and type-safe).
//!
//! 2. **Write calling code** generic over `R: NeedsInterface<Iface>`.  
//!    The concrete `R` is the `Context<…>` that contains `Tagged<Iface::Key, Iface>`.
//!
//! 3. **Provide a handler** by building a `Layer` that returns an instance of the
//!    interface struct. Swap handlers by changing which layer you stack.
//!
//! ## Relationship to other strata
//!
//! - Uses [`service_key!`](crate::service_key) + [`Tagged`](crate::context::Tagged) (Stratum 3).
//! - Plugs into [`Layer`](crate::layer::Layer) / [`Stack`](crate::layer::Stack) (Stratum 5).
//! - Calling code uses [`asks`](crate::kernel::effect::asks) to retrieve the
//!   interface from the context.

use crate::context::{Cons, Context, Nil, Tagged};

// ── Marker trait ─────────────────────────────────────────────────────────────

/// Marker: environment `R` contains an implementation of interface `I`.
///
/// This is the ergonomic bound for calling code that needs a specific interface.
/// It is a thin alias over the concrete `Context` + `Get` infrastructure so
/// callers can write `NeedsInterface<MyIface>` instead of raw HList path types.
///
/// Currently requires naming the concrete `Context<…>` type at the call site
/// (Rust lacks HKT for true MTL-style polymorphism), but documents the *intent*
/// that calling code is generic over the handler implementation.
pub trait NeedsInterface<I: EffectInterface> {
  /// Retrieve a reference to the interface implementation.
  fn get_interface(&self) -> &I;
}

// Blanket: any Context<Cons<Tagged<I::Key, I>, Tail>> satisfies NeedsInterface<I>.
impl<I, Tail> NeedsInterface<I> for Context<Cons<Tagged<I::Key, I>, Tail>>
where
  I: EffectInterface,
  I::Key: 'static,
  Tail: 'static,
{
  #[inline]
  fn get_interface(&self) -> &I {
    &self.0.0.value
  }
}

// ── Core trait ───────────────────────────────────────────────────────────────

/// A named set of typed operations (analogous to a Koka effect declaration).
///
/// Implement this trait on a struct whose fields are `Box<dyn Fn(…) -> …>` or
/// plain closures; those fields are the *operations* of the interface.
///
/// The `Key` associated type is the zero-sized nominal tag (created with
/// [`service_key!`](crate::service_key)) that identifies this interface in the
/// `Context` HList.
///
/// # Example
///
/// ```rust
/// use effect_rs::algebra::interface::EffectInterface;
///
/// effect_rs::service_key!(pub struct LoggerKey);
///
/// pub struct Logger {
///     pub log: Box<dyn Fn(&str) + Send + Sync>,
/// }
///
/// impl EffectInterface for Logger {
///     type Key = LoggerKey;
/// }
/// ```
pub trait EffectInterface: Sized + 'static {
  /// The zero-sized nominal tag that identifies this interface in the context.
  type Key: 'static;
}

// ── Ergonomic accessor ───────────────────────────────────────────────────────
//
// Handler adapter and context builder are the primary ergonomics; calling code
// uses `asks` (crate::kernel::effect::asks) to read the tagged interface from
// the context environment directly:
//
// ```rust,ignore
// // Instead of a generic via(), use asks() scoped to the concrete env type:
// fn do_work(n: i32) -> Effect<i32, (), MyCtx> {
//     asks(move |r: &mut MyCtx| {
//         let iface: &MyIface = r.get::<MyIface::Key>();
//         (iface.operate)(n)
//     })
// }
// ```
//
// This keeps the type-checking straightforward and delegates path lookup to the
// already-proven `Get` infrastructure.

// ── Handler adapter ──────────────────────────────────────────────────────────

/// Build a [`Layer`](crate::layer::Layer) that provides an interface `I` via
/// an infallible factory closure.
///
/// The returned layer, when built, inserts `Tagged::<I::Key, I>` into the
/// context.
///
/// # Example
///
/// ```rust,ignore
/// let layer = handler(|| Logger {
///     log: Box::new(|msg| eprintln!("{msg}")),
/// });
/// let cell: Tagged<LoggerKey, Logger> = layer.build().unwrap();
/// ```
#[inline]
pub fn handler<I, F>(f: F) -> HandlerLayer<I, F>
where
  I: EffectInterface,
  F: Fn() -> I,
{
  HandlerLayer {
    f,
    _pd: std::marker::PhantomData,
  }
}

/// A [`Layer`](crate::layer::Layer) that produces `Tagged<I::Key, I>` from a factory.
pub struct HandlerLayer<I, F> {
  f: F,
  _pd: std::marker::PhantomData<fn() -> I>,
}

impl<I, F> crate::layer::Layer for HandlerLayer<I, F>
where
  I: EffectInterface,
  F: Fn() -> I,
{
  type Output = Tagged<I::Key, I>;
  type Error = crate::runtime::Never;

  fn build(&self) -> Result<Self::Output, Self::Error> {
    Ok(Tagged::<I::Key, I>::new((self.f)()))
  }
}

// ── Convenience: single-interface Context builder ────────────────────────────

/// Build a `Context<Cons<Tagged<I::Key, I>, Nil>>` from a handler factory.
///
/// Useful in tests to quickly wire one interface without hand-building an HList.
#[inline]
pub fn single_context<I, F>(f: F) -> Context<Cons<Tagged<I::Key, I>, Nil>>
where
  I: EffectInterface,
  F: Fn() -> I,
{
  Context::new(Cons(Tagged::<I::Key, I>::new(f()), Nil))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use super::*;
  use crate::layer::Layer as _;
  use crate::runtime::run_blocking;

  // ── Fixtures ────────────────────────────────────────────────────────────

  crate::service_key!(struct CounterKey);

  /// A simple "counter" interface with two operations.
  struct Counter {
    increment: Box<dyn Fn(i32) -> i32 + Send + Sync>,
    decrement: Box<dyn Fn(i32) -> i32 + Send + Sync>,
  }

  impl EffectInterface for Counter {
    type Key = CounterKey;
  }

  fn counter_add1() -> Counter {
    Counter {
      increment: Box::new(|n| n + 1),
      decrement: Box::new(|n| n - 1),
    }
  }

  fn counter_mul2() -> Counter {
    Counter {
      increment: Box::new(|n| n * 2),
      decrement: Box::new(|n| n / 2),
    }
  }

  type CounterCtx = Context<Cons<Tagged<CounterKey, Counter>, Nil>>;

  // ── HandlerLayer ────────────────────────────────────────────────────────

  mod handler_layer {
    use super::*;

    /// `HandlerLayer::build()` wraps the factory output in a `Tagged` cell.
    #[test]
    fn build_returns_tagged_cell() {
      let layer = handler(counter_add1);
      let cell = layer.build().expect("infallible");
      assert_eq!((cell.value.increment)(10), 11);
    }

    /// A second handler with different semantics can replace the first.
    #[test]
    fn alternate_handler_swaps_semantics() {
      let layer = handler(counter_mul2);
      let cell = layer.build().expect("infallible");
      assert_eq!((cell.value.increment)(10), 20);
    }
  }

  // ── single_context ──────────────────────────────────────────────────────

  mod single_context_builder {
    use super::*;

    #[test]
    fn builds_context_with_one_interface() {
      let ctx = single_context(counter_add1);
      let counter: &Counter = ctx.get::<CounterKey>();
      assert_eq!((counter.increment)(5), 6);
    }
  }

  // ── Calling code pattern: generic over R containing Counter ─────────────

  mod calling_code {
    use super::*;
    use crate::Effect;

    /// Calling code is written against the context type directly; in a real
    /// system it would be generic via a `NeedsInterface<Counter>` bound
    /// (which requires HList path resolution at compile time).
    fn do_increment(n: i32) -> Effect<i32, (), CounterCtx> {
      Effect::new(move |r: &mut CounterCtx| {
        let counter: &Counter = r.get::<CounterKey>();
        Ok((counter.increment)(n))
      })
    }

    fn do_decrement(n: i32) -> Effect<i32, (), CounterCtx> {
      Effect::new(move |r: &mut CounterCtx| {
        let counter: &Counter = r.get::<CounterKey>();
        Ok((counter.decrement)(n))
      })
    }

    /// With the add-1 handler, increment(41) = 42.
    #[test]
    fn add1_handler_increment() {
      let ctx = single_context(counter_add1);
      assert_eq!(run_blocking(do_increment(41), ctx), Ok(42));
    }

    /// Swap to mul-2 handler: increment(21) = 42.
    #[test]
    fn mul2_handler_increment() {
      let ctx = single_context(counter_mul2);
      assert_eq!(run_blocking(do_increment(21), ctx), Ok(42));
    }

    /// Same calling code works with decrement regardless of handler.
    #[test]
    fn handler_swap_is_transparent_to_callers() {
      let ctx_add = single_context(counter_add1);
      let ctx_mul = single_context(counter_mul2);

      let r1 = run_blocking(do_decrement(43), ctx_add); // 43 - 1 = 42
      let r2 = run_blocking(do_decrement(84), ctx_mul); // 84 / 2 = 42

      assert_eq!(r1, Ok(42));
      assert_eq!(r2, Ok(42));
    }

    /// Compose two operations: both use the same handler from the context.
    #[test]
    fn operations_compose_through_flat_map() {
      let ctx = single_context(counter_add1);
      let composed = do_increment(0)
        .flat_map(|n| do_increment(n))
        .flat_map(|n| do_increment(n));
      assert_eq!(run_blocking(composed, ctx), Ok(3)); // 0+1+1+1
    }
  }

  // ── Interface identity law ──────────────────────────────────────────────

  mod interface_identity_law {
    use super::*;

    /// `handler(f).build()` is equivalent to wrapping `f()` in a Tagged cell.
    #[test]
    fn handler_layer_equivalent_to_manual_tagged() {
      let via_layer = handler(counter_add1).build().expect("infallible");
      let manual = Tagged::<CounterKey, _>::new(counter_add1());

      // Both should produce the same operation behaviour
      assert_eq!(
        (via_layer.value.increment)(10),
        (manual.value.increment)(10)
      );
    }

    #[test]
    fn get_interface_returns_interface_from_context() {
      let ctx = single_context(counter_add1);
      let counter: &Counter = ctx.get_interface();
      assert_eq!((counter.increment)(10), 11);
    }
  }
}
