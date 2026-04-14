//! Wall-clock and instant types backed by [`jiff`] — mirrors Effect.ts `DateTime` / time zones.
//!
//! UTC instants use [`UtcDateTime`] ([`jiff::Timestamp`]). Zone-aware values use [`ZonedDateTime`]
//! ([`jiff::Zoned`]). Fallible IANA lookup is expressed as [`Effect`] via
//! [`timezone::named`].

use std::time::{Duration, SystemTime};

use jiff::SignedDuration;
use jiff::civil::{DateTime, DateTimeRound, date};
use jiff::tz::{self, TimeZone};
use jiff::{RoundMode, Timestamp, ToSpan, Unit, Zoned, ZonedRound};

use crate::runtime::Never;
use crate::{Effect, fail, succeed};

// ── TimeUnit ─────────────────────────────────────────────────────────────────

/// Calendar / clock unit for boundary and rounding helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeUnit {
  /// Calendar year boundary in UTC or zoned civil time.
  Year,
  /// Month boundary.
  Month,
  /// Week boundary (Monday-based).
  Week,
  /// Day boundary (midnight in the relevant zone / UTC).
  Day,
  /// Hour boundary.
  Hour,
  /// Minute boundary.
  Minute,
  /// Second boundary.
  Second,
}

impl TimeUnit {
  const fn to_jiff_unit(self) -> Option<Unit> {
    match self {
      Self::Second => Some(Unit::Second),
      Self::Minute => Some(Unit::Minute),
      Self::Hour => Some(Unit::Hour),
      Self::Day => Some(Unit::Day),
      Self::Week | Self::Month | Self::Year => None,
    }
  }
}

// ── UtcDateTime ──────────────────────────────────────────────────────────────

/// An absolute instant in UTC (nanosecond-precision Unix timeline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UtcDateTime(
  /// Underlying nanosecond-precision UTC instant ([`jiff::Timestamp`]).
  pub jiff::Timestamp,
);

impl UtcDateTime {
  /// Current system time as UTC ([`Timestamp::now`]).
  pub fn now() -> Effect<Self, Never, ()> {
    succeed(Self(Timestamp::now()))
  }

  /// Construct from Unix milliseconds when in range.
  #[inline]
  pub fn from_epoch_millis(ms: i64) -> Option<Self> {
    Timestamp::from_millisecond(ms).ok().map(Self)
  }

  /// Construct from Unix milliseconds; panics when out of range (caller assertion, not `unsafe`).
  #[inline]
  pub fn unsafe_make(ms: i64) -> Self {
    Self(
      Timestamp::from_millisecond(ms)
        .unwrap_or_else(|e| panic!("UtcDateTime::unsafe_make: invalid millis {ms}: {e}")),
    )
  }

  /// Convert from [`SystemTime`] (fallible on out-of-range values).
  #[inline]
  pub fn from_std(t: SystemTime) -> Option<Self> {
    Timestamp::try_from(t).ok().map(Self)
  }

  /// Unwrap to the raw [`Timestamp`].
  #[inline]
  pub fn inner(self) -> Timestamp {
    self.0
  }

  /// Unix time in whole milliseconds.
  #[inline]
  pub fn to_epoch_millis(&self) -> i64 {
    self.0.as_millisecond()
  }

  /// View / project this instant into a specific IANA or fixed-offset zone.
  #[inline]
  pub fn to_zoned(self, zone: TimeZone) -> ZonedDateTime {
    ZonedDateTime(self.0.to_zoned(zone))
  }

  /// Civil year in UTC.
  #[inline]
  pub fn year(&self) -> i16 {
    utc_civil(self.0).year()
  }

  /// Civil month in UTC (1–12).
  #[inline]
  pub fn month(&self) -> i8 {
    utc_civil(self.0).month()
  }

  /// Civil day of month in UTC.
  #[inline]
  pub fn day(&self) -> i8 {
    utc_civil(self.0).day()
  }

