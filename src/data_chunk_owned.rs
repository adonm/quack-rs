// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! RAII-owned `DuckDB` data chunk (`duckdb_create_data_chunk`).
//!
//! The borrow-style [`DataChunk`][crate::data_chunk::DataChunk] wrapper is
//! meant for chunks owned by a `DuckDB` callback (e.g. the `output` parameter
//! of a table scan). Most extension authors want the borrow form.
//!
//! [`OwnedDataChunk`] is the rare escape hatch: it allocates a fresh chunk
//! (typed columns + capacity) outside any callback, provides mutable access
//! via [`as_chunk`][OwnedDataChunk::as_chunk], and destroys it on drop. Useful
//! for building a chunk by hand to pass to [`Appender::append_chunk`][crate::appender::Appender::append_chunk]
//! or for buffer-state isolation in unit tests.

use libduckdb_sys::{
    duckdb_create_data_chunk, duckdb_data_chunk, duckdb_destroy_data_chunk, duckdb_logical_type,
    idx_t,
};

use crate::data_chunk::DataChunk;
use crate::types::LogicalType;

/// RAII-owned standalone `DuckDB` data chunk. Destroyed on drop.
pub struct OwnedDataChunk {
    raw: duckdb_data_chunk,
}

impl OwnedDataChunk {
    /// Allocates a new chunk with the given column types (in order) and the
    /// standard vector capacity (`duckdb_vector_size`).
    ///
    /// # Safety
    ///
    /// Requires `DuckDB` runtime to be initialised.
    #[must_use]
    pub unsafe fn new(column_types: &[LogicalType]) -> Self {
        let mut raw_types: Vec<duckdb_logical_type> =
            column_types.iter().map(LogicalType::as_raw).collect();
        let count = idx_t::try_from(raw_types.len()).unwrap_or(0);
        // SAFETY: column_types are valid handles; count matches the slice length.
        let raw = unsafe { duckdb_create_data_chunk(raw_types.as_mut_ptr(), count) };
        Self { raw }
    }

    /// Borrows this chunk for read/write operations via the borrow-style
    /// [`DataChunk`] wrapper. The returned reference is valid for the lifetime
    /// of this [`OwnedDataChunk`].
    #[must_use]
    pub const fn as_chunk(&self) -> DataChunk {
        // SAFETY: self.raw is a valid, non-null handle per constructor's
        // contract; the wrapper is a non-owning view that lives as long as
        // `self`.
        unsafe { DataChunk::from_raw(self.raw) }
    }

    /// Returns the raw `duckdb_data_chunk` handle without transferring ownership.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_data_chunk {
        self.raw
    }
}

impl Drop for OwnedDataChunk {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: self.raw is owned per constructor's contract.
            unsafe { duckdb_destroy_data_chunk(&raw mut self.raw) };
        }
    }
}