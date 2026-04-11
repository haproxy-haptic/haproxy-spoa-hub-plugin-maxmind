---

description: "Task list for feature 002-flexible-lookups"
---

# Tasks: Flexible lookup inputs and outputs

**Input**: Design documents from `/specs/002-flexible-lookups/`
**Prerequisites**: plan.md, spec.md, research.md, data-model.md, contracts/spoe-message-contract.md, quickstart.md

**Tests**: REQUIRED. The project constitution (II. Testing Standards) requires test coverage for primary success paths and at least one edge case per new feature, plus a regression test for any behavior change. Tests are written alongside implementation in this plan, not deferred to polish.

**Organization**: Tasks are grouped by user story. The feature is a rewrite of the lookup pipeline; most code lives in the Foundational phase because every user story exercises the same generic path. Story-specific work then layers on top.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Different file, no dependency on incomplete tasks — safe to run in parallel.
- **[Story]**: US1 / US2 / US3 maps to the user stories in `spec.md`.
- All paths are absolute.

---

## Phase 1: Setup

**Purpose**: Remove the 0.1.0 type-based extractor module so the rewrite has a clean slate, and confirm the dependency set is unchanged.

- [X] T001 Delete the obsolete typed-extractor module at `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/src/db_type.rs` (entire file). The new generic decoder makes it dead code.
- [X] T002 Update `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/src/lib.rs` to remove `pub mod db_type;` and add `pub mod path;` and `mod warn_log;`. Keep `pub mod config;`, `pub mod ip;`, `pub mod lookup;`, and `mod plugin;`.
- [X] T003 Verify `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/Cargo.toml` already declares `maxminddb = "0.27"`, `serde = "1"`, and `log = "0.4"` (no new dependencies are required by this feature). If anything is missing, add it.

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Build the shared building blocks that every user story needs: the leaf-rendering deserializer, the bounded warning helper, the new config types, and the runtime types that hold compiled lookups and the loaded databases. Without these, no user story can land.

**⚠️ CRITICAL**: No user-story phase may begin until Phase 2 is complete.

