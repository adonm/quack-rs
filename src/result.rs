// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Result inspection and streaming chunk fetch.
//!
//! [`QueryResult`] is an RAII wrapper around a `duckdb_result`, returned by
//! [`PreparedStatement::execute`][crate::prepared::PreparedStatement::execute]
//! or `DuckDB` query execution. For modern extensions this is *not* the path
//! for reading extension-internal data (use the chunk APIs inside callbacks);
//! it's used when an extension issues its own SQL via a connection it holds.
//!
//! The deprecated full-materialised row/column accessors (`duckdb_value_int32`,
//! `duckdb_result_get_chunk`, `duckdb_row_count`, etc.) are *not* wrapped —
//! `DuckDB` has marked them for removal and the pull-based
//! [`fetch_chunk`][QueryResult::fetch_chunk] /
//! [`stream_fetch_chunk`][QueryResult::stream_fetch_chunk] path is the
//! supported replacement.

use std::os::raw::c_char;

use libduckdb_sys::{
    duckdb_column_count, duckdb_column_logical_type, duckdb_column_name, duckdb_column_type,
    duckdb_data_chunk, duckdb_destroy_result, duckdb_fetch_chunk, duckdb_result,
    duckdb_result_error, duckdb_result_error_type, duckdb_result_return_type,
    duckdb_result_statement_type, duckdb_rows_changed, duckdb_stream_fetch_chunk, idx_t,
};

use crate::data_chunk::DataChunk;
use crate::types::{LogicalType, TypeId};

/// The high-level outcome of a statement (`QUERY_RESULT`/`CHANGED_ROWS`/`NOTHING`/`ERROR`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultType {
    /// `DuckDB` has produced a query result set (a `SELECT`-style statement).
    QueryResult,
    /// The statement changed rows (`INSERT`/`UPDATE`/`DELETE`/...).
    ChangedRows,
    /// The statement did not produce a query result and did not change any rows.
    Nothing,
    /// A future / unknown result-type enum value.
    Unknown(u32),
}

impl ResultType {
    const fn from_raw(raw: libduckdb_sys::duckdb_result_type) -> Self {
        use libduckdb_sys as sys;
        if raw == sys::duckdb_result_type_DUCKDB_RESULT_TYPE_CHANGED_ROWS {
            return Self::ChangedRows;
        }
        if raw == sys::duckdb_result_type_DUCKDB_RESULT_TYPE_NOTHING {
            return Self::Nothing;
        }
        if raw == sys::duckdb_result_type_DUCKDB_RESULT_TYPE_QUERY_RESULT {
            return Self::QueryResult;
        }
        Self::Unknown(raw)
    }
}

/// The kind of statement that produced this result (`SELECT`/`INSERT`/...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatementType(pub u32);

impl StatementType {
    /// Returns the raw 32-bit `DuckDB` statement-type enum value.
    ///
    /// See `duckdb_statement_type` in `duckdb.h` for the full mapping; the
    /// constants are exposed as `libduckdb_sys::DUCKDB_STATEMENT_*`.
    #[inline]
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// RAII wrapper around `duckdb_result`. Destroyed on drop via
/// `duckdb_destroy_result`.
pub struct QueryResult {
    res: duckdb_result,
}

impl QueryResult {
    /// Wraps a raw `duckdb_result` and takes ownership of it.
    ///
    /// # Safety
    ///
    /// `res` must be a valid `duckdb_result` returned by `DuckDB` and the caller
    /// must not destroy it (the wrapper destroys it on drop).
    #[must_use]
    pub const fn from_raw(res: duckdb_result) -> Self {
        Self { res }
    }

    /// Returns `true` if the statement reported an error.
    ///
    /// Errors surface as a string via [`error_message`][Self::error_message]
    /// and as a category code via [`error_type`][Self::error_type].
    pub fn has_error(&mut self) -> bool {
        // SAFETY: self.res is valid per constructor's contract.
        let p = unsafe { duckdb_result_error(&raw mut self.res) };
        !p.is_null()
    }

    /// Returns the error message as an owned `String`, or `None` if there is no
    /// error or the error string was not valid UTF-8.
    ///
    /// The pointer returned by `duckdb_result_error` is borrowed from the
    /// `duckdb_result` and lives until [`QueryResult`]'s drop.
    pub fn error_message(&mut self) -> Option<String> {
        // SAFETY: self.res is valid per constructor's contract.
        let p: *const c_char = unsafe { duckdb_result_error(&raw mut self.res) };
        if p.is_null() {
            return None;
        }
        // SAFETY: p is a NUL-terminated C string from `DuckDB`, valid until drop.
        unsafe { std::ffi::CStr::from_ptr(p) }.to_str().ok().map(str::to_owned)
    }

