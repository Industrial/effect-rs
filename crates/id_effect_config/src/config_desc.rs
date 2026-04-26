//! Effect.ts-style `Config<T>` descriptor — a lazy, composable configuration description.
//!
//! Unlike the low-level [`crate::load`] functions, a [`Config<T>`] is a *description* of what to
//! load.  Compose it with [`Config::with_default`], [`Config::map`], [`all`], [`zip_with`], etc.,
//! then evaluate with [`Config::run`] (service-injected) or [`Config::load`] (direct provider).
//!
//! # Path conventions
//!
//! Path strings use `.` as the segment separator: `"server.host"` splits into
//! `["server", "host"]` which each [`crate::ConfigProvider`] then joins using its own
//! key delimiter (`_` for env, `.` for figment).
//!
//! # Example
//!
//! ```rust
//! use std::sync::Arc;
//! use id_effect_config::{Config, MapConfigProvider, config_env, ConfigError};
//! use id_effect::run_blocking;
//!
//! let p = MapConfigProvider::from_pairs([
//!   ("HOST", "localhost"),
//!   ("PORT", "8080"),
//! ]);
//! let env = config_env(p);
//!
//! let host: String = run_blocking(
//!   Config::string("HOST").run::<String, ConfigError, _>(),
//!   env.clone(),
//! )
//! .unwrap();
//! let port: i64 = run_blocking(
//!   Config::integer("PORT").with_default(3000).run::<i64, ConfigError, _>(),
//!   env.clone(),
//! )
//! .unwrap();
//! assert_eq!(host, "localhost");
//! assert_eq!(port, 8080);
//! ```

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use ::id_effect::{Effect, Get, Here, effect};
use id_effect::duration::duration;
use id_effect_logger::LogLevel;
use rayon::prelude::*;
use url::Url;

use crate::ambient::current_config_provider;
use crate::error::ConfigError;
use crate::provider::{ConfigProvider, ConfigProviderKey, NeedsConfigProvider};
use crate::secret::Secret;

// Shared loading function: takes a provider reference, returns a Result.
type LoadFn<T> = Arc<dyn Fn(&dyn ConfigProvider) -> Result<T, ConfigError> + Send + Sync>;

// ── Internal adapter: prepends fixed segments to every path lookup ────────────

struct PrefixProvider<'a> {
  inner: &'a dyn ConfigProvider,
  prefix: Arc<Vec<String>>,
}

impl ConfigProvider for PrefixProvider<'_> {
  fn load_raw(&self, path: &[&str]) -> Result<Option<String>, ConfigError> {
    let mut full: Vec<String> = self.prefix.iter().cloned().collect();
    full.extend(path.iter().map(|s| (*s).to_string()));
    let full_refs: Vec<&str> = full.iter().map(String::as_str).collect();
    self.inner.load_raw(&full_refs)
  }

  fn seq_delim(&self) -> &'static str {
    self.inner.seq_delim()
  }
}

// ── Config<T> ─────────────────────────────────────────────────────────────────

/// A lazy, composable configuration descriptor (Effect.ts `Config<T>`).
///
/// Create with type-specific constructors ([`Config::string`], [`Config::integer`], …),
/// compose with combinators ([`Config::with_default`], [`Config::map`], [`Config::nested`], …),
/// and evaluate with [`Config::run`] (service-injected) or [`Config::load`] (direct provider).
pub struct Config<T: 'static> {
  loader: LoadFn<T>,
}

impl<T: 'static> fmt::Debug for Config<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Config<{}>", std::any::type_name::<T>())
  }
}

impl<T: 'static> Clone for Config<T> {
  fn clone(&self) -> Self {
    Self {
      loader: self.loader.clone(),
    }
  }
}

// ── Core impl (no Clone bound on T) ──────────────────────────────────────────

