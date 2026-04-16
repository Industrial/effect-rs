//! Effect.ts-style [`ConfigProvider`]: pluggable sources with path / sequence delimiters.
//!
//! Mirrors [`ConfigProvider.fromEnv`](https://effect.website/docs/configuration) and
//! `ConfigProvider.fromMap` (path delimiter, sequence delimiter).

use std::collections::HashMap;
use std::fmt;
use std::future::ready;
use std::sync::Arc;

use ::figment::Figment;
use ::figment::value::{Num, Value};

use ::id_effect::{BoxFuture, Get, Here, IntoBind};

use crate::error::ConfigError;

// ── Service tag, struct, and NeedsConfigProvider ──────────────────────────────

::id_effect::service_key!(
  /// Tag for [`ConfigProviderService`] in an [`id_effect::Context`] stack.
  pub struct ConfigProviderKey
);

/// Injectable wrapper around an `Arc<dyn ConfigProvider>`.
///
/// Extract it with `Get::<ConfigProviderKey, Here>::get(r)` inside an `effect!`
/// body, or use `~ConfigProviderService` for the async variant.
#[derive(Clone)]
pub struct ConfigProviderService(pub Arc<dyn ConfigProvider>);

impl fmt::Debug for ConfigProviderService {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("ConfigProviderService")
      .field(&"<dyn ConfigProvider>")
      .finish()
  }
}

impl<'a, R> IntoBind<'a, R, ConfigProviderService, ConfigError> for ConfigProviderService
where
  R: Get<ConfigProviderKey, Here, Target = ConfigProviderService> + 'a,
{
  fn into_bind(self, r: &'a mut R) -> BoxFuture<'a, Result<ConfigProviderService, ConfigError>> {
    Box::pin(ready(Ok(Get::<ConfigProviderKey, Here>::get(r).clone())))
  }
}

/// Supertrait alias — write `R: NeedsConfigProvider` instead of the full `Get<…>` bound.
pub trait NeedsConfigProvider:
  Get<ConfigProviderKey, Here, Target = ConfigProviderService>
{
}
impl<R: Get<ConfigProviderKey, Here, Target = ConfigProviderService>> NeedsConfigProvider for R {}

/// Options aligned with Effect `ConfigProvider.fromEnv` (`pathDelim`, `seqDelim`).
#[derive(Clone, Debug)]
pub struct ProviderOptions {
  /// Joins path segments for lookups (Effect default: `"_"`).
  pub path_delim: &'static str,
  /// Separates list elements in a single string (Effect default: `","`).
  pub seq_delim: &'static str,
}

impl Default for ProviderOptions {
  fn default() -> Self {
    Self {
      path_delim: "_",
      seq_delim: ",",
    }
  }
}

/// Abstract configuration source (Effect `ConfigProvider`).
pub trait ConfigProvider: Send + Sync {
  /// Look up a scalar; segments are joined with [`ProviderOptions::path_delim`] (or provider-specific rules).
  fn load_raw(&self, path: &[&str]) -> Result<Option<String>, ConfigError>;

  /// Delimiter used by [`crate::read_string_list`] (Effect `seqDelim`).
  fn seq_delim(&self) -> &'static str {
    ","
  }

  /// Scope this provider under `prefix`, prepending those segments to every lookup
  /// (Effect `ConfigProvider.within`).
  ///
  /// ```rust
  /// use id_effect_config::{ConfigProvider, MapConfigProvider};
  ///
  /// let base = MapConfigProvider::from_pairs([("SERVER_HOST", "localhost")]);
  /// let scoped = base.within("SERVER");
  /// let host = scoped.load_raw(&["HOST"]).unwrap();
  /// assert_eq!(host, Some("localhost".to_string()));
  /// ```
  fn within(self, prefix: impl Into<String>) -> ScopedConfigProvider<Self>
  where
    Self: Sized,
  {
    ScopedConfigProvider::new(self, prefix)
  }

  /// Try `self` first; fall back to `fallback` on missing keys (Effect `ConfigProvider.orElse`).
  ///
  /// This is the *provider-level* fallback.  For a *descriptor-level* fallback see
  /// [`Config::with_default`](crate::Config::with_default).
  fn or_else<B: ConfigProvider + Clone>(self, fallback: B) -> OrElseConfigProvider<Self, B>
  where
    Self: Sized + Clone,
  {
    OrElseConfigProvider::new(self, fallback)
  }
}

