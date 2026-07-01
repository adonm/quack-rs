// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Query profiling info tree (`DuckDB` 1.5.0+).
//!
//! [`ProfilingInfo`] wraps a `duckdb_profiling_info` handle, obtained via
//! [`Connection::get_profiling_info`][crate::connection::Connection::get_profiling_info]
//! after executing a query. The handle is a (possibly deep) tree of operator
//! execution metrics — walk it with [`child_count`][ProfilingInfo::child_count]
//! and [`child`][ProfilingInfo::child] to inspect each operator's
//! [`metrics`][ProfilingInfo::metrics] or named [`value`][ProfilingInfo::value].
//!
//! The handle is owned (RAII on drop). The `DuckDB` docs do not document a
//! destructor for `duckdb_profiling_info`; the handle lives for the duration
//! of the connection. We therefore expose it as a non-RAII *borrow* wrapper —
//! it does not own or destroy the underlying handle.

use libduckdb_sys::{
    duckdb_get_profiling_info, duckdb_profiling_info, duckdb_profiling_info_get_child,
    duckdb_profiling_info_get_child_count, duckdb_profiling_info_get_metrics,
    duckdb_profiling_info_get_value, idx_t,
};

use crate::value::Value;

/// Borrow wrapper around a `duckdb_profiling_info` handle.
///
/// Does NOT own the handle — the handle is owned by the connection that
/// executed the profiled query. Dropping this wrapper does nothing.
pub struct ProfilingInfo {
    info: duckdb_profiling_info,
}

impl ProfilingInfo {
    /// Wraps a raw `duckdb_profiling_info` handle.
    ///
    /// # Safety
    ///
    /// `info` must be a valid `duckdb_profiling_info` obtained from a
    /// `DuckDB` profiling-info API call. The handle is borrowed for the duration
    /// of this wrapper; the caller must ensure the owning connection outlives
    /// the wrapper.
    #[inline]
    #[must_use]
    pub const unsafe fn from_raw(info: duckdb_profiling_info) -> Self {
        Self { info }
    }

    /// Returns the raw `duckdb_profiling_info` handle without transferring ownership.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_profiling_info {
        self.info
    }

    /// Returns the metrics bundle for this operator as an owned `Value` (typically
    /// a `STRUCT` of named metric values). Returns `None` if `DuckDB` returns a
    /// null handle.
    #[must_use]
    pub fn metrics(&self) -> Option<Value> {
        // SAFETY: self.info is valid per constructor's contract.
        let raw = unsafe { duckdb_profiling_info_get_metrics(self.info) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: raw is a fresh, owned duckdb_value.
        Some(unsafe { Value::from_raw(raw) })
    }

    /// Returns the named metric `key` for this operator as an owned `Value`. The
    /// key must be NUL-free; if `key` contains an interior NUL it is truncated
    /// at that point.
    ///
    /// Returns `None` if the key is unknown or if `DuckDB` returns a null handle.
    #[must_use]
    pub fn value(&self, key: &str) -> Option<Value> {
        let c_key = std::ffi::CString::new(key).unwrap_or_else(|_| {
            let pos = key.bytes().position(|b| b == 0).unwrap_or(key.len());
            // SAFETY: pos is at the first null byte, so key[..pos] has no nulls.
            unsafe { std::ffi::CString::from_vec_with_nul_unchecked(key.as_bytes()[..pos].to_vec()) }
        });
        // SAFETY: self.info is valid per constructor's contract; c_key is NUL-terminated.
        let raw = unsafe { duckdb_profiling_info_get_value(self.info, c_key.as_ptr()) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: raw is a fresh, owned duckdb_value.
        Some(unsafe { Value::from_raw(raw) })
    }

    /// Returns the number of child operators (sub-trees) under this operator.
    #[must_use]
    pub fn child_count(&self) -> u64 {
        // SAFETY: self.info is valid per constructor's contract.
        unsafe { duckdb_profiling_info_get_child_count(self.info) as u64 }
    }

    /// Returns the child profiling-info at `index`, or `None` if out of bounds.
    ///
    /// The returned [`ProfilingInfo`] borrows the same connection's profiling tree.
    #[must_use]
    pub fn child(&self, index: u64) -> Option<Self> {
        let idx = idx_t::try_from(index).ok()?;
        // SAFETY: self.info is valid per constructor's contract.
        let raw = unsafe { duckdb_profiling_info_get_child(self.info, idx) };
        if raw.is_null() {
            return None;
        }
        // SAFETY: raw is a borrowed handle owned by the connection; valid for the
        // same lifetime as this wrapper.
        Some(unsafe { Self::from_raw(raw) })
    }
}

// `ProfilingInfo` does not own its handle — no Drop.

/// Root-handle helper for connection-scoped profiling info.
///
/// Pulls the root [`ProfilingInfo`] for the most recently executed query on
/// `con`. Used by
/// [`Connection::get_profiling_info`][crate::connection::Connection::get_profiling_info].
///
/// # Safety
///
/// `con` must be a valid, open `duckdb_connection` and the `DuckDB` runtime
/// must be initialised. Returns `None` if `DuckDB` returns a null handle
/// (e.g. no query has been executed yet).
pub unsafe fn get_profiling_info(con: libduckdb_sys::duckdb_connection) -> Option<ProfilingInfo> {
    // SAFETY: con is valid per caller's contract.
    let raw = unsafe { duckdb_get_profiling_info(con) };
    if raw.is_null() {
        return None;
    }
    // SAFETY: raw is a borrowed handle owned by the connection.
    Some(unsafe { ProfilingInfo::from_raw(raw) })
}