# quack-rs Full C-API Coverage — PR Plan

**Goal:** close every gap in quack-rs's coverage of the `duckdb_ext_api_v1`
function-pointer struct (the surface DuckDB passes to loadable extensions
at init time), so downstream extensions like `duckdb_sedona` never hit a
"quack-rs doesn't wrap that" wall.

This is scoped as **one comprehensive PR** (one branch, one merge), structured
as a stack of small, independently-reviewable conventional commits — each
commit closes one logical domain and ships its own tests. The maintainer can
spot-review commit-by-commit, or squash on merge.

---

## Baseline (measured against `duckdb_ext_api_v1`, v1.2.0)

Source of truth: `duckdb/src/include/duckdb/main/capi/extension_api.hpp`
(`duckdb_ext_api_v1` struct, ~428 function-pointer fields).

| Metric | Count |
|---|---|
| Total `duckdb_ext_api_v1` function-pointer fields | 428 |
| Fields invoked at least once in quack-rs today | 157 (**37%**) |
| Fields never invoked anywhere in quack-rs | 271 (**63%**) |
| Safe pub-fn count, existing modules | ~250 across 17 modules |

Coverage by domain (estimated, call-site basis):

| Domain | Wrapped | Gap |
|---|---|---|
| scalar / aggregate / table / cast function builders | ~70% | lifecycle extras + bind/init-data getters |
| vector read/write + complex + validity | ~85% | `vector_reference`, `slice_vector`, `create_vector` |
| logical_type constructors + child inspection | ~70% | union/enum/array/map typed constructors |
| value (typed constructors/accessors) | ~40% | compound types (map/union/array/enum struct/list values), `display_string`, full `create_*` family |
| appender | ~30% | typed `append_*` (24), `begin/end_row`, `append_default_to_chunk`, `column_count/type`, `append_value` |
| prepared statements + pending result | 0% | full surface (**~54 functions**) |
| result handling (`fetch_chunk`, column metadata, error type) | 0% | full surface (~15) |
| arrow interop (query/scan/destroy) | 0% | full surface (~14, deprecated-but-still-supported path) |
| profiling info | 0% | full surface (5) |
| task state / parallel execution | 0% | full surface (6) |
| selection_vector + create_vector | ~30% | `create_selection_vector`, `get_data_ptr`, `slice_vector` |
| primitive conversions (date/time/timestamp/hugeint/decimal/time_tz) | ~30% | `create/get_timestamp_{s,ms,ns,tz}`, `from/to_time_tz`, `is_finite_timestamp_*` |
| string_t primitives | ~30% | `string_is_inlined`, `string_t_length`, `string_t_data` exposed |
| client_context / instance_cache / file_system / table_description | ~50% | `get_table_names`, `connection_get_client_context`, full instance_cache lifecycle |
| secrets / tls / warning / validate | n/a | not part of `duckdb_ext_api_v1` — already-quack-specific modules |
| **deprecated result API** (`duckdb_value_*`, `result_get_chunk`, arrow streaming) | 0% | **out of scope** — DuckDB marks these deprecated |

---

## Scope

### In scope

Every function-pointer field of `duckdb_ext_api_v1` that is not currently
invoked in quack-rs, with two exceptions:

1. **Deprecated result API** (~45 functions in a clearly-marked "deprecated"
   section of `extension_api.hpp`, e.g. `duckdb_value_int32`,
   `duckdb_result_get_chunk`, `duckdb_value_string_internal`,
   `duckdb_execute_prepared_streaming`). These are explicitly marked for
   removal in DuckDB and wrapping them adds long-tail maintenance burden for
   no extension value. We add a **single module-level doc note**
   (`src/result_deprecated.rs` never created; instead a paragraph in
   `lib.rs` "Unsupported" section) explaining why.

2. **Arrow interop** (`duckdb_query_arrow`, `duckdb_arrow_scan`,
   `duckdb_arrow_array_scan`, `destroy_arrow`, etc.) — kept in scope *only*
   if DuckDB has not marked these deprecated in v1.2.0. Action: verify in
   `duckdb.h`; if deprecated, treat as (1). Otherwise wrap under a new
   `arrow` module gated by an `arrow` feature so SDK consumers who don't need
   Arrow don't pull in `arrow-sys` semantics.

