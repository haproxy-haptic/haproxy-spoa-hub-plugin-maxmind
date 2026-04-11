# Implementation Plan: Flexible lookup inputs and outputs

**Branch**: `002-flexible-lookups` | **Date**: 2026-04-11 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/002-flexible-lookups/spec.md`

## Summary

Replace the 0.1.0 fixed-type configuration model (`type = "city"` with hard-coded field bundles) with an alias + named-lookup model that mirrors the HAProxy Enterprise `maxmind-load` / `maxmind-lookup` converter pair. Operators load each MMDB file under an arbitrary alias and define any number of named lookups, each binding `(message_name, db_alias, input_arg_name, field_path[], output_var_name)`. The plugin executes every lookup bound to an incoming SPOE message, traverses each lookup's field path through the decoded MMDB record, and emits each lookup's leaf value as a transaction variable plus a sibling `<output>_status` variable carrying `ok` / `not_found` / `error`. Database files remain in-memory (`Reader<Vec<u8>>`); the per-message latency budget (<1 ms p99) is preserved.

## Technical Context

**Language/Version**: Rust edition 2024, MSRV 1.86 (unchanged from 0.1.0)
**Primary Dependencies**: `haproxy-spoa-hub-plugin-api` 0.1, `abi_stable` 0.11, `maxminddb` 0.27, `serde` 1, `log` 0.4 (unchanged)
**Storage**: Local-filesystem MMDB files loaded once at init via `Reader::open_readfile`; no per-request I/O
**Testing**: `cargo test` against MaxMind's official test fixtures (already vendored as a git submodule); Criterion benchmarks for latency regression checks
**Target Platform**: Linux x86_64 / aarch64 / armv7 (existing CI matrix)
**Project Type**: Single Rust library (`cdylib` + `rlib`) loaded by haproxy-spoa-hub
**Performance Goals**: ≤1 ms p99 per SPOA message for a single lookup against a loaded database; combined runtime of N lookups against the same message ≈ N × single-lookup cost
**Constraints**: No allocations on the hot path beyond what `maxminddb` already performs internally; no per-request file I/O; field-path traversal must not allocate per node beyond bounded `String` rendering
**Scale/Scope**: Tens of databases and tens to low-hundreds of lookups per deployment; one plugin process per HAProxy instance

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Notes |
|---|---|---|
| I. Code Quality | PASS | The new model collapses ~300 lines of per-database extractor code into one generic field-path walker plus one decoder; net code shrinks. Linters and clippy must remain warning-free. |
| II. Testing Standards | PASS | Existing fixtures cover all seven schemas. New tests will exercise: alias loading, field-path traversal (nested map, leaf, missing, non-leaf), boolean/numeric/string rendering, multi-lookup-per-message routing, status-variable emission, malformed-input handling, deduplicated warnings. |
| III. UX Consistency | PASS (with documented breaking change) | Config keys (`databases`, `lookups`) follow hub conventions. Breaking change is documented in CHANGELOG and the migration section of the README. The plugin still fails fast on startup for unknown aliases, duplicate output variables, and missing required fields. |
| IV. Performance | PASS | In-memory readers retained. Field-path traversal walks an already-decoded `serde_json::Value` (or `maxminddb::Value`) — bounded by path depth (typically ≤4). Benchmarks must show ≤1 ms p99 single lookup; CI bench artifact gates the change. |
| Plugin Contract — stable message name | PASS WITH NOTE | The plugin no longer hard-codes a single message name; operator config declares them. This preserves the *spirit* of the rule (each declared message name is stable for the deployment that declares it; renaming it is a breaking change for that deployment) while enabling the flexibility the feature requires. See Complexity Tracking. |

All gates pass; proceeding to Phase 0.

## Project Structure

### Documentation (this feature)

```text
specs/002-flexible-lookups/
├── plan.md              # This file
├── research.md          # Phase 0 output
├── data-model.md        # Phase 1 output
├── quickstart.md        # Phase 1 output
├── contracts/
│   └── spoe-message-contract.md
├── checklists/
│   └── requirements.md  # Spec quality checklist (already present)
└── tasks.md             # Phase 2 output (/speckit.tasks)
```

### Source Code (repository root)

```text
src/
├── lib.rs               # module declarations (unchanged shape)
├── plugin.rs            # SpoePlugin glue: routes messages to lookups, emits TxnVariables
├── config.rs            # PluginConfig, DatabaseEntry (alias + path), Lookup (the new entity)
├── lookup.rs            # LoadedDatabase (Reader keyed by alias), execute(lookup, message)
├── ip.rs                # extract_ip / spoe_value_to_ip — unchanged
├── path.rs              # NEW: field-path traversal over decoded MMDB records, leaf rendering
└── warn_log.rs          # NEW: bounded/deduplicated warning helper (per-(lookup, path) once-per-window)

# DELETE: src/db_type.rs — replaced entirely by config.rs::Lookup + lookup.rs + path.rs

tests/
├── plugin_test.rs       # config parsing, IP extraction, schema (rewritten for new shape)
├── lookup_test.rs       # rewritten: alias loading, field-path traversal, status variable, multi-lookup
└── fixtures/MaxMind-DB/ # unchanged submodule

benches/
└── lookup_bench.rs      # extended: new model single-lookup, multi-lookup-per-message, deep-path
```

**Structure Decision**: Single-crate Rust library, identical layout to 0.1.0 with two new files (`path.rs`, `warn_log.rs`) and one deletion (`db_type.rs`). The new files map cleanly to the new responsibilities; no module reshuffling beyond that.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| Plugin Contract: message name now operator-declared rather than plugin-fixed | The whole feature is "let operators decide which messages map to which extractions". A plugin-fixed name would force every deployment to either accept the same fixed message (back to 0.1.0) or fork the source. | Keeping a single hard-coded message name and mapping internal logic from message arguments was rejected because SPOE arguments are static per message in HAProxy config — there is no way to vary the field path or db alias per request without varying the message name. |