- [X] T004 [P] Create `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/src/path.rs` containing the `AnyLeaf` enum with variants `Str(String)`, `Int(i64)`, `UInt(u64)`, `Float(f64)`, `Bool(bool)`. Implement `serde::Deserialize` via a custom `Visitor` whose `visit_str`, `visit_borrowed_str`, `visit_string`, `visit_i64`, `visit_u64`, `visit_f64`, and `visit_bool` methods produce the matching variants. Add an `into_string(self) -> String` method that renders text verbatim, integers and floats via `Display`, and booleans as `"true"` / `"false"`. Add a public helper `compile_path(keys: &[String]) -> Vec<maxminddb::PathElement<'_>>` that maps each owned key string to a `PathElement::Key(&str)` slice (the slice borrows from the input vec — call sites pass `&Vec<String>` and rebuild per request). Add unit tests in the same file under `#[cfg(test)] mod tests` covering: string leaf, int leaf, uint leaf, float leaf, true and false bool, and verifying `compile_path` round-trips a 3-key path.
- [X] T005 [P] Create `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/src/warn_log.rs` containing `WarnOnce { window: Duration, seen: Mutex<HashMap<(String, String), Instant>> }` with `WarnOnce::new(window: Duration)` and `should_warn(&self, lookup: &str, path: &str) -> bool` per research D3. Include the opportunistic GC: when `seen.len() > 1024`, run `retain(|_, t| now.duration_since(*t) < self.window)` before inserting. Add unit tests covering: first call returns true, second call within window returns false, second call after window returns true (use a 50 ms window in the test and `std::thread::sleep`), GC triggers above the threshold.
- [X] T006 Rewrite `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/src/config.rs` to define the new config shape. Replace the old `DatabaseEntry { db_type, path }` and the `locale` / `fields` fields with: `PluginConfig { databases: BTreeMap<String, DatabaseEntry>, lookups: Vec<Lookup> }`, `DatabaseEntry { path: String }`, `Lookup { name: String, message: String, database: String, input: String, path: Vec<String>, output_var: String }`. Implement `Lookup` deserialization so that `input` defaults to `"ip"` when omitted. Implement `PluginConfig::from_context(&PluginContext) -> Result<Self, String>` which: (a) parses the `databases` table and the `lookups` array out of `PluginContext.config`; (b) returns clear errors for missing or empty `databases`, missing or empty `lookups`, missing required lookup fields, and any unknown 0.1.0 keys (`type`, `locale`, `fields`) with a hint pointing at the migration section of the README. Implement `PluginConfig::validate(&self) -> Result<(), String>` enforcing every rule from `data-model.md`: every `Lookup.database` resolves to a `databases` key, every `Lookup.name` is unique, every `Lookup.output_var` is unique, every `Lookup.path` is non-empty, every `Lookup.message` is non-empty. Call `validate()` from the end of `from_context`. Implement `PluginConfig::config_schema() -> &'static str` returning the JSON Schema sketch from `data-model.md`. Wire this so the existing `define_plugin!` glue can use it.
- [X] T007 Rewrite `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/src/lookup.rs` to define the runtime types and the per-message execution path. Add: `LoadedDatabases { readers: BTreeMap<String, maxminddb::Reader<Vec<u8>>> }` with `LoadedDatabases::open(&BTreeMap<String, DatabaseEntry>) -> Result<Self, String>` that opens each file via `Reader::open_readfile` exactly once; `CompiledLookup { name, database, input, path: Vec<String>, path_display: String, output_var, status_var }` built from a `Lookup` (the `path_display` field is the slash-joined path used as the warn-once key and in error messages; `status_var` is `format!("{}_status", lookup.output_var)` precomputed); `LookupRouter { by_message: BTreeMap<String, Vec<usize>>, compiled: Vec<CompiledLookup> }` built from `Vec<Lookup>` so that `by_message[msg]` indexes into `compiled`; an enum `LookupStatus { Ok, NotFound, Error }` with `as_str(&self) -> &'static str` returning `"ok"` / `"not_found"` / `"error"`. Add `execute(&self, lookup: &CompiledLookup, databases: &LoadedDatabases, ip: std::net::IpAddr, warn: &WarnOnce) -> (String, LookupStatus)` that: (a) looks up the database reader by alias (init guarantees presence); (b) calls `reader.lookup(ip)` — on `Err(AddressNotFound)` style miss returns `("".into(), NotFound)`; (c) builds the `Vec<PathElement>` from `lookup.path` via `path::compile_path`; (d) calls `result.decode_path::<crate::path::AnyLeaf>(&elements)` and maps `Ok(Some(leaf)) → (leaf.into_string(), Ok)`, `Ok(None) → warn-once + ("".into(), NotFound)`, `Err(_) → warn-once + ("".into(), Error)`. Use `WarnOnce::should_warn(&lookup.name, &lookup.path_display)` to gate the warning emission and `log::warn!` for the message itself.

**Checkpoint**: With Phase 2 complete, the foundation compiles, has unit tests for `AnyLeaf` and `WarnOnce`, and can in principle run a single hard-coded lookup. No SPOE message routing yet — that lands in US1.

---

## Phase 3: User Story 1 - Operator picks exactly which database fields to extract (Priority: P1) 🎯 MVP

**Goal**: Operators can configure named lookups against any field path of any loaded database, get exactly the variables they asked for as transaction-scope SPOE variables, and receive a sibling status variable per lookup.

**Independent Test**: Configure two lookups against the bundled MaxMind test fixtures (`country.iso_code` in a City database, `autonomous_system_number` in an ASN database), send a message with a known IP, and assert that exactly the two requested transaction variables plus their two `_status` siblings are populated and carry the expected values. No other variables are emitted.

### Tests for User Story 1