  /// Civil hour in UTC (0–23).
  #[inline]
  pub fn hour(&self) -> i8 {
    utc_civil(self.0).hour()
  }

  /// Civil minute in UTC.
  #[inline]
  pub fn minute(&self) -> i8 {
    utc_civil(self.0).minute()
  }

  /// Civil second in UTC.
  #[inline]
  pub fn second(&self) -> i8 {
    utc_civil(self.0).second()
  }

  /// Add `d` to this instant; panics on overflow.
  #[inline]
  pub fn add_duration(&self, d: Duration) -> Self {
    Self(
      self
        .0
        .checked_add(d)
        .unwrap_or_else(|e| panic!("UtcDateTime::add_duration overflow: {e}")),
    )
  }

  /// Subtract `d` from this instant; panics on underflow.
  #[inline]
  pub fn subtract_duration(&self, d: Duration) -> Self {
    Self(
      self
        .0
        .checked_sub(d)
        .unwrap_or_else(|e| panic!("UtcDateTime::subtract_duration overflow: {e}")),
    )
  }

  /// First instant of the unit (UTC civil calendar).
  pub fn start_of(&self, unit: TimeUnit) -> Self {
    Self(start_timestamp_utc(self.0, unit).expect("start_of: jiff round error"))
  }

  /// Last representable instant inside the unit (UTC civil calendar), inclusive.
  pub fn end_of(&self, unit: TimeUnit) -> Self {
    let start = self.start_of(unit);
    let next = advance_start_utc(start.0, unit).expect("end_of: advance");
    Self(
      next
        .checked_sub(1.nanosecond())
        .expect("end_of: subtract 1ns"),
    )
  }

  /// Round to the nearest boundary of `unit` (UTC civil calendar).
  pub fn nearest(&self, unit: TimeUnit) -> Self {
    Self(nearest_timestamp_utc(self.0, unit).expect("nearest: jiff error"))
  }

  /// Signed difference `other − self` in whole milliseconds.
  #[inline]
  pub fn distance_millis(&self, other: &Self) -> i64 {
    other.to_epoch_millis() - self.to_epoch_millis()
  }

  /// Absolute span between `self` and `other` as [`Duration`].
  #[inline]
  pub fn distance_duration(&self, other: &Self) -> Duration {
    let sd = (other.0.as_duration() - self.0.as_duration()).abs();
    Duration::try_from(sd).expect("absolute span fits std::time::Duration")
  }

  /// RFC 3339 / ISO-8601 instant string with `Z` suffix.
  #[inline]
  pub fn format_iso(&self) -> String {
    self.0.to_string()
  }

  /// `strftime`-style formatting in UTC.
  #[inline]
  pub fn format(&self, fmt: &str) -> String {
    self.0.strftime(fmt).to_string()
  }

  /// Strictly before `other` on the UTC timeline.
  #[inline]
  pub fn less_than(&self, other: &Self) -> bool {
    self.0 < other.0
  }

  /// Strictly after `other` on the UTC timeline.
  #[inline]
  pub fn greater_than(&self, other: &Self) -> bool {
    self.0 > other.0
  }

  /// Inclusive range check between `min` and `max` on the UTC timeline.
  #[inline]
  pub fn between(&self, min: &Self, max: &Self) -> bool {
    self.0 >= min.0 && self.0 <= max.0
  }
}

// ── ZonedDateTime ────────────────────────────────────────────────────────────

/// An instant with a resolved time zone ([`jiff::Zoned`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ZonedDateTime(
  /// Underlying zoned instant ([`jiff::Zoned`]).
  pub Zoned,
);

impl ZonedDateTime {
  /// “Now” in the system default zone ([`Zoned::now`]).
  pub fn now() -> Effect<Self, Never, ()> {
    succeed(Self(Zoned::now()))
  }