fn join_path(path: &[&str], delim: &str) -> Result<String, ConfigError> {
  if path.is_empty() {
    return Err(ConfigError::Invalid {
      path: String::new(),
      value: String::new(),
      reason: "empty configuration path".into(),
    });
  }
  Ok(path.join(delim))
}

/// Reads from `std::env` using flattened keys (`SERVER_PORT` for path `["SERVER","PORT"]` with default delim).
#[derive(Clone, Debug)]
pub struct EnvConfigProvider {
  options: ProviderOptions,
}

impl EnvConfigProvider {
  /// `ConfigProvider.fromEnv()` with Effect defaults.
  #[inline]
  pub fn from_env() -> Self {
    Self::new(ProviderOptions::default())
  }

  /// Build with explicit path and sequence delimiter options.
  #[inline]
  pub fn new(options: ProviderOptions) -> Self {
    Self { options }
  }
}

impl ConfigProvider for EnvConfigProvider {
  fn load_raw(&self, path: &[&str]) -> Result<Option<String>, ConfigError> {
    let key = join_path(path, self.options.path_delim)?;
    match std::env::var(&key) {
      Ok(s) => Ok(Some(s)),
      Err(std::env::VarError::NotPresent) => Ok(None),
      Err(std::env::VarError::NotUnicode(_)) => Err(ConfigError::InvalidUtf8 { var: key }),
    }
  }

  fn seq_delim(&self) -> &'static str {
    self.options.seq_delim
  }
}

/// In-memory map for tests or static overrides (`ConfigProvider.fromMap`).
#[derive(Clone, Debug)]
pub struct MapConfigProvider {
  map: HashMap<String, String>,
  options: ProviderOptions,
}

impl MapConfigProvider {
  /// In-memory provider with default [`ProviderOptions`].
  #[inline]
  pub fn from_map(map: HashMap<String, String>) -> Self {
    Self::with_options(map, ProviderOptions::default())
  }

  /// In-memory provider with custom path and list delimiters.
  #[inline]
  pub fn with_options(map: HashMap<String, String>, options: ProviderOptions) -> Self {
    Self { map, options }
  }

  /// Build a map provider from `(key, value)` pairs (Effect `ConfigProvider.fromMap(new Map(...))`).
  #[inline]
  pub fn from_pairs<I, K, V>(pairs: I) -> Self
  where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
  {
    Self::from_pairs_with_options(pairs, ProviderOptions::default())
  }

  /// Like [`Self::from_pairs`], with explicit [`ProviderOptions`].
  #[inline]
  pub fn from_pairs_with_options<I, K, V>(pairs: I, options: ProviderOptions) -> Self
  where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
  {
    Self {
      map: pairs
        .into_iter()
        .map(|(k, v)| (k.into(), v.into()))
        .collect(),
      options,
    }
  }
}

impl ConfigProvider for MapConfigProvider {
  fn load_raw(&self, path: &[&str]) -> Result<Option<String>, ConfigError> {
    let key = join_path(path, self.options.path_delim)?;
    Ok(self.map.get(&key).cloned())
  }

  fn seq_delim(&self) -> &'static str {
    self.options.seq_delim
  }
}

/// Adapts a merged [`Figment`] as a provider (paths joined with `.`, matching Figment key paths).
#[derive(Clone, Debug)]
pub struct FigmentConfigProvider {
  figment: Arc<Figment>,
}

impl FigmentConfigProvider {
  /// Scalar reads against an owned merged [`Figment`].
  #[inline]
  pub fn new(figment: Figment) -> Self {
    Self {
      figment: Arc::new(figment),
    }
  }

  /// Share an existing [`Arc<Figment>`] across providers or layers.
  #[inline]
  pub fn from_shared(figment: Arc<Figment>) -> Self {
    Self { figment }
  }

  /// Borrow the underlying merged [`Figment`].
  #[inline]
  pub fn figment(&self) -> &Figment {
    self.figment.as_ref()
  }
}

