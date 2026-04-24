//! Deserialize a struct from a TOML file with Figment and [`id_effect_config::extract`].

use id_effect_config::{extract, figment};
use serde::Deserialize;
use std::io::Write;

#[derive(Deserialize)]
struct AppCfg {
  host: String,
  port: u16,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let dir = tempfile::tempdir()?;
  let path = dir.path().join("app.toml");
  let mut f = std::fs::File::create(&path)?;
  writeln!(
    f,
    r#"host = "127.0.0.1"
port = 8080"#
  )?;
  drop(f);

  let fig = figment::from_toml_file(&path);
  let cfg: AppCfg = extract(&fig)?;
  println!("host={} port={}", cfg.host, cfg.port);
  Ok(())
}