  /// Build from Unix milliseconds in `zone` when in range.
  #[inline]
  pub fn from_epoch_millis(ms: i64, zone: TimeZone) -> Option<Self> {
    let ts = Timestamp::from_millisecond(ms).ok()?;
    Some(Self(ts.to_zoned(zone)))
  }

  /// Like [`UtcDateTime::unsafe_make`] then project into `zone`.
  #[inline]
  pub fn unsafe_make(ms: i64, zone: TimeZone) -> Self {
    Self(UtcDateTime::unsafe_make(ms).0.to_zoned(zone))
  }

  /// Convert [`SystemTime`] through UTC then attach `zone`.
  #[inline]
  pub fn from_std(t: SystemTime, zone: TimeZone) -> Option<Self> {
    let ts = Timestamp::try_from(t).ok()?;
    Some(Self(ts.to_zoned(zone)))
  }

  /// Borrow the inner [`Zoned`].
  #[inline]
  pub fn inner(&self) -> &Zoned {
    &self.0
  }

  /// Consume and return the inner [`Zoned`].
  #[inline]
  pub fn into_inner(self) -> Zoned {
    self.0
  }

  /// Unix time in whole milliseconds (instant, ignoring display offset quirks).
  #[inline]
  pub fn to_epoch_millis(&self) -> i64 {
    self.0.timestamp().as_millisecond()
  }

  /// Civil year in this value’s zone.
  #[inline]
  pub fn year(&self) -> i16 {
    self.0.year()
  }

  /// Civil month (1–12) in this value’s zone.
  #[inline]
  pub fn month(&self) -> i8 {
    self.0.month()
  }

  /// Civil day of month in this value’s zone.
  #[inline]
  pub fn day(&self) -> i8 {
    self.0.day()
  }

  /// Civil hour (0–23) in this value’s zone.
  #[inline]
  pub fn hour(&self) -> i8 {
    self.0.hour()
  }

  /// Civil minute in this value’s zone.
  #[inline]
  pub fn minute(&self) -> i8 {
    self.0.minute()
  }

  /// Civil second in this value’s zone.
  #[inline]
  pub fn second(&self) -> i8 {
    self.0.second()
  }

  /// Resolved IANA or fixed-offset zone.
  #[inline]
  pub fn time_zone(&self) -> TimeZone {
    self.0.time_zone().clone()
  }

  /// Add `d` in this zone; panics on overflow.
  #[inline]
  pub fn add_duration(&self, d: Duration) -> Self {
    Self(
      self
        .0
        .clone()
        .checked_add(d)
        .unwrap_or_else(|e| panic!("ZonedDateTime::add_duration overflow: {e}")),
    )
  }

  /// Subtract `d` in this zone; panics on underflow.
  #[inline]
  pub fn subtract_duration(&self, d: Duration) -> Self {
    Self(
      self
        .0
        .clone()
        .checked_sub(d)
        .unwrap_or_else(|e| panic!("ZonedDateTime::subtract_duration overflow: {e}")),
    )
  }

  /// First instant of `unit` in this zone’s civil calendar.
  pub fn start_of(&self, unit: TimeUnit) -> Self {
    Self(start_zoned(&self.0, unit).expect("ZonedDateTime::start_of"))
  }

  /// Last representable instant inside `unit` in this zone, inclusive.
  pub fn end_of(&self, unit: TimeUnit) -> Self {
    let start = self.start_of(unit);
    let tz = start.time_zone();
    let next = advance_start_zoned(&start.0, unit).expect("ZonedDateTime::end_of advance");
    let ts = next
      .timestamp()
      .checked_sub(1.nanosecond())
      .expect("ZonedDateTime::end_of subtract");
    Self(ts.to_zoned(tz))
  }

  /// Round to the nearest boundary of `unit` in this zone’s civil calendar.
  pub fn nearest(&self, unit: TimeUnit) -> Self {
    Self(nearest_zoned(&self.0, unit).expect("ZonedDateTime::nearest"))
  }

