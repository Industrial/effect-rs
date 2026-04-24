//! `run_main` — tracing (optional), [`run_blocking`](id_effect::runtime::run_blocking), stderr, [`ExitCode`](std::process::ExitCode).

use std::fmt::Debug;
use std::process::ExitCode;

use id_effect::{Effect, TracingConfig, install_tracing_layer, runtime::run_blocking};

use crate::exit_code_for_result;

/// Options for [`run_main`].
#[derive(Clone, Debug)]
pub struct RunMainConfig {
  /// When `true`, installs the default tracing layer via [`install_tracing_layer`]
  /// ([`TracingConfig::enabled`]) before running `effect`.
  pub install_tracing: bool,
}

impl RunMainConfig {
  /// No tracing install; only runs the effect and maps [`Result`] to [`ExitCode`].
  #[inline]
  pub fn minimal() -> Self {
    Self {
      install_tracing: false,
    }
  }

  /// Same as [`RunMainConfig::minimal`] but installs tracing first.
  #[inline]
  pub fn with_tracing() -> Self {
    Self {
      install_tracing: true,
    }
  }
}

/// Run `effect` with `env`, optionally install tracing, log `Err` to **stderr**, return [`ExitCode`].
///
/// Typed failures (`Err(e)`) are printed with [`Debug`] (covers `E = ()` and structured errors).
/// For user-facing CLI output, map errors to `String` (or implement a thin `Display` wrapper)
/// before returning them from the effect.
///
/// For richer `Cause` handling (defect vs interrupt), use [`run_test`](id_effect::testing::run_test)
/// or a custom driver, then [`exit_code_for_exit`](crate::exit_code_for_exit).
#[inline]
pub fn run_main<A, E, R>(effect: Effect<A, E, R>, env: R, config: RunMainConfig) -> ExitCode
where
  A: 'static,
  E: 'static + Debug,
  R: 'static,
{
  if config.install_tracing {
    let _ = run_blocking(install_tracing_layer(TracingConfig::enabled()), ());
  }
  let result = run_blocking(effect, env);
  if let Err(ref e) = result {
    eprintln!("error: {e:?}");
  }
  exit_code_for_result(result)
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::{fail, succeed};

  mod run_main {
    use super::*;

    #[test]
    fn with_success_returns_success_exit_code() {
      let code = run_main(succeed::<u8, (), ()>(9), (), RunMainConfig::minimal());
      assert_eq!(code, ExitCode::SUCCESS);
    }

    #[test]
    fn with_failure_returns_nonzero_exit_code() {
      let code = run_main(fail::<u8, &str, ()>("boom"), (), RunMainConfig::minimal());
      assert_eq!(code, ExitCode::from(1u8));
    }

    #[test]
    fn with_tracing_flag_does_not_panic_on_success() {
      let code = run_main(succeed::<(), (), ()>(()), (), RunMainConfig::with_tracing());
      assert_eq!(code, ExitCode::SUCCESS);
    }
  }
}
