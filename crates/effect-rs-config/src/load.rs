//! Low-level reads against the injected [`crate::ConfigProvider`] service.
//!
//! Every public function returns `Effect<A, E, R>` where `R: NeedsConfigProvider`.
//! The provider is extracted synchronously from the environment via
//! `Get::<ConfigProviderKey, Here>::get(r)` so all effects stay non-async and
//! the `EFFECT_PREFER_FROM_ASYNC_OVER_NEW_ASYNC` lint is never triggered.

use ::effect::{Effect, Get, Here, effect};

use crate::error::ConfigError;
use crate::provider::{ConfigProviderKey, NeedsConfigProvider};

// ── helpers ──────────────────────────────────────────────────────────────────

/// `Config.withDefault` as a method — only [`ConfigError::Missing`] is swapped.
///
/// This is a trait-impl method so the `effect!` lint is not required here.
pub trait WithConfigDefault<A, R>: Sized {
  /// Return `default` in place of the effect value when the error is [`ConfigError::Missing`].
  fn with_default(self, default: A) -> Effect<A, ConfigError, R>;
}

impl<A, R> WithConfigDefault<A, R> for Effect<A, ConfigError, R>
where
  A: Clone + 'static,
  R: 'static,
{
  fn with_default(self, default: A) -> Effect<A, ConfigError, R> {
    self.catch(move |e| match e {
      ConfigError::Missing { .. } => ::effect::succeed(default.clone()),
      other => ::effect::fail(other),
    })
  }
}

// ── path helpers ──────────────────────────────────────────────────────────────

/// Build a multi-segment path, e.g. `nested_path("SERVER", &["HOST"])` → `["SERVER", "HOST"]`.
#[inline]
pub fn nested_path(namespace: &str, leaf: &[&str]) -> Vec<String> {
  std::iter::once(namespace.to_string())
    .chain(leaf.iter().map(|s| (*s).to_string()))
    .collect()
}

// ── primitive reads ───────────────────────────────────────────────────────────

/// Required string.
pub fn read_string<A, E, R>(path: &[&str]) -> Effect<A, E, R>
where
  A: From<String> + 'static,
  E: From<ConfigError> + 'static,
  R: NeedsConfigProvider + 'static,
{
  let path_owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
  effect!(|r: &mut R| {
    let provider = Get::<ConfigProviderKey, Here>::get(r);
    let refs: Vec<&str> = path_owned.iter().map(String::as_str).collect();
    let path_str = refs.join(".");
    match provider.0.load_raw(&refs) {
      Err(e) => return Err(E::from(e)),
      Ok(None) => return Err(E::from(ConfigError::Missing { path: path_str })),
      Ok(Some(s)) => A::from(s),
    }
  })
}

/// Optional string — missing key yields `None`.
pub fn read_string_opt<A, E, R>(path: &[&str]) -> Effect<A, E, R>
where
  A: From<Option<String>> + 'static,
  E: From<ConfigError> + 'static,
  R: NeedsConfigProvider + 'static,
{
  let path_owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
  effect!(|r: &mut R| {
    let provider = Get::<ConfigProviderKey, Here>::get(r);
    let refs: Vec<&str> = path_owned.iter().map(String::as_str).collect();
    match provider.0.load_raw(&refs) {
      Err(e) => return Err(E::from(e)),
      Ok(raw) => A::from(raw),
    }
  })
}

/// Floating-point number parsed from a string scalar.
pub fn read_number<A, E, R>(path: &[&str]) -> Effect<A, E, R>
where
  A: From<f64> + 'static,
  E: From<ConfigError> + 'static,
  R: NeedsConfigProvider + 'static,
{
  let path_owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
  effect!(|r: &mut R| {
    let provider = Get::<ConfigProviderKey, Here>::get(r);
    let refs: Vec<&str> = path_owned.iter().map(String::as_str).collect();
    let path_str = refs.join(".");
    let s = match provider.0.load_raw(&refs) {
      Err(e) => return Err(E::from(e)),
      Ok(None) => {
        return Err(E::from(ConfigError::Missing {
          path: path_str.clone(),
        }));
      }
      Ok(Some(s)) => s,
    };
    let n = s.parse::<f64>().map_err(|e| {
      E::from(ConfigError::Invalid {
        path: path_str,
        value: s,
        reason: e.to_string(),
      })
    })?;
    A::from(n)
  })
}

/// Signed 64-bit integer parsed from a string scalar.
pub fn read_i64<A, E, R>(path: &[&str]) -> Effect<A, E, R>
where
  A: From<i64> + 'static,
  E: From<ConfigError> + 'static,
  R: NeedsConfigProvider + 'static,
{
  let path_owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
  effect!(|r: &mut R| {
    let provider = Get::<ConfigProviderKey, Here>::get(r);
    let refs: Vec<&str> = path_owned.iter().map(String::as_str).collect();
    let path_str = refs.join(".");
    let s = match provider.0.load_raw(&refs) {
      Err(e) => return Err(E::from(e)),
      Ok(None) => {
        return Err(E::from(ConfigError::Missing {
          path: path_str.clone(),
        }));
      }
      Ok(Some(s)) => s,
    };
    let n = s.parse::<i64>().map_err(|e| {
      E::from(ConfigError::Invalid {
        path: path_str,
        value: s,
        reason: e.to_string(),
      })
    })?;
    A::from(n)
  })
}

