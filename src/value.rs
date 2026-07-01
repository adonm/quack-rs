// SPDX-License-Identifier: MIT
// Copyright 2026 Tom F. <https://github.com/tomtom215/>
// My way of giving something small back to the open source community
// and encouraging more Rust development!

//! RAII wrapper around `DuckDB` values (`duckdb_value`).
//!
//! [`Value`] provides safe, typed access to `DuckDB` values returned from bind
//! parameter extraction, configuration options, and other APIs. It automatically
//! calls [`duckdb_destroy_value`] on drop, eliminating the manual cleanup that
//! every extension author currently has to remember.
//!
//! # Example
//!
//! ```rust,no_run
//! use quack_rs::value::Value;
//! use quack_rs::table::BindInfo;
//! use libduckdb_sys::duckdb_bind_info;
//!
//! unsafe extern "C" fn my_bind(info: duckdb_bind_info) {
//!     let bind = unsafe { BindInfo::new(info) };
//!     // RAII: Value is destroyed automatically when it goes out of scope.
//!     let val = unsafe { Value::from_raw(bind.get_parameter(0)) };
//!     if let Ok(s) = val.as_str() {
//!         // use s...
//!     }
//! }
//! ```

use std::ffi::CStr;
use std::os::raw::c_char;

use crate::error::ExtensionError;
use crate::types::LogicalType;
#[cfg(feature = "duckdb-1-5")]
use libduckdb_sys::{
    duckdb_create_time_ns, duckdb_get_time_ns, duckdb_time_ns, duckdb_value_to_string,
};
use libduckdb_sys::{
    duckdb_blob, duckdb_date, duckdb_decimal, duckdb_destroy_value, duckdb_free,
    duckdb_get_bit, duckdb_get_blob, duckdb_get_bool, duckdb_get_date, duckdb_get_decimal,
    duckdb_get_double, duckdb_get_float, duckdb_get_hugeint, duckdb_get_int16, duckdb_get_int32,
    duckdb_get_int64, duckdb_get_int8, duckdb_get_interval, duckdb_get_list_child,
    duckdb_get_list_size, duckdb_get_map_key, duckdb_get_map_size, duckdb_get_map_value,
    duckdb_get_struct_child, duckdb_get_time, duckdb_get_timestamp, duckdb_get_uhugeint,
    duckdb_get_uint16, duckdb_get_uint32, duckdb_get_uint64, duckdb_get_uint8, duckdb_get_uuid,
    duckdb_get_value_type, duckdb_get_varchar, duckdb_hugeint, duckdb_interval,
    duckdb_is_null_value, duckdb_time, duckdb_timestamp, duckdb_uhugeint, duckdb_value,
};
#[cfg(feature = "duckdb-1-5")]
use libduckdb_sys::{
    duckdb_create_timestamp_s, duckdb_create_timestamp_ms, duckdb_create_timestamp_ns,
    duckdb_create_timestamp_tz, duckdb_get_time_tz, duckdb_get_timestamp_ms,
    duckdb_get_timestamp_ns, duckdb_get_timestamp_s, duckdb_get_timestamp_tz, duckdb_time_tz,
    duckdb_timestamp_ms, duckdb_timestamp_ns, duckdb_timestamp_s,
};
use libduckdb_sys::{
    duckdb_create_bool, duckdb_create_date, duckdb_create_double, duckdb_create_float,
    duckdb_create_hugeint, duckdb_create_int16, duckdb_create_int32, duckdb_create_int64,
    duckdb_create_int8, duckdb_create_interval, duckdb_create_null_value, duckdb_create_timestamp,
    duckdb_create_uint16, duckdb_create_uint32, duckdb_create_uint64, duckdb_create_uint8,
    duckdb_create_uhugeint, duckdb_create_uuid, duckdb_create_varchar,
};

/// An owned, RAII-managed `DuckDB` value.
///
/// When dropped, the underlying `duckdb_value` handle is destroyed via
/// [`duckdb_destroy_value`]. This eliminates the manual `duckdb_destroy_value`
/// calls that are easy to forget and lead to memory leaks.
///
/// # Creation
///
/// Obtain a `Value` from:
/// - [`BindInfo::get_parameter_value`][crate::table::BindInfo::get_parameter_value]
/// - [`BindInfo::get_named_parameter_value`][crate::table::BindInfo::get_named_parameter_value]
/// - [`Value::from_raw`] (escape hatch for raw `duckdb_value` handles)
///
/// # Extraction
///
/// Use typed accessors to extract the underlying data:
/// - [`as_str`][Value::as_str] — `VARCHAR` → `String`
/// - [`as_i32`][Value::as_i32] — `INTEGER` → `i32`
/// - [`as_i64`][Value::as_i64] — `BIGINT` → `i64`
/// - [`as_f32`][Value::as_f32] — `FLOAT` → `f32`
/// - [`as_f64`][Value::as_f64] — `DOUBLE` → `f64`
/// - [`as_bool`][Value::as_bool] — `BOOLEAN` → `bool`
pub struct Value {
    raw: duckdb_value,
}

impl Value {
    /// Wraps a raw `duckdb_value` handle.
    ///
    /// The returned `Value` takes ownership and will call `duckdb_destroy_value`
    /// on drop.
    ///
    /// # Safety
    ///
    /// `raw` must be a valid `duckdb_value` obtained from a `DuckDB` API call
    /// (e.g., `duckdb_bind_get_parameter`). The caller must not destroy the
    /// value after passing it to this function.
    #[inline]
    #[must_use]
    pub const unsafe fn from_raw(raw: duckdb_value) -> Self {
        Self { raw }
    }