> Write these tests before the corresponding implementation tasks and watch them fail.

- [X] T008 [P] [US1] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/plugin_test.rs`, replace the 0.1.0 config tests with new ones for the alias-and-lookup shape: `config_with_databases_map_and_lookups_parses_correctly` (one db, one lookup, asserts `databases.len() == 1`, `lookups.len() == 1`, `lookups[0].input == "ip"`); `config_lookup_input_defaults_to_ip` (omits `input`, expects `"ip"`); `config_missing_databases_returns_error` (asserts the error message contains the word `databases`); `config_missing_lookups_returns_error`; `config_unknown_database_alias_returns_error` (asserts error names the offending lookup); `config_duplicate_output_var_returns_error`; `config_duplicate_lookup_name_returns_error`; `config_legacy_type_key_returns_migration_error` (passes a 0.1.0-style `type = "city"` and asserts the error mentions migration); `config_schema_is_valid_json` (parses `PluginConfig::config_schema()` with `serde_json` and asserts `properties.databases` and `properties.lookups` exist).
- [X] T009 [P] [US1] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/lookup_test.rs`, replace the 0.1.0 typed-extractor tests with: `extract_country_iso_code_from_city_db_returns_value_and_ok_status` (uses the bundled `GeoIP2-City-Test.mmdb` fixture, looks up a known IP, asserts the primary variable equals the expected ISO code and the status variable equals `"ok"`); `extract_missing_field_returns_empty_and_not_found_status` (configures a path like `["nonexistent"]`, asserts empty value and status `"not_found"`); `extract_non_leaf_field_returns_empty_and_error_status` (configures `["country"]` which is a map, asserts empty value and status `"error"`); `extract_localized_name_via_path_returns_value` (path `["city", "names", "de"]` against a fixture city, asserts the German name); `extract_boolean_field_renders_true_or_false` (path `["country", "is_in_european_union"]` against an EU IP, asserts the value is the literal string `"true"`); `extract_numeric_field_renders_canonical_decimal` (path `["location", "latitude"]`, asserts the value is a parseable float string); `lookup_for_ip_not_in_database_returns_empty_and_not_found_status` (private IP); `lookup_for_invalid_ip_string_returns_empty_and_error_status` (string `"not-an-ip"` as input).
- [X] T010 [P] [US1] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/lookup_test.rs`, add `multi_lookup_against_same_database_loads_file_once`: configure two lookups whose `database` alias is the same and whose paths differ; build a `LoadedDatabases`; assert that `loaded.readers.len() == 1` (the loader deduplicates by alias, not by lookup count).

### Implementation for User Story 1

- [X] T011 [US1] Rewrite `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/src/plugin.rs` to use the new state shape and routing. Replace `PluginState { config: PluginConfig, databases: Vec<LoadedDatabase> }` with `PluginState { databases: LoadedDatabases, router: LookupRouter, warn_once: WarnOnce }`. In `init`: parse and validate config via `PluginConfig::from_context`; build `LoadedDatabases::open(&config.databases)`; build `LookupRouter::from_lookups(config.lookups)`; build `WarnOnce::new(Duration::from_secs(300))`; store all three in the `OnceLock`. Log `maxmind plugin loaded {n} database(s) and {m} lookup(s)`. In `process`: resolve `state.router.by_message[message.name]` to a slice of `&CompiledLookup`s; if absent, return `Err("no lookup bound to message '<name>'")`; for each compiled lookup, read `message.args[lookup.input]` and convert via `crate::ip::spoe_value_to_ip` (mapping any error to the `Error` status with empty value rather than aborting the whole message — but if the *argument* is missing entirely, return an error to the hub per FR-008 / contracts doc); call `lookup::execute(...)`; emit two `TxnVariable`s per lookup (primary `output_var` and sibling `status_var`) into the `RVec`. Return `Ok(ProcessingResult { variables })`. Keep `name`, `version`, and `config_schema` returning the new schema.
- [X] T012 [US1] Update `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/src/ip.rs` if needed so that `spoe_value_to_ip` returns a typed error that the plugin can distinguish from "argument missing" (the existing code already errors on invalid input — make sure the error type or message lets `plugin.rs` route invalid IPs to status `Error` rather than a hard plugin error, while keeping the truly missing-argument case as a hard error). Add or extend a unit test in `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/plugin_test.rs` to assert the distinction.
- [X] T013 [US1] Run `cargo test --workspace` from `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind`. All US1 tests (T008–T010) plus the foundational unit tests (T004, T005) must pass before moving on. Fix any remaining compile errors and assertion failures by editing the implementation files only — do not weaken the tests.

**Checkpoint**: At this point, US1 is fully functional. An operator can configure any field path of any loaded database, send an SPOE message, and receive the leaf value plus a status variable. The MVP is shippable.

---

## Phase 4: User Story 2 - Operator chooses which client value to look up (Priority: P2)

**Goal**: Operators can name the SPOE argument that carries the lookup value (defaulting to `ip`), letting HAProxy feed the lookup from any sample expression — `src`, `req.hdr(X-Forwarded-For)`, a stick-table key, etc. — without plugin code changes.

**Independent Test**: Configure two lookups bound to two different message names, one reading the default `ip` argument and one reading a custom `client_ip` argument. Construct two `SpoeMessage`s — one with `args["ip"]`, one with `args["client_ip"]` — with different IP values. Assert each lookup geolocates its own value, not the other's.

### Tests for User Story 2

- [X] T014 [P] [US2] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/plugin_test.rs`, add `lookup_reads_custom_input_arg_name`: build a `PluginConfig` with one lookup whose `input = "client_ip"`; construct an `SpoeMessage` carrying `client_ip = SpoeValue::String(...)`; assert the plugin uses that value and not any `ip` argument.
- [X] T015 [P] [US2] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/plugin_test.rs`, add `lookup_with_missing_input_arg_returns_hub_error`: lookup declares `input = "client_ip"`; message arrives with no `client_ip` argument; assert the plugin returns an error whose message mentions both the lookup name and the missing argument name.
- [X] T016 [P] [US2] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/lookup_test.rs`, add `lookup_with_ipv6_input_value_returns_record`: feed an IPv6 address through `SpoeValue::Ipv6` into a lookup against the bundled City fixture and assert the corresponding IPv6 record fields come back. Add `lookup_with_string_ipv4_input_value_returns_record`: feed `SpoeValue::String("89.160.20.128")` and assert it geolocates the same as the binary form.

