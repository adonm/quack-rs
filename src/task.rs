// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Parallel scheduling state for `DuckDB` (1.5+).
//!
//! Two independent surfaces live here:
//!
//! - [`TaskState`] — RAII wrapper around `duckdb_task_state`, used to drive
//!   `DuckDB`'s internal task scheduler from outside a scan callback. Useful
//!   for long-running extension background work that wants to participate in
//!   `DuckDB`'s thread pool.
//! - Connection-scoped async helpers: [`query_progress`], [`interrupt`],
//!   [`execution_is_finished`]. These give an extension the ability to poll
//!   a long-running query and pre-empt it without owning the
//!   `duckdb_connection`'s query path.

use libduckdb_sys::{
    duckdb_create_task_state, duckdb_destroy_task_state, duckdb_execution_is_finished,
    duckdb_execute_n_tasks_state, duckdb_execute_tasks, duckdb_execute_tasks_state,
    duckdb_finish_execution, duckdb_interrupt, duckdb_query_progress, duckdb_task_state,
    duckdb_task_state_is_finished, idx_t,
};

/// RAII wrapper around a `duckdb_task_state` (1.5+).
///
/// The state is destroyed on drop via `duckdb_destroy_task_state` — there is no
/// failure mode for destroy, so dropping is infallible.
pub struct TaskState {
    state: duckdb_task_state,
}

impl TaskState {
    /// Creates a fresh task state bound to the given `database`.
    ///
    /// # Safety
    ///
    /// `database` must be a valid, open `duckdb_database`. The `DuckDB` runtime
    /// must be initialised.
    #[must_use]
    pub unsafe fn new(database: libduckdb_sys::duckdb_database) -> Self {
        // SAFETY: database is valid per caller's contract.
        let state = unsafe { duckdb_create_task_state(database) };
        Self { state }
    }

    /// Runs `DuckDB` tasks on the current thread until there are none left.
    ///
    /// Returns when the queue is drained or [`is_finished`][Self::is_finished]
    /// flips to `true`.
    pub fn execute_all(&self) {
        // SAFETY: self.state is valid per constructor's contract.
        unsafe { duckdb_execute_tasks_state(self.state) };
    }

    /// Runs at most `max_tasks` `DuckDB` tasks on the current thread.
    ///
    /// Returns the number of tasks actually executed.
    pub fn execute_n(&self, max_tasks: u64) -> u64 {
        let n = idx_t::try_from(max_tasks).unwrap_or(idx_t::MAX);
        // SAFETY: self.state is valid per constructor's contract.
        unsafe { duckdb_execute_n_tasks_state(self.state, n) as u64 }
    }

    /// Signals `DuckDB` that execution on this task state should finish.
    ///
    /// After this call [`is_finished`][Self::is_finished] will eventually return
    /// `true` once any in-flight tasks drain.
    pub fn finish(&self) {
        // SAFETY: self.state is valid per constructor's contract.
        unsafe { duckdb_finish_execution(self.state) };
    }

    /// Returns `true` when [`finish`][Self::finish] has been called *and* all
    /// in-flight tasks have drained.
    pub fn is_finished(&self) -> bool {
        // SAFETY: self.state is valid per constructor's contract.
        unsafe { duckdb_task_state_is_finished(self.state) }
    }

    /// Returns the raw `duckdb_task_state` handle without transferring ownership.
    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> duckdb_task_state {
        self.state
    }
}

impl Drop for TaskState {
    fn drop(&mut self) {
        if !self.state.is_null() {
            // SAFETY: self.state is owned per constructor's contract.
            unsafe { duckdb_destroy_task_state(self.state) };
        }
    }
}

/// Runs up to `max_tasks` `DuckDB` tasks on the current thread against the
/// scheduler bound to `database`. Convenience for one-shot scheduling without
/// the caller owning a [`TaskState`] RAII wrapper.
///
/// # Safety
///
/// `database` must be a valid, open `duckdb_database`.
pub unsafe fn execute_tasks(database: libduckdb_sys::duckdb_database, max_tasks: u64) {
    let n = idx_t::try_from(max_tasks).unwrap_or(idx_t::MAX);
    // SAFETY: database is valid per caller's contract.
    unsafe { duckdb_execute_tasks(database, n) };
}

/// Connection-scoped progress snapshot for the currently-running query on
/// `connection` (1.5+).
///
/// `DuckDB` reports `{ percentage: double, rows_processed: u64, total_rows_to_process: u64 }`.
/// For a non-streaming query the percentage goes from 0.0 to 100.0; for a
/// streaming query it may stay at 0.0 (no fixed total).
///
/// # Safety
///
/// `connection` must be a valid, open `duckdb_connection`. Reading progress is
/// best-effort and may return zero if no query is running.
#[must_use]
pub unsafe fn query_progress(
    connection: libduckdb_sys::duckdb_connection,
) -> libduckdb_sys::duckdb_query_progress_type {
    // SAFETY: connection is valid per caller's contract.
    unsafe { duckdb_query_progress(connection) }
}

/// Interrupts the currently-running query on `connection` (1.5+).
///
/// The interrupted query will fail with an "interrupted" error on its next
/// check.
///
/// # Safety
///
/// `connection` must be a valid, open `duckdb_connection`.
pub unsafe fn interrupt(connection: libduckdb_sys::duckdb_connection) {
    // SAFETY: connection is valid per caller's contract.
    unsafe { duckdb_interrupt(connection) };
}

/// Returns `true` when execution on `connection` has finished — i.e. the
/// currently-running query has completed (or been interrupted).
///
/// # Safety
///
/// `connection` must be a valid, open `duckdb_connection`.
#[must_use]
pub unsafe fn execution_is_finished(connection: libduckdb_sys::duckdb_connection) -> bool {
    // SAFETY: connection is valid per caller's contract.
    unsafe { duckdb_execution_is_finished(connection) }
}