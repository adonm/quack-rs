// SPDX-License-Identifier: MIT
// Copyright 2026 Tom F. <https://github.com/tomtom215/>
// My way of giving something small back to the open source community
// and encouraging more Rust development!

//! Bulk data appending (`DuckDB` 1.5.0+).
//!
//! [`Appender`] is an RAII wrapper around `DuckDB`'s appender — the fastest way
//! to bulk-insert rows into an existing table. This wrapper pairs the core
//! appender lifecycle (create, append a data chunk, flush, close) with the
//! 1.5.0 additions: structured [`ErrorData`] reporting
//! ([`error_data`][Appender::error_data]), reverting buffered-but-unflushed rows
//! ([`clear`][Appender::clear]), and appending a column's `DEFAULT` value into a
//! chunk ([`append_default_to_chunk`][Appender::append_default_to_chunk]).
//!
//! # Example
//!
//! ```rust,no_run
//! use quack_rs::appender::Appender;
//! use quack_rs::data_chunk::DataChunk;
//! use libduckdb_sys::duckdb_connection;
//!
//! # unsafe fn demo(con: duckdb_connection, chunk: &DataChunk) -> Result<(), quack_rs::error_data::ErrorData> {
//! // SAFETY: `con` is a valid, open connection.
//! let appender = unsafe { Appender::new(con, None, c"my_table") }?;
//! appender.append_chunk(chunk)?;
//! appender.flush()?;
//! # Ok(())
//! # }
//! ```

use std::ffi::CStr;

use libduckdb_sys::{
    duckdb_append_blob, duckdb_append_bool, duckdb_append_data_chunk, duckdb_append_date,
    duckdb_append_default, duckdb_append_default_to_chunk, duckdb_append_double,
    duckdb_append_float, duckdb_append_hugeint, duckdb_append_int16, duckdb_append_int32,
    duckdb_append_int64, duckdb_append_int8, duckdb_append_interval, duckdb_append_null,
    duckdb_append_time, duckdb_append_timestamp, duckdb_append_uint16, duckdb_append_uint32,
    duckdb_append_uint64, duckdb_append_uint8, duckdb_append_uhugeint, duckdb_append_value,
    duckdb_append_varchar, duckdb_append_varchar_length, duckdb_appender, duckdb_appender_add_column,
    duckdb_appender_begin_row, duckdb_appender_clear, duckdb_appender_clear_columns,
    duckdb_appender_close, duckdb_appender_column_count, duckdb_appender_column_type,
    duckdb_appender_create, duckdb_appender_create_ext, duckdb_appender_destroy,
    duckdb_appender_end_row, duckdb_appender_error, duckdb_appender_error_data,
    duckdb_appender_flush, duckdb_connection, duckdb_date, duckdb_hugeint, duckdb_interval,
    duckdb_logical_type, duckdb_state, duckdb_time, duckdb_timestamp, duckdb_uhugeint,
    DuckDBSuccess,
};

use crate::data_chunk::DataChunk;
use crate::error_data::ErrorData;
use crate::types::LogicalType;
use crate::value::Value;

/// Converts an optional `&CStr` into a (possibly null) C string pointer.
#[inline]
fn opt_ptr(s: Option<&CStr>) -> *const std::os::raw::c_char {
    s.map_or(std::ptr::null(), CStr::as_ptr)
}

/// RAII wrapper for a `duckdb_appender`.
///
/// The appender is flushed and destroyed automatically on drop. To surface any
/// error from the final flush, call [`close`][Appender::close] explicitly before
/// dropping.
pub struct Appender {
    appender: duckdb_appender,
}

