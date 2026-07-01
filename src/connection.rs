// SPDX-License-Identifier: MIT
// Copyright 2026 Tom F. <https://github.com/tomtom215/>
// My way of giving something small back to the open source community
// and encouraging more Rust development!

//! [`Connection`] — version-agnostic extension registration facade.
//!
//! [`Connection`] wraps the `duckdb_connection` and `duckdb_database` handles
//! provided to your extension during initialization. It implements the
//! [`Registrar`] trait, which provides a uniform API for registering all
//! extension components that works identically across `DuckDB` 1.4.x and 1.5.x.
//!
//! # Obtaining a `Connection`
//!
//! Use [`init_extension_v2`][crate::entry_point::init_extension_v2] or the
//! [`entry_point_v2!`][crate::entry_point_v2] macro. These pass a `&Connection`
//! to your registration callback instead of the raw `duckdb_connection`.
//!
//! ```rust,no_run
//! use quack_rs::connection::{Connection, Registrar};
//! use quack_rs::error::ExtensionError;
//! use quack_rs::scalar::ScalarFunctionBuilder;
//! use quack_rs::types::TypeId;
//!
//! unsafe fn register_all(reg: &impl Registrar) -> Result<(), ExtensionError> {
//!     let builder = ScalarFunctionBuilder::try_new("my_fn")?
//!         .returns(TypeId::BigInt);
//!     unsafe { reg.register_scalar(builder) }
//! }
//!
//! quack_rs::entry_point_v2!(my_extension_init_c_api, |con| {
//!     unsafe { register_all(con) }
//! });
//! ```
//!
//! # `DuckDB` version compatibility
//!
//! [`Connection`] and [`Registrar`] provide a stable API across `DuckDB` 1.4.x
//! and 1.5.x. The underlying C API version string (`"v1.2.0"`) is unchanged
//! across both releases, confirmed by E2E tests against both `DuckDB` 1.4.4 and
//! `DuckDB` 1.5.0.
//!
//! When a future `DuckDB` release changes the C API version or adds new
//! registration surface, additional methods will be added to [`Connection`]
//! behind a version-specific feature flag (e.g. `duckdb-1-5`).

use core::ffi::c_void;

use libduckdb_sys::{duckdb_connection, duckdb_database, duckdb_delete_callback_t};

use crate::aggregate::{AggregateFunctionBuilder, AggregateFunctionSetBuilder};
use crate::cast::CastFunctionBuilder;
#[cfg(feature = "duckdb-1-5")]
use crate::copy_function::CopyFunctionBuilder;
use crate::error::ExtensionError;
use crate::replacement_scan::{ReplacementScanBuilder, ReplacementScanFn};
use crate::scalar::{ScalarFunctionBuilder, ScalarFunctionSetBuilder};
use crate::sql_macro::SqlMacro;
use crate::table::TableFunctionBuilder;

/// Version-agnostic trait for registering `DuckDB` extension components.
///
/// Implemented by [`Connection`]. Writing registration code against this trait
/// means the same code compiles and runs on `DuckDB` 1.4.x and 1.5.x without
/// modification.
///
/// # Example
///
/// ```rust,no_run
/// use quack_rs::connection::Registrar;
/// use quack_rs::error::ExtensionError;
/// use quack_rs::scalar::ScalarFunctionBuilder;
/// use quack_rs::types::TypeId;
///
/// /// Register all functions for this extension.
/// ///
/// /// # Safety
/// ///
/// /// `reg` must provide a valid `DuckDB` connection for the duration of this call.
/// unsafe fn register_all(reg: &impl Registrar) -> Result<(), ExtensionError> {
///     let builder = ScalarFunctionBuilder::try_new("my_fn")?
///         .returns(TypeId::BigInt);
///     unsafe { reg.register_scalar(builder) }
/// }
/// ```
pub trait Registrar {
    /// Register a scalar function.
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid for the duration of this call.
    unsafe fn register_scalar(&self, builder: ScalarFunctionBuilder) -> Result<(), ExtensionError>;

    /// Register a scalar function set (multiple overloads under one name).
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid for the duration of this call.
    unsafe fn register_scalar_set(
        &self,
        builder: ScalarFunctionSetBuilder,
    ) -> Result<(), ExtensionError>;

    /// Register an aggregate function.
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid for the duration of this call.
    unsafe fn register_aggregate(
        &self,
        builder: AggregateFunctionBuilder,
    ) -> Result<(), ExtensionError>;

    /// Register an aggregate function set (multiple overloads under one name).
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid for the duration of this call.
    unsafe fn register_aggregate_set(
        &self,
        builder: AggregateFunctionSetBuilder,
    ) -> Result<(), ExtensionError>;

    /// Register a table function.
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid for the duration of this call.
    unsafe fn register_table(&self, builder: TableFunctionBuilder) -> Result<(), ExtensionError>;

    /// Register a `SQL` macro (scalar or table-returning).
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid for the duration of this call.
    unsafe fn register_sql_macro(&self, sql_macro: SqlMacro) -> Result<(), ExtensionError>;

