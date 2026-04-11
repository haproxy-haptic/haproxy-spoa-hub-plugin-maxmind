# Implementation Plan: MaxMind GeoIP SPOA Plugin

**Branch**: `001-maxmind-geoip-plugin` | **Date**: 2026-04-09 | **Spec**: [spec.md](./spec.md)
**Input**: Feature specification from `/specs/001-maxmind-geoip-plugin/spec.md`

## Summary

Implement a haproxy-spoa-hub plugin that performs MaxMind GeoIP lookups
against MMDB databases. The plugin receives an IP address via SPOE
message, looks it up in one or more configured databases (City, Country,
ASN, Anonymous IP, ISP, Connection Type, Domain), and returns geolocation
variables as HAProxy transaction variables. Uses the `maxminddb` crate
for in-memory MMDB reading and follows the same architecture as the
fingerprinting plugin.

## Technical Context

**Language/Version**: Rust (edition 2024, MSRV 1.86)
**Primary Dependencies**: `haproxy-spoa-hub-plugin-api` 0.1, `abi_stable` 0.11, `maxminddb` 0.27, `serde` 1, `log` 0.4
**Storage**: MaxMind MMDB files loaded into memory at init (no runtime I/O)
**Testing**: `cargo test` with MaxMind test MMDB fixtures from `maxmind/MaxMind-DB`
**Target Platform**: Linux (amd64, arm64, armv7) — same as fingerprinting plugin
**Project Type**: Library (cdylib + rlib)
**Performance Goals**: <1 ms p99 per SPOE message lookup
**Constraints**: No per-request disk I/O, bounded memory (DB size + 10%)
**Scale/Scope**: Single plugin crate, ~500-800 lines of source code

## Constitution Check

*GATE: Must pass before Phase 0 research. Re-check after Phase 1 design.*

| Principle | Status | Evidence |
|-----------|--------|----------|
| I. Code Quality | PASS | Single-responsibility modules (config, lookup, plugin). Minimal deps (maxminddb, abi_stable, serde, log). Explicit error handling via Result types. |
| II. Testing Standards | PASS | Tests use MaxMind's official test MMDB fixtures (deterministic, no network). Test names describe scenario + outcome. |
| III. UX Consistency | PASS | Config follows hub TOML conventions (same as fingerprinting plugin). Fail-fast on missing/invalid DB path. Actionable error messages. |
| IV. Performance | PASS | In-memory reader via `Reader<Vec<u8>>`. No per-request disk I/O. OnceLock for config. Benchmark suite included. |
| Plugin Contract | PASS | Implements SpoePlugin via define_plugin! macro. Uses SPOP native types. Stable message name "geoip-lookup". |
| Dev Workflow | PASS | Conventional commits. CI pipeline (fmt, clippy, test, build). |

No violations. Complexity Tracking section not needed.

## Project Structure

### Documentation (this feature)

```text
specs/001-maxmind-geoip-plugin/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   └── spoe-message-contract.md
└── tasks.md
```

### Source Code (repository root)

```text
src/
├── lib.rs                # Module declarations
├── plugin.rs             # SpoePlugin trait implementation (define_plugin!)
├── config.rs             # PluginConfig parsing from hub TOML
├── lookup.rs             # Database loading, IP lookup, field extraction
└── db_type.rs            # DatabaseType enum and per-type field extraction

tests/
├── plugin_test.rs        # Config parsing, end-to-end message processing
├── lookup_test.rs        # Database loading, IP lookup against test fixtures
└── fixtures/             # MaxMind test MMDB files (git submodule or vendored)

benches/
└── lookup_bench.rs       # Criterion microbenchmarks

examples/
├── hub.toml              # Hub configuration example
└── spoe-maxmind.conf     # HAProxy SPOE configuration example
```

**Structure Decision**: Single-crate Rust library mirroring the
fingerprinting plugin layout. Source in `src/`, tests in `tests/`,
benchmarks in `benches/`, examples in `examples/`.
