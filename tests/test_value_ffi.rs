// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Integration tests for `Value` typed constructors/accessors that require a
//! live `DuckDB` runtime. Run with:
//! `DUCKDB_DOWNLOAD_LIB=1 cargo test --test test_value_ffi --features bundled-test-prebuilt,duckdb-1-5`

use quack_rs::testing::InMemoryDb;
use quack_rs::value::Value;

#[test]
fn value_create_and_extract_bool() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let v = Value::boolean(true);
    assert!(v.as_bool());
}

#[test]
fn value_create_and_extract_int() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let v = Value::integer(42);
    assert_eq!(v.as_i32(), 42);
}

#[test]
fn value_create_and_extract_blob() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let bytes = vec![0xde, 0xad, 0xbe, 0xef];
    let v = Value::blob(&bytes);
    assert_eq!(v.as_blob().unwrap(), bytes);
}

#[test]
fn value_varchar_roundtrip() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let v = Value::varchar("hello world");
    assert_eq!(v.as_str().unwrap(), "hello world");
}

#[test]
fn value_sql_null() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let v = Value::sql_null();
    assert!(v.is_sql_null());
    assert!(!v.is_null());
}

#[test]
fn value_display_string() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let v = Value::integer(42);
    assert_eq!(v.display_string(), Some("42".to_string()));
}

#[test]
fn value_hugeint_roundtrip() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let n = i128::from(i64::MAX) * 100;
    let v = Value::hugeint(n);
    assert_eq!(v.as_i128(), n);
}
