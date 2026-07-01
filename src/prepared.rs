// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Prepared statements and parameter binding.
//!
//! [`PreparedStatement`] wraps a `duckdb_prepared_statement` and offers typed
//! binding for every `DuckDB` parameter type plus execution into a
//! [`QueryResult`][crate::result::QueryResult]. An optional [`Pending`]
//! pipeline wraps `DuckDB`'s async-task pending-result API for chunk-driven
//! streaming of long-running queries.
//!
//! All operations are RAII: the statement is destroyed on drop via
//! `duckdb_destroy_prepare`; pending results via `duckdb_destroy_pending`.
//!
//! # Example
//!
//! ```rust,no_run
//! use quack_rs::prepared::PreparedStatement;
//! use quack_rs::result::QueryResult;
//! use libduckdb_sys::duckdb_connection;
//!
//! # unsafe fn demo(con: duckdb_connection) -> Result<(), quack_rs::error::ExtensionError> {
//! // SAFETY: con is a valid, open connection.
//! let mut stmt = unsafe { PreparedStatement::prepare(con, "SELECT $1::INTEGER + $2::INTEGER") }?;
//! stmt.bind_i32(1, 41)?;
//! stmt.bind_i32(2, 1)?;
//! // SAFETY: stmt's bindings are valid; the runtime is initialised.
//! let _result = unsafe { stmt.execute() }?;
//! # Ok(())
//! # }
//! ```

use std::ffi::CString;
use std::os::raw::c_void;

use libduckdb_sys::{
    duckdb_bind_blob, duckdb_bind_boolean, duckdb_bind_date, duckdb_bind_decimal,
    duckdb_bind_double, duckdb_bind_float, duckdb_bind_hugeint, duckdb_bind_int16,
    duckdb_bind_int32, duckdb_bind_int64, duckdb_bind_int8, duckdb_bind_interval,
    duckdb_bind_null, duckdb_bind_parameter_index, duckdb_bind_time, duckdb_bind_timestamp,
    duckdb_bind_timestamp_tz, duckdb_bind_uint16, duckdb_bind_uint32, duckdb_bind_uint64,
    duckdb_bind_uint8, duckdb_bind_uhugeint, duckdb_bind_value, duckdb_bind_varchar,
    duckdb_bind_varchar_length, duckdb_clear_bindings, duckdb_connection, duckdb_date,
    duckdb_decimal, duckdb_destroy_pending, duckdb_destroy_prepare, duckdb_execute_pending,
    duckdb_execute_prepared, duckdb_extract_statements, duckdb_extract_statements_error,
    duckdb_hugeint, duckdb_interval, duckdb_nparams, duckdb_param_logical_type, duckdb_param_type,
    duckdb_parameter_name, duckdb_pending_error, duckdb_pending_execution_is_finished,
    duckdb_pending_execute_check_state, duckdb_pending_execute_task, duckdb_pending_prepared,
    duckdb_pending_result, duckdb_pending_state, duckdb_prepare, duckdb_prepare_error,
    duckdb_prepare_extracted_statement, duckdb_prepared_statement,
    duckdb_prepared_statement_type, duckdb_state, duckdb_time, duckdb_timestamp,
    duckdb_uhugeint, idx_t, DuckDBSuccess,
};

use crate::error::ExtensionError;
use crate::result::QueryResult;
use crate::types::{LogicalType, TypeId};
use crate::value::Value;

/// Synchronous state of a pending-result task (`DuckDB` `duckdb_pending_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingState {
    /// `DuckDB` is finished; call [`Pending::execute`].
    ResultReady,
    /// More tasks must be executed; call [`Pending::execute_task`] again.
    NotReady,
    /// The statement produced an error; call [`Pending::error_message`].
    Error,
    /// A task has been scheduled (parallel execution); no manual polling needed.
    NoTasksAvailable,
    /// Unknown future enum value.
    Unknown(u32),
}

