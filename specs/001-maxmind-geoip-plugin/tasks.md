# Tasks: MaxMind GeoIP SPOA Plugin

**Input**: Design documents from `/specs/001-maxmind-geoip-plugin/`
**Prerequisites**: plan.md (required), spec.md (required for user stories), research.md, data-model.md, contracts/

**Tests**: Included — the constitution mandates tests for new features (Principle II).

**Organization**: Tasks are grouped by user story to enable independent implementation and testing of each story.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: Can run in parallel (different files, no dependencies)
- **[Story]**: Which user story this task belongs to (e.g., US1, US2, US3)
- Include exact file paths in descriptions

## Phase 1: Setup

**Purpose**: Project initialization and basic structure

- [x] T001 Create Cargo.toml with package metadata, crate-type = ["cdylib", "rlib"], dependencies (haproxy-spoa-hub-plugin-api 0.1, abi_stable 0.11, maxminddb 0.27, serde 1, log 0.4), dev-dependencies (criterion 0.5), and release profile (lto = true, codegen-units = 1)
- [x] T002 [P] Add MaxMind test MMDB fixtures as git submodule at tests/fixtures/MaxMind-DB from github.com/maxmind/MaxMind-DB
- [x] T003 [P] Create src/lib.rs with module declarations (pub mod config, pub mod lookup, pub mod db_type; mod plugin)
- [x] T004 [P] Create examples/hub.toml with sample hub configuration showing City + ASN database setup
- [x] T005 [P] Create examples/spoe-maxmind.conf with sample HAProxy SPOE configuration

---

## Phase 2: Foundational (Blocking Prerequisites)

**Purpose**: Core types and configuration that all user stories depend on

- [x] T006 Implement DatabaseType enum in src/db_type.rs with variants (City, Country, Asn, Anonymous, Isp, ConnType, Domain), from_str parsing, and config string constants
- [x] T007 Implement PluginConfig in src/config.rs with from_context() parsing databases array, locale, and optional fields allowlist from PluginContext, plus config_schema() returning JSON Schema
- [x] T008 Implement IP extraction helper in src/plugin.rs that converts SpoeValue (IPv4, IPv6, String) to std::net::IpAddr

**Checkpoint**: Foundation ready — user story implementation can now begin

---

## Phase 3: User Story 1 — GeoIP Lookup by Client IP (Priority: P1)

**Goal**: Accept an IP via SPOE message, look it up in loaded MMDB databases, return geolocation variables

**Independent Test**: Send a geoip-lookup message with known test IPs and verify returned variables match MaxMind test data

### Tests for User Story 1

> **NOTE: Write these tests FIRST, ensure they FAIL before implementation**

- [x] T009 [P] [US1] Test City database lookup with known IP (89.160.20.128) returns expected country, city, coordinates in tests/lookup_test.rs
- [x] T010 [P] [US1] Test ASN database lookup with known IP returns expected ASN number and organization in tests/lookup_test.rs
- [x] T011 [P] [US1] Test lookup with private IP (127.0.0.1) returns empty strings for all fields in tests/lookup_test.rs
- [x] T012 [P] [US1] Test lookup with IPv6 address returns correct data in tests/lookup_test.rs
- [x] T013 [P] [US1] Test lookup with multiple databases (City + ASN) returns variables from both in tests/lookup_test.rs
- [x] T014 [P] [US1] Test IP extraction from SpoeValue::Ipv4, SpoeValue::Ipv6, and SpoeValue::String in tests/plugin_test.rs
- [x] T015 [P] [US1] Test missing IP argument returns error in tests/plugin_test.rs

### Implementation for User Story 1

- [x] T016 [US1] Implement database loading in src/lookup.rs: load_databases() that takes a list of DatabaseEntry, opens each MMDB file via Reader::open_readfile(), returns Vec<LoadedDatabase>
- [x] T017 [US1] Implement per-type field extraction in src/db_type.rs: extract_fields() method on DatabaseType that deserializes the lookup result into the appropriate geoip2::* struct and returns Vec<(name, value)> pairs
- [x] T018 [US1] Implement lookup_ip() in src/lookup.rs that takes an IpAddr and &[LoadedDatabase], iterates databases, calls extract_fields per type, applies field allowlist, and returns RVec<TxnVariable>
- [x] T019 [US1] Implement SpoePlugin via define_plugin! macro in src/plugin.rs: new(), init() (load config + databases into OnceLock), name() → "maxmind", version(), process() (extract IP, call lookup_ip, return ProcessingResult), config_schema()
- [x] T020 [US1] Add end-to-end test in tests/plugin_test.rs: construct SpoeMessage with known IP, call process(), verify returned TxnVariable names and values

**Checkpoint**: User Story 1 should be fully functional — City and ASN lookups working with test fixtures

---

## Phase 4: User Story 2 — Flexible Database Configuration (Priority: P2)

**Goal**: Support all database types via configuration, validate paths at startup, clear errors on failure

**Independent Test**: Configure plugin with various database combinations and invalid paths, verify correct loading or descriptive errors

### Tests for User Story 2

