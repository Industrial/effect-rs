//! Procedural macros for the workspace `effect` crate.
//!
//! Doc links cannot use the `id_effect::…` prefix here: this crate defines an `effect` function, which
//! shadows the `effect` crate name in rustdoc link resolution.
#![allow(rustdoc::broken_intra_doc_links)]
#![deny(missing_docs)]

mod capability_attr;
mod derive_fsm;
mod derive_optics;
mod derive_schema_parser;
mod distributed_behavior;
mod effect_data;
mod effect_graph;
mod effect_tagged;
mod expand;
mod infer_caps;
mod match_effect;
mod parse;
mod provider_spec;
mod transform;

use proc_macro::TokenStream;

/// Derive macro: structural [`PartialEq`], [`Eq`], and [`Hash`] for Effect.ts-style data types.
///
/// Types implementing these impls automatically satisfy [`id_effect::data::EffectData`] via the
/// blanket implementation in the `effect` crate.
#[proc_macro_derive(EffectData)]
pub fn derive_effect_data(input: TokenStream) -> TokenStream {
  effect_data::derive_effect_data(input)
}

/// Injects `pub _tag: &'static str`, an [`id_effect::match_::HasTag`] impl, and
/// `EFFECT_TAGGED_TAG` on the struct (see generated inherent associated const).
///
/// Only supports structs with **named fields**. Place **above** `#[derive(EffectData, …)]`.
#[proc_macro_attribute]
pub fn effect_tagged(attr: TokenStream, item: TokenStream) -> TokenStream {
  effect_tagged::expand(attr, item)
}

/// Generates a capability key struct and [`id_effect::CapabilityKey`] impl for a trait or struct.
#[proc_macro_attribute]
pub fn capability(attr: TokenStream, item: TokenStream) -> TokenStream {
  capability_attr::expand(attr, item)
}

/// Derive [`id_effect::ProviderSpec`] when paired with `#[provides(Capability)]`.
#[proc_macro_derive(ProviderSpec, attributes(provides, named))]
pub fn derive_provider_spec(input: TokenStream) -> TokenStream {
  provider_spec::derive(input)
}

/// Enum match helper — prefix variant patterns with `EnumPath` for exhaustiveness checking.
///
/// ```ignore
/// match_effect!(Color, value, {
///     Red(n) => n,
///     Green => 0,
///     Blue => 1,
/// })
/// ```
#[proc_macro]
pub fn match_effect(input: TokenStream) -> TokenStream {
  match_effect::expand(input)
}

/// Procedural do-notation macro for [`id_effect::Effect`].
///
/// See the `effect` crate documentation for usage.
#[proc_macro]
pub fn effect(input: TokenStream) -> TokenStream {
  let input = proc_macro2::TokenStream::from(input);
  let kind = match parse::parse_effect_input(input) {
    Ok(k) => k,
    Err(e) => return e.to_compile_error().into(),
  };
  match expand::expand(kind) {
    Ok(ts) => ts.into(),
    Err(e) => e.to_compile_error().into(),
  }
}

/// Derive field lenses and enum prisms for [`id_effect_optics`].
#[proc_macro_derive(Optics, attributes(lens, prism, optional, optics))]
pub fn derive_optics(input: TokenStream) -> TokenStream {
  derive_optics::derive_optics(input)
}

/// Stub derive for future FSM transition tables (`id_effect_fsm`).
#[proc_macro_derive(Fsm, attributes(state, transition, initial))]
pub fn derive_fsm(input: TokenStream) -> TokenStream {
  derive_fsm::derive_fsm(input)
}

/// Stub derive for future schema-driven parser codegen (`id_effect_parse`).
#[proc_macro_derive(SchemaParser, attributes(schema, parser))]
pub fn derive_schema_parser(input: TokenStream) -> TokenStream {
  derive_schema_parser::derive_schema_parser(input)
}

/// Mark a process struct as a remotely spawnable behavior.
///
/// ```ignore
/// #[distributed_behavior("counter")]
/// struct Counter { ... }
/// // Counter::BEHAVIOR_NAME == "counter"
/// // Counter::register_behavior(&mut registry);
/// ```
#[proc_macro_attribute]
pub fn distributed_behavior(attr: TokenStream, item: TokenStream) -> TokenStream {
  distributed_behavior::expand(attr, item)
}
