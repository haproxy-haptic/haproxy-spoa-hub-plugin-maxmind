# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