  /// Signed difference of instants `other − self` in whole milliseconds.
  #[inline]
  pub fn distance_millis(&self, other: &Self) -> i64 {
    other.to_epoch_millis() - self.to_epoch_millis()
  }

  /// Absolute span between the two instants as [`Duration`].
  #[inline]
  pub fn distance_duration(&self, other: &Self) -> Duration {
    let sd = (other.0.timestamp().as_duration() - self.0.timestamp().as_duration()).abs();
    Duration::try_from(sd).expect("absolute span fits std::time::Duration")
  }

  /// Full ISO-8601 string including offset / zone name per [`jiff`].
  #[inline]
  pub fn format_iso(&self) -> String {
    self.0.to_string()
  }

  /// `strftime`-style formatting in this value’s zone.
  #[inline]
  pub fn format(&self, fmt: &str) -> String {
    self.0.strftime(fmt).to_string()
  }

  /// Compare instants: strictly before `other`.
  #[inline]
  pub fn less_than(&self, other: &Self) -> bool {
    self.0.timestamp() < other.0.timestamp()
  }

  /// Compare instants: strictly after `other`.
  #[inline]
  pub fn greater_than(&self, other: &Self) -> bool {
    self.0.timestamp() > other.0.timestamp()
  }

  /// Inclusive range check on the underlying instants.
  #[inline]
  pub fn between(&self, min: &Self, max: &Self) -> bool {
    let t = self.0.timestamp();
    t >= min.0.timestamp() && t <= max.0.timestamp()
  }
}

// ── AnyDateTime ──────────────────────────────────────────────────────────────

/// Either UTC or zone-aware wall time.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnyDateTime {
  /// UTC instant.
  Utc(UtcDateTime),
  /// Zone-aware instant.
  Zoned(ZonedDateTime),
}

// ── timezone ─────────────────────────────────────────────────────────────────

/// UTC, fixed-offset, and IANA named zones ([`TimeZone`] helpers).
pub mod timezone {
  use super::*;

  /// Invalid IANA identifier (or other lookup failure) for [`named`].
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct TimeZoneError {
    /// Input string that could not be resolved to a zone.
    pub id: String,
  }

  impl std::fmt::Display for TimeZoneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
      write!(f, "unknown or invalid time zone {:?}", self.id)
    }
  }

  impl std::error::Error for TimeZoneError {}

  /// The static UTC zone ([`TimeZone::UTC`]).
  #[inline]
  pub fn utc() -> TimeZone {
    TimeZone::UTC
  }

  /// Fixed offset from UTC in whole minutes (not IANA; no DST).
  #[inline]
  pub fn offset(minutes: i32) -> TimeZone {
    let seconds = minutes.checked_mul(60).expect("offset minutes overflow");
    tz::Offset::from_seconds(seconds)
      .expect("offset seconds out of range")
      .to_time_zone()
  }

  /// Resolve a named zone from the bundled IANA database.
  pub fn named(iana_id: &str) -> Effect<TimeZone, TimeZoneError, ()> {
    match TimeZone::get(iana_id) {
      Ok(tz) => succeed(tz),
      Err(_) => fail(TimeZoneError {
        id: iana_id.to_string(),
      }),
    }
  }

  /// Parse `Europe/London` via IANA lookup, or a numeric offset like `+01:00` / `-0530`.
  pub fn from_str(s: &str) -> Option<TimeZone> {
    let s = s.trim();
    if s.is_empty() {
      return None;
    }
    if let Ok(tz) = TimeZone::get(s) {
      return Some(tz);
    }
    parse_fixed_offset_time_zone(s)
  }
}

// ── UTC helpers ──────────────────────────────────────────────────────────────

#[inline]
fn utc_civil(ts: Timestamp) -> DateTime {
  ts.to_zoned(TimeZone::UTC).datetime()
}

