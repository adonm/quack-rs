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

## Current state (as of commit HEAD of `feat/c-api-full-coverage`)

| | Count | % |
|---|---|---|
| Total `duckdb_ext_api_v1` fields      | 428 | 100 |
| Wrapped (invoked at least once)      | 322 | **75%** |
| Intentionally unsupported (deprecated) | ~72 | 17% |
| Pending follow-up                    | ~34 | 8% |

**Wrapped**: 322 of 428 ext-API fields are reachable through quack-rs
safe-Rust wrappers. This includes:
- Scalar/aggregate/table/cast function builders and bind/init/data getters.
- Vector read/write, complex (LIST/STRUCT/MAP/ARRAY), validity, plus
  `OwnedVector` (create/slice/reference) and `SelectionVector`.
- Value typed constructors and accessors for every primitive and every
  timestamp variant; compound navigation (`list_child`, `struct_child`,
  `map_key`/`map_value`, `value_type`, `is_sql_null`, `display_string`).
- Logical type constructors and child inspection for LIST/MAP/
  STRUCT/ARRAY/UNION/ENUM/DECIMAL.
- Appender full lifecycle: typed `append_*`, `begin_row`/`end_row`,
  `append_default`/`append_null`/`append_value`, column subset
  management, metadata (`column_count`/`column_type`/`error_message`).
- `PreparedStatement` + typed `bind_*` for all parameter shapes,
  `execute`, `extract_and_prepare`, `Pending` async pipeline
  (`pending_prepared`, `execute_task`, `check_state`, \
  `execute_pending`, `pending_error`).
- `QueryResult` + `fetch_chunk`/`stream_fetch_chunk` + column metadata.
- `ClientContext`, `InstanceCache`, `TableDescription::column_has_default`,
  `Connection::get_table_names`/`get_client_context`.

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
| Profiling info | `duckdb_profiling_info_get_metrics`, `get_child_count`, `get_child`, `get_value`, `get_profiling_info` (`Connection`-scoped) | Useful for perf regression tests but no current sedona consumer; will wrap once a concrete need exists. |
| Task state | `duckdb_create_task_state`, `execute_tasks`, `execute_tasks_state`, `execute_n_tasks_state`, `finish_execution`, `task_state_is_finished`, `destroy_task_state`, `execution_is_finished` | DuckDB's parallel table-scan path uses `init_set_max_threads` + `local_init` (both already wrapped); task-state is for manual scheduling which sedona doesn't require yet. |
| Date/time/timestamp primitive conversions | `duckdb_from_date`, `to_date`, `is_finite_date`, `from_time`/`to_time`, `create_time_tz`, `from_time_tz`, `from_timestamp`/`to_timestamp`, `is_finite_timestamp`, `is_finite_timestamp_s/ms/ns`, `hugeint`<->`double`, `uhugeint`<->`double`, `decimal`<->`double` | Most extensions don't need to roundtrip these serialised struct layouts; `Value` already exposes them transparently via `as_*_raw` accessors. Wrap when a downstream caller needs them as primitives. |
| `string_t` introspection | `duckdb_string_is_inlined`, `duckdb_string_t_length`, `duckdb_string_t_data` | Already used internally by `vector::string`; outer API surface not yet exposed because quack-rs prefers `VectorReader::read_str` over raw `duckdb_string_t` arithmetic. |
| `duckdb_register_logical_type` | (single field) | The `duckdb_create_type_info` argument is opaque ("Reserved for future use" per `duckdb.h`) — no public API to construct one. Cannot be safely wrapped until upstream exposes its builder or marks it as a no-op pass-through. |

## Bumping coverage further

The path to 100% of *supported* `duckdb_ext_api_v1` is:
1. Wrap the deprecated family with a `pkg(deprecated)` feature flag (low-risk
   because they are existing C-API pathways, just slated for removal), OR
2. Upstream-document them as out-of-scope per above (preferred).
3. Wrap `register_logical_type` once `duckdb_create_type_info` has a real
   builder API upstream.
4. Wrap the profiling/task-state/primitive-conversion tracks once a sedona
   use case materialises.