fn num_to_string(n: Num) -> String {
  match n {
    Num::U8(v) => v.to_string(),
    Num::U16(v) => v.to_string(),
    Num::U32(v) => v.to_string(),
    Num::U64(v) => v.to_string(),
    Num::U128(v) => v.to_string(),
    Num::USize(v) => v.to_string(),
    Num::I8(v) => v.to_string(),
    Num::I16(v) => v.to_string(),
    Num::I32(v) => v.to_string(),
    Num::I64(v) => v.to_string(),
    Num::I128(v) => v.to_string(),
    Num::ISize(v) => v.to_string(),
    Num::F32(v) => v.to_string(),
    Num::F64(v) => v.to_string(),
  }
}

fn figment_value_as_raw_string(v: &Value) -> Result<String, ConfigError> {
  match v {
    Value::String(_, s) => Ok(s.clone()),
    Value::Char(_, c) => Ok(c.to_string()),
    Value::Bool(_, b) => Ok(b.to_string()),
    Value::Num(_, n) => Ok(num_to_string(*n)),
    _ => Err(ConfigError::Invalid {
      path: String::new(),
      value: format!("{v:?}"),
      reason: "expected a scalar string, bool, or number".into(),
    }),
  }
}

impl ConfigProvider for FigmentConfigProvider {
  fn load_raw(&self, path: &[&str]) -> Result<Option<String>, ConfigError> {
    let key_path = join_path(path, ".")?;
    let fig = self.figment.as_ref();
    if !fig.contains(&key_path) {
      return Ok(None);
    }
    let v = fig.find_value(&key_path).map_err(ConfigError::from)?;
    figment_value_as_raw_string(&v).map(Some)
  }
}

/// Try `primary` first; if it returns [`None`], use `fallback` (Effect-style provider composition).
#[derive(Clone, Debug)]
pub struct OrElseConfigProvider<A, B> {
  primary: A,
  fallback: B,
}

impl<A, B> OrElseConfigProvider<A, B> {
  /// Try `primary` first; on missing keys, delegate to `fallback`.
  #[inline]
  pub fn new(primary: A, fallback: B) -> Self {
    Self { primary, fallback }
  }
}

impl<A: ConfigProvider + Clone + 'static, B: ConfigProvider + Clone + 'static> ConfigProvider
  for OrElseConfigProvider<A, B>
{
  fn load_raw(&self, path: &[&str]) -> Result<Option<String>, ConfigError> {
    let path_refs_primary: Vec<&str> = path.to_vec();
    match self.primary.load_raw(&path_refs_primary)? {
      None => self.fallback.load_raw(path),
      some => Ok(some),
    }
  }

  /// Uses the primary provider’s delimiter; keep both providers on the same convention when using [`crate::read_string_list`].
  fn seq_delim(&self) -> &'static str {
    self.primary.seq_delim()
  }
}

// ── ScopedConfigProvider ──────────────────────────────────────────────────────

/// Wraps an inner provider, prepending fixed path segments to every lookup.
///
/// Created via [`ConfigProvider::within`].
///
/// ```rust
/// use id_effect_config::{ConfigProvider, MapConfigProvider, config};
///
/// let base = MapConfigProvider::from_pairs([("DB_HOST", "localhost"), ("DB_PORT", "5432")]);
/// let db = base.within("DB");
/// assert_eq!(config::string(&db, "HOST").unwrap(), "localhost");
/// assert_eq!(config::integer(&db, "PORT").unwrap(), 5432);
/// ```
#[derive(Clone, Debug)]
pub struct ScopedConfigProvider<P> {
  inner: P,
  prefix: Vec<String>,
}

impl<P> ScopedConfigProvider<P> {
  /// Build a scoped provider.  `prefix` is split on `'.'` to form path segments.
  pub fn new(inner: P, prefix: impl Into<String>) -> Self {
    Self {
      inner,
      prefix: prefix
        .into()
        .split('.')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect(),
    }
  }

  /// Access the wrapped provider.
  #[inline]
  pub fn inner(&self) -> &P {
    &self.inner
  }

  /// The prefix segments prepended to every lookup.
  #[inline]
  pub fn prefix_segments(&self) -> &[String] {
    &self.prefix
  }
}