    /// Returns the `DuckDB` error category code (see `duckdb_error_type`).
    pub fn error_type(&mut self) -> u32 {
        // SAFETY: self.res is valid.
        unsafe { duckdb_result_error_type(&raw mut self.res) }
    }

    /// Returns the high-level result type (`QueryResult` / `ChangedRows` / `Nothing` / `Unknown`).
    pub fn result_type(&self) -> ResultType {
        // SAFETY: self.res is valid.
        ResultType::from_raw(unsafe { duckdb_result_return_type(self.res) })
    }

    /// Returns the [`StatementType`] of the executed statement.
    pub fn statement_type(&self) -> StatementType {
        // SAFETY: self.res is valid.
        StatementType(unsafe { duckdb_result_statement_type(self.res) })
    }

    /// Returns the number of columns in the result set.
    ///
    /// Zero for non-`SELECT` statements.
    pub fn column_count(&mut self) -> u64 {
        // SAFETY: self.res is valid.
        unsafe { duckdb_column_count(&raw mut self.res) as u64 }
    }

    /// Returns the name of the column at `index` (0-based).
    ///
    /// Returns `None` if `index` is out of bounds or the name is not valid UTF-8.
    pub fn column_name(&mut self, index: u64) -> Option<String> {
        // SAFETY: self.res is valid.
        let p = unsafe { duckdb_column_name(&raw mut self.res, index as idx_t) };
        if p.is_null() {
            return None;
        }
        // SAFETY: p is a NUL-terminated C string owned by `DuckDB` for the call.
        unsafe { std::ffi::CStr::from_ptr(p) }.to_str().ok().map(str::to_owned)
    }

    /// Returns the column's type enum at `index`.
    ///
    /// Returns a sentinel `TypeId` on out-of-bounds; behaviour follows `DuckDB`
    /// for invalid indices.
    pub fn column_type(&mut self, index: u64) -> TypeId {
        // SAFETY: self.res is valid.
        let raw = unsafe { duckdb_column_type(&raw mut self.res, index as idx_t) };
        TypeId::from_duckdb_type(raw)
    }

    /// Returns the column's logical type at `index`, as an RAII [`LogicalType`].
    pub fn column_logical_type(&mut self, index: u64) -> Option<LogicalType> {
        // SAFETY: self.res is valid.
        let lt = unsafe { duckdb_column_logical_type(&raw mut self.res, index as idx_t) };
        if lt.is_null() {
            return None;
        }
        // SAFETY: lt is a fresh owned handle from `DuckDB`.
        Some(unsafe { LogicalType::from_raw(lt) })
    }

    /// Returns the number of rows changed by the statement (`INSERT`/`UPDATE`/`DELETE`).
    pub fn rows_changed(&mut self) -> u64 {
        // SAFETY: self.res is valid.
        unsafe { duckdb_rows_changed(&raw mut self.res) as u64 }
    }

    /// Pulls the next [`DataChunk`] from the result, or `None` when finished.
    ///
    /// This is the supported pull-based iteration API; wraps `duckdb_fetch_chunk`.
    ///
    /// # Safety
    ///
    /// The underlying result must be valid (be the result of a successful
    /// `execute_prepared`/`query` call). The returned [`DataChunk`] is RAII-managed.
    pub unsafe fn fetch_chunk(&self) -> Option<DataChunk> {
        // SAFETY: self.res is valid per constructor's contract.
        let chunk: duckdb_data_chunk = unsafe { duckdb_fetch_chunk(self.res) };
        if chunk.is_null() {
            return None;
        }
        // SAFETY: chunk is a fresh owned handle.
        Some(unsafe { DataChunk::from_raw(chunk) })
    }

    /// Pulls the next streaming [`DataChunk`] for a streaming-result query.
    ///
    /// Like [`fetch_chunk`][Self::fetch_chunk] but for streaming execution paths
    /// where `DuckDB` may emit chunks lazily. Returns `None` at end-of-stream.
    ///
    /// # Safety
    ///
    /// Same as [`fetch_chunk`][Self::fetch_chunk].
    pub unsafe fn stream_fetch_chunk(&self) -> Option<DataChunk> {
        // SAFETY: self.res is valid per constructor's contract.
        let chunk: duckdb_data_chunk = unsafe { duckdb_stream_fetch_chunk(self.res) };
        if chunk.is_null() {
            return None;
        }
        Some(unsafe { DataChunk::from_raw(chunk) })
    }

    /// Returns the raw `duckdb_result` handle without transferring ownership.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_result {
        self.res
    }
}

impl Drop for QueryResult {
    fn drop(&mut self) {
        // SAFETY: self.res is owned per constructor's contract; destroy is the
        // correct cleanup. `&raw mut` borrows the field address without a
        // reference, matching the crate's existing pattern.
        unsafe { duckdb_destroy_result(&raw mut self.res) };
    }
}