impl<T: Send + Sync + 'static> Config<T> {
  fn new(
    f: impl Fn(&dyn ConfigProvider) -> Result<T, ConfigError> + Send + Sync + 'static,
  ) -> Self {
    Self {
      loader: Arc::new(f),
    }
  }

  /// Evaluate against `provider` directly (synchronous; useful for tests).
  #[inline]
  pub fn load(&self, provider: &dyn ConfigProvider) -> Result<T, ConfigError> {
    (self.loader)(provider)
  }

  /// Evaluate this descriptor as an [`Effect`], pulling the provider from the environment.
  ///
  /// `R` only needs to satisfy `NeedsConfigProvider`; callers compose whatever layer stack
  /// they like.  See [`config_env`](crate::config_env) for building a minimal context.
  pub fn run<A, E, R>(&self) -> Effect<A, E, R>
  where
    A: From<T> + 'static,
    E: From<ConfigError> + 'static,
    R: NeedsConfigProvider + 'static,
  {
    let loader = self.loader.clone();
    effect!(|r: &mut R| {
      let service = Get::<ConfigProviderKey, Here>::get(r);
      let t = (loader)(service.0.as_ref()).map_err(E::from)?;
      A::from(t)
    })
  }

  /// Like [`load`](Self::load), using the innermost ambient provider from
  /// [`with_config_provider`](crate::with_config_provider).
  ///
  /// Runs eagerly when called (reads the thread-local immediately).
  #[inline]
  pub fn load_current(&self) -> Result<T, ConfigError> {
    match current_config_provider() {
      Some(p) => (self.loader)(p.as_ref()),
      None => Err(ConfigError::Missing {
        path: "<ambient ConfigProvider>".into(),
      }),
    }
  }

  /// Transform the loaded value (Effect `Config.map`).
  ///
  /// ```rust
  /// use id_effect_config::{Config, MapConfigProvider};
  ///
  /// let p = MapConfigProvider::from_pairs([("PORT", "8080")]);
  /// let port_u16: u16 = Config::integer("PORT").map(|n| n as u16).load(&p).unwrap();
  /// assert_eq!(port_u16, 8080u16);
  /// ```
  pub fn map<U: Send + Sync + 'static>(
    self,
    f: impl Fn(T) -> U + Send + Sync + 'static,
  ) -> Config<U> {
    let loader = self.loader;
    let f = Arc::new(f);
    Config {
      loader: Arc::new(move |p| {
        let t = (loader)(p)?;
        Ok(f(t))
      }),
    }
  }

  /// Transform the loaded value, allowing failure (Effect `Config.mapAttempt`).
  pub fn map_attempt<U: Send + Sync + 'static>(
    self,
    f: impl Fn(T) -> Result<U, ConfigError> + Send + Sync + 'static,
  ) -> Config<U> {
    let loader = self.loader;
    let f = Arc::new(f);
    Config {
      loader: Arc::new(move |p| {
        let t = (loader)(p)?;
        f(t)
      }),
    }
  }

  /// Wrap the loaded value in [`Secret`] so it is never printed (Effect `Config.secret`).
  ///
  /// ```rust
  /// use id_effect_config::{Config, MapConfigProvider};
  ///
  /// let p = MapConfigProvider::from_pairs([("API_KEY", "s3cr3t")]);
  /// let key = Config::string("API_KEY").secret().load(&p).unwrap();
  /// assert_eq!(format!("{key:?}"), "<redacted>");
  /// assert_eq!(key.expose(), "s3cr3t");
  /// ```
  pub fn secret(self) -> Config<Secret<T>> {
    let loader = self.loader;
    Config {
      loader: Arc::new(move |p| Ok(Secret::new((loader)(p)?))),
    }
  }

  /// Scope all path lookups under `prefix` segments (Effect `Config.nested`).
  ///
  /// `Config::string("PORT").nested("SERVER")` looks up `["SERVER", "PORT"]`.
  ///
  /// ```rust
  /// use id_effect_config::{Config, MapConfigProvider};
  ///
  /// let p = MapConfigProvider::from_pairs([("SERVER_HOST", "127.0.0.1")]);
  /// let host = Config::string("HOST").nested("SERVER").load(&p).unwrap();
  /// assert_eq!(host, "127.0.0.1");
  /// ```
  pub fn nested(self, prefix: impl Into<String>) -> Self {
    let prefix_segs: Arc<Vec<String>> = Arc::new(
      prefix
        .into()
        .split('.')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect(),
    );
    let inner = self.loader;
    Self {
      loader: Arc::new(move |p| {
        let scoped = PrefixProvider {
          inner: p,
          prefix: prefix_segs.clone(),
        };
        inner(&scoped)
      }),
    }
  }
}

// ── Combinators that need T: Clone ────────────────────────────────────────────

