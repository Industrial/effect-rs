//! [`MapConfigProvider`] with [`id_effect_config::config`] helpers (nested keys, defaults).

use id_effect_config::MapConfigProvider;
use id_effect_config::config;

fn main() -> Result<(), id_effect_config::ConfigError> {
  let p = MapConfigProvider::from_pairs([("SERVICE_HOST", "localhost"), ("SERVICE_NAME", "demo")]);

  let host = config::nested_string(&p, "SERVICE", "HOST")?;
  let name = config::nested_string(&p, "SERVICE", "NAME")?;
  let threads = config::with_default(config::nested_integer(&p, "SERVICE", "THREADS"), 4)?;

  println!("host={host} name={name} threads={threads}");
  Ok(())
}
