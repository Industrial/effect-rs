//! Aggregate parse failures — mirrors collecting multiple validation issues (Effect.ts-style).
//!
//! See repository [`TESTING.md`](../../../../TESTING.md) for test naming and module layout.

use std::fmt;

use crate::schema::parse::ParseError;

/// Several [`ParseError`] values (e.g. future “validate all fields” paths).
#[derive(Clone, Debug, crate::EffectData)]
pub struct ParseErrors {
  /// Collected issues in stable order.
  pub issues: Vec<ParseError>,
}

impl ParseErrors {
  /// Build from a non-empty list of issues.
  pub fn new(issues: Vec<ParseError>) -> Self {
    Self { issues }
  }

  /// Single-issue convenience.
  pub fn one(err: ParseError) -> Self {
    Self { issues: vec![err] }
  }
}

impl fmt::Display for ParseErrors {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for (i, e) in self.issues.iter().enumerate() {
      if i > 0 {
        writeln!(f)?;
      }
      if e.path.is_empty() {
        write!(f, "{}", e.message)?;
      } else {
        write!(f, "{}: {}", e.path, e.message)?;
      }
    }
    Ok(())
  }
}

impl std::error::Error for ParseErrors {}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn display_joins_issues_with_newlines() {
    let e = ParseErrors::new(vec![
      ParseError::new("a", "bad a"),
      ParseError::new("b", "bad b"),
    ]);
    let s = e.to_string();
    assert!(s.contains("a: bad a"));
    assert!(s.contains("b: bad b"));
  }

  #[test]
  fn display_empty_path_omits_path_prefix() {
    let e = ParseErrors::one(ParseError::new("", "bare error"));
    assert_eq!(e.to_string(), "bare error");
  }
}