    /// Extracts the value as a `String` (`VARCHAR`).
    ///
    /// Internally calls `duckdb_get_varchar` and frees the returned C string
    /// with `duckdb_free`. Returns an error if the string is not valid UTF-8
    /// or if the value handle is null.
    ///
    /// # Errors
    ///
    /// Returns `ExtensionError` if the value is null or contains invalid UTF-8.
    pub fn as_str(&self) -> Result<String, ExtensionError> {
        if self.raw.is_null() {
            return Err(ExtensionError::new("Value is null"));
        }
        // SAFETY: self.raw is a valid duckdb_value per constructor contract.
        let c_str: *mut c_char = unsafe { duckdb_get_varchar(self.raw) };
        if c_str.is_null() {
            return Err(ExtensionError::new("duckdb_get_varchar returned null"));
        }
        // SAFETY: c_str is a valid null-terminated C string allocated by DuckDB.
        let result = unsafe { CStr::from_ptr(c_str) }
            .to_str()
            .map(str::to_owned)
            .map_err(|_| ExtensionError::new("Value contains invalid UTF-8"));
        // SAFETY: c_str was allocated by DuckDB and must be freed with duckdb_free.
        unsafe { duckdb_free(c_str.cast()) };
        result
    }

    /// Extracts the value as an owned `Vec<u8>` (`BLOB`), binary-safe.
    ///
    /// `DuckDB` allocates the blob's backing buffer; this method copies it into
    /// an owned `Vec<u8>` and frees the original with `duckdb_free`. There is no
    /// UTF-8 validation, so this is the correct way to read binary bind
    /// parameters (e.g. a WKB geometry passed to a set-returning table function).
    ///
    /// # Errors
    ///
    /// Returns `ExtensionError` if the value handle is null or `duckdb_get_blob`
    /// returns a null data pointer for a non-empty size.
    pub fn as_blob(&self) -> Result<Vec<u8>, ExtensionError> {
        if self.raw.is_null() {
            return Err(ExtensionError::new("Value is null"));
        }
        // SAFETY: self.raw is a valid duckdb_value per constructor contract.
        let blob: duckdb_blob = unsafe { duckdb_get_blob(self.raw) };
        if blob.data.is_null() {
            return if blob.size == 0 {
                Ok(Vec::new())
            } else {
                Err(ExtensionError::new("duckdb_get_blob returned null data"))
            };
        }
        // SAFETY: blob.data is a DuckDB-allocated buffer of exactly `blob.size`
        // bytes, valid until we free it below.
        let slice = unsafe {
            std::slice::from_raw_parts(blob.data.cast::<u8>(), usize::try_from(blob.size).unwrap_or(0))
        };
        let out = slice.to_vec();
        // SAFETY: blob.data was allocated by DuckDB and must be freed with duckdb_free.
        unsafe { duckdb_free(blob.data.cast()) };
        Ok(out)
    }

