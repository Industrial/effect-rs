//! [`OrElseConfigProvider`](id_effect_config::OrElseConfigProvider): try one source, then another.

use id_effect_config::config;
use id_effect_config::{MapConfigProvider, OrElseConfigProvider};
use std::collections::HashMap;

fn main() -> Result<(), id_effect_config::ConfigError> {
  let primary = MapConfigProvider::from_map(HashMap::new());
  let backup = MapConfigProvider::from_pairs([("API_KEY", "fallback-key")]);
  let p = OrElseConfigProvider::new(primary, backup);

  println!("{}", config::string(&p, "API_KEY")?);
  Ok(())
}
