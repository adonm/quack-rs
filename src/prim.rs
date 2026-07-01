// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Primitive `DuckDB` conversion helpers.
//!
//! Date/time/timestamp (de)composition, HUGEINT/UHUGEINT/DECIMAL ↔ `f64`,
//! the boolean-time predicates `is_finite_*` that distinguish `±infinity`
//! from finite values, and `duckdb_string_t` introspection.
//!
//! These mirror the `duckdb_*` C-API helpers in the v1.2.0 ext-API vtable.
//!
//! Most extension authors won't need them directly — [`Value`][crate::value::Value]
//! exposes underlying scalar values via the `as_*_raw` accessors — but they're
//! useful for (a) building SQL-level enum constants from Rust, (b) round-trip
//! property tests, and (c) custom-type code that needs to serialise date/time
//! values itself.

use libduckdb_sys::{
    duckdb_date, duckdb_date_struct, duckdb_decimal, duckdb_decimal_to_double, duckdb_double_to_decimal,
    duckdb_double_to_hugeint, duckdb_double_to_uhugeint, duckdb_from_date, duckdb_from_time,
    duckdb_from_time_tz, duckdb_from_timestamp, duckdb_hugeint, duckdb_hugeint_to_double,
    duckdb_is_finite_date, duckdb_is_finite_timestamp, duckdb_string_t, duckdb_string_is_inlined,
    duckdb_string_t_data, duckdb_string_t_length, duckdb_time, duckdb_time_struct, duckdb_time_tz,
    duckdb_time_tz_struct, duckdb_timestamp, duckdb_timestamp_struct, duckdb_to_date, duckdb_to_time,
    duckdb_to_timestamp, duckdb_uhugeint, duckdb_uhugeint_to_double,
};
#[cfg(feature = "duckdb-1-5")]
use libduckdb_sys::{
    duckdb_is_finite_timestamp_ms, duckdb_is_finite_timestamp_ns, duckdb_is_finite_timestamp_s,
    duckdb_timestamp_ms, duckdb_timestamp_ns, duckdb_timestamp_s,
};

/// `DATE` decomposition (`{year, month, day}`).
pub type DateParts = duckdb_date_struct;

/// `TIME` decomposition (`{hour, min, sec, micros}`).
pub type TimeParts = duckdb_time_struct;

/// `TIMESTAMP` decomposition (`{date, time}`).
pub type TimestampParts = duckdb_timestamp_struct;

/// `TIME_TZ` decomposition (`{time: {hour, min, sec, micros}, offset: i32}`).
pub type TimeTzParts = duckdb_time_tz_struct;

// === Date conversions ===

/// Decomposes a `duckdb_date` (`{ days: i32 }` since epoch) to
/// `{year, month, day}`.
///
/// # Safety
/// Requires `DuckDB` runtime to be initialised — call from inside a registered
/// extension. (The C helper relies on the dispatch table.)
#[inline]
#[must_use]
pub unsafe fn from_date(d: duckdb_date) -> DateParts {
    // SAFETY: dispatch-table-backed helper; behaves for any `duckdb_date` value.
    unsafe { duckdb_from_date(d) }
}

/// Composes a `duckdb_date` from `{year, month, day}`.
///
/// # Safety
/// Requires `DuckDB` runtime. `month` must be 1..=12 and `day` 1..=31.
#[inline]
#[must_use]
pub unsafe fn to_date(parts: DateParts) -> duckdb_date {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_to_date(parts) }
}

/// Returns `true` if `d` is a finite date (i.e. not `-infinity` / `infinity`).
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn is_finite_date(d: duckdb_date) -> bool {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_is_finite_date(d) }
}

// === Time conversions ===

/// Decomposes a `duckdb_time` (`{ micros: i64 }` since midnight) to
/// `{hour, min, sec, micros}`.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn from_time(t: duckdb_time) -> TimeParts {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_from_time(t) }
}

/// Composes a `duckdb_time` from `{hour, min, sec, micros}`.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn to_time(parts: TimeParts) -> duckdb_time {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_to_time(parts) }
}

/// Composes a `duckdb_time_tz` from raw micros and an offset (in seconds east
/// of UTC). Useful when building a `TIME_TZ` `Value` for bind parameters.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn create_time_tz(micros: i64, offset: i32) -> duckdb_time_tz {
    // SAFETY: see [`from_date`].
    unsafe { libduckdb_sys::duckdb_create_time_tz(micros, offset) }
}

/// Decomposes a `duckdb_time_tz` to `{time, offset}`.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn from_time_tz(t: duckdb_time_tz) -> TimeTzParts {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_from_time_tz(t) }
}

// === Timestamp conversions ===

/// Decomposes a `duckdb_timestamp` (micros since epoch) to `{date, time}`.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn from_timestamp(ts: duckdb_timestamp) -> TimestampParts {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_from_timestamp(ts) }
}

