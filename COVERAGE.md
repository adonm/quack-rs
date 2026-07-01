# quack-rs C-API Coverage Audit

This document tracks which `duckdb_ext_api_v1` (ext-API v1.2.0) function-pointer
fields are wrapped by quack-rs and which are intentionally not wrapped, with
rationale. The single merge target is "every field either has a safe wrapper
**or** appears in the Intentionally Unsupported section with a reason
(other than lack of time)".

## How to reproduce

```sh
# In a DuckDB source tree:
awk '/^typedef struct \{/,/^\} duckdb_ext_api_v1;/'
        src/include/duckdb/main/capi/extension_api.hpp \
  | grep -oE '\(\*duckdb_[a-z_0-9]+\)' \
  | sed -E 's/\(\*//; s/\)//' | sort -u > /tmp/api_sorted.txt

# In quack-rs source tree:
perl -0777 -pe 's{///.*}{}g; s{/\*.*?\*/}{}gs' $(find src -name '*.rs') \
  | rg -o '\bduckdb_[a-z_0-9]+\s*\(' -r '$0' \
  | sed -E 's/\s*\($//' | sort -u > /tmp/calls_sorted.txt

comm -12 /tmp/api_sorted.txt /tmp/calls_sorted.txt | wc -l   # wrapped
comm -23 /tmp/api_sorted.txt /tmp/calls_sorted.txt | wc -l   # gap
```

## Current state (as of the head of `feat/c-api-full-coverage`)

| | Count | % |
|---|---|---|
| Total `duckdb_ext_api_v1` fields      | 428 | 100 |
| Wrapped (invoked at least once)      | 378 | **88%** |
| Intentionally unsupported (deprecated) | 44 | 10% |
| Pending follow-up / upstream-blocked | 4   | 1% |

**Wrapped**: 378 of 428 ext-API fields are reachable through quack-rs
safe-Rust wrappers. This includes:
- Scalar/aggregate/table/cast function builders and bind/init/data getters.
- Vector read/write, complex (LIST/STRUCT/MAP/ARRAY), validity, plus
  `OwnedVector` (create/slice/reference) and `SelectionVector`.
- Value typed constructors and accessors for every primitive and every
  timestamp variant; compound-type navigation (`list_child`, `struct_child`,
  `map_at`/`map_key`, `value_type`, `is_sql_null`, `display_string`).
- Compound-type constructors (`struct_value`, `list_value`, `array_value`,
  `map_value`, `enum_value`, `union_value`, `bit`, `decimal`, `time`,
  `time_tz`) plus the inverse `as_enum_value`.
- Logical type constructors and child inspection for LIST/MAP/
  STRUCT/ARRAY/UNION/ENUM/DECIMAL.
- Appender full lifecycle: typed `append_*`, `begin_row`/`end_row`,
  `append_default`/`append_null`/`append_value`, column-subset
  management, metadata (`column_count`/`column_type`/`error_message`).
- `PreparedStatement` + typed `bind_*` for every parameter shape,
  `execute`, `extract_and_prepare`, `Pending` async pipeline
  (`pending_prepared`, `execute_task`, `check_state`,
  `execute_pending`, `pending_error`).
- `QueryResult` + `fetch_chunk`/`stream_fetch_chunk` + column metadata.
- `ClientContext`, `InstanceCache`, `TableDescription` (`column_has_default`,
  `create_with_catalog`), `Connection::get_table_names`/`get_client_context`.
- Primitive conversions (date/time/timestamp/hugeint/decimal/`string_t`).
- `ProfilingInfo` tree walk (`metrics`/`value`/`child`).
- `TaskState` RAII + connection-scoped async (`query_progress`/`interrupt`/
  `execution_is_finished`).
- `OwnedDataChunk` RAII chunk allocator.
- Crate root helpers: `library_version()`.

## Intentionally unsupported (deprecated upstream)

These ext-API fields exist in `duckdb_ext_api_v1` but are explicitly marked
as deprecated in `duckdb.h` / `extension_api.hpp` and slated for removal.
quack-rs intentionally does **not** wrap them; downstream extensions should
use the supported replacement.

