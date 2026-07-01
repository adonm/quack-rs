// SPDX-License-Identifier: MIT
// Copyright 2026 Adon Metcalfe <adonm@fastmail.fm>

//! Integration tests for `prim` round-trip conversions that require a live
//! `DuckDB` runtime. Run with:
//! `DUCKDB_DOWNLOAD_LIB=1 cargo test --test test_prim_ffi --features bundled-test-prebuilt,duckdb-1-5`

use quack_rs::prim;
use quack_rs::testing::InMemoryDb;

#[test]
fn date_roundtrip() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let parts = prim::DateParts {
        year: 2024,
        month: 7,
        day: 14,
    };
    let d = unsafe { prim::to_date(parts) };
    let back = unsafe { prim::from_date(d) };
    assert_eq!(back.year, 2024);
    assert_eq!(back.month, 7);
    assert_eq!(back.day, 14);
    assert!(unsafe { prim::is_finite_date(d) });
}

#[test]
fn time_roundtrip() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let parts = prim::TimeParts {
        hour: 12,
        min: 30,
        sec: 45,
        micros: 123_456,
    };
    let t = unsafe { prim::to_time(parts) };
    let back = unsafe { prim::from_time(t) };
    assert_eq!(back.hour, 12);
    assert_eq!(back.min, 30);
    assert_eq!(back.sec, 45);
    assert_eq!(back.micros, 123_456);
}

#[test]
fn timestamp_roundtrip() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let date = prim::DateParts {
        year: 2000,
        month: 1,
        day: 1,
    };
    let time = prim::TimeParts {
        hour: 0,
        min: 0,
        sec: 0,
        micros: 0,
    };
    let parts = prim::TimestampParts { date, time };
    let ts = unsafe { prim::to_timestamp(parts) };
    let back = unsafe { prim::from_timestamp(ts) };
    assert_eq!(back.date.year, 2000);
    assert_eq!(back.time.hour, 0);
    assert!(unsafe { prim::is_finite_timestamp(ts) });
}

#[test]
fn hugeint_to_double_and_back() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let h = libduckdb_sys::duckdb_hugeint {
        lower: 42,
        upper: 0,
    };
    let d = unsafe { prim::hugeint_to_double(h) };
    assert!((d - 42.0).abs() < f64::EPSILON);
    let back = unsafe { prim::double_to_hugeint(42.0) };
    assert_eq!(back.lower, 42);
    assert_eq!(back.upper, 0);
}

#[test]
fn decimal_roundtrip() {
    let _db = InMemoryDb::open().expect("InMemoryDb::open");
    let d = unsafe { prim::double_to_decimal(2.71, 18, 2) };
    let back = unsafe { prim::decimal_to_double(d) };
    assert!((back - 2.71).abs() < 0.01);
}