    /// Register a custom type cast function.
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid for the duration of this call.
    unsafe fn register_cast(&self, builder: CastFunctionBuilder) -> Result<(), ExtensionError>;

    /// Register a custom `COPY TO` function (`DuckDB` 1.5.0+).
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid for the duration of this call.
    #[cfg(feature = "duckdb-1-5")]
    unsafe fn register_copy_function(
        &self,
        builder: CopyFunctionBuilder,
    ) -> Result<(), ExtensionError>;
}

/// Wraps the `duckdb_connection` and `duckdb_database` provided to your
/// extension at load time.
///
/// `Connection` implements [`Registrar`], offering a single, uniform API for
/// registering all extension components. It also exposes
/// [`register_replacement_scan`][Self::register_replacement_scan] and
/// [`register_replacement_scan_with_data`][Self::register_replacement_scan_with_data],
/// which require the `duckdb_database` handle and therefore cannot be part of
/// the `Registrar` trait.
///
/// # Obtaining a `Connection`
///
/// Use [`init_extension_v2`][crate::entry_point::init_extension_v2] (or the
/// [`entry_point_v2!`][crate::entry_point_v2] macro). Both pass a `&Connection`
/// to your registration callback.
///
/// # Version compatibility
///
/// `Connection` provides a uniform API across `DuckDB` 1.4.x and 1.5.x.
/// When future `DuckDB` releases add new C API surface, additional methods will
/// be gated on the corresponding feature flag.
pub struct Connection {
    con: duckdb_connection,
    db: duckdb_database,
}

impl Connection {
    /// Create a `Connection` from raw `DuckDB` handles.
    ///
    /// # Safety
    ///
    /// Both `con` and `db` must be valid, non-null handles for the duration of
    /// the `Connection`'s lifetime. Intended for internal use by
    /// [`init_extension_v2`][crate::entry_point::init_extension_v2].
    #[inline]
    pub(crate) const unsafe fn from_raw(con: duckdb_connection, db: duckdb_database) -> Self {
        Self { con, db }
    }

    /// Return the raw `duckdb_connection` handle.
    ///
    /// Use this to call C API functions that `quack-rs` does not yet wrap.
    #[inline]
    pub const fn as_raw_connection(&self) -> duckdb_connection {
        self.con
    }

    /// Return the raw `duckdb_database` handle.
    ///
    /// Use this to call C API functions that require the database handle, such
    /// as replacement scan registration or (with `duckdb-1-5`) config option
    /// registration.
    #[inline]
    pub const fn as_raw_database(&self) -> duckdb_database {
        self.db
    }

    /// Register a replacement scan backed by a raw function pointer and extra
    /// data.
    ///
    /// For the ergonomic owned-data variant, see
    /// [`register_replacement_scan_with_data`][Self::register_replacement_scan_with_data].
    ///
    /// # Safety
    ///
    /// - The underlying `duckdb_database` must be valid.
    /// - `extra_data` must remain valid until `delete_callback` is called (or
    ///   until the database is closed if `delete_callback` is `None`).
    pub unsafe fn register_replacement_scan(
        &self,
        callback: ReplacementScanFn,
        extra_data: *mut c_void,
        delete_callback: duckdb_delete_callback_t,
    ) {
        // SAFETY: self.db is valid per Connection invariant.
        unsafe {
            ReplacementScanBuilder::register(self.db, callback, extra_data, delete_callback);
        }
    }

