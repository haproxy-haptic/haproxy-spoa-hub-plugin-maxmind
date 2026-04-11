# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Breaking**: Replaced the fixed-type configuration model with an alias-and-lookup model that mirrors the HAProxy Enterprise `maxmind-load` / `maxmind-lookup` converter pair. Operators now load each MMDB file under an arbitrary alias and define named lookups, each binding one SPOE message to one field path inside one database. See the "Migration from 0.1.0" section of the README.
- **Breaking**: Removed the `locale` configuration key. Localized names are now addressable via the field path (e.g. `path = ["city", "names", "de"]`).
- **Breaking**: Removed the `fields` allowlist. Operators define only the lookups they want — the allowlist is implicit in the lookup definitions.
- **Breaking**: Removed the per-database `type` enum (`city`, `country`, `asn`, …). Any MMDB file can be loaded under any alias and addressed by field path.
- **Breaking**: The plugin no longer ships a fixed `geoip-lookup` message name. Each lookup declares its own `message` name; multiple lookups may share a message name and run in one round-trip.

### Added

- New `path` module: `AnyLeaf` decoder that renders any primitive MMDB leaf (string, integer, float, boolean) as a string via a single `decode_path::<AnyLeaf>` call.
- New `warn_log` module: bounded warn-once helper deduplicating warnings per `(lookup_name, path)` per 5-minute window, with no new dependencies.
- Sibling status variable `<output_var>_status` (one of `ok`, `not_found`, `error`) emitted alongside every primary variable so HAProxy ACLs can branch on the lookup outcome without parsing the value.
- Configurable per-lookup `input` argument name (default `ip`), letting operators feed any HAProxy sample expression into the lookup via the SPOE message.

### Removed

- `db_type` module and the per-type field bundles it implemented. Field selection is now operator-driven through the `lookups` array.

## [0.1.0]

### Added

- MaxMind GeoIP lookup plugin for haproxy-spoa-hub
- Support for all MaxMind database types: City, Country, ASN, Anonymous IP, ISP, Connection Type, Domain
- In-memory MMDB reader via `maxminddb` crate (no per-request disk I/O)
- Configurable locale for name fields (de, en, es, fr, ja, pt-BR, ru, zh-CN) with fallback to English
- Optional field allowlist to restrict returned variables
- Multi-database support: load and query multiple MMDB files in a single plugin instance
- Fail-fast initialization with descriptive errors for missing or invalid database paths
- IPv4, IPv6, and string-formatted IP address support
- Transaction-scoped output variables following haproxy-spoa-hub conventions
- JSON Schema for plugin configuration validation
- Criterion microbenchmarks (City: ~3.2µs, ASN: ~276ns, City+ASN: ~4.2µs per lookup)
- Test suite using MaxMind's official test MMDB fixtures
- Example SPOE and hub configurations
- Release builds use LTO and single codegen unit for optimal performance