impl PendingState {
    const fn from_raw(raw: duckdb_pending_state) -> Self {
        use libduckdb_sys as sys;
        if raw == sys::duckdb_pending_state_DUCKDB_PENDING_RESULT_READY {
            return Self::ResultReady;
        }
        if raw == sys::duckdb_pending_state_DUCKDB_PENDING_RESULT_NOT_READY {
            return Self::NotReady;
        }
        if raw == sys::duckdb_pending_state_DUCKDB_PENDING_ERROR {
            return Self::Error;
        }
        if raw == sys::duckdb_pending_state_DUCKDB_PENDING_NO_TASKS_AVAILABLE {
            return Self::NoTasksAvailable;
        }
        Self::Unknown(raw)
    }
}

/// RAII wrapper around `duckdb_prepared_statement`. Destroyed on drop.
pub struct PreparedStatement {
    stmt: duckdb_prepared_statement,
}

/// Converts `&str` to `CString` without panicking (truncates at the first `NUL`).
fn str_to_cstring(s: &str) -> CString {
    CString::new(s).unwrap_or_else(|_| {
        let pos = s.bytes().position(|b| b == 0).unwrap_or(s.len());
        // SAFETY: pos is at the first null byte, so s[..pos] has no nulls.
        CString::new(&s.as_bytes()[..pos]).unwrap_or_default()
    })
}