### Implementation for User Story 2

- [X] T017 [US2] Confirm that the `plugin.rs::process` implementation from T011 already reads `message.args[lookup.input]` rather than a hard-coded `"ip"` key. If T011 hard-coded `"ip"`, fix it now: read the per-lookup `input` field from the `CompiledLookup`. The change is mechanical because `input` is already part of `CompiledLookup` (from T007).
- [X] T018 [US2] Run `cargo test --workspace` from `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind`. T014–T016 must pass. If any older tests assumed the hard-coded `ip` arg name, update the tests' message construction (not the production code) to use the lookup's declared `input` name.

**Checkpoint**: US2 complete. HAProxy operators can now pick any sample expression as the lookup input by configuring `input` per lookup and matching `args <name>=<expr>` in the SPOE config.

---

## Phase 5: User Story 3 - Operator composes several independent lookups (Priority: P3)

**Goal**: Many lookups can be bound to the same SPOE message name, executing in one round-trip and emitting all their output variables together. Removing one lookup from configuration removes only its variables; adding a new lookup against an already-loaded database carries no extra load cost.

**Independent Test**: Configure three lookups against three different database aliases, all bound to the same message name. Send one message and assert all six variables (three primary + three status) are populated. Then re-build the config without one lookup and assert only four variables are produced.

### Tests for User Story 3

