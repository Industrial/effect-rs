//! Attribute: `#[distributed_behavior("name")]` on a process struct.
//!
//! Generates `BEHAVIOR_NAME` and `register_behavior(registry)` that call
//! `BehaviorRegistry::register::<Self>(BEHAVIOR_NAME)`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemStruct, LitStr, parse_macro_input};

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
  let name = parse_macro_input!(attr as LitStr);
  let input = parse_macro_input!(item as ItemStruct);
  let ident = &input.ident;
  let name_val = name.value();

  let expanded = quote! {
    #input

    impl #ident {
      /// Stable cluster-wide behavior name (FLAME registration key).
      pub const BEHAVIOR_NAME: &'static str = #name_val;

      /// Register this behavior with a [`id_effect_node::BehaviorRegistry`].
      pub fn register_behavior(registry: &mut id_effect_node::BehaviorRegistry) {
        registry.register::<Self>(Self::BEHAVIOR_NAME);
      }
    }
  };
  expanded.into()
}