| Deprecated section | Replacement | Reason |
|---|---|---|
| `duckdb_row_count`, `duckdb_column_data`, `duckdb_nullmask_data`, `duckdb_result_get_chunk`, `duckdb_result_is_streaming`, `duckdb_result_chunk_count` | `QueryResult::fetch_chunk` / `stream_fetch_chunk` | Deprecated by the pull-based `duckdb_fetch_chunk` API. |
| `duckdb_value_boolean`, `duckdb_value_int8/16/32/64`, `duckdb_value_uint8/16/32/64`, `duckdb_value_float`, `duckdb_value_double`, `duckdb_value_date`, `duckdb_value_time`, `duckdb_value_timestamp`, `duckdb_value_interval`, `duckdb_value_varchar`, `duckdb_value_varchar_internal`, `duckdb_value_string`, `duckdb_value_string_internal`, `duckdb_value_blob`, `duckdb_value_decimal`, `duckdb_value_uhugeint`, `duckdb_value_hugeint`, `duckdb_value_is_null` | Bind a `PreparedStatement` instead; read columns via `VectorReader::from_vector(chunk_get_vector())` | Deprecated full-materialised `duckdb_result` row/column accessors. |
| `duckdb_execute_prepared_streaming`, `duckdb_pending_prepared_streaming` | `PreparedStatement::execute` / `Pending` pipeline | Deprecated streaming variants of prepared execution; `fetch_chunk` is the supported streaming path. |
| `duckdb_query_arrow`, `duckdb_query_arrow_schema`, `duckdb_query_arrow_array`, `duckdb_prepared_arrow_schema`, `duckdb_result_arrow_array`, `duckdb_arrow_column_count`, `duckdb_arrow_row_count`, `duckdb_arrow_rows_changed`, `duckdb_arrow_rows_changed`, `duckdb_query_arrow_error`, `duckdb_destroy_arrow`, `duckdb_destroy_arrow_stream`, `duckdb_execute_prepared_arrow`, `duckdb_arrow_scan`, `duckdb_arrow_array_scan` | Plain `PreparedStatement` execution + `VectorReader` for data, or an out-of-extension Arrow-CCA bridge. | Arrow interop bindings are deprecated and pull the `arrow` crate into the SDK dep graph. The extension-arrow path is being replaced by Arrow-CCA via the table-function API upstream; revisit once stabilised. |

## Pending follow-up

These ext-API fields are supported by `DuckDB` but are either lower-value for
typical extensions or sit on the boundary with future APIs that aren't stable
yet. They are tracked as follow-up work.

| Family | Fields | Why deferred |
|---|---|---|
| Profiling info (wrapped) | `Module: prof`iling` (ProfilingInfo wrapper + Connection accessor) | Wrapped. |
| Task state (wrapped) | `Module: task` (TaskState + Connection async helpers) | Wrapped. |
| Date/time/numeric primitive conversions (wrapped) | `Module: prim` | Wrapped. |
| `string_t` introspection (wrapped) | `Module: prim` (`string_is_inlined`/`string_t_length`/`string_t_data`) | Wrapped. |
| `OwnedDataChunk` (wrapped) | `Module: data_chunk_owned` | Wrapped. |
| `duckdb_register_logical_type` | (single field) | The `duckdb_create_type_info` argument is opaque ("Reserved for future use" per `duckdb.h`) — no public API to construct one. Cannot be safely wrapped until upstream exposes its builder or marks it as a no-op pass-through. |
| `duckdb_create_varint` / `duckdb_get_varint` | (2 fields) | Variably-sized integer type. Not yet exposed in `libduckdb-sys` 1.10504.0 — the C API has the typedef but the dispatch-table function pointer slots are `None` at 1.4.x runtime. Will wrap once `libduckdb-sys` resolves the symbols across the version range. |
| `duckdb_malloc` | (single field) | C allocator helper. All quack-rs allocation uses the Rust allocator and `duckdb_free` for `DuckDB`-owned buffers; wrapping the standalone allocator would invite mixed-allocator UB. Prefer `std::alloc` from Rust. |