### Out of scope (explicitly)

- The standalone **DuckDB client C API** that is *not* in `duckdb_ext_api_v1`
  (e.g. `parquet_*`, `re2_*`, `arrow_c_*` non-extension functions). quack-rs
  is an extension SDK; client app functions are the `duckdb` crate's job.
- The C++ extension API in `duckdb/main/capi/extension_api.hpp` *is* just the
  `duckdb_ext_api_v1` struct (there is no separate C++ surface — it's a C
  vtable). So "C and C++ API" collapses to the single vtable.

---

## Phases (one conventional commit each)

Each phase lands as one commit, gated by tests. Plan commits in this order so
later phases build on earlier ones; each phase's commit message follows the
upstream Conventional Commits style
(`<type>(<scope>): <subject>`).

### Phase 0 — `chore(test): extend harness to exercise new wrappers`
Prepare the test infrastructure so subsequent phases ship coverage without
each needing scaffold churn.
- Extend `testing/mock_vector.rs` and `testing/in_memory_db.rs` to expose a
  hook for `PreparedStatement::new`, `Result::fetch_chunk`, `Value::create_*`
  smoke tests.
- Add `testing/mock_value.rs` for `Value` round-trip assertions.
- **Unblocks sedona?** Indirectly — every later phase depends on this.

### Phase 1 — `feat(value): complete typed accessors and constructors`
Close the `Value` gap (~117 ext-API functions touch value CRUD).
- `Value::create_*` for every logical type (currently partial — missing
  `create_timestamp_tz/s/ms/ns`, `create_bit`, `create_uuid`, `create_enum_value`,
  `create_union_value`, `create_map_value`, `create_array_value`,
  `create_struct_value`, `create_varint`, `create_decimal`-config helpers).
- `Value::as_*` for every `duckdb_get_*` counterpart: `as_timestamp_tz/s/ms/ns`,
  `as_time_tz`, `as_bit`, `as_uuid`, `as_enum_value`, `as_varint`, `as_decimal`.
- Compound-type navigation: `map_size`, `map_key`, `map_value`, `list_size`,
  `list_child`, `struct_child`, `enum_value`, `value_type`.
- `Value::display_string` (wraps `duckdb_value_to_string`).
- `is_null_value` / `create_null_value`.
- RAII discipline: every `duckdb_create_*` returns a `duckdb_value` owner;
  every `duckdb_get_*` returns borrowed data; the existing `Value` Drop
  already calls `duckdb_destroy_value`. Audit each new constructor for the
  ownership contract (created → owned by `Value`; get → borrow).
- Tests: `value` round-trip property test for every typed constructor ↔ accessor
  pair using proptest (already a dev-dep).
