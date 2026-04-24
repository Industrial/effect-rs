//! Map [`id_effect::Exit`], [`id_effect::Cause`], and [`Result`] to [`std::process::ExitCode`].

use id_effect::{Cause, Exit};
use std::process::ExitCode;

/// Numeric byte used inside [`ExitCode`] for a [`Cause`] tree (deepest / worst wins for composites).
///
/// Convention (documented in the mdBook *CLI exit codes* chapter):
///
/// | Kind | Byte | Meaning |
/// |------|------|---------|
/// | success (`Exit::Success`) | `0` | OK |
/// | [`Cause::Fail`] | `1` | expected / typed failure |
/// | [`Cause::Die`] | `101` | defect (panic-style) |
/// | [`Cause::Interrupt`] | `130` | cancellation (shell SIGINT convention) |
/// | [`Cause::Both`] / [`Cause::Then`] | `max(left, right)` | preserve strongest signal |
pub fn cause_max_exit_byte<E>(cause: &Cause<E>) -> u8 {
  match cause {
    Cause::Fail(_) => 1,
    Cause::Die(_) => 101,
    Cause::Interrupt(_) => 130,
    Cause::Both(l, r) | Cause::Then(l, r) => cause_max_exit_byte(l).max(cause_max_exit_byte(r)),
  }
}

/// [`ExitCode`] for a finished [`Exit`] value.
#[inline]
pub fn exit_code_for_exit<A, E>(exit: Exit<A, E>) -> ExitCode {
  match exit {
    Exit::Success(_) => ExitCode::SUCCESS,
    Exit::Failure(cause) => exit_code_for_cause(cause),
  }
}

/// [`ExitCode`] from a structured [`Cause`].
#[inline]
pub fn exit_code_for_cause<E>(cause: Cause<E>) -> ExitCode {
  ExitCode::from(cause_max_exit_byte(&cause))
}

/// Map a synchronous [`Result`] from [`id_effect::runtime::run_blocking`] to an [`ExitCode`].
///
/// - `Ok(_)` → [`ExitCode::SUCCESS`]
/// - `Err(_)` → `1` (typed channel; use [`exit_code_for_exit`] when you have a full [`Exit`])
#[inline]
pub fn exit_code_for_result<A, E>(result: Result<A, E>) -> ExitCode {
  match result {
    Ok(_) => ExitCode::SUCCESS,
    Err(_) => ExitCode::from(1u8),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::FiberId;
  use rstest::rstest;

  mod cause_max_exit_byte {
    use super::*;

    #[test]
    fn fail_maps_to_one() {
      assert_eq!(cause_max_exit_byte(&Cause::<&str>::fail("x")), 1);
    }

    #[test]
    fn die_maps_to_101() {
      assert_eq!(cause_max_exit_byte(&Cause::<&str>::die("boom")), 101);
    }

    #[test]
    fn interrupt_maps_to_one_thirty() {
      assert_eq!(
        cause_max_exit_byte(&Cause::<&str>::interrupt(FiberId::new(1))),
        130
      );
    }

    #[test]
    fn both_takes_max_of_children() {
      let c = Cause::both(Cause::<&str>::fail("a"), Cause::<&str>::die("b"));
      assert_eq!(cause_max_exit_byte(&c), 101);
    }

    #[test]
    fn then_takes_max_of_children() {
      let c = Cause::then(
        Cause::<&str>::fail("a"),
        Cause::<&str>::interrupt(FiberId::new(9)),
      );
      assert_eq!(cause_max_exit_byte(&c), 130);
    }

    #[test]
    fn nested_both_respects_deepest_severity() {
      let inner = Cause::both(Cause::<&str>::fail("f"), Cause::<&str>::die("d"));
      let c = Cause::then(Cause::<&str>::fail("first"), inner);
      assert_eq!(cause_max_exit_byte(&c), 101);
    }
  }

  mod exit_code_for_exit {
    use super::*;
    use id_effect::Exit;

    #[test]
    fn success_yields_success_exit_code() {
      assert_eq!(
        exit_code_for_exit(Exit::<(), &str>::succeed(())),
        ExitCode::SUCCESS
      );
    }

    #[test]
    fn failure_fail_yields_exit_code_one() {
      let code = exit_code_for_exit(Exit::<(), &str>::fail("oops"));
      assert_eq!(code, ExitCode::from(1u8));
    }
  }

  mod exit_code_for_result {
    use super::*;

    #[rstest]
    #[case::ok_ok(Ok::<u8, &str>(7), 0u8)]
    #[case::err_maps_one(Err::<u8, &str>("e"), 1u8)]
    fn maps_result_to_expected_byte(#[case] input: Result<u8, &str>, #[case] expected: u8) {
      assert_eq!(exit_code_for_result(input), ExitCode::from(expected));
    }
  }
}