impl<P: ConfigProvider> ConfigProvider for ScopedConfigProvider<P> {
  fn load_raw(&self, path: &[&str]) -> Result<Option<String>, ConfigError> {
    let mut full: Vec<String> = self.prefix.clone();
    full.extend(path.iter().map(|s| (*s).to_string()));
    let refs: Vec<&str> = full.iter().map(String::as_str).collect();
    self.inner.load_raw(&refs)
  }

  fn seq_delim(&self) -> &'static str {
    self.inner.seq_delim()
  }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
  use std::collections::HashMap;
  use std::sync::Arc;

  use super::*;

  fn pairs(entries: &[(&str, &str)]) -> MapConfigProvider {
    MapConfigProvider::from_pairs(entries.iter().copied())
  }

  // ── ConfigProviderService Debug ───────────────────────────────────────────

  #[test]
  fn config_provider_service_debug_format() {
    let p = pairs(&[("K", "v")]);
    let svc = ConfigProviderService(Arc::new(p));
    let s = format!("{svc:?}");
    assert!(s.contains("ConfigProviderService"));
  }

  // ── ConfigProvider default seq_delim ─────────────────────────────────────

  #[test]
  fn default_seq_delim_is_comma() {
    struct MinimalProvider;
    impl ConfigProvider for MinimalProvider {
      fn load_raw(&self, _path: &[&str]) -> Result<Option<String>, ConfigError> {
        Ok(None)
      }
      // seq_delim not overridden → default impl returns ","
    }
    assert_eq!(MinimalProvider.seq_delim(), ",");
  }

  // ── ConfigProvider::within ────────────────────────────────────────────────

  #[test]
  fn within_scopes_lookup() {
    let p = pairs(&[("SERVER_HOST", "localhost")]);
    let scoped = p.within("SERVER");
    assert_eq!(
      scoped.load_raw(&["HOST"]).unwrap(),
      Some("localhost".to_string())
    );
  }

  // ── ConfigProvider::or_else (trait method) ────────────────────────────────

  #[test]
  fn or_else_trait_method_falls_back() {
    let a = MapConfigProvider::from_map(HashMap::new());
    let b = pairs(&[("K", "from-b")]);
    let composed = a.or_else(b);
    assert_eq!(
      composed.load_raw(&["K"]).unwrap(),
      Some("from-b".to_string())
    );
  }

  // ── EnvConfigProvider::from_env ───────────────────────────────────────────

  #[test]
  fn env_config_provider_from_env_has_comma_seq_delim() {
    let p = EnvConfigProvider::from_env();
    assert_eq!(p.seq_delim(), ",");
  }

  // ── EnvConfigProvider::new with custom options ────────────────────────────

  #[test]
  fn env_config_provider_new_custom_options() {
    let opts = ProviderOptions {
      path_delim: ".",
      seq_delim: ";",
    };
    let p = EnvConfigProvider::new(opts);
    assert_eq!(p.seq_delim(), ";");
  }

  // ── ProviderOptions::default ───────────────────────────────────────────────

  #[test]
  fn provider_options_default_values() {
    let opts = ProviderOptions::default();
    assert_eq!(opts.path_delim, "_");
    assert_eq!(opts.seq_delim, ",");
  }

  // ── FigmentConfigProvider::figment() accessor ─────────────────────────────

  #[test]
  fn figment_config_provider_figment_accessor() {
    let fig = Figment::new();
    let p = FigmentConfigProvider::new(fig);
    // just verify it doesn't panic and returns a reference
    let _ = p.figment();
  }

  // ── OrElseConfigProvider: primary found ───────────────────────────────────

  #[test]
  fn or_else_provider_primary_found_returns_primary() {
    let a = pairs(&[("K", "from-a")]);
    let b = pairs(&[("K", "from-b")]);
    let composed = OrElseConfigProvider::new(a, b);
    assert_eq!(
      composed.load_raw(&["K"]).unwrap(),
      Some("from-a".to_string())
    );
  }

  // ── OrElseConfigProvider::seq_delim delegates to primary ─────────────────

  #[test]
  fn or_else_provider_seq_delim_uses_primary() {
    let a = MapConfigProvider::with_options(
      HashMap::new(),
      ProviderOptions {
        path_delim: "_",
        seq_delim: ";",
      },
    );
    let b = pairs(&[("K", "v")]);
    let composed = OrElseConfigProvider::new(a, b);
    assert_eq!(composed.seq_delim(), ";");
  }

  // ── ScopedConfigProvider::seq_delim delegates to inner ───────────────────

  #[test]
  fn scoped_provider_seq_delim_delegates_to_inner() {
    let p = MapConfigProvider::with_options(
      HashMap::new(),
      ProviderOptions {
        path_delim: "_",
        seq_delim: "|",
      },
    );
    let scoped = ScopedConfigProvider::new(p, "NS");
    assert_eq!(scoped.seq_delim(), "|");
  }

  // ── join_path: empty path returns error ───────────────────────────────────

  #[test]
  fn map_provider_empty_path_returns_error() {
    let p = MapConfigProvider::from_map(HashMap::new());
    let err = p.load_raw(&[]).unwrap_err();
    assert!(matches!(err, ConfigError::Invalid { .. }));
  }

  // ── FigmentConfigProvider: various Num variants via Serialized ────────────

  #[cfg(feature = "toml")]
  mod figment_num_variants {
    use super::*;
    use ::figment::providers::Serialized;
    use serde::Serialize;

    fn provider_from<T: Serialize>(value: T) -> FigmentConfigProvider {
      FigmentConfigProvider::new(Figment::from(Serialized::defaults(value)))
    }

    #[derive(Serialize)]
    struct U8Val {
      val: u8,
    }
    #[derive(Serialize)]
    struct U16Val {
      val: u16,
    }
    #[derive(Serialize)]
    struct U32Val {
      val: u32,
    }
    #[derive(Serialize)]
    struct U64Val {
      val: u64,
    }
    #[derive(Serialize)]
    struct I8Val {
      val: i8,
    }
    #[derive(Serialize)]
    struct I16Val {
      val: i16,
    }
    #[derive(Serialize)]
    struct I32Val {
      val: i32,
    }
    #[derive(Serialize)]
    struct F32Val {
      val: f32,
    }
    #[derive(Serialize)]
    struct CharVal {
      val: char,
    }

    #[test]
    fn num_u8() {
      let p = provider_from(U8Val { val: 200 });
      assert_eq!(p.load_raw(&["val"]).unwrap(), Some("200".to_string()));
    }

    #[test]
    fn num_u16() {
      let p = provider_from(U16Val { val: 1000 });
      assert_eq!(p.load_raw(&["val"]).unwrap(), Some("1000".to_string()));
    }

    #[test]
    fn num_u32() {
      let p = provider_from(U32Val { val: 70000 });
      assert_eq!(p.load_raw(&["val"]).unwrap(), Some("70000".to_string()));
    }

    #[test]
    fn num_u64() {
      let p = provider_from(U64Val { val: 1_000_000 });
      assert_eq!(p.load_raw(&["val"]).unwrap(), Some("1000000".to_string()));
    }

    #[test]
    fn num_i8() {
      let p = provider_from(I8Val { val: -5 });
      assert_eq!(p.load_raw(&["val"]).unwrap(), Some("-5".to_string()));
    }

    #[test]
    fn num_i16() {
      let p = provider_from(I16Val { val: -300 });
      assert_eq!(p.load_raw(&["val"]).unwrap(), Some("-300".to_string()));
    }

    #[test]
    fn num_i32() {
      let p = provider_from(I32Val { val: 100_000 });
      assert_eq!(p.load_raw(&["val"]).unwrap(), Some("100000".to_string()));
    }

    #[test]
    fn num_f32() {
      let p = provider_from(F32Val { val: 1.5 });
      let s = p.load_raw(&["val"]).unwrap().unwrap();
      let v: f64 = s.parse().unwrap();
      assert!((v - 1.5).abs() < 0.01);
    }

    #[test]
    fn char_variant() {
      let p = provider_from(CharVal { val: 'x' });
      assert_eq!(p.load_raw(&["val"]).unwrap(), Some("x".to_string()));
    }
  }
}
