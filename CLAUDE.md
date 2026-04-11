# haproxy-spoa-hub-plugin-maxmind Development Guidelines

Auto-generated from all feature plans. Last updated: 2026-04-11

## Active Technologies
- Rust edition 2024, MSRV 1.86 (unchanged from 0.1.0) + `haproxy-spoa-hub-plugin-api` 0.1, `abi_stable` 0.11, `maxminddb` 0.27, `serde` 1, `log` 0.4 (unchanged) (002-flexible-lookups)
- Local-filesystem MMDB files loaded once at init via `Reader::open_readfile`; no per-request I/O (002-flexible-lookups)

- Rust (edition 2024, MSRV 1.86) + `haproxy-spoa-hub-plugin-api` 0.1, `abi_stable` 0.11, `maxminddb` 0.27, `serde` 1, `log` 0.4 (001-maxmind-geoip-plugin)

## Project Structure

```text
src/
tests/
```

## Commands

cargo test [ONLY COMMANDS FOR ACTIVE TECHNOLOGIES][ONLY COMMANDS FOR ACTIVE TECHNOLOGIES] cargo clippy

## Code Style

Rust (edition 2024, MSRV 1.86): Follow standard conventions

## Recent Changes
- 002-flexible-lookups: Added Rust edition 2024, MSRV 1.86 (unchanged from 0.1.0) + `haproxy-spoa-hub-plugin-api` 0.1, `abi_stable` 0.11, `maxminddb` 0.27, `serde` 1, `log` 0.4 (unchanged)

- 001-maxmind-geoip-plugin: Added Rust (edition 2024, MSRV 1.86) + `haproxy-spoa-hub-plugin-api` 0.1, `abi_stable` 0.11, `maxminddb` 0.27, `serde` 1, `log` 0.4

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