- [x] T021 [P] [US2] Test config parsing with single database entry in tests/plugin_test.rs
- [x] T022 [P] [US2] Test config parsing with multiple database entries (City, ASN, Anonymous, ISP, ConnType, Domain) in tests/plugin_test.rs
- [x] T023 [P] [US2] Test init failure with nonexistent database path returns descriptive error in tests/plugin_test.rs
- [x] T024 [P] [US2] Test config_schema() returns valid JSON Schema in tests/plugin_test.rs

### Implementation for User Story 2

- [x] T025 [US2] Add Anonymous IP field extraction in src/db_type.rs: deserialize into geoip2::AnonymousIp, return is_anonymous, is_anonymous_vpn, is_hosting_provider, is_public_proxy, is_residential_proxy, is_tor_exit_node
- [x] T026 [P] [US2] Add ISP field extraction in src/db_type.rs: deserialize into geoip2::Isp, return isp, organization, autonomous_system_number, autonomous_system_organization
- [x] T027 [P] [US2] Add Connection Type field extraction in src/db_type.rs: deserialize into geoip2::ConnectionType, return connection_type
- [x] T028 [P] [US2] Add Domain field extraction in src/db_type.rs: deserialize into geoip2::Domain, return domain
- [x] T029 [US2] Add Country-only field extraction in src/db_type.rs: deserialize into geoip2::Country, return country and continent fields (subset of City)
- [x] T030 [US2] Add test for each additional database type with MaxMind test fixtures in tests/lookup_test.rs

**Checkpoint**: All 7 database types fully supported and tested

---

## Phase 5: User Story 3 — Configurable Output Fields and Locale (Priority: P3)

**Goal**: Support locale configuration for name fields and optional field allowlist to restrict output

**Independent Test**: Configure non-English locale and restricted field list, verify output matches expectations

### Tests for User Story 3

- [x] T031 [P] [US3] Test locale "de" returns German city and country names in tests/lookup_test.rs
- [x] T032 [P] [US3] Test locale fallback to "en" when requested locale has no translation in tests/lookup_test.rs
- [x] T033 [P] [US3] Test field allowlist filters output to only specified fields in tests/lookup_test.rs
- [x] T034 [P] [US3] Test default config (no locale, no fields) returns all fields in English in tests/lookup_test.rs

### Implementation for User Story 3

- [x] T035 [US3] Add locale parameter to extract_fields() in src/db_type.rs: pass locale when reading names maps, fall back to "en" if locale key not present
- [x] T036 [US3] Add field allowlist filtering in src/lookup.rs: after collecting all variables, filter by config.fields if Some, pass through if None

**Checkpoint**: All user stories complete — locale and field filtering working

---

## Phase 6: Polish & Cross-Cutting Concerns

**Purpose**: Benchmarks, documentation, and final quality checks

- [x] T037 [P] Create Criterion benchmark in benches/lookup_bench.rs: bench City lookup, ASN lookup, and combined City+ASN lookup per IP
- [x] T038 [P] Run cargo clippy with -D warnings and fix any issues
- [x] T039 [P] Run cargo fmt and verify formatting
- [x] T040 Verify all tests pass with cargo test
- [x] T041 Verify build produces libmaxmind_plugin.so with cargo build --release

---

## Dependencies & Execution Order

### Phase Dependencies

- **Setup (Phase 1)**: No dependencies — can start immediately
- **Foundational (Phase 2)**: Depends on T001 (Cargo.toml) and T003 (lib.rs)
- **User Story 1 (Phase 3)**: Depends on Phase 2 completion
- **User Story 2 (Phase 4)**: Depends on Phase 3 (extends db_type.rs and lookup.rs)
- **User Story 3 (Phase 5)**: Depends on Phase 3 (modifies extract_fields signature)
- **Polish (Phase 6)**: Depends on all user stories complete

### Within Each User Story

- Tests MUST be written and FAIL before implementation
- Types/enums before logic
- Lookup logic before plugin integration
- Unit tests before end-to-end tests

### Parallel Opportunities

- T002, T003, T004, T005 can all run in parallel (Phase 1)
- T009–T015 can all run in parallel (US1 tests)
- T021–T024 can all run in parallel (US2 tests)
- T025–T028 can run in parallel (US2 per-type extraction)
- T031–T034 can all run in parallel (US3 tests)
- T037–T039 can all run in parallel (Polish)

---

## Implementation Strategy

### MVP First (User Story 1 Only)

1. Complete Phase 1: Setup
2. Complete Phase 2: Foundational
3. Complete Phase 3: User Story 1 (City + ASN lookups)
4. **STOP and VALIDATE**: Test with real MMDB files if available
5. Plugin is deployable with City + ASN support

### Incremental Delivery

1. Setup + Foundational → Project compiles
2. User Story 1 → City/ASN lookups work (MVP)
3. User Story 2 → All 7 database types supported
4. User Story 3 → Locale + field filtering
5. Polish → Benchmarks, lint, docs

---

## Notes

- [P] tasks = different files, no dependencies
- [Story] label maps task to specific user story for traceability
- Each user story is independently completable and testable
- Verify tests fail before implementing
- Commit after each task or logical group
- Test fixtures from MaxMind-DB repo provide deterministic, reproducible test data