    /// Extracts the value as an `i32` (`INTEGER`).
    ///
    /// `DuckDB` will attempt to cast the value to `INTEGER`. If the value is not
    /// numeric, this returns 0.
    #[inline]
    #[must_use]
    pub fn as_i32(&self) -> i32 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_int32(self.raw) }
    }

    /// Extracts the value as an `i64` (`BIGINT`).
    ///
    /// `DuckDB` will attempt to cast the value to `BIGINT`. If the value is not
    /// numeric, this returns 0.
    #[inline]
    #[must_use]
    pub fn as_i64(&self) -> i64 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_int64(self.raw) }
    }

    /// Extracts the value as an `f32` (`FLOAT`).
    ///
    /// `DuckDB` will attempt to cast the value to `FLOAT`. If the value is not
    /// numeric, this returns 0.0.
    #[inline]
    #[must_use]
    pub fn as_f32(&self) -> f32 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_float(self.raw) }
    }

    /// Extracts the value as an `f64` (`DOUBLE`).
    ///
    /// `DuckDB` will attempt to cast the value to `DOUBLE`. If the value is not
    /// numeric, this returns 0.0.
    #[inline]
    #[must_use]
    pub fn as_f64(&self) -> f64 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_double(self.raw) }
    }

    /// Extracts the value as a `bool` (`BOOLEAN`).
    ///
    /// `DuckDB` will attempt to cast the value to `BOOLEAN`. If the value is not
    /// convertible, this returns `false`.
    #[inline]
    #[must_use]
    pub fn as_bool(&self) -> bool {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_bool(self.raw) }
    }

    /// Extracts the value as an `i8` (`TINYINT`).
    ///
    /// `DuckDB` will attempt to cast the value to `TINYINT`. If the value is not
    /// numeric, this returns 0.
    #[inline]
    #[must_use]
    pub fn as_i8(&self) -> i8 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_int8(self.raw) }
    }

    /// Extracts the value as an `i16` (`SMALLINT`).
    ///
    /// `DuckDB` will attempt to cast the value to `SMALLINT`. If the value is not
    /// numeric, this returns 0.
    #[inline]
    #[must_use]
    pub fn as_i16(&self) -> i16 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_int16(self.raw) }
    }

    /// Extracts the value as a `u8` (`UTINYINT`).
    ///
    /// `DuckDB` will attempt to cast the value to `UTINYINT`. If the value is not
    /// numeric, this returns 0.
    #[inline]
    #[must_use]
    pub fn as_u8(&self) -> u8 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_uint8(self.raw) }
    }

    /// Extracts the value as a `u16` (`USMALLINT`).
    ///
    /// `DuckDB` will attempt to cast the value to `USMALLINT`. If the value is not
    /// numeric, this returns 0.
    #[inline]
    #[must_use]
    pub fn as_u16(&self) -> u16 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_uint16(self.raw) }
    }

    /// Extracts the value as a `u32` (`UINTEGER`).
    ///
    /// `DuckDB` will attempt to cast the value to `UINTEGER`. If the value is not
    /// numeric, this returns 0.
    #[inline]
    #[must_use]
    pub fn as_u32(&self) -> u32 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_uint32(self.raw) }
    }

    /// Extracts the value as a `u64` (`UBIGINT`).
    ///
    /// `DuckDB` will attempt to cast the value to `UBIGINT`. If the value is not
    /// numeric, this returns 0.
    #[inline]
    #[must_use]
    pub fn as_u64(&self) -> u64 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_uint64(self.raw) }
    }

    /// Extracts the value as an `i128` (`HUGEINT`).
    ///
    /// `DuckDB` returns `HUGEINT` as `{ lower: u64, upper: i64 }`. This method
    /// reconstructs the full `i128` value.
    #[inline]
    #[must_use]
    pub fn as_i128(&self) -> i128 {
        // SAFETY: self.raw is valid per constructor contract.
        let h = unsafe { duckdb_get_hugeint(self.raw) };
        i128::from(h.upper) << 64 | i128::from(h.lower)
    }

    /// Extracts the value as a `String`, returning `default` on failure.
    ///
    /// Convenience for `val.as_str().unwrap_or_else(|_| default.to_owned())`.
    #[inline]
    #[must_use]
    pub fn as_str_or(&self, default: &str) -> String {
        self.as_str().unwrap_or_else(|_| default.to_owned())
    }

    /// Extracts the value as a `String`, returning an empty string on failure.
    ///
    /// Convenience for `val.as_str().unwrap_or_default()`.
    #[inline]
    #[must_use]
    pub fn as_str_or_default(&self) -> String {
        self.as_str().unwrap_or_default()
    }

    /// Extracts the value as an `i32`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_i32_or(&self, default: i32) -> i32 {
        if self.is_null() {
            default
        } else {
            self.as_i32()
        }
    }

    /// Extracts the value as an `i64`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_i64_or(&self, default: i64) -> i64 {
        if self.is_null() {
            default
        } else {
            self.as_i64()
        }
    }

    /// Extracts the value as an `f32`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_f32_or(&self, default: f32) -> f32 {
        if self.is_null() {
            default
        } else {
            self.as_f32()
        }
    }

    /// Extracts the value as an `f64`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_f64_or(&self, default: f64) -> f64 {
        if self.is_null() {
            default
        } else {
            self.as_f64()
        }
    }

    /// Extracts the value as a `bool`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_bool_or(&self, default: bool) -> bool {
        if self.is_null() {
            default
        } else {
            self.as_bool()
        }
    }

    /// Extracts the value as an `i8`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_i8_or(&self, default: i8) -> i8 {
        if self.is_null() {
            default
        } else {
            self.as_i8()
        }
    }

    /// Extracts the value as an `i16`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_i16_or(&self, default: i16) -> i16 {
        if self.is_null() {
            default
        } else {
            self.as_i16()
        }
    }

    /// Extracts the value as a `u8`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_u8_or(&self, default: u8) -> u8 {
        if self.is_null() {
            default
        } else {
            self.as_u8()
        }
    }

    /// Extracts the value as a `u16`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_u16_or(&self, default: u16) -> u16 {
        if self.is_null() {
            default
        } else {
            self.as_u16()
        }
    }

    /// Extracts the value as a `u32`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_u32_or(&self, default: u32) -> u32 {
        if self.is_null() {
            default
        } else {
            self.as_u32()
        }
    }

    /// Extracts the value as a `u64`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_u64_or(&self, default: u64) -> u64 {
        if self.is_null() {
            default
        } else {
            self.as_u64()
        }
    }

    /// Extracts the value as an `i128`, returning `default` if the handle is null.
    #[inline]
    #[must_use]
    pub fn as_i128_or(&self, default: i128) -> i128 {
        if self.is_null() {
            default
        } else {
            self.as_i128()
        }
    }

    /// Creates a `TIME_NS` value (time of day with nanosecond precision) from a
    /// raw nanosecond count (`DuckDB` 1.5.0+).
    ///
    /// Pairs with [`as_time_ns`][Value::as_time_ns] and the
    /// [`TypeId::TimeNs`][crate::types::TypeId::TimeNs] column type.
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn time_ns(nanos: i64) -> Self {
        // SAFETY: duckdb_create_time_ns accepts any nanosecond count and returns
        // an owned duckdb_value.
        let raw = unsafe { duckdb_create_time_ns(duckdb_time_ns { nanos }) };
        Self { raw }
    }

    /// Extracts the value as a `TIME_NS` nanosecond count (`DuckDB` 1.5.0+).
    ///
    /// Returns 0 if the value is not a `TIME_NS`.
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn as_time_ns(&self) -> i64 {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_time_ns(self.raw) }.nanos
    }

    /// Returns the canonical string representation of this value, as `DuckDB`
    /// would render it (`DuckDB` 1.5.0+).
    ///
    /// Returns `None` if the handle is null or the rendered text is not valid
    /// UTF-8. This is primarily useful for diagnostics and error messages, where
    /// it works for any value type (not just `VARCHAR`).
    #[cfg(feature = "duckdb-1-5")]
    #[must_use]
    pub fn display_string(&self) -> Option<String> {
        if self.raw.is_null() {
            return None;
        }
        // SAFETY: self.raw is a valid duckdb_value per constructor contract.
        let c_str: *mut c_char = unsafe { duckdb_value_to_string(self.raw) };
        if c_str.is_null() {
            return None;
        }
        // SAFETY: c_str is a valid null-terminated string allocated by DuckDB.
        let result = unsafe { CStr::from_ptr(c_str) }
            .to_str()
            .ok()
            .map(str::to_owned);
        // SAFETY: c_str was allocated by DuckDB and must be freed with duckdb_free.
        unsafe { duckdb_free(c_str.cast()) };
        result
    }

    // === Date / time / interval accessors ===

    /// Extracts the value as a `duckdb_date` (`{ days: i32 }` since the epoch).
    ///
    /// `DuckDB` will attempt to cast the value to `DATE`. Returns 0 if not convertible.
    #[inline]
    #[must_use]
    pub fn as_date_raw(&self) -> duckdb_date {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_date(self.raw) }
    }

    /// Extracts the value as a `duckdb_time` (`{ micros: i64 }` since midnight).
    ///
    /// `DuckDB` will attempt to cast the value to `TIME`. Returns 0 if not convertible.
    #[inline]
    #[must_use]
    pub fn as_time_raw(&self) -> duckdb_time {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_time(self.raw) }
    }

    /// Extracts the value as a `duckdb_timestamp` (`{ micros: i64 }` since the epoch).
    ///
    /// `DuckDB` will attempt to cast the value to `TIMESTAMP`. Returns 0 if not convertible.
    #[inline]
    #[must_use]
    pub fn as_timestamp_raw(&self) -> duckdb_timestamp {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_timestamp(self.raw) }
    }

    /// Extracts the value as an `INTERVAL` (`{ months, days, micros }`).
    #[inline]
    #[must_use]
    pub fn as_interval_raw(&self) -> duckdb_interval {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_interval(self.raw) }
    }

    /// Extracts the value as a `u128` (`UHUGEINT`).
    ///
    /// `DuckDB` returns `UHUGEINT` as `{ lower: u64, upper: u64 }`.
    #[inline]
    #[must_use]
    pub fn as_u128(&self) -> u128 {
        // SAFETY: self.raw is valid per constructor contract.
        let h = unsafe { duckdb_get_uhugeint(self.raw) };
        u128::from(h.lower) | (u128::from(h.upper) << 64)
    }

    /// Extracts the value as a `DECIMAL` (`{ width, scale, value: hugeint }`).
    #[inline]
    #[must_use]
    pub fn as_decimal_raw(&self) -> duckdb_decimal {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_decimal(self.raw) }
    }

    /// Extracts the value as a `UUID` (`duckdb_uhugeint` → `u128`).
    #[inline]
    #[must_use]
    pub fn as_uuid(&self) -> u128 {
        // SAFETY: self.raw is valid per constructor contract.
        let h = unsafe { duckdb_get_uuid(self.raw) };
        u128::from(h.lower) | (u128::from(h.upper) << 64)
    }

    /// Extracts the value as a BIT — `(padding_byte, padded_data)`.
    ///
    /// `DuckDB` returns `duckdb_bit { data: *mut u8, size: idx_t }`; the first byte
    /// holds the number of padding bits (0..7) and the remaining `size-1` bytes
    /// are the big-endian bit vector, MSB first. Returns an owned `Vec<u8>`.
    ///
    /// # Errors
    ///
    /// Returns `ExtensionError` if the handle is null or `data` is null for a non-empty size.
    pub fn as_bit(&self) -> Result<Vec<u8>, ExtensionError> {
        if self.raw.is_null() {
            return Err(ExtensionError::new("Value is null"));
        }
        // SAFETY: self.raw is valid per constructor contract.
        let bit: libduckdb_sys::duckdb_bit = unsafe { duckdb_get_bit(self.raw) };
        if bit.data.is_null() {
            return if bit.size == 0 {
                Ok(Vec::new())
            } else {
                Err(ExtensionError::new("duckdb_get_bit returned null data"))
            };
        }
        // SAFETY: bit.data is a DuckDB-allocated buffer of exactly `bit.size` bytes.
        let slice = unsafe {
            std::slice::from_raw_parts(bit.data.cast::<u8>(), usize::try_from(bit.size).unwrap_or(0))
        };
        let out = slice.to_vec();
        // SAFETY: bit.data was allocated by DuckDB and must be freed with duckdb_free.
        unsafe { duckdb_free(bit.data.cast()) };
        Ok(out)
    }

    // === Timestamp variants (TIME_TZ, TIMESTAMP_S/MS/NS) ===

    /// Extracts the value as a `TIME_TZ` (`{ bits: u64 }` → microseconds + offset).
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn as_time_tz_raw(&self) -> duckdb_time_tz {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_time_tz(self.raw) }
    }

    /// Extracts the value as a `TIMESTAMP_TZ` (micros since epoch, same layout as `duckdb_timestamp`).
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn as_timestamp_tz_raw(&self) -> duckdb_timestamp {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_timestamp_tz(self.raw) }
    }

    /// Extracts the value as a `TIMESTAMP_S` (`{ seconds: i64 }` since the epoch).
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn as_timestamp_s_raw(&self) -> duckdb_timestamp_s {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_timestamp_s(self.raw) }
    }

    /// Extracts the value as a `TIMESTAMP_MS` (`{ millis: i64 }` since the epoch).
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn as_timestamp_ms_raw(&self) -> duckdb_timestamp_ms {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_timestamp_ms(self.raw) }
    }

    /// Extracts the value as a `TIMESTAMP_NS` (`{ nanos: i64 }` since the epoch).
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn as_timestamp_ns_raw(&self) -> duckdb_timestamp_ns {
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_get_timestamp_ns(self.raw) }
    }

    // === Compound-type navigation ===

    /// Returns the logical type of this value (RAII `LogicalType`).
    ///
    /// Returns `None` if the underlying handle is null.
    #[must_use]
    pub fn value_type(&self) -> Option<LogicalType> {
        if self.raw.is_null() {
            return None;
        }
        // SAFETY: self.raw is valid per constructor contract.
        let raw = unsafe { duckdb_get_value_type(self.raw) };
        if raw.is_null() {
            return None;
        }
        Some(unsafe { LogicalType::from_raw(raw) })
    }

    /// Returns `true` if this value is `SQL NULL`.
    ///
    /// `DuckDB` distinguishes the `SQL NULL` marker from non-null values of any type;
    /// this is *not* the same as a null `Value` handle (see [`is_null`][Value::is_null]).
    #[inline]
    #[must_use]
    pub fn is_sql_null(&self) -> bool {
        if self.raw.is_null() {
            return false;
        }
        // SAFETY: self.raw is valid per constructor contract.
        unsafe { duckdb_is_null_value(self.raw) }
    }

    /// Returns the number of entries in a MAP value.
    ///
    /// Returns 0 for non-MAP values or a null handle.
    #[inline]
    #[must_use]
    pub fn map_size(&self) -> usize {
        if self.raw.is_null() {
            return 0;
        }
        // SAFETY: self.raw is valid per constructor contract.
        usize::try_from(unsafe { duckdb_get_map_size(self.raw) }).unwrap_or(0)
    }

    /// Returns the key at `index` of a MAP value as an owned `Value`.
    ///
    /// Returns `None` if the handle is null, the value is not a MAP, or the index is out of bounds.
    #[must_use]
    pub fn map_key(&self, index: usize) -> Option<Self> {
        if self.raw.is_null() {
            return None;
        }
        let idx = libduckdb_sys::idx_t::try_from(index).ok()?;
        // SAFETY: self.raw is valid per constructor contract; DuckDB returns a fresh owned value.
        let raw = unsafe { duckdb_get_map_key(self.raw, idx) };
        if raw.is_null() {
            return None;
        }
        Some(unsafe { Self::from_raw(raw) })
    }

    /// Returns the value at `index` of a MAP value as an owned `Value`.
    ///
    /// Returns `None` if the handle is null, the value is not a MAP, or the index is out of bounds.
    #[must_use]
    pub fn map_at(&self, index: usize) -> Option<Self> {
        if self.raw.is_null() {
            return None;
        }
        let idx = libduckdb_sys::idx_t::try_from(index).ok()?;
        // SAFETY: self.raw is valid per constructor contract; DuckDB returns a fresh owned value.
        let raw = unsafe { duckdb_get_map_value(self.raw, idx) };
        if raw.is_null() {
            return None;
        }
        Some(unsafe { Self::from_raw(raw) })
    }

    /// Returns the number of children in a LIST value.
    ///
    /// Returns 0 for non-LIST values or a null handle.
    #[inline]
    #[must_use]
    pub fn list_size(&self) -> usize {
        if self.raw.is_null() {
            return 0;
        }
        // SAFETY: self.raw is valid per constructor contract.
        usize::try_from(unsafe { duckdb_get_list_size(self.raw) }).unwrap_or(0)
    }

    /// Returns the child at `index` of a LIST value as an owned `Value`.
    ///
    /// Returns `None` if the handle is null, the value is not a LIST, or the index is out of bounds.
    #[must_use]
    pub fn list_child(&self, index: usize) -> Option<Self> {
        if self.raw.is_null() {
            return None;
        }
        let idx = libduckdb_sys::idx_t::try_from(index).ok()?;
        // SAFETY: self.raw is valid per constructor contract; DuckDB returns a fresh owned value.
        let raw = unsafe { duckdb_get_list_child(self.raw, idx) };
        if raw.is_null() {
            return None;
        }
        Some(unsafe { Self::from_raw(raw) })
    }

    /// Returns the child at `index` of a STRUCT value as an owned `Value`.
    ///
    /// Returns `None` if the handle is null, the value is not a STRUCT, or the index is out of bounds.
    #[must_use]
    pub fn struct_child(&self, index: usize) -> Option<Self> {
        if self.raw.is_null() {
            return None;
        }
        let idx = libduckdb_sys::idx_t::try_from(index).ok()?;
        // SAFETY: self.raw is valid per constructor contract; DuckDB returns a fresh owned value.
        let raw = unsafe { duckdb_get_struct_child(self.raw, idx) };
        if raw.is_null() {
            return None;
        }
        Some(unsafe { Self::from_raw(raw) })
    }

    /// Returns `true` if the underlying handle is null.
    #[inline]
    #[must_use]
    pub const fn is_null(&self) -> bool {
        self.raw.is_null()
    }

    /// Returns the raw `duckdb_value` handle without consuming the `Value`.
    ///
    /// The `Value` still owns the handle and will destroy it on drop.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_value {
        self.raw
    }

    /// Consumes the `Value` and returns the raw `duckdb_value` handle.
    ///
    /// The caller takes ownership and is responsible for calling
    /// `duckdb_destroy_value` when done.
    #[inline]
    #[must_use]
    pub const fn into_raw(self) -> duckdb_value {
        let raw = self.raw;
        std::mem::forget(self);
        raw
    }

    // === Typed constructors (raw → owned Value) ===

    /// Creates a `BOOLEAN` `Value` from a `bool`.
    #[inline]
    #[must_use]
    pub fn boolean(v: bool) -> Self {
        // SAFETY: duckdb_create_bool accepts any bool and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_bool(v) };
        Self { raw }
    }

    /// Creates a `VARCHAR` `Value` from a Rust string.
    ///
    /// # Panics
    ///
    /// Panics if `s` contains an interior `NUL` byte (`\0`), as `DuckDB` `VARCHAR`
    /// is `NUL`-terminated.
    #[must_use]
    pub fn varchar(s: &str) -> Self {
        // SAFETY: duckdb_create_varchar copies the C string into an owned value.
        let raw = unsafe {
            duckdb_create_varchar(
                std::ffi::CString::new(s).expect("varchar contains a NUL byte").as_ptr(),
            )
        };
        Self { raw }
    }

    /// Creates an `i8` (`TINYINT`) `Value`.
    #[inline]
    #[must_use]
    pub fn tinyint(v: i8) -> Self {
        // SAFETY: duckdb_create_int8 accepts any i8 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_int8(v) };
        Self { raw }
    }

    /// Creates an `i16` (`SMALLINT`) `Value`.
    #[inline]
    #[must_use]
    pub fn smallint(v: i16) -> Self {
        // SAFETY: duckdb_create_int16 accepts any i16 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_int16(v) };
        Self { raw }
    }

    /// Creates an `i32` (`INTEGER`) `Value`.
    #[inline]
    #[must_use]
    pub fn integer(v: i32) -> Self {
        // SAFETY: duckdb_create_int32 accepts any i32 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_int32(v) };
        Self { raw }
    }

    /// Creates an `i64` (`BIGINT`) `Value`.
    #[inline]
    #[must_use]
    pub fn bigint(v: i64) -> Self {
        // SAFETY: duckdb_create_int64 accepts any i64 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_int64(v) };
        Self { raw }
    }

    /// Creates an `i128` (`HUGEINT`) `Value` from its `{lower, upper}` halves.
    #[inline]
    #[must_use]
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn hugeint(v: i128) -> Self {
        // SAFETY: duckdb_create_hugeint accepts any {lower, upper} pair and returns
        // an owned duckdb_value.
        let h = duckdb_hugeint {
            lower: v as u64,
            upper: (v >> 64) as i64,
        };
        let raw = unsafe { duckdb_create_hugeint(h) };
        Self { raw }
    }

    /// Creates a `u8` (`UTINYINT`) `Value`.
    #[inline]
    #[must_use]
    pub fn utinyint(v: u8) -> Self {
        // SAFETY: duckdb_create_uint8 accepts any u8 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_uint8(v) };
        Self { raw }
    }

    /// Creates a `u16` (`USMALLINT`) `Value`.
    #[inline]
    #[must_use]
    pub fn usmallint(v: u16) -> Self {
        // SAFETY: duckdb_create_uint16 accepts any u16 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_uint16(v) };
        Self { raw }
    }

    /// Creates a `u32` (`UINTEGER`) `Value`.
    #[inline]
    #[must_use]
    pub fn uinteger(v: u32) -> Self {
        // SAFETY: duckdb_create_uint32 accepts any u32 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_uint32(v) };
        Self { raw }
    }

    /// Creates a `u64` (`UBIGINT`) `Value`.
    #[inline]
    #[must_use]
    pub fn ubigint(v: u64) -> Self {
        // SAFETY: duckdb_create_uint64 accepts any u64 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_uint64(v) };
        Self { raw }
    }

    /// Creates a `u128` (`UHUGEINT`) `Value` from its `{lower, upper}` halves.
    #[inline]
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn uhugeint(v: u128) -> Self {
        // SAFETY: duckdb_create_uhugeint accepts any {lower, upper} pair and returns
        // an owned duckdb_value.
        let h = duckdb_uhugeint {
            lower: v as u64,
            upper: (v >> 64) as u64,
        };
        let raw = unsafe { duckdb_create_uhugeint(h) };
        Self { raw }
    }

    /// Creates an `f32` (`FLOAT`) `Value`.
    #[inline]
    #[must_use]
    pub fn float(v: f32) -> Self {
        // SAFETY: duckdb_create_float accepts any f32 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_float(v) };
        Self { raw }
    }

    /// Creates an `f64` (`DOUBLE`) `Value`.
    #[inline]
    #[must_use]
    pub fn double(v: f64) -> Self {
        // SAFETY: duckdb_create_double accepts any f64 and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_double(v) };
        Self { raw }
    }

    /// Creates a `DATE` `Value` from a `duckdb_date` (`{ days: i32 }` since epoch).
    #[inline]
    #[must_use]
    pub fn date(v: duckdb_date) -> Self {
        // SAFETY: duckdb_create_date accepts any {days} and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_date(v) };
        Self { raw }
    }

    /// Creates a `TIMESTAMP` `Value` from a `duckdb_timestamp` (`{ micros: i64 }`).
    #[inline]
    #[must_use]
    pub fn timestamp(v: duckdb_timestamp) -> Self {
        // SAFETY: duckdb_create_timestamp accepts any micros and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_timestamp(v) };
        Self { raw }
    }

    /// Creates a `TIMESTAMP_TZ` `Value` from a `duckdb_timestamp` `DuckDB` 1.5.0+.
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn timestamp_tz(v: duckdb_timestamp) -> Self {
        // SAFETY: duckdb_create_timestamp_tz accepts any micros and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_timestamp_tz(v) };
        Self { raw }
    }

    /// Creates a `TIMESTAMP_S` `Value` from a `duckdb_timestamp_s` `DuckDB` 1.5.0+.
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn timestamp_s(v: duckdb_timestamp_s) -> Self {
        // SAFETY: duckdb_create_timestamp_s accepts any {seconds} and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_timestamp_s(v) };
        Self { raw }
    }

    /// Creates a `TIMESTAMP_MS` `Value` from a `duckdb_timestamp_ms` `DuckDB` 1.5.0+.
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn timestamp_ms(v: duckdb_timestamp_ms) -> Self {
        // SAFETY: duckdb_create_timestamp_ms accepts any {millis} and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_timestamp_ms(v) };
        Self { raw }
    }

    /// Creates a `TIMESTAMP_NS` `Value` from a `duckdb_timestamp_ns` `DuckDB` 1.5.0+.
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn timestamp_ns(v: duckdb_timestamp_ns) -> Self {
        // SAFETY: duckdb_create_timestamp_ns accepts any {nanos} and returns an owned duckdb_value.
        let raw = unsafe { duckdb_create_timestamp_ns(v) };
        Self { raw }
    }

    /// Creates an `INTERVAL` `Value` from `{ months, days, micros }`.
    #[inline]
    #[must_use]
    pub fn interval(months: i32, days: i32, micros: i64) -> Self {
        // SAFETY: duckdb_create_interval accepts any {months, days, micros} triple
        // and returns an owned duckdb_value.
        let raw = unsafe {
            duckdb_create_interval(duckdb_interval {
                months,
                days,
                micros,
            })
        };
        Self { raw }
    }

    /// Creates a `UUID` `Value` from a `u128`.
    #[inline]
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn uuid(v: u128) -> Self {
        // SAFETY: duckdb_create_uuid accepts any {lower, upper} pair and returns
        // an owned duckdb_value.
        let h = duckdb_uhugeint {
            lower: v as u64,
            upper: (v >> 64) as u64,
        };
        let raw = unsafe { duckdb_create_uuid(h) };
        Self { raw }
    }

    /// Creates a `SQL NULL` `Value` (the NULL marker, distinct from a null handle).
    #[inline]
    #[must_use]
    pub fn sql_null() -> Self {
        // SAFETY: duckdb_create_null_value returns an owned duckdb_value representing SQL NULL.
        let raw = unsafe { duckdb_create_null_value() };
        Self { raw }
    }

    /// Creates a `BLOB` `Value` from raw bytes.
    #[must_use]
    pub fn blob(data: &[u8]) -> Self {
        // SAFETY: duckdb_create_blob copies the buffer into an owned duckdb_value.
        let raw = unsafe {
            libduckdb_sys::duckdb_create_blob(
                data.as_ptr(),
                libduckdb_sys::idx_t::try_from(data.len()).unwrap_or(libduckdb_sys::idx_t::MAX),
            )
        };
        Self { raw }
    }

    /// Creates a `TIME` `Value` from a `duckdb_time` (`{ micros: i64 }` since midnight).
    #[inline]
    #[must_use]
    pub fn time(v: duckdb_time) -> Self {
        let raw = unsafe { libduckdb_sys::duckdb_create_time(v) };
        Self { raw }
    }

    /// Creates a `DECIMAL` `Value` from `{ width, scale, value: hugeint }`.
    #[inline]
    #[must_use]
    pub fn decimal(v: duckdb_decimal) -> Self {
        let raw = unsafe { libduckdb_sys::duckdb_create_decimal(v) };
        Self { raw }
    }

    /// Creates a `TIME_TZ` `Value` from a `duckdb_time_tz` (`DuckDB` 1.5+).
    #[cfg(feature = "duckdb-1-5")]
    #[inline]
    #[must_use]
    pub fn time_tz(v: libduckdb_sys::duckdb_time_tz) -> Self {
        let raw = unsafe { libduckdb_sys::duckdb_create_time_tz_value(v) };
        Self { raw }
    }

    /// Creates a `BIT` `Value` from a raw byte buffer (`DuckDB` copies it).
    #[must_use]
    pub fn bit(data: &[u8]) -> Self {
        let bit = libduckdb_sys::duckdb_bit {
            data: data.as_ptr().cast_mut(),
            size: libduckdb_sys::idx_t::try_from(data.len()).unwrap_or(libduckdb_sys::idx_t::MAX),
        };
        let raw = unsafe { libduckdb_sys::duckdb_create_bit(bit) };
        Self { raw }
    }

    /// Creates a `STRUCT` `Value` from its logical type and the child `Value`s
    /// in field-declaration order. Takes ownership of each consumed `Value`.
    #[must_use]
    pub fn struct_value(logical_type: &LogicalType, values: Vec<Self>) -> Self {
        let mut raws: Vec<libduckdb_sys::duckdb_value> =
            values.into_iter().map(Self::into_raw).collect();
        let raw = unsafe {
            libduckdb_sys::duckdb_create_struct_value(logical_type.as_raw(), raws.as_mut_ptr())
        };
        Self { raw }
    }

    /// Creates a `LIST` `Value` from its child logical type and element `Value`s.
    /// Takes ownership of each consumed `Value`.
    #[must_use]
    pub fn list_value(logical_type: &LogicalType, values: Vec<Self>) -> Self {
        let mut raws: Vec<libduckdb_sys::duckdb_value> =
            values.into_iter().map(Self::into_raw).collect();
        let count =
            libduckdb_sys::idx_t::try_from(raws.len()).unwrap_or(libduckdb_sys::idx_t::MAX);
        let raw = unsafe {
            libduckdb_sys::duckdb_create_list_value(logical_type.as_raw(), raws.as_mut_ptr(), count)
        };
        Self { raw }
    }

    /// Creates an `ARRAY` `Value` of fixed size. Takes ownership of each
    /// consumed `Value`.
    #[must_use]
    pub fn array_value(logical_type: &LogicalType, values: Vec<Self>) -> Self {
        let mut raws: Vec<libduckdb_sys::duckdb_value> =
            values.into_iter().map(Self::into_raw).collect();
        let count =
            libduckdb_sys::idx_t::try_from(raws.len()).unwrap_or(libduckdb_sys::idx_t::MAX);
        let raw = unsafe {
            libduckdb_sys::duckdb_create_array_value(
                logical_type.as_raw(),
                raws.as_mut_ptr(),
                count,
            )
        };
        Self { raw }
    }

    /// Creates an `ENUM` `Value` from its logical type and the dictionary index.
    #[inline]
    #[must_use]
    pub fn enum_value(logical_type: &LogicalType, index: u64) -> Self {
        let raw = unsafe { libduckdb_sys::duckdb_create_enum_value(logical_type.as_raw(), index) };
        Self { raw }
    }

    /// Returns the dictionary index of an `ENUM` `Value` (0 for non-ENUM values).
    #[inline]
    #[must_use]
    pub fn as_enum_value(&self) -> u64 {
        unsafe { libduckdb_sys::duckdb_get_enum_value(self.raw) }
    }

    /// Creates a `MAP` `Value` from its logical type and parallel key/value
    /// slices. Takes ownership of each consumed `Value`.
    #[must_use]
    pub fn map_value(logical_type: &LogicalType, keys: Vec<Self>, values: Vec<Self>) -> Self {
        let mut keys_raw: Vec<libduckdb_sys::duckdb_value> =
            keys.into_iter().map(Self::into_raw).collect();
        let mut vals_raw: Vec<libduckdb_sys::duckdb_value> =
            values.into_iter().map(Self::into_raw).collect();
        let count = libduckdb_sys::idx_t::try_from(keys_raw.len())
            .unwrap_or(libduckdb_sys::idx_t::MAX);
        let raw = unsafe {
            libduckdb_sys::duckdb_create_map_value(
                logical_type.as_raw(),
                keys_raw.as_mut_ptr(),
                vals_raw.as_mut_ptr(),
                count,
            )
        };
        Self { raw }
    }

    /// Creates a `UNION` `Value` from its logical type, the active member's
    /// tag index, and the active member's `Value` (`DuckDB` 1.5+). Takes
    /// ownership of `value`.
    #[cfg(feature = "duckdb-1-5")]
    #[must_use]
    pub fn union_value(logical_type: &LogicalType, tag_index: u64, value: Self) -> Self {
        let raw_val = Self::into_raw(value);
        let raw = unsafe {
            libduckdb_sys::duckdb_create_union_value(
                logical_type.as_raw(),
                libduckdb_sys::idx_t::try_from(tag_index).unwrap_or(libduckdb_sys::idx_t::MAX),
                raw_val,
            )
        };
        Self { raw }
    }
}