/// Boolean parsed from `"true"` / `"false"` / `"1"` / `"0"` / `"yes"` / `"no"`.
pub fn read_bool<A, E, R>(path: &[&str]) -> Effect<A, E, R>
where
  A: From<bool> + 'static,
  E: From<ConfigError> + 'static,
  R: NeedsConfigProvider + 'static,
{
  let path_owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
  effect!(|r: &mut R| {
    let provider = Get::<ConfigProviderKey, Here>::get(r);
    let refs: Vec<&str> = path_owned.iter().map(String::as_str).collect();
    let path_str = refs.join(".");
    let s = match provider.0.load_raw(&refs) {
      Err(e) => return Err(E::from(e)),
      Ok(None) => {
        return Err(E::from(ConfigError::Missing {
          path: path_str.clone(),
        }));
      }
      Ok(Some(s)) => s,
    };
    let b = match s.to_ascii_lowercase().as_str() {
      "true" | "1" | "yes" => true,
      "false" | "0" | "no" => false,
      _ => {
        return Err(E::from(ConfigError::Invalid {
          path: path_str,
          value: s,
          reason: "expected boolean string".into(),
        }));
      }
    };
    A::from(b)
  })
}

/// Sequence of strings split by [`ConfigProvider::seq_delim`].
pub fn read_string_list<A, E, R>(path: &[&str]) -> Effect<A, E, R>
where
  A: From<Vec<String>> + 'static,
  E: From<ConfigError> + 'static,
  R: NeedsConfigProvider + 'static,
{
  let path_owned: Vec<String> = path.iter().map(|s| s.to_string()).collect();
  effect!(|r: &mut R| {
    let provider = Get::<ConfigProviderKey, Here>::get(r);
    let refs: Vec<&str> = path_owned.iter().map(String::as_str).collect();
    let path_str = refs.join(".");
    let s = match provider.0.load_raw(&refs) {
      Err(e) => return Err(E::from(e)),
      Ok(None) => return Err(E::from(ConfigError::Missing { path: path_str })),
      Ok(Some(s)) => s,
    };
    let delim = provider.0.seq_delim();
    let list: Vec<String> = s
      .split(delim)
      .map(str::trim)
      .filter(|x| !x.is_empty())
      .map(str::to_string)
      .collect();
    A::from(list)
  })
}

// ── nested convenience ────────────────────────────────────────────────────────

/// [`nested_path`] then [`read_string`].
pub fn read_nested_string<A, E, R>(namespace: &str, leaf: &[&str]) -> Effect<A, E, R>
where
  A: From<String> + 'static,
  E: From<ConfigError> + 'static,
  R: NeedsConfigProvider + 'static,
{
  let path_owned = nested_path(namespace, leaf);
  effect!(|r: &mut R| {
    let provider = Get::<ConfigProviderKey, Here>::get(r);
    let refs: Vec<&str> = path_owned.iter().map(String::as_str).collect();
    let path_str = refs.join(".");
    match provider.0.load_raw(&refs) {
      Err(e) => return Err(E::from(e)),
      Ok(None) => return Err(E::from(ConfigError::Missing { path: path_str })),
      Ok(Some(s)) => A::from(s),
    }
  })
}

