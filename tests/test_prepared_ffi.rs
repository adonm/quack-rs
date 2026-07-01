// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Integration tests for `PreparedStatement` / `QueryResult` round-trips that
//! require a live `DuckDB` runtime. Run with:
//! `DUCKDB_DOWNLOAD_LIB=1 cargo test --test test_prepared_ffi --features bundled-test-prebuilt,duckdb-1-5`

use libduckdb_sys::{duckdb_close, duckdb_connect, duckdb_disconnect, duckdb_open};
use quack_rs::prepared::PreparedStatement;

use quack_rs::testing::InMemoryDb;
use quack_rs::value::Value;

/// Opens a fresh in-memory `DuckDB` connection after the dispatch table is live.
unsafe fn open_conn() -> (libduckdb_sys::duckdb_database, libduckdb_sys::duckdb_connection) {
    let mut db: libduckdb_sys::duckdb_database = std::ptr::null_mut();
    let mut con: libduckdb_sys::duckdb_connection = std::ptr::null_mut();
    unsafe {
        assert_eq!(duckdb_open(std::ptr::null(), &raw mut db), libduckdb_sys::DuckDBSuccess);
        assert_eq!(duckdb_connect(db, &raw mut con), libduckdb_sys::DuckDBSuccess);
    }
    (db, con)
}

unsafe fn close_conn(mut db: libduckdb_sys::duckdb_database, mut con: libduckdb_sys::duckdb_connection) {
    unsafe {
        duckdb_disconnect(&raw mut con);
        duckdb_close(&raw mut db);
    }
}

#[test]
fn prepared_statement_bind_and_execute() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let (db, con) = unsafe { open_conn() };
    let mut stmt = unsafe { PreparedStatement::prepare(con, "SELECT $1::INTEGER + $2::INTEGER AS s") }
        .expect("prepare");
    assert_eq!(stmt.parameter_count(), 2);
    stmt.bind_i32(1, 41).expect("bind 1");
    stmt.bind_i32(2, 1).expect("bind 2");
    let res = unsafe { stmt.execute() }.expect("execute");
    let chunk_opt = unsafe { res.fetch_chunk() };
    let chunk = chunk_opt.expect("first chunk");
    assert!(chunk.size() > 0, "expected at least one row");
    unsafe { close_conn(db, con) };
}

#[test]
fn prepared_statement_bind_value_roundtrip() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let (db, con) = unsafe { open_conn() };
    let v = Value::varchar("hello world");
    let mut stmt = unsafe { PreparedStatement::prepare(con, "SELECT $1::VARCHAR AS s") }
        .expect("prepare");
    stmt.bind_value(1, &v).expect("bind value");
    let _res = unsafe { stmt.execute() }.expect("execute");
    unsafe { close_conn(db, con) };
}

#[test]
fn prepared_statement_blob_roundtrip() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let (db, con) = unsafe { open_conn() };
    let bytes = vec![0xde, 0xad, 0xbe, 0xef, 0xff];
    let mut stmt = unsafe { PreparedStatement::prepare(con, "SELECT $1::BLOB AS b") }
        .expect("prepare");
    stmt.bind_blob(1, &bytes).expect("bind blob");
    let _res = unsafe { stmt.execute() }.expect("execute");
    unsafe { close_conn(db, con) };
}

#[test]
fn result_metadata_columns() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let (db, con) = unsafe { open_conn() };
    let stmt = unsafe {
        PreparedStatement::prepare(con, "SELECT 1 AS one, 'two' AS two, 3.14 AS pi")
    }
    .expect("prepare");
    let mut res = unsafe { stmt.execute() }.expect("execute");
    assert_eq!(res.column_count(), 3);
    assert_eq!(res.column_name(0), Some("one".to_owned()));
    assert_eq!(res.column_name(1), Some("two".to_owned()));
    assert_eq!(res.column_name(2), Some("pi".to_owned()));
    unsafe { close_conn(db, con) };
}

#[test]
fn prepare_error_reports_message() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let (db, con) = unsafe { open_conn() };
    let err = unsafe { PreparedStatement::prepare(con, "SELECT * FROM nonexistent_table_xyz") };
    assert!(err.is_err());
    let msg = err.err().unwrap().as_str().to_owned();
    assert!(msg.contains("nonexistent"), "expected error message to mention the table, got: {msg}");
    unsafe { close_conn(db, con) };
}

#[test]
fn extract_and_prepare_multi_statement() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let (db, con) = unsafe { open_conn() };
    let stmt = unsafe {
        quack_rs::prepared::extract_and_prepare(con, "SELECT 1; SELECT 2;", 1)
    }
    .expect("extract + prepare");
    let res = unsafe { stmt.execute() }.expect("execute");
    let _chunk = unsafe { res.fetch_chunk() }.expect("there should be a row");
    unsafe { close_conn(db, con) };
}