impl<T: Clone + Send + Sync + 'static> Config<T> {
  /// Fall back to `default` when the key is missing (Effect `Config.withDefault`).
  ///
  /// Only [`ConfigError::Missing`] triggers the fallback; parse errors still propagate.
  ///
  /// ```rust
  /// use id_effect_config::{Config, MapConfigProvider};
  ///
  /// let p = MapConfigProvider::from_pairs::<[(&str, &str); 0], _, _>([]);
  /// let port = Config::integer("PORT").with_default(3000).load(&p).unwrap();
  /// assert_eq!(port, 3000);
  /// ```
  pub fn with_default(self, default: T) -> Self {
    let loader = self.loader;
    Self {
      loader: Arc::new(move |p| match (loader)(p) {
        Ok(v) => Ok(v),
        Err(ConfigError::Missing { .. }) => Ok(default.clone()),
        Err(e) => Err(e),
      }),
    }
  }

  /// Validate the loaded value; produces `ConfigError::Invalid` when `predicate` is false.
  ///
  /// ```rust
  /// use id_effect_config::{Config, MapConfigProvider};
  ///
  /// let p = MapConfigProvider::from_pairs([("PORT", "99999")]);
  /// let err = Config::integer("PORT")
  ///   .validate("port must be 1–65535", |n| (1..=65535).contains(n))
  ///   .load(&p)
  ///   .unwrap_err();
  /// assert!(err.to_string().contains("port must be 1–65535"));
  /// ```
  pub fn validate(
    self,
    reason: &'static str,
    predicate: impl Fn(&T) -> bool + Send + Sync + 'static,
  ) -> Self {
    let loader = self.loader;
    let predicate = Arc::new(predicate);
    Self {
      loader: Arc::new(move |p| {
        let v = (loader)(p)?;
        if predicate(&v) {
          Ok(v)
        } else {
          Err(ConfigError::Invalid {
            path: String::new(),
            value: String::new(),
            reason: reason.to_string(),
          })
        }
      }),
    }
  }
}

// ── Type-specific constructors ────────────────────────────────────────────────

fn split_dotted(path: &str) -> Arc<Vec<String>> {
  Arc::new(
    path
      .split('.')
      .filter(|s| !s.is_empty())
      .map(String::from)
      .collect(),
  )
}

impl Config<String> {
  /// Load a required string (Effect `Config.string`).
  ///
  /// `path` may be dotted to express nesting: `"server.host"` → `["server", "host"]`.
  pub fn string(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      match p.load_raw(&refs)? {
        None => Err(ConfigError::Missing { path: path_str }),
        Some(s) => Ok(s),
      }
    })
  }
}

impl Config<Option<String>> {
  /// Load an optional string; a missing key yields `None` (Effect `Config.option`).
  pub fn optional_string(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      p.load_raw(&refs)
    })
  }
}

impl Config<f64> {
  /// Load a floating-point number (Effect `Config.number`).
  pub fn number(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      let s = match p.load_raw(&refs)? {
        None => {
          return Err(ConfigError::Missing {
            path: path_str.clone(),
          });
        }
        Some(s) => s,
      };
      s.parse::<f64>().map_err(|e| ConfigError::Invalid {
        path: path_str,
        value: s,
        reason: e.to_string(),
      })
    })
  }
}

impl Config<i64> {
  /// Load a signed integer (Effect `Config.integer`).
  pub fn integer(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      let s = match p.load_raw(&refs)? {
        None => {
          return Err(ConfigError::Missing {
            path: path_str.clone(),
          });
        }
        Some(s) => s,
      };
      s.parse::<i64>().map_err(|e| ConfigError::Invalid {
        path: path_str,
        value: s,
        reason: e.to_string(),
      })
    })
  }
}

impl Config<bool> {
  /// Load a boolean (Effect `Config.boolean`).
  pub fn boolean(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      let s = match p.load_raw(&refs)? {
        None => {
          return Err(ConfigError::Missing {
            path: path_str.clone(),
          });
        }
        Some(s) => s,
      };
      match s.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        _ => Err(ConfigError::Invalid {
          path: path_str,
          value: s,
          reason: "expected boolean string".into(),
        }),
      }
    })
  }
}

impl Config<Vec<String>> {
  /// Load a delimiter-separated string list (Effect `Config.array(Config.string(…))`).
  pub fn string_list(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      let s = match p.load_raw(&refs)? {
        None => return Err(ConfigError::Missing { path: path_str }),
        Some(s) => s,
      };
      let delim = p.seq_delim();
      Ok(
        s.split(delim)
          .map(str::trim)
          .filter(|x| !x.is_empty())
          .map(str::to_string)
          .collect(),
      )
    })
  }

  /// CSV list (comma-separated, trimmed); ignores [`ConfigProvider::seq_delim`] (Effect `Config.repeat`).
  pub fn repeat(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      let s = match p.load_raw(&refs)? {
        None => return Err(ConfigError::Missing { path: path_str }),
        Some(s) => s,
      };
      Ok(split_csv_list(&s))
    })
  }
}