/// [`nested_path`] then [`read_string_list`].
pub fn read_nested_string_list<A, E, R>(namespace: &str, leaf: &[&str]) -> Effect<A, E, R>
where
  A: From<Vec<String>> + 'static,
  E: From<ConfigError> + 'static,
  R: NeedsConfigProvider + 'static,
{
  let path_owned = nested_path(namespace, leaf);
  effect!(|r: &mut R| {
    let provider = Get::<ConfigProviderKey, Here>::get(r);
    let refs: Vec<&str> = path_owned.iter().map(String::as_str).collect();
    let path_str = refs.join(".");
    let s = match provider.0.load_raw(&refs) {
      Err(e) => return Err(E::from(e)),
      Ok(None) => return Err(E::from(ConfigError::Missing { path: path_str })),
      Ok(Some(s)) => s,
    };
    let delim = provider.0.seq_delim();
    let list: Vec<String> = s
      .split(delim)
      .map(str::trim)
      .filter(|x| !x.is_empty())
      .map(str::to_string)
      .collect();
    A::from(list)
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::config_env;
  use crate::ConfigEnv;
  use crate::MapConfigProvider;
  use crate::ProviderOptions;
  use ::effect::run_blocking;

  fn env_map(pairs: &[(&str, &str)]) -> ConfigEnv {
    config_env(MapConfigProvider::from_pairs(
      pairs.iter().copied().map(|(k, v)| (k, v)),
    ))
  }

  #[test]
  fn nested_path_builds_segments() {
    assert_eq!(
      nested_path("SERVER", &["HOST", "PORT"]),
      vec!["SERVER", "HOST", "PORT"]
    );
  }

  #[test]
  fn read_string_ok_and_missing() {
    let v: String = run_blocking(
      read_string::<String, ConfigError, _>(&["K"]),
      env_map(&[("K", "hello")]),
    )
    .unwrap();
    assert_eq!(v, "hello");

    let err = run_blocking(read_string::<String, ConfigError, _>(&["MISSING"]), env_map(&[]))
      .unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));
  }

  #[test]
  fn read_string_opt_some_and_none() {
    let v: Option<String> = run_blocking(
      read_string_opt::<Option<String>, ConfigError, _>(&["K"]),
      env_map(&[("K", "x")]),
    )
    .unwrap();
    assert_eq!(v, Some("x".into()));

    let none: Option<String> = run_blocking(
      read_string_opt::<Option<String>, ConfigError, _>(&["MISSING"]),
      env_map(&[]),
    )
    .unwrap();
    assert_eq!(none, None);
  }

  #[test]
  fn read_number_ok_missing_invalid() {
    let n: f64 = run_blocking(
      read_number::<f64, ConfigError, _>(&["N"]),
      env_map(&[("N", "3.5")]),
    )
    .unwrap();
    assert!((n - 3.5).abs() < f64::EPSILON);

    let err = run_blocking(read_number::<f64, ConfigError, _>(&["MISSING"]), env_map(&[]))
      .unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));

    let err = run_blocking(
      read_number::<f64, ConfigError, _>(&["N"]),
      env_map(&[("N", "not-a-number")]),
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  #[test]
  fn read_i64_ok_missing_invalid() {
    let n: i64 =
      run_blocking(read_i64::<i64, ConfigError, _>(&["N"]), env_map(&[("N", "-42")])).unwrap();
    assert_eq!(n, -42);

    let err = run_blocking(read_i64::<i64, ConfigError, _>(&["MISSING"]), env_map(&[]))
      .unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));

    let err = run_blocking(
      read_i64::<i64, ConfigError, _>(&["N"]),
      env_map(&[("N", "1.2")]),
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  #[test]
  fn read_bool_variants_and_invalid() {
    for (raw, expected) in [
      ("true", true),
      ("TRUE", true),
      ("1", true),
      ("yes", true),
      ("false", false),
      ("0", false),
      ("no", false),
    ] {
      let b: bool = run_blocking(
        read_bool::<bool, ConfigError, _>(&["B"]),
        env_map(&[("B", raw)]),
      )
      .unwrap();
      assert_eq!(b, expected, "raw={raw}");
    }

    let err = run_blocking(read_bool::<bool, ConfigError, _>(&["MISSING"]), env_map(&[]))
      .unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));

    let err = run_blocking(
      read_bool::<bool, ConfigError, _>(&["B"]),
      env_map(&[("B", "maybe")]),
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  #[test]
  fn read_string_list_splits_on_seq_delim() {
    let mut m = std::collections::HashMap::new();
    m.insert("TAGS".into(), "a, b ,c".into());
    let p = MapConfigProvider::with_options(
      m,
      ProviderOptions {
        path_delim: "_",
        seq_delim: ",",
      },
    );
    let env = config_env(p);
    let tags: Vec<String> =
      run_blocking(read_string_list::<Vec<String>, ConfigError, _>(&["TAGS"]), env).unwrap();
    assert_eq!(tags, vec!["a", "b", "c"]);
  }

  #[test]
  fn read_nested_string_and_list() {
    let v: String = run_blocking(
      read_nested_string::<String, ConfigError, _>("SERVER", &["HOST"]),
      env_map(&[("SERVER_HOST", "z")]),
    )
    .unwrap();
    assert_eq!(v, "z");

    let list: Vec<String> = run_blocking(
      read_nested_string_list::<Vec<String>, ConfigError, _>("APP", &["IDS"]),
      env_map(&[("APP_IDS", "1,2")]),
    )
    .unwrap();
    assert_eq!(list, vec!["1", "2"]);
  }

  #[test]
  fn read_with_empty_path_is_invalid() {
    let err = run_blocking(
      read_string::<String, ConfigError, _>(&[]),
      env_map(&[]),
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  #[test]
  fn with_default_replaces_only_missing() {
    let def: String = run_blocking(
      read_string::<String, ConfigError, _>(&["K"]).with_default("default".into()),
      env_map(&[]),
    )
    .unwrap();
    assert_eq!(def, "default");

    let v: String = run_blocking(
      read_string::<String, ConfigError, _>(&["K"]).with_default("default".into()),
      env_map(&[("K", "real")]),
    )
    .unwrap();
    assert_eq!(v, "real");

    let err = run_blocking(
      read_number::<f64, ConfigError, _>(&["N"]).with_default(0.0),
      env_map(&[("N", "bad")]),
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }
}