fn civil_to_utc(ts: DateTime) -> Result<Timestamp, jiff::Error> {
  Ok(ts.to_zoned(TimeZone::UTC)?.timestamp())
}

fn start_timestamp_utc(ts: Timestamp, unit: TimeUnit) -> Result<Timestamp, jiff::Error> {
  match unit {
    TimeUnit::Second | TimeUnit::Minute | TimeUnit::Hour | TimeUnit::Day => {
      let dt = utc_civil(ts);
      let u = unit.to_jiff_unit().expect("mapped unit");
      let rounded = dt.round(DateTimeRound::new().smallest(u).mode(RoundMode::Trunc))?;
      civil_to_utc(rounded)
    }
    TimeUnit::Week => {
      let dt = utc_civil(ts);
      let d = dt.date();
      let off = i64::from(d.weekday().to_monday_zero_offset());
      let week_start = d.checked_sub(off.days())?.at(0, 0, 0, 0);
      civil_to_utc(week_start)
    }
    TimeUnit::Month => {
      let dt = utc_civil(ts);
      let d = date(dt.year(), dt.month(), 1);
      civil_to_utc(d.at(0, 0, 0, 0))
    }
    TimeUnit::Year => {
      let dt = utc_civil(ts);
      let d = date(dt.year(), 1, 1);
      civil_to_utc(d.at(0, 0, 0, 0))
    }
  }
}

fn advance_start_utc(ts: Timestamp, unit: TimeUnit) -> Result<Timestamp, jiff::Error> {
  let dt = utc_civil(ts);
  let span = unit_span(unit)?;
  let next = dt.checked_add(span)?;
  civil_to_utc(next)
}

fn unit_span(unit: TimeUnit) -> Result<jiff::Span, jiff::Error> {
  Ok(match unit {
    TimeUnit::Second => 1.second(),
    TimeUnit::Minute => 1.minute(),
    TimeUnit::Hour => 1.hour(),
    TimeUnit::Day => 1.day(),
    TimeUnit::Week => 1.week(),
    TimeUnit::Month => 1.month(),
    TimeUnit::Year => 1.year(),
  })
}

fn nearest_timestamp_utc(ts: Timestamp, unit: TimeUnit) -> Result<Timestamp, jiff::Error> {
  if let Some(u) = unit.to_jiff_unit() {
    let dt = utc_civil(ts);
    return civil_to_utc(dt.round(u)?);
  }
  let start = start_timestamp_utc(ts, unit)?;
  let next = advance_start_utc(start, unit)?;
  let span = (next.as_duration() - start.as_duration()).abs();
  let half_nanos = span.as_nanos() / 2;
  let half = SignedDuration::from_nanos_i128(half_nanos);
  let mid_ts = start.checked_add(half).map_err(|_| {
    jiff::Error::from_args(format_args!("nearest_timestamp_utc: midpoint overflow"))
  })?;
  if ts < mid_ts { Ok(start) } else { Ok(next) }
}

// ── Zoned helpers ────────────────────────────────────────────────────────────

fn start_zoned(z: &Zoned, unit: TimeUnit) -> Result<Zoned, jiff::Error> {
  let tz = z.time_zone().clone();
  match unit {
    TimeUnit::Second | TimeUnit::Minute | TimeUnit::Hour | TimeUnit::Day => {
      let u = unit.to_jiff_unit().expect("mapped");
      z.round(ZonedRound::new().smallest(u).mode(RoundMode::Trunc))
    }
    TimeUnit::Week => {
      let dt = z.datetime();
      let d = dt.date();
      let off = i64::from(d.weekday().to_monday_zero_offset());
      let week_start = d.checked_sub(off.days())?.at(0, 0, 0, 0);
      week_start.to_zoned(tz)
    }
    TimeUnit::Month => {
      let dt = z.datetime();
      let d = date(dt.year(), dt.month(), 1);
      d.at(0, 0, 0, 0).to_zoned(tz)
    }
    TimeUnit::Year => {
      let dt = z.datetime();
      let d = date(dt.year(), 1, 1);
      d.at(0, 0, 0, 0).to_zoned(tz)
    }
  }
}