fn split_csv_list(s: &str) -> Vec<String> {
  s.split(',')
    .map(str::trim)
    .filter(|x| !x.is_empty())
    .map(str::to_string)
    .collect()
}

impl Config<Duration> {
  /// Load a [`Duration`] using [`id_effect::duration::duration::decode`] (Effect.ts `Duration` decode).
  ///
  /// ```rust
  /// use std::time::Duration;
  /// use id_effect_config::{Config, MapConfigProvider};
  ///
  /// let p = MapConfigProvider::from_pairs([("TIMEOUT", "5s")]);
  /// let t = Config::duration("TIMEOUT").load(&p).unwrap();
  /// assert_eq!(t, Duration::from_secs(5));
  /// ```
  pub fn duration(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      let s = match p.load_raw(&refs)? {
        None => {
          return Err(ConfigError::Missing {
            path: path_str.clone(),
          });
        }
        Some(s) => s,
      };
      duration::decode(s.trim()).map_err(|e| ConfigError::Invalid {
        path: path_str,
        value: e.input.clone(),
        reason: e.to_string(),
      })
    })
  }
}

impl Config<LogLevel> {
  /// Load a [`LogLevel`] (case-insensitive; see [`LogLevel`](id_effect_logger::LogLevel)).
  pub fn log_level(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      let s = match p.load_raw(&refs)? {
        None => {
          return Err(ConfigError::Missing {
            path: path_str.clone(),
          });
        }
        Some(s) => s,
      };
      LogLevel::from_str(s.trim()).map_err(|reason| ConfigError::Invalid {
        path: path_str,
        value: s,
        reason,
      })
    })
  }
}

impl Config<Url> {
  /// Load a [`Url`] string.
  pub fn url(path: impl Into<String>) -> Self {
    let segs = split_dotted(&path.into());
    Self::new(move |p| {
      let refs: Vec<&str> = segs.iter().map(String::as_str).collect();
      let path_str = refs.join(".");
      let s = match p.load_raw(&refs)? {
        None => {
          return Err(ConfigError::Missing {
            path: path_str.clone(),
          });
        }
        Some(s) => s,
      };
      s.parse::<Url>().map_err(|e| ConfigError::Invalid {
        path: path_str,
        value: s,
        reason: e.to_string(),
      })
    })
  }
}

// ── Free-function combinators ─────────────────────────────────────────────────

/// Combine two configs into a tuple (Effect `Config.all` / `Config.zip`).
///
/// Both are loaded; the first error encountered is returned.
///
/// ```rust
/// use id_effect_config::{config, Config, MapConfigProvider};
///
/// let p = MapConfigProvider::from_pairs([("HOST", "0.0.0.0"), ("PORT", "9000")]);
/// let (host, port) = config::all(Config::string("HOST"), Config::integer("PORT"))
///   .load(&p)
///   .unwrap();
/// assert_eq!(host, "0.0.0.0");
/// assert_eq!(port, 9000);
/// ```
pub fn all<A, B>(a: Config<A>, b: Config<B>) -> Config<(A, B)>
where
  A: Send + Sync + 'static,
  B: Send + Sync + 'static,
{
  Config {
    loader: Arc::new(move |p| {
      let av = (a.loader)(p)?;
      let bv = (b.loader)(p)?;
      Ok((av, bv))
    }),
  }
}

/// Combine three configs into a 3-tuple.
pub fn all3<A, B, C>(a: Config<A>, b: Config<B>, c: Config<C>) -> Config<(A, B, C)>
where
  A: Send + Sync + 'static,
  B: Send + Sync + 'static,
  C: Send + Sync + 'static,
{
  Config {
    loader: Arc::new(move |p| {
      let av = (a.loader)(p)?;
      let bv = (b.loader)(p)?;
      let cv = (c.loader)(p)?;
      Ok((av, bv, cv))
    }),
  }
}

/// Combine two configs and merge the results with `f` (Effect `Config.zipWith`).
pub fn zip_with<A, B, C>(
  a: Config<A>,
  b: Config<B>,
  f: impl Fn(A, B) -> C + Send + Sync + 'static,
) -> Config<C>
where
  A: Send + Sync + 'static,
  B: Send + Sync + 'static,
  C: Send + Sync + 'static,
{
  let f = Arc::new(f);
  Config {
    loader: Arc::new(move |p| {
      let av = (a.loader)(p)?;
      let bv = (b.loader)(p)?;
      Ok(f(av, bv))
    }),
  }
}

