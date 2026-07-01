// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Standalone (chunk-independent) `DuckDB` vectors (`DuckDB` 1.5.0+).
//!
//! Gated by the `duckdb-1-5` feature; the underlying
//! `duckdb_create_vector`/`duckdb_slice_vector`/`duckdb_vector_reference_*`
//! C-API fields were added to the ext-api vtable in `DuckDB` 1.5.0.
//!
//! `DuckDB` 1.5.0 introduced an API for creating vectors that are not tied to
//! a specific data chunk — useful when building vectors for `duckdb_append_value`,
//! `duckdb_arrow_array_scan`, or as inputs to other vectorised APIs.
//!
//! An [`OwnedVector`] wraps a `duckdb_vector` created by `duckdb_create_vector`
//! and destroys it on drop. Use [`slice`][OwnedVector::slice] to reorder/filter
//! via a [`SelectionVector`][crate::selection_vector::SelectionVector] without
//! copying payload, and [`reference_value`] / [`reference_vector`] to alias
//! another vector's contents (zero-copy).

use libduckdb_sys::{
    duckdb_create_vector, duckdb_destroy_vector, duckdb_slice_vector, duckdb_vector,
    duckdb_vector_reference_value, duckdb_vector_reference_vector, idx_t,
};

use crate::selection_vector::SelectionVector;
use crate::types::LogicalType;
use crate::value::Value;

/// An owned `DuckDB` vector allocated via `duckdb_create_vector` (`DuckDB` 1.5.0+).
///
/// The vector is destroyed on drop. Not associated with a `DataChunk`.
pub struct OwnedVector {
    vec: duckdb_vector,
}

impl OwnedVector {
    /// Allocates a standalone vector of the given logical type and capacity.
    ///
    /// # Safety
    ///
    /// `DuckDB` runtime must be initialised (the typical extension load path
    /// guarantees this). The returned vector is uninitialised; the caller is
    /// responsible for writing into it before reading.
    #[must_use]
    pub unsafe fn new(logical_type: &LogicalType, capacity: usize) -> Self {
        let cap = idx_t::try_from(capacity).unwrap_or(idx_t::MAX);
        // SAFETY: logical_type.as_raw() is a valid duckdb_logical_type handle.
        let vec = unsafe { duckdb_create_vector(logical_type.as_raw(), cap) };
        Self { vec }
    }

    /// Returns the raw `duckdb_vector` handle without transferring ownership.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_vector {
        self.vec
    }

    /// Reorders rows in this vector according to `selection`, in place.
    ///
    /// After the call, row `i` of this vector holds the row that was at
    /// `selection.as_slice()[i]`. The selection length becomes the new vector
    /// length.
    ///
    /// # Safety
    ///
    /// - Every entry in `selection` must be `<` the current vector capacity.
    /// - The underlying vector must be valid (non-null).
    pub unsafe fn slice(&mut self, selection: &SelectionVector, len: usize) {
        let raw_len = idx_t::try_from(len).unwrap_or(idx_t::MAX);
        // SAFETY: self.vec is valid per constructor's contract; selection.as_raw()
        // is a valid duckdb_selection_vector owned by the caller for this call.
        unsafe { duckdb_slice_vector(self.vec, selection.as_raw(), raw_len) };
    }

    /// Fills this vector with a single scalar value (broadcasts `value` to every row).
    ///
    /// `value` is *not* consumed by this call; the underlying reference is tracked
    /// by `DuckDB`. The caller must keep `value` alive for at least as long as this
    /// vector.
    ///
    /// # Safety
    ///
    /// - `value` must be a non-null `duckdb_value` whose logical type matches this
    ///   vector's logical type.
    /// - The underlying vector must be valid.
    pub unsafe fn reference_value(&mut self, value: &Value) {
        // SAFETY: self.vec is valid; value.as_raw() is a valid duckdb_value.
        unsafe { duckdb_vector_reference_value(self.vec, value.as_raw()) };
    }

    /// Makes this vector a zero-copy alias of `from` (same payload, valid y bitmap, type).
    ///
    /// After the call, this vector shares `from`'s backing memory; modifying either
    /// affects both. The caller must keep `from` alive for at least as long as this
    /// vector so the shared buffer remains valid.
    ///
    /// # Safety
    ///
    /// - `from` must be a valid `duckdb_vector`.
    /// - The underlying vector (self) must be valid.
    pub unsafe fn reference_vector(&mut self, from: &Self) {
        // SAFETY: self.vec and from.vec are valid per constructors' contracts.
        unsafe { duckdb_vector_reference_vector(self.vec, from.as_raw()) };
    }
}

impl Drop for OwnedVector {
    fn drop(&mut self) {
        if !self.vec.is_null() {
            // SAFETY: self.vec is a valid, owned duckdb_vector per constructor.
            // raw-mut borrows the field address without creating a reference,
            // which is the same pattern used elsewhere in this crate.
            unsafe { duckdb_destroy_vector(&raw mut self.vec) };
        }
    }
}