- **Unblocks sedona?** Yes — sedona needs `Value::create_blob` already (we
  added `as_blob` upstream in PR #104); future sedona features want
  `Value::as_list_child` for nested geometry collections returned from
  table functions, and `display_string` for error messages.

### Phase 2 — `feat(types): complete logical-type constructors and child inspection`
~14 ext-API functions in the `logical_type` family are unwrapped.
- `LogicalType::union(member_types, member_names, member_count)`.
- `LogicalType::enum_type(dictionary_values)`.
- `LogicalType::array(child, size)` — currently has `create_array_type`; verify.
- Child accessors: `union_type_member_{count,name,type}`, `enum_dictionary_{size,value}`,
  `array_type_{child_type,array_size}`, `map_type_{key_type,value_type}`,
  `struct_type_{child_count,child_name,child_type}`, `list_type_child_type`.
- `decimal_{width,scale,internal_type}`, `decimal_type(width, scale)`.
- `logical_type_get_alias` / `set_alias`.
- `register_logical_type` (with a `create_type_info` callback shim — currently
  a placeholder type).
- Tests: round-trip construction ↔ inspection for each composite type.
- **Unblocks sedona?** Yes — needed if we ever want sedona to register a
  custom `GEOMETRY` logical type alias via DuckDB's type registry, rather
  than the current `BLOB` carrier.

### Phase 3 — `feat(appender): full appender lifecycle`
~40 ext-API functions in the appender family are unwrapped.
- Typed `Appender::append_*` for every primitive (`int8/16/32/64`, `uint8/16/32/64`,
  `hugeint`, `uhugeint`, `float`, `double`, `date`, `time`, `timestamp`,
  `interval`, `varchar`, `varchar_length`, `blob`, `null`, `default`).
- `Appender::append_value(&Value)` (`duckdb_append_value`).
- `Appender::append_data_chunk(&DataChunk)` (already exists — verify).
- `Appender::append_default_to_chunk(chunk, col, row)` (1.5+).
- `Appender::begin_row` / `end_row`.
- `Appender::column_count` / `column_type` / `error`.
- `Appender::flush` (already exists — verify).
- `Appender::create_ext` (multi-catalog form).
- `Appender::add_column` / `clear_columns` (for column-subset appending).
- Tests: round-trip a row of each primitive type, then verify via a query.
- **Unblocks sedona?** Yes — sedona-side `COPY ... TO 'file.wkb'` rewrites
  want `append_blob` to write WKB into a stage table. Currently we'd have
  to fall back to raw SQL strings.

### Phase 4 — `feat(function): expose bind/init/data getters + scalar bind lifecycle`
~25 ext-API functions in the function-info / scalar-bind family.
- `function_info.get_extra_info`, `get_bind_data`, `get_init_data`,
  `get_local_init_data` — let the callback read opaque state written by
  bind/init. (Currently we set them, but never read them in callbacks.)
- `scalar_function_set_bind` (1.5+) + the `bind_info` lifecycle:
  `scalar_function_bind_set_error`, `scalar_function_get_client_context`,
  `scalar_function_set_bind_data`, `scalar_function_get_bind_data`.
- Aggregate: `aggregate_function_set_destructor`, `set_special_handling`.
- Table: `table_function_set_local_init` + `table_function_supports_projection_pushdown`.
- Tests: a scalar function that uses bind data to switch behaviour between
  invocations; a table function with local-init state threaded through to
  scan.
- **Unblocks sedona?** Yes — core to making `ST_Dump` and future `ST_DumpRings`
  / `ST_GeneratePoints` table functions thread-parallel via the local-init
  state (DuckDB parallel table-scan contract).

### Phase 5 — `feat(cast): per-row errors and cast-mode getter`
~6 ext-API functions.
- `cast_function_get_cast_mode` (returns `DUCKDB_CAST_NORMAL` / `_TRY` / `_FAIL`).
- `cast_function_set_row_error` (per-row error, vs the chunk-wide `set_error`).
- `cast_function_set_implicit_cast_cost`.
- Tests: a cast function that rejects certain rows with per-row errors;
  verify the rest of the chunk still succeeds.
- **Unblocks sedona?** Yes — needed for proper `GEOMETRY ↔ WKB` cast
  errors that don't abort the whole query.

### Phase 6 — `feat(prepared): prepared statements + parameter binding`
~54 ext-API functions — the single largest gap.
- New `PreparedStatement` RAII wrapper:
  `prepare(conn, sql)`, `destroy_prepare`, `prepare_error`, `nparams`,
  `parameter_name`, `param_type`, `param_logical_type`, `clear_bindings`,
  `prepared_statement_type`, `bind_parameter_index`.
- Typed `bind_*` setters (`bind_boolean`, `bind_int8/16/32/64`,
  `bind_uint8/16/32/64`, `bind_hugeint`, `bind_uhugeint`, `bind_float`,
  `bind_double`, `bind_decimal`, `bind_date`, `bind_time`,
  `bind_timestamp`, `bind_timestamp_tz`, `bind_interval`, `bind_varchar`,
  `bind_varchar_length`, `bind_blob`, `bind_null`, `bind_value`).
- Execution: `execute_prepared` → `Result`.
- Statement extraction: `extract_statements`, `prepare_extracted_statement`,
  `extract_statements_error`, `destroy_extracted`.
- Pending-result async pipeline: `pending_prepared`, `destroy_pending`,
  `pending_error`, `pending_execute_task`, `pending_execute_check_state`,
  `execute_pending`, `pending_execution_is_finished`.
- Tests: prepare, bind each typed parameter, execute, assert result.
- **Unblocks sedona?** Yes — sedona's planned `ST_EvalExpression` and
  constraint-checking helpers rebind geometry parameters; currently we
  emit SQL strings, which is a SQL-injection footgun.

### Phase 7 — `feat(result): result inspection + streaming fetch`
~15 ext-API functions.
- `Result::destroy`, `column_name`, `column_type`, `column_logical_type`,
  `column_count`, `rows_changed`, `result_error`, `result_error_type`,
  `result_return_type`, `result_statement_type`.
- `Result::fetch_chunk()` (pull-based chunk iteration).
- `Result::stream_fetch_chunk()` (deprecated alias — document and forward).
- `Result::query_progress` / `interrupt` (connection-level — move to `Connection`).
- Tests: run a query, fetch chunks, assert column metadata.
- **Unblocks sedona?** Yes — for sedona's regression test runner to read
  arbitrary SQL results without dropping to the `duckdb` crate.

### Phase 8 — `feat(arrow): Arrow C Stream / Array interop`
~14 ext-API functions — gated behind a new `arrow` feature flag.
- `query_arrow`, `query_arrow_schema`, `query_arrow_array`,
  `query_arrow_error`, `arrow_column_count`, `arrow_row_count`,
  `arrow_rows_changed`, `destroy_arrow`, `destroy_arrow_stream`,
  `prepared_arrow_schema`, `execute_prepared_arrow`,
  `arrow_scan`, `arrow_array_scan`.
- Implement against `arrow` crate's `FFI_ArrowArray`/`FFI_ArrowSchema` types
  (already-C-compatible). Add `arrow` as an optional dependency, gated.
- Tests: round-trip a DuckDB query → Arrow stream → back via `arrow_scan`.
- **Unblocks sedona?** Optional but high-value: lets sedona expose
  its geometry columns as Arrow for in-process pandas/polars ingest without
  a serialization round-trip.
- **Decision point:** verify in `duckdb.h` whether `duckdb_query_arrow` is
  still in the supported set. If the DuckDB team has marked these
  deprecated in favour of Arrow-CCA via `duckdb_create_table_function`,
  drop this phase and document.

### Phase 9 — `feat(profiling): query profiling info tree`
5 ext-API functions.
- `Connection::get_profiling_info()` → `ProfilingInfo` handle.
- `ProfilingInfo::get_value(key)`, `get_metrics()`, `child_count()`,
  `get_child(index)` — a recursive tree of operator metrics.
- Tests: profile a simple query, walk the tree, assert at least the root
  has children.
- **Unblocks sedona?** Indirect — useful for sedona performance regression
  tests ("is `ST_Union_agg`'s combine step actually amortized?") but not
  blocking.

### Phase 10 — `feat(task): task-state for parallel table scans`
6 ext-API functions.
- `Database::create_task_state`, `execute_tasks_state`,
  `execute_n_tasks_state`, `finish_execution`, `task_state_is_finished`,
  `destroy_task_state`, `Database::execute_tasks(max)`.
- Tests: spin up a parallel scan, drive it via task state, assert completion.
- **Unblocks sedona?** Yes — needed for thread-pool-driven parallelism in
  large table-scan output. DuckDB's parallel table function scan does not
  require task state (it uses `init_set_max_threads`), but long-running
  extension tasks (e.g. tile pyramid generation) want manual scheduling.

### Phase 11 — `feat(vector): create/slice/reference + selection vector`
8 ext-API functions.
- `Vector::create(logical_type, capacity)` (1.5+) — standalone vector not
  tied to a data chunk. Lets extensions build vectors for `append_value`
  and `arrow_array_scan` inputs.
- `Vector::destroy`, `slice(selection_vector, len)`, `reference_value(&Value)`,
  `reference_vector(&Vector)`.
- `SelectionVector::new(size)`, `destroy`, `data_ptr()` → `&mut [sel_t]`.
- Tests: create a vector, fill it, slice it, reference it into a chunk.
- **Unblocks sedona?** Yes — `reference_value` is how DuckDB wraps a
  single-value argument into a 1-row vector; needed for any scalar
  function that wants to forward its input to another scalar.

### Phase 12 — `feat(prim): date/time/timestamp/hugeint conversions`
~20 ext-API functions — the `from_*`, `to_*`, `is_finite_*`,
`create_time_tz`, `from_time_tz`, `double_to_*`, primitives.
- Centralise in `src/prim_time.rs` and `src/prim_numeric.rs` (or extend
  `interval.rs`'s existing primitive-conversion scope).
- Timestamp `s/ms/ns` conversions: `is_finite_timestamp_s/ms/ns`,
  `create_timestamp_s/ms/ns`, `get_timestamp_s/ms/ns`.
- `time_tz`: `create_time_tz`, `from_time_tz`.
- `hugeint`/`uhugeint`/`decimal`: `double_to_*`, `*_to_double`,
  `double_to_decimal`, `decimal_to_double`.
- `string_t`: `string_is_inlined`, `string_t_length`, `string_t_data` —
  expose as safe `StringT`-view helpers (currently only used internally
  in `vector::string`).
- `malloc` / `free` — already wrapped; verify.
- `vector_size()` — already wrapped; verify.
- Tests: round-trip every numeric conversion with proptest property checks.
- **Unblocks sedona?** Low priority directly; supports Phase 1's typed
  value accessors.

### Phase 13 — `feat(context): client_context + instance_cache + table_description parity`
~7 ext-API functions.
- `Connection::get_client_context()` → `ClientContext`.
- `ClientContext::connection_id()`, `destroy` (RAII).
- `Connection::get_table_names(query, qualified)` (returns a list value).
- `Catalog::type_name` (verify — already in `catalog.rs`).
- `InstanceCache::create`, `get_or_create_from_cache`, `destroy`.
- `TableDescription::has_default(index)`, `column_name(index)`.
- Tests: connect, get client context, round-trip an instance cache entry.
- **Unblocks sedona?** Yes — sedona's `ST_RegisterSpatialReference` (a
  planned DDL function) needs `ClientContext` to inject the SRS row into
  a session-scoped catalog.

### Phase 14 — `docs(deprecated): document intentionally-unsupported ext api`
- Add a single section to `lib.rs`'s crate-level `//!` doc and a new
  `COVERAGE.md` file listing every ext-API function intentionally *not*
  wrapped (the deprecated result-set family) with the DuckDB team's
  deprecation note as rationale.
- Cross-link to `duckdb.h` deprecation comment.
- **No code.** Closes the coverage audit: every ext-API function is either
  wrapped or listed.

---

## PR mechanics

- **Branch name:** `feat/c-api-full-coverage`
- **Base:** latest `tomtom215/quack-rs` `main` *after* PR #104 merges
  (so the dispatch-table blob fix is in the baseline). If the maintainer
  prefers to land this independently of #104, base on `v0.14.0` and note
  the small textual merge with #104's `string.rs` changes.
- **Commits:** one per phase, in the order above. Each commit is small
  (~50-300 LOC new + tests), independently buildable, conventional-commit
  titled. Rebase before push to keep history linear.
- **Feature gates:** Arrow (Phase 8) behind a new `arrow` feature. New
  C-API additions that exist only on `duckdb-1-5` stay under that gate.
  New `duckdb-1-5-3` items stay under that gate.
- **Tests:** each phase ships unit tests under `tests/` (proptest where
  property-shaped) plus, where feasible, a `bundled-test` integration test
  using `InMemoryDb`. CI matrix on the fork: `cargo clippy --all-targets --
  -D warnings`, `cargo test --all-targets`, `cargo test --all-features`.
- **PR body:** one paragraph per phase (motivation + the functions closed),
  linked to this plan doc. Note the singular goal: "every
  `duckdb_ext_api_v1` field either has a safe wrapper or appears in the
  COVERAGE.md unsupported list with rationale".
- **Versioning:** bump `Cargo.toml` to `0.15.0` on the PR branch (minor:
  pure API additions, plus two new feature gates). Update `CHANGELOG.md`.

---

## Verification matrix

| Check | Command | Phase-relevant |
|---|---|---|
| Clippy zero-warning | `cargo clippy --all-targets -- -D warnings` | every |
| Unit tests (default features) | `cargo test --lib` | every |
| Unit tests (all gated features) | `cargo test --all-features` | Phases 4, 6, 8, 13 |
| Bundled-DB integration | `cargo test --features bundled-test-prebuilt` | Phases 3, 6, 7, 8, 10 |
| `#![deny(unsafe_op_in_unsafe_fn)]` still passes | (lint built-in) | every |
| `#![warn(missing_docs)]` still passes | (lint built-in) | every |
| Coverage diff vs `duckdb_ext_api_v1` | re-run the measurement script in this doc | final |

The audit re-measurement (a small awk script over `extension_api.hpp` and
`rg` over `src/`) goes in `COVERAGE.md`'s final section so the
maintainer can reproduce "37% → 100%".

---

## Out-of-scope risks (call out in PR body)

- **Arrow deprecation:** verify Phase 8 before committing. If DuckDB has
  moved Arrow IO to Arrow-CCA via `duckdb_create_table_function`, drop
  Arrow-wrapping and document.
- **Future C-API additions:** this PR closes the v1.2.0 surface only. New
  fields added in DuckDB 1.5.x's `duckdb_ext_api_v1` (if any beyond the
  audit date) need a follow-up. The audit script in `COVERAGE.md` makes
  that a one-command check.