fn advance_start_zoned(z: &Zoned, unit: TimeUnit) -> Result<Zoned, jiff::Error> {
  let tz = z.time_zone().clone();
  let dt = z.datetime();
  let span = unit_span(unit)?;
  dt.checked_add(span)?.to_zoned(tz)
}

fn nearest_zoned(z: &Zoned, unit: TimeUnit) -> Result<Zoned, jiff::Error> {
  if let Some(u) = unit.to_jiff_unit() {
    return z.round(u);
  }
  let start = start_zoned(z, unit)?;
  let next = advance_start_zoned(&start, unit)?;
  let span = (next.timestamp().as_duration() - start.timestamp().as_duration()).abs();
  let half_nanos = span.as_nanos() / 2;
  let half = SignedDuration::from_nanos_i128(half_nanos);
  let mid_ts = start
    .timestamp()
    .checked_add(half)
    .map_err(|_| jiff::Error::from_args(format_args!("nearest_zoned: midpoint overflow")))?;
  if z.timestamp() < mid_ts {
    Ok(start)
  } else {
    Ok(next)
  }
}

fn parse_fixed_offset_time_zone(s: &str) -> Option<TimeZone> {
  let bytes = s.as_bytes();
  let (sign, rest) = match bytes.first()? {
    b'+' => (1i64, &s[1..]),
    b'-' => (-1i64, &s[1..]),
    _ => return None,
  };
  let rest = rest.trim();
  if rest.is_empty() {
    return None;
  }
  let mut parts = rest.split(':');
  let h: i64 = parts.next()?.parse().ok()?;
  let m: i64 = parts.next().map_or(0, |x| x.parse().unwrap_or(0));
  let sec: i64 = parts.next().map_or(0, |x| x.parse().unwrap_or(0));
  if parts.next().is_some() {
    return None;
  }
  let total = sign.checked_mul(
    h.checked_mul(3600)?
      .checked_add(m.checked_mul(60)?)?
      .checked_add(sec)?,
  )?;
  let seconds = i32::try_from(total).ok()?;
  Some(tz::Offset::from_seconds(seconds).ok()?.to_time_zone())
}

#[cfg(test)]
mod tests {
  use super::timezone;
  use super::*;
  use crate::failure::exit::Exit;
  use crate::testing::test_runtime::run_test;

  #[test]
  fn now_returns_utc_datetime() {
    let Exit::Success(utc) = run_test(UtcDateTime::now(), ()) else {
      panic!("expected success");
    };
    assert!(utc.to_epoch_millis() > 0);
  }

  #[test]
  fn from_epoch_millis_roundtrips() {
    let ms = 1_700_000_000_123i64;
    let u = UtcDateTime::from_epoch_millis(ms).expect("in range");
    assert_eq!(u.to_epoch_millis(), ms);
  }

  #[test]
  fn format_iso_produces_valid_rfc3339() {
    let u = UtcDateTime::unsafe_make(0);
    let s = u.format_iso();
    let parsed: Timestamp = s.parse().expect("parse RFC3339");
    assert_eq!(parsed, u.0);
  }

  #[test]
  fn start_of_day_zeroes_time_components() {
    let u = UtcDateTime::unsafe_make(1_700_000_000_123);
    let sod = u.start_of(TimeUnit::Day);
    assert_eq!(sod.hour(), 0);
    assert_eq!(sod.minute(), 0);
    assert_eq!(sod.second(), 0);
  }

  #[test]
  fn add_duration_crosses_day_boundary() {
    let u = UtcDateTime::unsafe_make(1_700_000_000_000);
    let day = u.start_of(TimeUnit::Day);
    let next = day.add_duration(Duration::from_secs(86_400));
    assert!(next.day() != day.day() || next.month() != day.month() || next.year() != day.year());
  }

