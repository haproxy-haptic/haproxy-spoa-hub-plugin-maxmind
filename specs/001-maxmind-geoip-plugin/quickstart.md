# Quickstart: MaxMind GeoIP SPOA Plugin

**Feature**: 001-maxmind-geoip-plugin
**Date**: 2026-04-09

## Prerequisites

- Rust 1.86+ with `cargo`
- A MaxMind MMDB database file (e.g., GeoLite2-City.mmdb)
- A running haproxy-spoa-hub instance

## Build

```bash
cargo build --release
```

The plugin library is produced at `target/release/libmaxmind_plugin.so`.

## Configure the Hub

Copy the plugin library to the hub's plugin directory and add to the
hub configuration:

```toml
[[plugins]]
name = "maxmind"
library = "libmaxmind_plugin.so"
messages = ["geoip-lookup"]

[plugins.params]
locale = "en"

[[plugins.params.databases]]
type = "city"
path = "/data/GeoLite2-City.mmdb"
```

## Configure HAProxy

Create an SPOE configuration file (`spoe-maxmind.conf`):

```
[maxmind]
spoe-agent maxmind-agent
    messages    geoip-lookup
    option var-prefix maxmind
    option continue-on-error
    timeout hello      2s
    timeout idle       5m
    timeout processing 10ms
    use-backend spoa-hub-backend

spoe-message geoip-lookup
    args ip=src
    event on-frontend-http-request
```

Reference it from the HAProxy frontend:

```
frontend www
    mode http
    bind *:80
    filter spoe engine maxmind config /etc/haproxy/spoe-maxmind.conf

    http-request set-header X-Country %[var(txn.maxmind.country_iso_code)]
    http-request set-header X-City    %[var(txn.maxmind.city_name)]

    default_backend webservers
```

## Run Tests

```bash
cargo test
```

Tests use MaxMind's official test MMDB fixtures — no real database
files or network access required.

## Verify

Send a request through HAProxy and check response headers:

```bash
curl -v http://localhost:80/
```

Look for `X-Country` and `X-City` headers in the response.