    /// Register a replacement scan with owned extra data.
    ///
    /// Boxes `data` and registers a drop destructor automatically. This is the
    /// safe, ergonomic alternative to
    /// [`register_replacement_scan`][Self::register_replacement_scan].
    ///
    /// # Safety
    ///
    /// The underlying `duckdb_database` must be valid.
    pub unsafe fn register_replacement_scan_with_data<T: 'static>(
        &self,
        callback: ReplacementScanFn,
        data: T,
    ) {
        // SAFETY: self.db is valid per Connection invariant.
        unsafe {
            ReplacementScanBuilder::register_with_data(self.db, callback, data);
        }
    }

    /// Returns the list of table names referenced by `query` as a `DuckDB`
    /// `LIST` `Value` of `VARCHAR`s (`DuckDB` 1.5.0+).
    ///
    /// Each entry is a fully qualified (`catalog.schema.table`) name when
    /// `qualified` is `true`, otherwise bare `table` names. Use
    /// [`Value::list_size`][crate::value::Value::list_size] /
    /// [`Value::list_child`][crate::value::Value::list_child] to enumerate the
    /// returned names; the returned [`Value`] is RAII-managed.
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid (requires `DuckDB` runtime).
    /// If `query` contains an interior `NUL` byte it is truncated at that point.
    #[cfg(feature = "duckdb-1-5")]
    pub unsafe fn get_table_names(&self, query: &str, qualified: bool) -> crate::value::Value {
        // SAFETY: CString::new never panics thanks to the truncation below.
        let c_query = std::ffi::CString::new(query).unwrap_or_else(|_| {
            let pos = query.bytes().position(|b| b == 0).unwrap_or(query.len());
            // SAFETY: pos is at the first null byte, so query[..pos] has no nulls.
            std::ffi::CString::new(&query.as_bytes()[..pos]).unwrap_or_default()
        });
        // SAFETY: self.con is valid; c_query is a valid NUL-terminated C string.
        let raw = unsafe {
            libduckdb_sys::duckdb_get_table_names(self.con, c_query.as_ptr(), qualified)
        };
        // SAFETY: raw is a fresh, owned duckdb_value returned by `DuckDB`.
        unsafe { crate::value::Value::from_raw(raw) }
    }

    /// Returns this connection's [`ClientContext`] handle (`DuckDB` 1.5.0+).
    ///
    /// The `ClientContext` exposes connection-scoped catalog access,
    /// configuration queries, and the connection ID.
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid (requires `DuckDB` runtime).
    /// The returned `ClientContext` borrows from this `Connection`; do not
    /// keep it past the connection's lifetime.
    #[cfg(feature = "duckdb-1-5")]
    pub unsafe fn get_client_context(&self) -> crate::client_context::ClientContext {
        let mut ctx: libduckdb_sys::duckdb_client_context = core::ptr::null_mut();
        // SAFETY: self.con is valid; this writes a fresh handle into ctx.
        unsafe { libduckdb_sys::duckdb_connection_get_client_context(self.con, &raw mut ctx) };
        // SAFETY: ctx is a fresh, owned client-context handle from `DuckDB`.
        unsafe { crate::client_context::ClientContext::from_raw(ctx) }
    }

    /// Returns the root profiling-info handle for the most recently executed
    /// query on this connection (`DuckDB` 1.5.0+).
    ///
    /// Returns `None` if no query has been profiled yet (or `DuckDB` returns a
    /// null handle). The returned [`ProfilingInfo`][crate::profiling::ProfilingInfo]
    /// borrows from this connection — it must not outlive it.
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid (requires `DuckDB` runtime).
    #[cfg(feature = "duckdb-1-5")]
    pub unsafe fn get_profiling_info(&self) -> Option<crate::profiling::ProfilingInfo> {
        // SAFETY: self.con is valid; the returned handle is borrowed by the connection.
        unsafe { crate::profiling::get_profiling_info(self.con) }
    }

    /// Returns a progress snapshot for the currently-running query on this
    /// connection (`DuckDB` 1.5.0+).
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid (requires `DuckDB` runtime).
    #[cfg(feature = "duckdb-1-5")]
    pub unsafe fn query_progress(&self) -> libduckdb_sys::duckdb_query_progress_type {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { crate::task::query_progress(self.con) }
    }

    /// Interrupts the currently-running query on this connection (`DuckDB` 1.5+).
    /// The query will fail with an "interrupted" error on its next check.
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid (requires `DuckDB` runtime).
    #[cfg(feature = "duckdb-1-5")]
    pub unsafe fn interrupt(&self) {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { crate::task::interrupt(self.con) };
    }

    /// Returns `true` when the currently-running query on this connection has
    /// finished (`DuckDB` 1.5+).
    ///
    /// # Safety
    ///
    /// The underlying connection must be valid (requires `DuckDB` runtime).
    #[cfg(feature = "duckdb-1-5")]
    #[must_use]
    pub unsafe fn execution_is_finished(&self) -> bool {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { crate::task::execution_is_finished(self.con) }
    }
}

impl Registrar for Connection {
    unsafe fn register_scalar(&self, builder: ScalarFunctionBuilder) -> Result<(), ExtensionError> {
        // SAFETY: self.con is valid per Connection invariant; caller upholds builder contract.
        unsafe { builder.register(self.con) }
    }

    unsafe fn register_scalar_set(
        &self,
        builder: ScalarFunctionSetBuilder,
    ) -> Result<(), ExtensionError> {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { builder.register(self.con) }
    }

    unsafe fn register_aggregate(
        &self,
        builder: AggregateFunctionBuilder,
    ) -> Result<(), ExtensionError> {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { builder.register(self.con) }
    }

    unsafe fn register_aggregate_set(
        &self,
        builder: AggregateFunctionSetBuilder,
    ) -> Result<(), ExtensionError> {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { builder.register(self.con) }
    }

    unsafe fn register_table(&self, builder: TableFunctionBuilder) -> Result<(), ExtensionError> {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { builder.register(self.con) }
    }

    unsafe fn register_sql_macro(&self, sql_macro: SqlMacro) -> Result<(), ExtensionError> {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { sql_macro.register(self.con) }
    }

    unsafe fn register_cast(&self, builder: CastFunctionBuilder) -> Result<(), ExtensionError> {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { builder.register(self.con) }
    }

    #[cfg(feature = "duckdb-1-5")]
    unsafe fn register_copy_function(
        &self,
        builder: CopyFunctionBuilder,
    ) -> Result<(), ExtensionError> {
        // SAFETY: self.con is valid per Connection invariant.
        unsafe { builder.register(self.con) }
    }
}
