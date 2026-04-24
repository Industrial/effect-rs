//! Integration tests: [`ie_cli_probe`](https://github.com/Industrial/id_effect/tree/main/crates/id_effect_cli) exit status.

use std::process::Command;

fn probe() -> Command {
  Command::new(env!("CARGO_BIN_EXE_ie_cli_probe"))
}

mod ie_cli_probe_when_mode_ok {
  use super::*;

  #[test]
  fn exits_with_success_status() {
    let status = probe()
      .args(["--mode", "ok"])
      .status()
      .expect("spawn ie_cli_probe");
    assert!(status.success());
  }
}

mod ie_cli_probe_when_mode_fail {
  use super::*;

  #[test]
  fn exits_with_code_one() {
    let status = probe()
      .args(["--mode", "fail"])
      .status()
      .expect("spawn ie_cli_probe");
    assert_eq!(status.code(), Some(1));
  }
}