- [X] T019 [P] [US3] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/plugin_test.rs`, add `multi_lookup_per_message_executes_all_and_returns_union`: configure three lookups all bound to message name `geoip-enrich` against the City, ASN, and Anonymous IP fixtures; send one `SpoeMessage` named `geoip-enrich`; assert exactly six `TxnVariable`s come back (three primary + three status) and that each primary holds a non-empty value for a known IP.
- [X] T020 [P] [US3] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/plugin_test.rs`, add `removing_a_lookup_removes_only_its_variables`: build a config with three same-message lookups, send a message and capture the variable count; rebuild without the second lookup, send the same message, assert exactly four variables are produced and that the second lookup's variable name is absent.
- [X] T021 [P] [US3] In `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/tests/plugin_test.rs`, add `unknown_message_name_returns_error`: build a config with one lookup bound to `geoip-enrich`; send a message named `something-else`; assert the plugin returns an error mentioning the unknown message name.

### Implementation for User Story 3

- [X] T022 [US3] Confirm the `LookupRouter::from_lookups` implementation from T007 groups indices by message name (a `BTreeMap<String, Vec<usize>>`) rather than enforcing one-to-one. If T007 enforced one-to-one, change the validation in T006 / construction in T007 to allow multiple lookups per message name (clarification Q2). The routing loop in T011 already iterates the slice — no change there.
- [X] T023 [US3] Run `cargo test --workspace` from `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind`. T019–T021 must pass.

**Checkpoint**: All three user stories independently functional.

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Documentation, examples, performance validation, and the final cleanup pass mandated by the personal CLAUDE.md (DRY/SOLID/dead code/naming check).

- [X] T024 [P] Update `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/examples/hub.toml` to use the new alias-and-lookup shape from `quickstart.md`. Replace the old `[[plugins.params.databases]] type=...` blocks with the `[plugins.params.databases]` table form and add representative `[[plugins.params.lookups]]` entries for country code, ASN, and the German city name.
- [X] T025 [P] Update `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/examples/spoe-maxmind.conf` so the SPOE message name matches the new example (e.g. `geoip-enrich`) and the args list matches the lookups' `input` names.
- [X] T026 Update `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/README.md` to document the new model: replace the "Configuration reference" section with the alias-and-lookup shape, replace the per-database "Output variables" tables with a "How field paths work" section that points operators at MaxMind's MMDB schemas, add a "Migration from 0.1.0" section mirroring the table at the bottom of `contracts/spoe-message-contract.md`, and update the HAProxy usage example to show the `_status` variable. Match existing tone and structure per the personal CLAUDE.md docs guidance.
- [X] T027 Add an entry to `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/CHANGELOG.md` under a new `## [0.2.0] - Unreleased` section listing: breaking config schema change (with migration pointer), new field-path traversal model, new `<output>_status` sibling variables, removed `locale` and `fields` keys, removed fixed database `type` enum.
- [X] T028 Extend `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/benches/lookup_bench.rs` with two new criterion benchmarks: `single_lookup_country_iso_code` (one configured lookup against the City fixture, measure the per-message processing time) and `three_lookups_one_message` (three lookups bound to the same message name, measure that the cost is roughly 3× the single-lookup cost). Run `cargo bench` from the repo root and confirm the single-lookup p99 stays under 1 ms; record the numbers in the PR description, not in the repo.
- [X] T029 Run `cargo clippy --workspace -- -D warnings` and `cargo fmt -- --check` from `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind`. Fix every warning and any formatting drift in the production code (not by adding `#[allow]`).
- [X] T030 Post-task cleanup pass per the personal CLAUDE.md: scan `src/` for any leftover references to the deleted `db_type` module, locale handling, or the old `fields` allowlist; remove anything dead. Verify module names (`config`, `lookup`, `path`, `warn_log`, `ip`, `plugin`) still describe what their contents do after the rewrite. Confirm there is no duplicated logic between `lookup::execute` and `plugin::process` that should be consolidated.

---