impl PreparedStatement {
    /// Prepares `sql` on the given connection and returns a [`PreparedStatement`].
    ///
    /// # Safety
    ///
    /// `con` must be a valid, open `duckdb_connection` and the `DuckDB` runtime
    /// must be initialised. If `sql` contains an interior `NUL` byte it is
    /// truncated at that point before being passed to `DuckDB`.
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionError`] if `DuckDB` rejects the statement; the error
    /// message (`duckdb_prepare_error`) is included.
    pub unsafe fn prepare(con: duckdb_connection, sql: &str) -> Result<Self, ExtensionError> {
        let c_sql = str_to_cstring(sql);
        let mut raw: duckdb_prepared_statement = std::ptr::null_mut();
        // SAFETY: con is valid; c_sql is a NUL-terminated C string; raw is out-pointer.
        let state = unsafe { duckdb_prepare(con, c_sql.as_ptr(), &raw mut raw) };
        if state == DuckDBSuccess && !raw.is_null() {
            Ok(Self { stmt: raw })
        } else {
            // SAFETY: raw is the (possibly null) prepared-statement handle returned
            // by `duckdb_prepare`; reading its error string is safe even on failure.
            let msg = unsafe { prepare_error_string(raw) };
            Err(ExtensionError::new(&msg))
        }
    }

    /// Returns the error message from the most recent prepare/prepare-extracted
    /// operation. Owned `String`, or `None` if there was no error / null stmt.
    ///
    /// The caller typically does not call this directly — [`prepare`][Self::prepare]
    /// and [`extract_statements`][Self::extract_and_prepare] fold the error into
    /// the returned `Result`.
    #[must_use]
    pub fn error_message(&self) -> Option<String> {
        // SAFETY: self.stmt is valid per constructor's contract.
        Some(unsafe { prepare_error_string(self.stmt) })
    }

    /// Returns the number of bound parameters the statement expects.
    #[must_use]
    pub fn parameter_count(&self) -> u64 {
        // SAFETY: self.stmt is valid.
        unsafe { duckdb_nparams(self.stmt) }
    }

    /// Returns the name of the parameter at 1-based `index`, or `None` on
    /// out-of-bounds / un-named parameters / invalid UTF-8.
    #[must_use]
    pub fn parameter_name(&self, index: u64) -> Option<String> {
        // SAFETY: self.stmt is valid.
        let p = unsafe { duckdb_parameter_name(self.stmt, index) };
        if p.is_null() {
            return None;
        }
        // SAFETY: p is a NUL-terminated C string owned by `DuckDB` for the call.
        let s = unsafe { std::ffi::CStr::from_ptr(p) }
            .to_str()
            .ok()?
            .to_owned();
        Some(s)
    }

    /// Returns the column type of the parameter at 1-based `index`.
    ///
    /// Returns [`TypeId::Varchar`] (matching `DuckDB`'s fallback for unknown
    /// indices) plus a `None` from [`parameter_logical_type`] for the type-safe
    /// variant.
    #[must_use]
    pub fn parameter_type(&self, index: u64) -> TypeId {
        // SAFETY: self.stmt is valid.
        let raw = unsafe { duckdb_param_type(self.stmt, index) };
        TypeId::from_duckdb_type(raw)
    }

    /// Returns the logical type of the parameter at 1-based `index`, or `None`
    /// when the parameter has no logical type (e.g. on a parameterised SQL
    /// macro that doesn't fix its argument types).
    #[must_use]
    pub fn parameter_logical_type(&self, index: u64) -> Option<LogicalType> {
        // SAFETY: self.stmt is valid.
        let lt = unsafe { duckdb_param_logical_type(self.stmt, index) };
        if lt.is_null() {
            return None;
        }
        // SAFETY: lt is a fresh owned handle from `DuckDB`.
        Some(unsafe { LogicalType::from_raw(lt) })
    }

    /// Clears all bound parameters. After this call every parameter is unbound.
    ///
    /// # Errors
    ///
    /// Returns [`ExtensionError`] on `DuckDB` failure (e.g. invalid handle).
    pub fn clear_bindings(&mut self) -> Result<(), ExtensionError> {
        // SAFETY: self.stmt is valid.
        let state = unsafe { duckdb_clear_bindings(self.stmt) };
        check_state(state)
    }

    /// Returns the `DuckDB` statement-type code for this prepared statement
    /// (`DUCKDB_STATEMENT_*` constants on `libduckdb_sys`).
    #[must_use]
    pub fn statement_type(&self) -> u32 {
        // SAFETY: self.stmt is valid.
        unsafe { duckdb_prepared_statement_type(self.stmt) }
    }

    /// Resolves a named parameter to its 1-based index, or `None` if the name
    /// isn't a parameter on this statement.
    ///
    /// If `name` contains an interior `NUL` it is truncated at that point.
    #[must_use]
    pub fn parameter_index(&self, name: &str) -> Option<u64> {
        let c_name = str_to_cstring(name);
        let mut idx: idx_t = 0;
        // SAFETY: self.stmt is valid; idx is a valid out-pointer.
        let state = unsafe { duckdb_bind_parameter_index(self.stmt, &raw mut idx, c_name.as_ptr()) };
        if state == DuckDBSuccess {
            Some(idx)
        } else {
            None
        }
    }

    // ── Typed binders (1-based parameter index) ─────────────────────────────


    /// Binds a `BOOLEAN` at 1-based `index`.
    pub fn bind_bool(&mut self, index: u64, v: bool) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_boolean(self.stmt, index, v) })
    }

    /// Binds a `TINYINT` at 1-based `index`.
    pub fn bind_i8(&mut self, index: u64, v: i8) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_int8(self.stmt, index, v) })
    }

    /// Binds a `SMALLINT` at 1-based `index`.
    pub fn bind_i16(&mut self, index: u64, v: i16) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_int16(self.stmt, index, v) })
    }

    /// Binds an `INTEGER` at 1-based `index`.
    pub fn bind_i32(&mut self, index: u64, v: i32) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_int32(self.stmt, index, v) })
    }

    /// Binds a `BIGINT` at 1-based `index`.
    pub fn bind_i64(&mut self, index: u64, v: i64) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_int64(self.stmt, index, v) })
    }

    /// Binds a `UTINYINT` at 1-based `index`.
    pub fn bind_u8(&mut self, index: u64, v: u8) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_uint8(self.stmt, index, v) })
    }

    /// Binds a `USMALLINT` at 1-based `index`.
    pub fn bind_u16(&mut self, index: u64, v: u16) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_uint16(self.stmt, index, v) })
    }

    /// Binds a `UINTEGER` at 1-based `index`.
    pub fn bind_u32(&mut self, index: u64, v: u32) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_uint32(self.stmt, index, v) })
    }

    /// Binds a `UBIGINT` at 1-based `index`.
    pub fn bind_u64(&mut self, index: u64, v: u64) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_uint64(self.stmt, index, v) })
    }

    /// Binds a `HUGEINT` (`i128`) at 1-based `index`.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    pub fn bind_i128(&mut self, index: u64, v: i128) -> Result<(), ExtensionError> {
        let h = duckdb_hugeint {
            lower: v as u64,
            upper: (v >> 64) as i64,
        };
        check_state(unsafe { duckdb_bind_hugeint(self.stmt, index, h) })
    }

    /// Binds a `UHUGEINT` (`u128`) at 1-based `index`.
    #[allow(clippy::cast_possible_truncation)]
    pub fn bind_u128(&mut self, index: u64, v: u128) -> Result<(), ExtensionError> {
        let h = duckdb_uhugeint {
            lower: v as u64,
            upper: (v >> 64) as u64,
        };
        check_state(unsafe { duckdb_bind_uhugeint(self.stmt, index, h) })
    }

    /// Binds a `FLOAT` at 1-based `index`.
    pub fn bind_f32(&mut self, index: u64, v: f32) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_float(self.stmt, index, v) })
    }

    /// Binds a `DOUBLE` at 1-based `index`.
    pub fn bind_f64(&mut self, index: u64, v: f64) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_double(self.stmt, index, v) })
    }

    /// Binds a `DECIMAL` at 1-based `index` (`{width, scale, value: hugeint}`).
    pub fn bind_decimal(&mut self, index: u64, v: duckdb_decimal) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_decimal(self.stmt, index, v) })
    }

    /// Binds a `DATE` at 1-based `index`.
    pub fn bind_date(&mut self, index: u64, v: duckdb_date) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_date(self.stmt, index, v) })
    }

    /// Binds a `TIME` at 1-based `index`.
    pub fn bind_time(&mut self, index: u64, v: duckdb_time) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_time(self.stmt, index, v) })
    }

    /// Binds a `TIMESTAMP` at 1-based `index`.
    pub fn bind_timestamp(&mut self, index: u64, v: duckdb_timestamp) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_timestamp(self.stmt, index, v) })
    }

    /// Binds a `TIMESTAMP_TZ` at 1-based `index` (same `duckdb_timestamp` layout).
    pub fn bind_timestamp_tz(&mut self, index: u64, v: duckdb_timestamp) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_timestamp_tz(self.stmt, index, v) })
    }

    /// Binds an `INTERVAL` at 1-based `index`.
    pub fn bind_interval(&mut self, index: u64, v: duckdb_interval) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_interval(self.stmt, index, v) })
    }

    /// Binds a `VARCHAR` at 1-based `index` from a Rust string. Truncates
    /// at the first interior `NUL` byte.
    pub fn bind_varchar(&mut self, index: u64, s: &str) -> Result<(), ExtensionError> {
        let c = str_to_cstring(s);
        check_state(unsafe { duckdb_bind_varchar(self.stmt, index, c.as_ptr()) })
    }

    /// Binds a `VARCHAR` from a length-prefixed byte slice (binary-safe).
    pub fn bind_varchar_bytes(&mut self, index: u64, bytes: &[u8]) -> Result<(), ExtensionError> {
        check_state(unsafe {
            duckdb_bind_varchar_length(
                self.stmt,
                index,
                bytes.as_ptr().cast(),
                u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            )
        })
    }

    /// Binds a `BLOB` at 1-based `index` from raw bytes.
    pub fn bind_blob(&mut self, index: u64, data: &[u8]) -> Result<(), ExtensionError> {
        check_state(unsafe {
            duckdb_bind_blob(
                self.stmt,
                index,
                data.as_ptr().cast::<c_void>(),
                u64::try_from(data.len()).unwrap_or(u64::MAX),
            )
        })
    }

    /// Binds a `NULL` at 1-based `index`.
    pub fn bind_null(&mut self, index: u64) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_null(self.stmt, index) })
    }

    /// Binds a previously-built [`Value`] at 1-based `index`.
    ///
    /// The value's logical type must be compatible with the parameter's type.
    /// `DuckDB` copies the value internally; the [`Value`] retains ownership
    /// and is destroyed on drop.
    pub fn bind_value(&mut self, index: u64, value: &Value) -> Result<(), ExtensionError> {
        check_state(unsafe { duckdb_bind_value(self.stmt, index, value.as_raw()) })
    }

    // ── Execution ───────────────────────────────────────────────────────────

    /// Executes the prepared statement, returning a [`Result`] on success or
    /// [`ExtensionError`] on failure.
    ///
    /// # Safety
    ///
    /// All required parameters must be bound; the underlying connection must
    /// still be valid; the `DuckDB` runtime must be initialised.
    pub unsafe fn execute(&self) -> Result<QueryResult, ExtensionError> {
        let mut res: libduckdb_sys::duckdb_result = libduckdb_sys::duckdb_result {
            deprecated_column_count: 0,
            deprecated_row_count: 0,
            deprecated_rows_changed: 0,
            deprecated_columns: std::ptr::null_mut(),
            deprecated_error_message: std::ptr::null_mut(),
            internal_data: std::ptr::null_mut(),
        };
        // SAFETY: self.stmt is valid; res is a valid out-pointer.
        let state = unsafe { duckdb_execute_prepared(self.stmt, &raw mut res) };
        if state == DuckDBSuccess {
            // SAFETY: res was written by `DuckDB` for us to own and destroy.
            Ok(QueryResult::from_raw(res))
        } else {
            // SAFETY: even on failure the result holds the error string; read
            // it before dropping. We still take ownership via from_raw so destroy
            // is called on drop below — `Result::Drop` runs at scope exit.
            let mut res = QueryResult::from_raw(res);
            let msg = res
                .error_message()
                .unwrap_or_else(|| "duckdb_execute_prepared failed".to_owned());
            Err(ExtensionError::new(&msg))
        }
    }

    /// Builds a [`Pending`] handle that drains the execute via `DuckDB`'s
    /// pending-result API. Useful for long-running queries where you want to
    /// poll completion rather than block in `execute()`.
    ///
    /// # Safety
    ///
    /// All required parameters must be bound; the `DuckDB` runtime must be
    /// initialised.
    pub unsafe fn pending(&self) -> Result<Pending, ExtensionError> {
        let mut pending: duckdb_pending_result = std::ptr::null_mut();
        // SAFETY: self.stmt is valid; pending is a valid out-pointer.
        let state = unsafe { duckdb_pending_prepared(self.stmt, &raw mut pending) };
        if state == DuckDBSuccess && !pending.is_null() {
            Ok(Pending { pending })
        } else if pending.is_null() {
            Err(ExtensionError::new("duckdb_pending_prepared failed"))
        } else {
            // SAFETY: even on failure DuckDB may write a non-null pending handle
            // carrying the error message; destroy it on the way out.
            let msg = unsafe { duckdb_pending_error(pending) };
            let s = if msg.is_null() {
                String::from("duckdb_pending_prepared failed")
            } else {
                unsafe { std::ffi::CStr::from_ptr(msg) }.to_string_lossy().into_owned()
            };
            unsafe { duckdb_destroy_pending(&raw mut pending) };
            Err(ExtensionError::new(&s))
        }
    }

    /// Returns the raw handle.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_prepared_statement {
        self.stmt
    }
}

impl Drop for PreparedStatement {
    fn drop(&mut self) {
        if !self.stmt.is_null() {
            // SAFETY: self.stmt is owned per constructor's contract.
            unsafe { duckdb_destroy_prepare(&raw mut self.stmt) };
        }
    }
}

/// RAII wrapper around a `duckdb_pending_result`. Destroyed on drop.
pub struct Pending {
    pending: duckdb_pending_result,
}

impl Pending {
    /// Executes one pending task. Returns the new [`PendingState`].
    ///
    /// Caller loops on [`execute_task`][Self::execute_task] until the state is
    /// [`PendingState::ResultReady`] (then call [`execute`][Self::execute]) or
    /// [`PendingState::Error`].
    #[must_use]
    pub fn execute_task(&mut self) -> PendingState {
        // SAFETY: self.pending is valid.
        PendingState::from_raw(unsafe { duckdb_pending_execute_task(self.pending) })
    }

    /// Checks the current state without executing a task.
    #[must_use]
    pub fn check_state(&self) -> PendingState {
        // SAFETY: self.pending is valid.
        PendingState::from_raw(unsafe { duckdb_pending_execute_check_state(self.pending) })
    }

    /// Returns `true` when `DuckDB` has finished executing the pending statement.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        unsafe { duckdb_pending_execution_is_finished(self.check_state().raw()) }
    }

    /// Returns the error message as an owned `String`, or `None` if there is
    /// no error. Returns the result of `duckdb_pending_error`.
    #[must_use]
    pub fn error_message(&self) -> Option<String> {
        // SAFETY: self.pending is valid.
        let p = unsafe { duckdb_pending_error(self.pending) };
        if p.is_null() {
            return None;
        }
        // SAFETY: p is a NUL-terminated C string owned by `DuckDB` for the call.
        Some(unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned())
    }

    /// Finishes execution and returns the [`Result`].
    ///
    /// # Safety
    ///
    /// Caller must have driven [`execute_task`][Self::execute_task] to
    /// [`PendingState::ResultReady`] before calling.
    pub unsafe fn execute(self) -> Result<QueryResult, ExtensionError> {
        let mut res: libduckdb_sys::duckdb_result = libduckdb_sys::duckdb_result {
            deprecated_column_count: 0,
            deprecated_row_count: 0,
            deprecated_rows_changed: 0,
            deprecated_columns: std::ptr::null_mut(),
            deprecated_error_message: std::ptr::null_mut(),
            internal_data: std::ptr::null_mut(),
        };
        // SAFETY: self.pending is valid and ready per caller's contract; res is
        // an out-pointer.
        let state = unsafe { duckdb_execute_pending(self.pending, &raw mut res) };
        if state == DuckDBSuccess {
            // SAFETY: res was written by DuckDB for us to own.
            Ok(QueryResult::from_raw(res))
        } else {
            let mut res = QueryResult::from_raw(res);
            let msg = res
                .error_message()
                .unwrap_or_else(|| "duckdb_execute_pending failed".to_owned());
            Err(ExtensionError::new(&msg))
        }
    }

    /// Returns the raw handle.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_pending_result {
        self.pending
    }
}

impl PendingState {
    /// Returns the raw 32-bit `DuckDB` `pending_state` enum value.
    #[inline]
    #[must_use]
    pub const fn raw(self) -> duckdb_pending_state {
        match self {
            Self::ResultReady => libduckdb_sys::duckdb_pending_state_DUCKDB_PENDING_RESULT_READY,
            Self::NotReady => libduckdb_sys::duckdb_pending_state_DUCKDB_PENDING_RESULT_NOT_READY,
            Self::Error => libduckdb_sys::duckdb_pending_state_DUCKDB_PENDING_ERROR,
            Self::NoTasksAvailable => libduckdb_sys::duckdb_pending_state_DUCKDB_PENDING_NO_TASKS_AVAILABLE,
            Self::Unknown(v) => v,
        }
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if !self.pending.is_null() {
            // SAFETY: self.pending is owned per constructor's contract.
            unsafe { duckdb_destroy_pending(&raw mut self.pending) };
        }
    }
}

/// Extracts a single SQL statement (by `index`) from a multi-statement `sql`
/// string and returns a [`PreparedStatement`] ready for binding.
///
/// `DuckDB` separates statement extraction (parsing) from preparation; use this
/// helper when `sql` may contain multiple statements separated by `;`.
///
/// # Safety
///
/// `con` must be a valid, open `duckdb_connection`. If `sql` contains an
/// interior `NUL` byte it is truncated at that point.
///
/// # Errors
///
/// Returns [`ExtensionError`] if extraction fails or `index` is out of bounds.
pub unsafe fn extract_and_prepare(
    con: duckdb_connection,
    sql: &str,
    index: u64,
) -> Result<PreparedStatement, ExtensionError> {
    let c_sql = str_to_cstring(sql);
    let mut extracted: libduckdb_sys::duckdb_extracted_statements = std::ptr::null_mut();
    // SAFETY: con is valid; c_sql is NUL-terminated; extracted is out-pointer.
    let count = unsafe { duckdb_extract_statements(con, c_sql.as_ptr(), &raw mut extracted) };
    if count == 0 || extracted.is_null() {
        // SAFETY: extracted is valid (possibly null); safe to read the error.
        let err_ptr = if extracted.is_null() {
            std::ptr::null()
        } else {
            unsafe { duckdb_extract_statements_error(extracted) }
        };
        let msg = if err_ptr.is_null() {
            "duckdb_extract_statements failed".to_owned()
        } else {
            unsafe { std::ffi::CStr::from_ptr(err_ptr) }
                .to_string_lossy()
                .into_owned()
        };
        // Drop the extracted-statements handle if it was non-null.
        if !extracted.is_null() {
            unsafe { libduckdb_sys::duckdb_destroy_extracted(&raw mut extracted) };
        }
        return Err(ExtensionError::new(&msg));
    }
    if index >= count {
        unsafe { libduckdb_sys::duckdb_destroy_extracted(&raw mut extracted) };
        return Err(ExtensionError::new("statement index out of range"));
    }
    let mut stmt: duckdb_prepared_statement = std::ptr::null_mut();
    // SAFETY: con is valid; extracted is valid; index < count; stmt is out-pointer.
    let state = unsafe {
        duckdb_prepare_extracted_statement(con, extracted, index, &raw mut stmt)
    };
    // Always drop the extracted handle now that the prepared statement owns its state.
    unsafe { libduckdb_sys::duckdb_destroy_extracted(&raw mut extracted) };
    if state == DuckDBSuccess && !stmt.is_null() {
        Ok(PreparedStatement { stmt })
    } else {
        let msg = unsafe { prepare_error_string(stmt) };
        Err(ExtensionError::new(&msg))
    }
}

/// Reads the prepare error string from a (possibly null) statement handle.
///
/// # Safety
///
/// `stmt` may be null (returns the generic fallback in that case).
unsafe fn prepare_error_string(stmt: duckdb_prepared_statement) -> String {
    if stmt.is_null() {
        return "duckdb_prepare failed".to_owned();
    }
    let p = unsafe { duckdb_prepare_error(stmt) };
    if p.is_null() {
        return "duckdb_prepare failed".to_owned();
    }
    // SAFETY: p is a NUL-terminated C string for the duration of this call.
    unsafe { std::ffi::CStr::from_ptr(p) }
        .to_string_lossy()
        .into_owned()
}

/// Maps a `duckdb_state` to `Result`, returning a generic message on failure.
fn check_state(state: duckdb_state) -> Result<(), ExtensionError> {
    if state == DuckDBSuccess {
        Ok(())
    } else {
        Err(ExtensionError::new("DuckDB bind failed"))
    }
}