impl Appender {
    /// Creates an appender for `table` in the given `schema` (or the default
    /// schema when `schema` is `None`).
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] if the appender cannot be created
    /// (for example because the table does not exist).
    ///
    /// # Safety
    ///
    /// `con` must be a valid, open `duckdb_connection`.
    pub unsafe fn new(
        con: duckdb_connection,
        schema: Option<&CStr>,
        table: &CStr,
    ) -> Result<Self, ErrorData> {
        let mut raw: duckdb_appender = std::ptr::null_mut();
        // SAFETY: con is valid per caller's contract; the string pointers are
        // valid for the call; raw is a valid out-pointer.
        let state =
            unsafe { duckdb_appender_create(con, opt_ptr(schema), table.as_ptr(), &raw mut raw) };
        let appender = Self { appender: raw };
        if state == DuckDBSuccess {
            Ok(appender)
        } else {
            Err(appender.error_data())
        }
    }

    /// Creates an appender for `table`, fully qualified by optional `catalog` and
    /// `schema`.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] if the appender cannot be created.
    ///
    /// # Safety
    ///
    /// `con` must be a valid, open `duckdb_connection`.
    pub unsafe fn with_catalog(
        con: duckdb_connection,
        catalog: Option<&CStr>,
        schema: Option<&CStr>,
        table: &CStr,
    ) -> Result<Self, ErrorData> {
        let mut raw: duckdb_appender = std::ptr::null_mut();
        // SAFETY: con is valid per caller's contract; the string pointers are
        // valid for the call; raw is a valid out-pointer.
        let state = unsafe {
            duckdb_appender_create_ext(
                con,
                opt_ptr(catalog),
                opt_ptr(schema),
                table.as_ptr(),
                &raw mut raw,
            )
        };
        let appender = Self { appender: raw };
        if state == DuckDBSuccess {
            Ok(appender)
        } else {
            Err(appender.error_data())
        }
    }

    /// Appends an entire [`DataChunk`] to the table.
    ///
    /// The chunk's column types must match the table's.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] if the append fails.
    pub fn append_chunk(&self, chunk: &DataChunk) -> Result<(), ErrorData> {
        // SAFETY: self.appender and chunk.as_raw() are valid.
        let state = unsafe { duckdb_append_data_chunk(self.appender, chunk.as_raw()) };
        self.check(state)
    }

    /// Writes the table column `col`'s `DEFAULT` value into row `row` of `chunk`.
    ///
    /// This is useful when building a chunk to append: columns without an
    /// explicit value can be filled with their schema default.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] if the default cannot be written.
    pub fn append_default_to_chunk(
        &self,
        chunk: &DataChunk,
        col: u64,
        row: u64,
    ) -> Result<(), ErrorData> {
        // SAFETY: self.appender and chunk.as_raw() are valid.
        let state =
            unsafe { duckdb_append_default_to_chunk(self.appender, chunk.as_raw(), col, row) };
        self.check(state)
    }

    // ── Row-scoped lifecycle ───────────────────────────────────────────────

    /// Marks the start of a new row being appended.
    ///
    /// Pair with [`append_*`][Self::append_bool] calls (one per table column,
    /// in column order) and [`end_row`][Self::end_row].
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] on failure.
    pub fn begin_row(&self) -> Result<(), ErrorData> {
        // SAFETY: self.appender is valid.
        self.check(unsafe { duckdb_appender_begin_row(self.appender) })
    }

    /// Marks the end of the current row.
    ///
    /// Must be called after every column has been filled for this row.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] on failure.
    pub fn end_row(&self) -> Result<(), ErrorData> {
        // SAFETY: self.appender is valid.
        self.check(unsafe { duckdb_appender_end_row(self.appender) })
    }

    /// Appends a `DEFAULT` value for the current column in the current row.
    ///
    /// Convenience for not having to know the column's type when its `DEFAULT`
    /// is intended.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] on failure.
    pub fn append_default(&self) -> Result<(), ErrorData> {
        // SAFETY: self.appender is valid.
        self.check(unsafe { duckdb_append_default(self.appender) })
    }

    /// Appends a `NULL` value for the current column in the current row.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] on failure.
    pub fn append_null(&self) -> Result<(), ErrorData> {
        // SAFETY: self.appender is valid.
        self.check(unsafe { duckdb_append_null(self.appender) })
    }

    /// Appends a previously-built [`Value`] for the current column/row.
    ///
    /// The value's logical type must be compatible with the column's type.
    ///
    /// # Safety
    ///
    /// The caller transfers ownership of `value` to the appender. After this
    /// call the [`Value`] must not be reused; the wrapper still destroys it on
    /// drop (which is the no-op for a transferred handle).
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] on failure.
    pub unsafe fn append_value(&self, value: &Value) -> Result<(), ErrorData> {
        // SAFETY: self.appender is valid; value.as_raw() is a valid duckdb_value.
        // DuckDB copies the value internally; we don't transfer ownership.
        self.check(unsafe { duckdb_append_value(self.appender, value.as_raw()) })
    }

    // ── Typed scalar appenders (per current column/row) ────────────────────

    /// Appends a `BOOLEAN` (`bool`) for the current column/row.
    pub fn append_bool(&self, v: bool) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_bool(self.appender, v) })
    }

    /// Appends a `TINYINT` (`i8`).
    pub fn append_i8(&self, v: i8) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_int8(self.appender, v) })
    }

    /// Appends a `SMALLINT` (`i16`).
    pub fn append_i16(&self, v: i16) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_int16(self.appender, v) })
    }

    /// Appends an `INTEGER` (`i32`).
    pub fn append_i32(&self, v: i32) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_int32(self.appender, v) })
    }

    /// Appends a `BIGINT` (`i64`).
    pub fn append_i64(&self, v: i64) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_int64(self.appender, v) })
    }

    /// Appends a `UTINYINT` (`u8`).
    pub fn append_u8(&self, v: u8) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_uint8(self.appender, v) })
    }

    /// Appends a `USMALLINT` (`u16`).
    pub fn append_u16(&self, v: u16) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_uint16(self.appender, v) })
    }

    /// Appends a `UINTEGER` (`u32`).
    pub fn append_u32(&self, v: u32) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_uint32(self.appender, v) })
    }

    /// Appends a `UBIGINT` (`u64`).
    pub fn append_u64(&self, v: u64) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_uint64(self.appender, v) })
    }

    /// Appends a `HUGEINT` (`i128`).
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn append_i128(&self, v: i128) -> Result<(), ErrorData> {
        let h = duckdb_hugeint {
            lower: v as u64,
            upper: (v >> 64) as i64,
        };
        self.check(unsafe { duckdb_append_hugeint(self.appender, h) })
    }

    /// Appends a `UHUGEINT` (`u128`).
    #[allow(clippy::cast_possible_truncation)]
    pub fn append_u128(&self, v: u128) -> Result<(), ErrorData> {
        let h = duckdb_uhugeint {
            lower: v as u64,
            upper: (v >> 64) as u64,
        };
        self.check(unsafe { duckdb_append_uhugeint(self.appender, h) })
    }

    /// Appends a `FLOAT` (`f32`).
    pub fn append_f32(&self, v: f32) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_float(self.appender, v) })
    }

    /// Appends a `DOUBLE` (`f64`).
    pub fn append_f64(&self, v: f64) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_double(self.appender, v) })
    }

    /// Appends a `DATE` (`duckdb_date` = `{ days: i32 }` since epoch).
    pub fn append_date(&self, v: duckdb_date) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_date(self.appender, v) })
    }

    /// Appends a `TIME` (`duckdb_time` = `{ micros: i64 }` since midnight).
    pub fn append_time(&self, v: duckdb_time) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_time(self.appender, v) })
    }

    /// Appends a `TIMESTAMP` (`duckdb_timestamp` = `{ micros: i64 }`).
    pub fn append_timestamp(&self, v: duckdb_timestamp) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_timestamp(self.appender, v) })
    }

    /// Appends an `INTERVAL` (`duckdb_interval` = `{ months, days, micros }`).
    pub fn append_interval(&self, v: duckdb_interval) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_append_interval(self.appender, v) })
    }

    /// Appends a `VARCHAR` from a Rust string (no interior `NUL` allowed;
    /// the string is truncated at the first `NUL` byte if any).
    pub fn append_varchar(&self, s: &str) -> Result<(), ErrorData> {
        let c = std::ffi::CString::new(s).unwrap_or_else(|_| {
            let pos = s.bytes().position(|b| b == 0).unwrap_or(s.len());
            unsafe { std::ffi::CString::from_vec_with_nul_unchecked(s.as_bytes()[..pos].to_vec()) }
        });
        self.check(unsafe { duckdb_append_varchar(self.appender, c.as_ptr()) })
    }

    /// Appends a `VARCHAR` from a length-prefixed byte slice (binary-safe; does
    /// not require NUL-termination or UTF-8 validation).
    pub fn append_varchar_bytes(&self, bytes: &[u8]) -> Result<(), ErrorData> {
        self.check(unsafe {
            duckdb_append_varchar_length(
                self.appender,
                bytes.as_ptr().cast(),
                u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            )
        })
    }

    /// Appends a `BLOB` from raw bytes.
    pub fn append_blob(&self, data: &[u8]) -> Result<(), ErrorData> {
        self.check(unsafe {
            duckdb_append_blob(
                self.appender,
                data.as_ptr().cast(),
                u64::try_from(data.len()).unwrap_or(u64::MAX),
            )
        })
    }

    // ── Column subset management ───────────────────────────────────────────

    /// Restricts subsequent appends to the named columns only (in declared order).
    ///
    /// Call [`add_column`][Self::add_column] once per column you intend to fill;
    /// every row appended afterwards must supply exactly those columns, in order.
    /// The column restriction lasts until [`clear_columns`][Self::clear_columns]
    /// or appender close.
    ///
    /// `name` must be a valid table column. If `name` contains an interior
    /// `NUL` it is truncated at that point.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] if the column does not exist.
    pub fn add_column(&self, name: &str) -> Result<(), ErrorData> {
        let c = std::ffi::CString::new(name).unwrap_or_else(|_| {
            let pos = name.bytes().position(|b| b == 0).unwrap_or(name.len());
            unsafe { std::ffi::CString::from_vec_with_nul_unchecked(name.as_bytes()[..pos].to_vec()) }
        });
        self.check(unsafe { duckdb_appender_add_column(self.appender, c.as_ptr()) })
    }

    /// Removes a previously-set [`add_column`][Self::add_column] restriction,
    /// restoring the all-columns default behaviour.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] on failure.
    pub fn clear_columns(&self) -> Result<(), ErrorData> {
        self.check(unsafe { duckdb_appender_clear_columns(self.appender) })
    }

    /// Returns the number of columns the appender is currently configured to
    /// append (either the table's full column count, or the count selected via
    /// [`add_column`][Self::add_column]).
    #[must_use]
    pub fn column_count(&self) -> u64 {
        // SAFETY: self.appender is valid.
        unsafe { duckdb_appender_column_count(self.appender) }
    }

    /// Returns the logical type of the column at `index` (0-based), as an
    /// RAII [`LogicalType`]. Returns `None` on null or out-of-bounds.
    #[must_use]
    pub fn column_type(&self, index: u64) -> Option<LogicalType> {
        // SAFETY: self.appender is valid.
        let lt: duckdb_logical_type = unsafe { duckdb_appender_column_type(self.appender, index) };
        if lt.is_null() {
            return None;
        }
        // SAFETY: lt is a fresh non-null handle from `DuckDB`.
        Some(unsafe { LogicalType::from_raw(lt) })
    }

    /// Returns the error message from the most recent failed operation as an
    /// owned `String`, or `None` if there is no error / `DuckDB` returned null.
    ///
    /// Prefer [`error_data`][Self::error_data] for structured error inspection.
    #[must_use]
    pub fn error_message(&self) -> Option<String> {
        // SAFETY: self.appender is valid.
        let c_ptr = unsafe { duckdb_appender_error(self.appender) };
        if c_ptr.is_null() {
            return None;
        }
        // SAFETY: c_ptr is a NUL-terminated C string from DuckDB, valid until the
        // next appender call.
        Some(unsafe { CStr::from_ptr(c_ptr) }.to_string_lossy().into_owned())
    }

    /// Flushes buffered rows to the table without closing the appender.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] if the flush fails (e.g. a constraint
    /// violation). On failure, buffered rows can be discarded with
    /// [`clear`][Appender::clear].
    pub fn flush(&self) -> Result<(), ErrorData> {
        // SAFETY: self.appender is valid.
        let state = unsafe { duckdb_appender_flush(self.appender) };
        self.check(state)
    }

    /// Flushes and closes the appender. No further rows may be appended.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] if the final flush fails.
    pub fn close(&self) -> Result<(), ErrorData> {
        // SAFETY: self.appender is valid.
        let state = unsafe { duckdb_appender_close(self.appender) };
        self.check(state)
    }

    /// Discards all buffered, unflushed rows.
    ///
    /// Useful for recovering after a [`flush`][Appender::flush] error without
    /// re-appending the rows that were already committed.
    ///
    /// # Errors
    ///
    /// Returns the structured [`ErrorData`] if the appender state is invalid.
    pub fn clear(&self) -> Result<(), ErrorData> {
        // SAFETY: self.appender is valid.
        let state = unsafe { duckdb_appender_clear(self.appender) };
        self.check(state)
    }

    /// Returns the structured error from the most recent failed operation.
    #[must_use]
    pub fn error_data(&self) -> ErrorData {
        // SAFETY: self.appender is valid (possibly representing a failed create);
        // the call returns an owned error data handle.
        let raw = unsafe { duckdb_appender_error_data(self.appender) };
        // SAFETY: raw is an owned duckdb_error_data (possibly null).
        unsafe { ErrorData::from_raw(raw) }
    }

    /// Returns the raw handle.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_appender {
        self.appender
    }

    /// Converts a `duckdb_state` into a `Result`, reading the appender's error
    /// data on failure.
    fn check(&self, state: duckdb_state) -> Result<(), ErrorData> {
        if state == DuckDBSuccess {
            Ok(())
        } else {
            Err(self.error_data())
        }
    }
}