/// Try `primary`; fall back to `fallback` on a missing key (Effect `Config.orElse`).
pub fn or_else<T: Send + Sync + 'static>(primary: Config<T>, fallback: Config<T>) -> Config<T> {
  Config {
    loader: Arc::new(move |p| match (primary.loader)(p) {
      Ok(v) => Ok(v),
      Err(ConfigError::Missing { .. }) => (fallback.loader)(p),
      Err(e) => Err(e),
    }),
  }
}

/// Load multiple configs and collect results into a `Vec` (Effect `Config.all` on a slice).
pub fn all_vec<T: Send + Sync + 'static>(configs: Vec<Config<T>>) -> Config<Vec<T>> {
  let configs = Arc::new(configs);
  Config {
    loader: Arc::new(move |p| configs.iter().map(|c| (c.loader)(p)).collect()),
  }
}

/// Like [`all_vec`], but evaluates each descriptor in parallel with rayon.
///
/// Order and error semantics match [`all_vec`]: results align with `configs`; the first load error
/// is returned (after parallel evaluation, errors are folded in slice order).
pub fn all_vec_par<T: Send + Sync + 'static>(configs: Vec<Config<T>>) -> Config<Vec<T>> {
  let configs = Arc::new(configs);
  Config {
    loader: Arc::new(move |p| {
      configs
        .par_iter()
        .map(|c| (c.loader)(p))
        .collect::<Vec<Result<T, ConfigError>>>()
        .into_iter()
        .collect()
    }),
  }
}

