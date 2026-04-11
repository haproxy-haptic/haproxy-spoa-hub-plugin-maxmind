# Research: MaxMind GeoIP SPOA Plugin

**Feature**: 001-maxmind-geoip-plugin
**Date**: 2026-04-09

## MMDB Reader Crate Selection

**Decision**: Use `maxminddb` crate v0.27.

**Rationale**: The only mature Rust crate for MMDB reading. Maintained
by Gregory Oschwald (author of MaxMind's official Go reader). Covers
all database types via built-in `geoip2::*` structs. Uses standard
serde for deserialization. Thread-safe (`Send + Sync`). Synchronous
API (no async runtime needed).

**Alternatives considered**:
- `geoip-api` — wraps C libmaxminddb, adds unsafe + C dependency
- `lazy-mmdb` — convenience wrapper around `maxminddb`, unnecessary layer
- Custom MMDB parser — unjustified complexity given mature crate exists

## Memory Model

**Decision**: Use `Reader::open_readfile()` which loads the entire MMDB
file into `Vec<u8>`.

**Rationale**: Avoids `unsafe` (mmap has UB risk if file is modified
while mapped). Self-contained — no file handle kept open. Thread-safe
without additional synchronization. Performance is equivalent for
read-only workloads since the OS page cache handles the I/O once.

**Alternatives considered**:
- `open_mmap` — marked unsafe, risk of UB during DB file updates
- `from_source` with custom allocation — unnecessary, `open_readfile`
  already returns `Reader<Vec<u8>>`

## Test Fixtures

**Decision**: Use MaxMind's official test MMDB files from
`github.com/maxmind/MaxMind-DB` repository (`test-data/` directory).

**Rationale**: Canonical test data maintained by MaxMind. Covers all
database types. Known test IPs documented (e.g., `89.160.20.128` for
City, `1.128.0.0` for ASN). Used by the `maxminddb` crate's own test
suite.

**Key test files**:
- `GeoIP2-City-Test.mmdb` — City lookups
- `GeoIP2-Country-Test.mmdb` — Country lookups
- `GeoLite2-ASN-Test.mmdb` — ASN lookups
- `GeoIP2-ISP-Test.mmdb` — ISP lookups
- `GeoIP2-Anonymous-IP-Test.mmdb` — Anonymous IP detection
- `GeoIP2-Connection-Type-Test.mmdb` — Connection type
- `GeoIP2-Domain-Test.mmdb` — Domain lookups

**Approach**: Add as git submodule at `tests/fixtures/MaxMind-DB` or
vendor the specific test files needed.

## Plugin Architecture

**Decision**: Follow the fingerprinting plugin pattern exactly.

**Key patterns**:
- `OnceLock<PluginConfig>` for thread-safe config initialization
- `define_plugin!` macro for FFI boilerplate
- Config parsed via `PluginConfig::from_context()`
- JSON Schema returned via `config_schema()`
- Session-scoped `TxnVariable` output

**Differences from fingerprinting**:
- Plugin holds loaded `Reader<Vec<u8>>` instances (stateful, not
  stateless like fingerprinting)
- Multiple readers possible (one per database type)
- Readers stored in `OnceLock<Vec<LoadedDatabase>>` alongside config
- Field extraction varies by database type (handled by `db_type.rs`)

## Database Type Handling

**Decision**: Use an enum `DatabaseType` with variants for each
supported type (City, Country, Asn, Anonymous, Isp, ConnType, Domain).
Each variant knows which fields to extract and which `geoip2::*` struct
to deserialize into.

**Rationale**: Type-safe, exhaustive match ensures all database types
are handled. Adding a new type requires adding one enum variant and
its field extraction logic — single point of change.

## Configuration Schema

**Decision**: Configuration via hub TOML `[plugins.params]` table:

```toml
[[plugins]]
name = "maxmind"
library = "libmaxmind_plugin.so"
messages = ["geoip-lookup"]

[plugins.params]
locale = "en"
# fields = ["country_iso_code", "city_name", "asn"]  # optional allowlist

[[plugins.params.databases]]
type = "city"
path = "/data/GeoLite2-City.mmdb"

[[plugins.params.databases]]
type = "asn"
path = "/data/GeoLite2-ASN.mmdb"
```

**Rationale**: Matches hub convention for plugin configuration. Array
of databases allows flexible multi-DB setups. `locale` and `fields`
are top-level since they apply globally.

## IP Argument Handling

**Decision**: Accept IP as SPOP IPv4, IPv6, or String type. Parse
strings via `std::net::IpAddr::from_str()`.

**Rationale**: HAProxy sends `src` as native IP type, but operators
may pass `req.hdr(X-Forwarded-For)` as string. Supporting both covers
all use cases without requiring operator workarounds.