/// Composes a `duckdb_timestamp` from `{date, time}`.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn to_timestamp(parts: TimestampParts) -> duckdb_timestamp {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_to_timestamp(parts) }
}

/// Returns `true` if `ts` is a finite `TIMESTAMP` (not `±infinity`).
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn is_finite_timestamp(ts: duckdb_timestamp) -> bool {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_is_finite_timestamp(ts) }
}

/// Returns `true` if the `TIMESTAMP_S` value `ts` is finite.
///
/// # Safety
/// Requires `DuckDB` runtime (`duckdb-1-5` feature).
#[cfg(feature = "duckdb-1-5")]
#[inline]
#[must_use]
pub unsafe fn is_finite_timestamp_s(ts: duckdb_timestamp_s) -> bool {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_is_finite_timestamp_s(ts) }
}

/// Returns `true` if the `TIMESTAMP_MS` value `ts` is finite.
///
/// # Safety
/// Requires `DuckDB` runtime (`duckdb-1-5` feature).
#[cfg(feature = "duckdb-1-5")]
#[inline]
#[must_use]
pub unsafe fn is_finite_timestamp_ms(ts: duckdb_timestamp_ms) -> bool {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_is_finite_timestamp_ms(ts) }
}

/// Returns `true` if the `TIMESTAMP_NS` value `ts` is finite.
///
/// # Safety
/// Requires `DuckDB` runtime (`duckdb-1-5` feature).
#[cfg(feature = "duckdb-1-5")]
#[inline]
#[must_use]
pub unsafe fn is_finite_timestamp_ns(ts: duckdb_timestamp_ns) -> bool {
    // SAFETY: see [`from_date`].
    unsafe { duckdb_is_finite_timestamp_ns(ts) }
}

// === HUGEINT/UHUGEINT/DECIMAL conversions ===

/// Converts a `HUGEINT` (`{lower, upper}`) to `f64`.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn hugeint_to_double(h: duckdb_hugeint) -> f64 {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_hugeint_to_double(h) }
}

/// Converts an `f64` to `HUGEINT` (`{lower, upper}`). The result is `i128::MAX`
/// on overflow / `NaN`/`inf` becomes the corresponding bound.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn double_to_hugeint(v: f64) -> duckdb_hugeint {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_double_to_hugeint(v) }
}

/// Converts a `UHUGEINT` (`{lower, upper}`) to `f64`.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn uhugeint_to_double(h: duckdb_uhugeint) -> f64 {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_uhugeint_to_double(h) }
}

/// Converts an `f64` to `UHUGEINT`. The result saturates to `u128::MAX` on
/// overflow.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn double_to_uhugeint(v: f64) -> duckdb_uhugeint {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_double_to_uhugeint(v) }
}

/// Converts an `f64` to `DECIMAL(width, scale)`. Returns `DECIMAL::MAX` on
/// overflow.
///
/// # Safety
/// Requires `DuckDB` runtime. `width` is 1..=38 and `scale` 0..=width.
#[inline]
#[must_use]
pub unsafe fn double_to_decimal(v: f64, width: u8, scale: u8) -> duckdb_decimal {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_double_to_decimal(v, width, scale) }
}

/// Converts a `DECIMAL(width, scale, value)` to `f64`.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn decimal_to_double(d: duckdb_decimal) -> f64 {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_decimal_to_double(d) }
}

// === `duckdb_string_t` introspection ===

/// Returns `true` if a `duckdb_string_t` value stores its payload inline
/// (i.e. ≤ 12 bytes), avoiding a heap allocation.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn string_is_inlined(s: duckdb_string_t) -> bool {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_string_is_inlined(s) }
}

/// Returns the length in bytes of a `duckdb_string_t`'s payload.
///
/// # Safety
/// Requires `DuckDB` runtime.
#[inline]
#[must_use]
pub unsafe fn string_t_length(s: duckdb_string_t) -> u32 {
    // SAFETY: dispatch-table-backed helper.
    unsafe { duckdb_string_t_length(s) }
}

/// Returns a pointer to a `duckdb_string_t`'s payload.
///
/// For inlined strings the pointer is into the `duckdb_string_t` itself; for
/// long strings the pointer is to the heap-allocated buffer. Use
/// [`string_t_length`] for the length.
///
/// # Safety
/// - Requires `DuckDB` runtime.
/// - The returned pointer aliases the string t's storage; valid until that
///   storage is freed (typically for the lifetime of the row in the parent
///   data chunk).
/// - The caller must not write through the returned `*const c_char`.
#[inline]
#[must_use]
pub unsafe fn string_t_data(s: *mut duckdb_string_t) -> *const std::os::raw::c_char {
    // SAFETY: dispatch-table-backed helper; the call doesn't borrow the pointed-to
    // `duckdb_string_t` as a Rust reference, only as a raw pointer.
    unsafe { duckdb_string_t_data(s) }
}