/// Namespace `inner` under `prefix` (same as [`Config::nested`]).
#[inline]
pub fn nest<T: Send + Sync + 'static>(prefix: impl Into<String>, inner: Config<T>) -> Config<T> {
  inner.nested(prefix)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use crate::MapConfigProvider;
  use id_effect_logger::LogLevel;

  use super::*;

  fn map_provider(pairs: &[(&str, &str)]) -> MapConfigProvider {
    MapConfigProvider::from_pairs(pairs.iter().copied())
  }

  fn go<T: Send + Sync + 'static>(c: Config<T>, p: &dyn ConfigProvider) -> Result<T, ConfigError> {
    c.load(p)
  }

  #[test]
  fn string_required_found() {
    let p = map_provider(&[("HOST", "localhost")]);
    assert_eq!(go(Config::string("HOST"), &p).unwrap(), "localhost");
  }

  #[test]
  fn string_required_missing_is_error() {
    let p = map_provider(&[]);
    assert!(go(Config::string("HOST"), &p).is_err());
  }

  #[test]
  fn with_default_on_missing() {
    let p = map_provider(&[]);
    let port = go(Config::integer("PORT").with_default(3000), &p).unwrap();
    assert_eq!(port, 3000);
  }

  #[test]
  fn with_default_not_used_when_present() {
    let p = map_provider(&[("PORT", "8080")]);
    let port = go(Config::integer("PORT").with_default(3000), &p).unwrap();
    assert_eq!(port, 8080);
  }

  #[test]
  fn map_transforms_value() {
    let p = map_provider(&[("PORT", "8080")]);
    let port_u16: u16 = go(Config::integer("PORT").map(|n| n as u16), &p).unwrap();
    assert_eq!(port_u16, 8080);
  }

  #[test]
  fn map_attempt_success() {
    let p = map_provider(&[("VAL", "42")]);
    let v = go(
      Config::string("VAL").map_attempt(|s| {
        s.parse::<i32>().map_err(|e| ConfigError::Invalid {
          path: "VAL".into(),
          value: s,
          reason: e.to_string(),
        })
      }),
      &p,
    )
    .unwrap();
    assert_eq!(v, 42i32);
  }

  #[test]
  fn validate_passes() {
    let p = map_provider(&[("PORT", "8080")]);
    let port = go(
      Config::integer("PORT").validate("must be > 0", |n| *n > 0),
      &p,
    )
    .unwrap();
    assert_eq!(port, 8080);
  }

  #[test]
  fn validate_fails() {
    let p = map_provider(&[("PORT", "99999")]);
    let err = go(
      Config::integer("PORT").validate("port must be 1–65535", |n| (1..=65535).contains(n)),
      &p,
    )
    .unwrap_err();
    assert!(err.to_string().contains("port must be 1–65535"));
  }

  #[test]
  fn secret_wraps_and_redacts() {
    let p = map_provider(&[("KEY", "s3cr3t")]);
    let k = go(Config::string("KEY").secret(), &p).unwrap();
    assert_eq!(format!("{k:?}"), "<redacted>");
    assert_eq!(k.expose(), "s3cr3t");
  }

  #[test]
  fn nested_prepends_prefix() {
    let p = map_provider(&[("SERVER_HOST", "0.0.0.0")]);
    let host = go(Config::string("HOST").nested("SERVER"), &p).unwrap();
    assert_eq!(host, "0.0.0.0");
  }

  #[test]
  fn all_combines_two() {
    let p = map_provider(&[("HOST", "::1"), ("PORT", "9000")]);
    let (host, port) = go(all(Config::string("HOST"), Config::integer("PORT")), &p).unwrap();
    assert_eq!(host, "::1");
    assert_eq!(port, 9000);
  }

  #[test]
  fn all3_combines_three() {
    let p = map_provider(&[("A", "1"), ("B", "2"), ("C", "3")]);
    let (a, b, c) = go(
      all3(
        Config::integer("A"),
        Config::integer("B"),
        Config::integer("C"),
      ),
      &p,
    )
    .unwrap();
    assert_eq!((a, b, c), (1, 2, 3));
  }

  #[test]
  fn zip_with_merges() {
    let p = map_provider(&[("HOST", "localhost"), ("PORT", "8080")]);
    let addr = go(
      zip_with(Config::string("HOST"), Config::integer("PORT"), |h, po| {
        format!("{h}:{po}")
      }),
      &p,
    )
    .unwrap();
    assert_eq!(addr, "localhost:8080");
  }

  #[test]
  fn or_else_falls_back_on_missing() {
    let p = map_provider(&[("FALLBACK_PORT", "4000")]);
    let port = go(
      or_else(Config::integer("PORT"), Config::integer("FALLBACK_PORT")),
      &p,
    )
    .unwrap();
    assert_eq!(port, 4000);
  }

  #[test]
  fn or_else_uses_primary_when_present() {
    let p = map_provider(&[("PORT", "8080"), ("FALLBACK_PORT", "4000")]);
    let port = go(
      or_else(Config::integer("PORT"), Config::integer("FALLBACK_PORT")),
      &p,
    )
    .unwrap();
    assert_eq!(port, 8080);
  }

  #[test]
  fn all_vec_collects() {
    let p = map_provider(&[("K0", "10"), ("K1", "20"), ("K2", "30")]);
    let vals = go(
      all_vec(vec![
        Config::integer("K0"),
        Config::integer("K1"),
        Config::integer("K2"),
      ]),
      &p,
    )
    .unwrap();
    assert_eq!(vals, vec![10, 20, 30]);
  }

  #[test]
  fn all_vec_par_matches_all_vec_success() {
    let p = map_provider(&[("K0", "10"), ("K1", "20"), ("K2", "30")]);
    let seq = go(
      all_vec(vec![
        Config::integer("K0"),
        Config::integer("K1"),
        Config::integer("K2"),
      ]),
      &p,
    )
    .unwrap();
    let par = go(
      all_vec_par(vec![
        Config::integer("K0"),
        Config::integer("K1"),
        Config::integer("K2"),
      ]),
      &p,
    )
    .unwrap();
    assert_eq!(seq, par);
  }

  #[test]
  fn all_vec_par_matches_all_vec_invalid_first_key() {
    let p = map_provider(&[("K0", "not-int"), ("K1", "20")]);
    let seq = go(
      all_vec(vec![Config::integer("K0"), Config::integer("K1")]),
      &p,
    );
    let par = go(
      all_vec_par(vec![Config::integer("K0"), Config::integer("K1")]),
      &p,
    );
    assert!(matches!(seq, Err(ConfigError::Invalid { .. })));
    assert!(matches!(par, Err(ConfigError::Invalid { .. })));
    assert_eq!(format!("{seq:?}"), format!("{par:?}"));
  }

  #[test]
  fn boolean_true_and_false() {
    let p = map_provider(&[("A", "true"), ("B", "false"), ("C", "1"), ("D", "0")]);
    assert!(go(Config::boolean("A"), &p).unwrap());
    assert!(!go(Config::boolean("B"), &p).unwrap());
    assert!(go(Config::boolean("C"), &p).unwrap());
    assert!(!go(Config::boolean("D"), &p).unwrap());
  }

  #[test]
  fn string_list_splits() {
    let p = map_provider(&[("TAGS", "a,b,c")]);
    let tags = go(Config::string_list("TAGS"), &p).unwrap();
    assert_eq!(tags, vec!["a", "b", "c"]);
  }

  mod duration {
    use super::*;

    fn dur(pairs: &[(&str, &str)], key: &'static str) -> Result<Duration, ConfigError> {
      let p = map_provider(pairs);
      go(Config::duration(key), &p)
    }

    #[test]
    fn bare_integer_is_millis() {
      assert_eq!(
        dur(&[("T", "500")], "T").unwrap(),
        Duration::from_millis(500)
      );
    }

    #[test]
    fn ms_suffix() {
      assert_eq!(
        dur(&[("T", "250ms")], "T").unwrap(),
        Duration::from_millis(250)
      );
    }

    #[test]
    fn seconds_suffix() {
      assert_eq!(dur(&[("T", "5s")], "T").unwrap(), Duration::from_secs(5));
    }

    #[test]
    fn minutes_suffix() {
      assert_eq!(
        dur(&[("T", "2min")], "T").unwrap(),
        Duration::from_secs(120)
      );
    }

    #[test]
    fn hours_suffix() {
      assert_eq!(dur(&[("T", "1h")], "T").unwrap(), Duration::from_secs(3600));
    }

    #[test]
    fn days_suffix() {
      assert_eq!(
        dur(&[("T", "1d")], "T").unwrap(),
        Duration::from_secs(86400)
      );
    }

    #[test]
    fn unknown_unit_is_error() {
      assert!(dur(&[("T", "5x")], "T").is_err());
    }
  }

  #[test]
  fn config_duration_parses_30_seconds() {
    let p = map_provider(&[("D", "30 seconds")]);
    assert_eq!(
      go(Config::duration("D"), &p).unwrap(),
      Duration::from_secs(30)
    );
  }

  #[test]
  fn config_log_level_parses_info() {
    let p = map_provider(&[("LVL", "info")]);
    assert_eq!(go(Config::log_level("LVL"), &p).unwrap(), LogLevel::Info);
  }

  #[test]
  fn config_url_parses_valid_url() {
    let p = map_provider(&[("U", "https://example.com/path?x=1")]);
    let u = go(Config::url("U"), &p).unwrap();
    assert_eq!(u.as_str(), "https://example.com/path?x=1");
  }

  #[test]
  fn config_nested_prefixes_keys() {
    let p = map_provider(&[("APP_SERVER_HOST", "h")]);
    let host = go(nest("APP_SERVER", Config::string("HOST")), &p).unwrap();
    assert_eq!(host, "h");
  }

  #[test]
  fn config_repeat_splits_csv() {
    let p = map_provider(&[("ITEMS", "a, b ,c")]);
    let v = go(Config::repeat("ITEMS"), &p).unwrap();
    assert_eq!(v, vec!["a", "b", "c"]);
  }

  // ── Debug / Clone ──────────────────────────────────────────────────────────

  #[test]
  fn config_debug_contains_type_name() {
    let c = Config::<String>::string("X");
    let s = format!("{c:?}");
    assert!(s.contains("Config"));
  }

  #[test]
  fn config_clone_independent_from_original() {
    let p = map_provider(&[("X", "hello")]);
    let c = Config::string("X");
    let c2 = c.clone();
    assert_eq!(go(c2, &p).unwrap(), "hello");
  }

  // ── Config::run ────────────────────────────────────────────────────────────

  #[test]
  fn config_run_via_effect() {
    use crate::{MapConfigProvider, config_env};

    let p = MapConfigProvider::from_pairs([("HOST", "localhost")]);
    let env = config_env(p);
    let host: String =
      id_effect::run_blocking(Config::string("HOST").run::<String, ConfigError, _>(), env).unwrap();
    assert_eq!(host, "localhost");
  }

  #[test]
  fn config_run_error_propagated() {
    use crate::{MapConfigProvider, config_env};

    let p = MapConfigProvider::from_pairs::<[(&str, &str); 0], _, _>([]);
    let env = config_env(p);
    let result: Result<String, ConfigError> = id_effect::run_blocking(
      Config::string("MISSING").run::<String, ConfigError, _>(),
      env,
    );
    assert!(result.is_err());
  }

  // ── Config::load_current ───────────────────────────────────────────────────

  #[test]
  fn load_current_no_ambient_returns_missing() {
    let err = Config::<String>::string("X").load_current().unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));
  }

  #[test]
  fn load_current_with_ambient_provider() {
    use crate::ambient::with_config_provider;
    use std::sync::Arc;

    let p = Arc::new(map_provider(&[("X", "from-ambient")]));
    let eff = with_config_provider(
      id_effect::Effect::new(|_| Config::string("X").load_current()),
      p,
    );
    assert_eq!(id_effect::run_blocking(eff, ()).unwrap(), "from-ambient");
  }

  // ── with_default non-missing error propagation ─────────────────────────────

  #[test]
  fn with_default_propagates_invalid_error() {
    let p = map_provider(&[("PORT", "not_a_number")]);
    let err = go(Config::integer("PORT").with_default(3000), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  // ── optional_string ────────────────────────────────────────────────────────

  #[test]
  fn optional_string_present() {
    let p = map_provider(&[("OPT", "value")]);
    assert_eq!(
      go(Config::optional_string("OPT"), &p).unwrap(),
      Some("value".to_string())
    );
  }

  #[test]
  fn optional_string_absent() {
    let p = map_provider(&[]);
    assert_eq!(go(Config::optional_string("OPT"), &p).unwrap(), None);
  }

  #[test]
  fn optional_string_dotted_path() {
    let p = map_provider(&[("SERVER_HOST", "localhost")]);
    assert_eq!(
      go(Config::optional_string("SERVER.HOST"), &p).unwrap(),
      Some("localhost".to_string())
    );
  }

  // ── boolean yes/no variants ────────────────────────────────────────────────

  #[test]
  fn boolean_yes_no_variants() {
    let p = map_provider(&[("A", "yes"), ("B", "no"), ("C", "YES"), ("D", "NO")]);
    assert!(go(Config::boolean("A"), &p).unwrap());
    assert!(!go(Config::boolean("B"), &p).unwrap());
    assert!(go(Config::boolean("C"), &p).unwrap());
    assert!(!go(Config::boolean("D"), &p).unwrap());
  }

  #[test]
  fn boolean_invalid_string_is_error() {
    let p = map_provider(&[("FLAG", "maybe")]);
    let err = go(Config::boolean("FLAG"), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  #[test]
  fn boolean_missing_is_error() {
    let p = map_provider(&[]);
    let err = go(Config::boolean("FLAG"), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));
  }

  // ── map_attempt failure ───────────────────────────────────────────────────

  #[test]
  fn map_attempt_failure_propagates_error() {
    let p = map_provider(&[("VAL", "not-a-number")]);
    let err = go(
      Config::string("VAL").map_attempt(|s| {
        s.parse::<i32>().map_err(|e| ConfigError::Invalid {
          path: "VAL".into(),
          value: s,
          reason: e.to_string(),
        })
      }),
      &p,
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  // ── number missing/invalid ────────────────────────────────────────────────

  #[test]
  fn number_missing_is_error() {
    let p = map_provider(&[]);
    let err = go(Config::number("N"), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));
  }

  #[test]
  fn number_invalid_is_error() {
    let p = map_provider(&[("N", "not_a_float")]);
    let err = go(Config::number("N"), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  // ── integer missing/invalid ───────────────────────────────────────────────

  #[test]
  fn integer_missing_is_error() {
    let p = map_provider(&[]);
    let err = go(Config::integer("N"), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));
  }

  #[test]
  fn integer_invalid_is_error() {
    let p = map_provider(&[("N", "not_an_int")]);
    let err = go(Config::integer("N"), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  // ── string_list missing ───────────────────────────────────────────────────

  #[test]
  fn string_list_missing_is_error() {
    let p = map_provider(&[]);
    let err = go(Config::string_list("TAGS"), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));
  }

  // ── repeat missing ────────────────────────────────────────────────────────

  #[test]
  fn repeat_missing_is_error() {
    let p = map_provider(&[]);
    let err = go(Config::repeat("ITEMS"), &p).unwrap_err();
    assert!(matches!(err, ConfigError::Missing { .. }));
  }

  // ── or_else propagates non-missing error ──────────────────────────────────

  #[test]
  fn or_else_propagates_invalid_error() {
    let p = map_provider(&[("PORT", "bad")]);
    let err = go(
      or_else(Config::integer("PORT"), Config::integer("FALLBACK_PORT")),
      &p,
    )
    .unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  // ── prefix provider seq_delim ─────────────────────────────────────────────

  #[test]
  fn nested_prefix_provider_seq_delim_delegates() {
    use crate::MapConfigProvider;
    use crate::provider::ProviderOptions;

    let opts = ProviderOptions {
      path_delim: "_",
      seq_delim: "|",
    };
    let map: std::collections::HashMap<String, String> =
      [("NS_ITEMS".to_string(), "a|b|c".to_string())]
        .into_iter()
        .collect();
    let p = MapConfigProvider::with_options(map, opts);
    let items = go(Config::string_list("ITEMS").nested("NS"), &p).unwrap();
    assert_eq!(items, vec!["a", "b", "c"]);
  }
}