impl Drop for Appender {
    fn drop(&mut self) {
        if !self.appender.is_null() {
            // SAFETY: self.appender is a valid handle that we own. Destroy flushes
            // and frees it; we intentionally ignore the state here (use `close`
            // beforehand to observe a final flush error).
            unsafe { duckdb_appender_destroy(&raw mut self.appender) };
        }
    }
}

#[cfg(all(test, feature = "_duckdb-testing"))]
mod tests {
    use super::*;

    #[test]
    fn create_for_missing_table_reports_error() {
        // Ensure the dispatch table is populated.
        let _db = crate::testing::InMemoryDb::open().unwrap();

        let mut db: libduckdb_sys::duckdb_database = std::ptr::null_mut();
        let mut con: duckdb_connection = std::ptr::null_mut();
        // SAFETY: dispatch table is initialized; null path opens in-memory.
        unsafe {
            assert_eq!(
                libduckdb_sys::duckdb_open(std::ptr::null(), &raw mut db),
                DuckDBSuccess
            );
            assert_eq!(
                libduckdb_sys::duckdb_connect(db, &raw mut con),
                DuckDBSuccess
            );
        }

        // SAFETY: con is a valid open connection.
        let result = unsafe { Appender::new(con, None, c"does_not_exist") };
        assert!(result.is_err(), "expected create to fail for missing table");
        let err = result.err().unwrap();
        assert!(err.has_error());

        // SAFETY: valid handles.
        unsafe {
            libduckdb_sys::duckdb_disconnect(&raw mut con);
            libduckdb_sys::duckdb_close(&raw mut db);
        }
    }
}