## Dependencies & Execution Order

### Phase Dependencies

- **Phase 1 (Setup)**: No dependencies.
- **Phase 2 (Foundational)**: Depends on Phase 1. BLOCKS all user stories. T004 and T005 are parallel-safe; T006 must follow T002 (lib.rs declares the new module list); T007 depends on T004 and T006.
- **Phase 3 (US1, MVP)**: Depends on Phase 2. The MVP-shippable cut.
- **Phase 4 (US2)**: Depends on Phase 3 because it modifies the same `plugin.rs::process` function the US1 implementation creates. The change is small but sequential.
- **Phase 5 (US3)**: Depends on Phase 3. May proceed in parallel with Phase 4 if T011 already implemented the router as many-to-one (it should — T007 specifies it).
- **Phase 6 (Polish)**: Depends on US1 at minimum; the doc/example tasks should reflect whichever stories are merged.

### User Story Dependencies

- **US1 (P1)**: Independent of US2/US3. Delivers the MVP. Configures the entire field-path traversal pipeline.
- **US2 (P2)**: Depends on US1's plugin glue but adds zero new modules — only widens the read path.
- **US3 (P3)**: Depends on US1's router but adds zero new modules — only relaxes the routing constraint and validates fan-out.

### Within Each User Story

- Tests are written first; production code follows; `cargo test` is the gate.
- Within Phase 2, the foundational pieces are mostly parallel-safe (different files); only `config.rs` and `lookup.rs` have an ordering constraint.

---

## Parallel Example: Phase 2 (Foundational)

```bash
# All three foundational helpers can be built in parallel — different files, no shared types yet:
Task: "T004  Create src/path.rs with AnyLeaf + compile_path + tests"
Task: "T005  Create src/warn_log.rs with WarnOnce + tests"
# T006 (config rewrite) must come before T007 (lookup runtime types) because LoadedDatabases::open
# borrows DatabaseEntry from config.rs — sequential there.
```

## Parallel Example: User Story 1 tests

```bash
# All three test files can be drafted in parallel by different developers — they live in two test files
# but each adds independent test functions:
Task: "T008  Add config tests in tests/plugin_test.rs"
Task: "T009  Add field-path traversal tests in tests/lookup_test.rs"
Task: "T010  Add database-load-once test in tests/lookup_test.rs"
```

---

## Implementation Strategy

### MVP First (US1)

1. Phase 1: Setup (T001–T003).
2. Phase 2: Foundational (T004–T007). The longest phase; everything else is layered on top.
3. Phase 3: US1 (T008–T013). Tests written first per the constitution; T013 is the green-light gate.
4. **STOP and validate** — the MVP is operator-defined field extraction with status variables.
5. Decide whether to ship US1 as 0.2.0-rc1 or fold US2/US3 into a single 0.2.0 release.

### Incremental Delivery

Each user story checkpoint is shippable in isolation:

- US1 only: operators get full field-path flexibility but must keep `args ip=src` in HAProxy.
- US1 + US2: operators can also pick the input expression. This is the "feature parity with HAProxy Enterprise" line.
- US1 + US2 + US3: operators get the round-trip-saving ability to bind several lookups to one message.

### Single-Developer Strategy

Because this feature is a coherent rewrite of one ~500-line module, a single developer is the expected workload. Run the phases sequentially and use the `cargo test` gates at T013 / T018 / T023 as the natural checkpoints to commit and push.

---

## Notes

- Every task references absolute paths under `/home/phil/Quellcode/Spielwiese/haproxy-spoa-hub-plugin-maxmind/`.
- [P] markers tag tasks that touch a different file from every other task currently in flight.
- Test tasks come before their implementation tasks within each user story per Constitution principle II (TDD).
- The performance gate (≤1 ms p99 single lookup) is enforced by T028 (benchmark) and constitutes a hard merge blocker per Constitution principle IV.
- Do not weaken or skip a test to make it pass — fix the production code instead.