impl Drop for Value {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: self.raw is a valid duckdb_value that we own.
            unsafe { duckdb_destroy_value(&raw mut self.raw) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_value_is_null() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert!(val.is_null());
    }

    #[test]
    fn null_value_as_str_returns_error() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert!(val.as_str().is_err());
    }

    #[test]
    fn into_raw_prevents_double_free() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        let raw = val.into_raw();
        assert!(raw.is_null());
        // No double-free: Value was forgotten via into_raw.
    }

    #[test]
    fn size_of_value() {
        assert_eq!(std::mem::size_of::<Value>(), std::mem::size_of::<usize>());
    }

    #[test]
    fn as_str_or_returns_default_for_null() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert_eq!(val.as_str_or("fallback"), "fallback");
    }

    #[test]
    fn as_str_or_default_returns_empty_for_null() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert_eq!(val.as_str_or_default(), "");
    }

    #[test]
    fn as_i64_or_returns_default_for_null() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert_eq!(val.as_i64_or(99), 99);
    }

    #[test]
    fn as_i32_or_returns_default_for_null() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert_eq!(val.as_i32_or(42), 42);
    }

    #[test]
    fn as_bool_or_returns_default_for_null() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert!(val.as_bool_or(true));
        assert!(!val.as_bool_or(false));
    }

    #[test]
    fn as_f64_or_returns_default_for_null() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert!((val.as_f64_or(2.72) - 2.72).abs() < f64::EPSILON);
    }

    #[test]
    fn as_f32_or_returns_default_for_null() {
        let val = unsafe { Value::from_raw(std::ptr::null_mut()) };
        assert!((val.as_f32_or(2.5) - 2.5).abs() < f32::EPSILON);
    }
}