  #[test]
  fn named_timezone_fails_on_invalid_id() {
    let exit = run_test(timezone::named("Not/A/Valid/Zone"), ());
    assert!(
      matches!(exit, Exit::Failure(_)),
      "expected failure, got {exit:?}"
    );
  }

  #[rstest::rstest]
  #[case(TimeUnit::Second)]
  #[case(TimeUnit::Minute)]
  #[case(TimeUnit::Hour)]
  #[case(TimeUnit::Day)]
  fn utc_start_nearest_round_trip(#[case] unit: TimeUnit) {
    let u = UtcDateTime::unsafe_make(1_720_000_000_000);
    let s = u.start_of(unit);
    let n = u.nearest(unit);
    assert!(s.less_than(&u) || s.0 == u.0);
    let _ = n.to_epoch_millis();
  }

  #[rstest::rstest]
  #[case(TimeUnit::Week)]
  #[case(TimeUnit::Month)]
  #[case(TimeUnit::Year)]
  fn utc_week_month_year_start_end_nearest(#[case] unit: TimeUnit) {
    let u = UtcDateTime::unsafe_make(1_720_000_000_000);
    let _ = u.start_of(unit);
    let _ = u.end_of(unit);
    let _ = u.nearest(unit);
  }

  #[test]
  fn utc_from_std_unix_epoch() {
    let u = UtcDateTime::from_std(std::time::UNIX_EPOCH).expect("epoch");
    assert_eq!(u.to_epoch_millis(), 0);
  }

  #[test]
  fn utc_civil_accessors_and_compare() {
    let a = UtcDateTime::unsafe_make(1_700_000_000_000);
    let b = UtcDateTime::unsafe_make(1_700_000_001_000);
    assert!(a.year() >= 2023);
    assert!((1..=12).contains(&a.month()));
    assert!((1..=31).contains(&a.day()));
    assert!(a.less_than(&b));
    assert!(b.greater_than(&a));
    assert!(b.between(&a, &b));
    assert_eq!(a.distance_millis(&b), 1000);
    assert!(a.distance_duration(&b) <= std::time::Duration::from_secs(2));
    let _ = a.format("%Y");
    let _ = a.format_iso();
  }

  #[test]
  fn zoned_now_and_helpers() {
    let Exit::Success(z) = run_test(ZonedDateTime::now(), ()) else {
      panic!("expected success");
    };
    let _ = z.year();
    let _ = z.format_iso();
    let _ = z.format("%H");
    let utc = UtcDateTime::unsafe_make(1_720_000_000_000);
    let london = timezone::named("Europe/London");
    let Exit::Success(tz) = run_test(london, ()) else {
      panic!("zone");
    };
    let z2 = ZonedDateTime::from_epoch_millis(1_720_000_000_000, tz).expect("zoned");
    assert_eq!(z2.to_epoch_millis(), 1_720_000_000_000);
    let z3 = utc.to_zoned(z2.time_zone());
    assert_eq!(z3.to_epoch_millis(), z2.to_epoch_millis());
    let _ = z2.start_of(TimeUnit::Day);
    let _ = z2.end_of(TimeUnit::Hour);
    let _ = z2.nearest(TimeUnit::Week);
  }

  #[test]
  fn timezone_helpers_parse_offsets() {
    assert!(timezone::utc() == TimeZone::UTC);
    let o = timezone::offset(90);
    let z = ZonedDateTime::unsafe_make(0, o);
    assert_eq!(z.to_epoch_millis(), 0);
    assert!(timezone::from_str("Europe/London").is_some());
    assert!(timezone::from_str("+01:00").is_some());
    assert!(timezone::from_str("-05:30").is_some());
    assert!(timezone::from_str("  ").is_none());
    assert!(timezone::from_str("not-a-zone-or-offset").is_none());
  }
}