- **`create_type_info` opaque type:** `duckdb_register_logical_type` takes a
  `duckdb_create_type_info` argument whose construction API is not part of
  `duckdb_ext_api_v1` (it's a callback target the extension fills). Phase 2
  will need a thin shim — likely the callback-data pattern we use for
  `extra_info`. If this turns out to need an upstream DuckDB change, we
  split Phase 2's `register_logical_type` into a Phase 2b follow-up.
- **MSRV bump:** if Arrow-CCA interop pulls in `arrow` >=52 (MSRV 1.85+),
  the `arrow` feature raises crate MSRV from 1.87 → unchanged. Verify at
  Phase 8.

---

## Sequencing vs sedona

The user wants this PR landed *before* resuming `duckdb_sedona` "without any
API limitations". Concrete unblocks per phase, in priority order:

1. **Phase 1 (Value)** — sedona's `Value::as_blob` (PR #104) + this phase's
   `as_list_child` / `display_string` / compound-type accessors → unblocks
   nested geometry collection returns and richer error strings.
2. **Phase 4 (Function-info getters + scalar bind)** — unblocks
   thread-parallel `ST_Dump*` table functions and bind-time type inference
   for variadic geometry aggregates.
3. **Phase 3 (Appender)** — unblocks `COPY ... TO 'file.wkb'` from a
   stage table, not raw SQL.
4. **Phase 13 (Client context)** — unblocks SRS catalog injection.
5. **Phase 5 (Cast row errors)** — unblocks graceful `GEOMETRY ↔ WKB`
   cast failures.
6. **Phase 11 (Vector create/slice/reference)** — unblocks scalar-fn
   arg forwarding for `ST_Transform`'s planned CRS lookup callback.
7. Phases 6, 7, 8, 9, 10, 12, 2, 14 — broad parity, fewer near-term
   sedona touch points.

Recommended execution: ship all phases in one PR (maintainer preference for
one-branch merges), but write the commit messages so the
lowest-sedona-priority phases (6, 7, 9, 10, 12, 14) could be deferred into a
follow-up PR if review bandwidth is tight. The order above is the
all-in-one order; the "must-have-first" subset for sedona resumption is
Phases 1, 4, 5, 11, 13 + Phase 2 only if we plan a custom `GEOMETRY` type.

---

## Reproduce the audit

```sh
# In a DuckDB source tree with src/include/duckdb/main/capi/extension_api.hpp:
awk '/^typedef struct \{/,/^\} duckdb_ext_api_v1;/' \
    src/include/duckdb/main/capi/extension_api.hpp \
  | grep -oE '\(\*duckdb_[a-z_0-9]+\)' \
  | sed -E 's/\(\*//; s/\)//' | sort -u > /tmp/api_sorted.txt

# In quack-rs source tree (strip doc comments, then find call sites):
perl -0777 -pe 's{///.*}{}g; s{/\*.*?\*/}{}gs' $(find src -name '*.rs') \
  | rg -o '\bduckdb_[a-z_0-9]+\s*\(' -r '$0' \
  | sed -E 's/\s*\($//' | sort -u > /tmp/calls_sorted.txt

# Coverage:
comm -12 /tmp/api_sorted.txt /tmp/calls_sorted.txt | wc -l   # wrapped
comm -23 /tmp/api_sorted.txt /tmp/calls_sorted.txt | wc -l   # gap
```