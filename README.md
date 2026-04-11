# HAProxy SPOA Hub — MaxMind Plugin

A [haproxy-spoa-hub](https://gitlab.com/haproxy-haptic/haproxy-spoa-hub) plugin that performs MaxMind GeoIP lookups. Supports all MaxMind database types: City, Country, ASN, Anonymous IP, ISP, Connection Type, and Domain.

## Overview

The plugin receives an IP address from HAProxy via the SPOE protocol, looks it up in one or more configured MaxMind MMDB databases, and returns geolocation variables as transaction-scoped HAProxy variables. This brings [HAProxy Enterprise's MaxMind module](https://www.haproxy.com/documentation/haproxy-enterprise/enterprise-modules/geolocation/maxmind/) functionality to HAProxy Community Edition.

Databases are loaded into memory at startup — there is no per-request disk I/O. Lookups complete in single-digit microseconds.

## Quick Start

### Prerequisites

- [haproxy-spoa-hub](https://gitlab.com/haproxy-haptic/haproxy-spoa-hub) built and running
- One or more MaxMind MMDB database files (e.g., [GeoLite2-City](https://dev.maxmind.com/geoip/geolite2-free-geolocation-data))
- Rust toolchain (>= 1.86) for building the plugin

### Build

```bash
cargo build --release
```

The output is `target/release/libmaxmind_plugin.so`. Copy it to the hub's plugin directory.

### Configure the Hub

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

[[plugins.params.databases]]
type = "asn"
path = "/data/GeoLite2-ASN.mmdb"
```

### Configure HAProxy

#### SPOE agent definition (`spoe-maxmind.conf`)

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

#### HAProxy frontend (`haproxy.cfg`)

```haproxy
frontend www
    bind *:80
    filter spoe engine maxmind config /etc/haproxy/spoe-maxmind.conf

    http-request set-header X-Country %[var(txn.maxmind.country_iso_code)]
    http-request set-header X-City    %[var(txn.maxmind.city_name)]
    http-request set-header X-ASN     %[var(txn.maxmind.autonomous_system_number)]

    # Geo-based routing
    use_backend eu-servers if { var(txn.maxmind.continent_code) -m str EU }

    default_backend app

backend spoa-hub-backend
    mode spop
    server hub /var/run/spoe/hub.sock
```

### Verify

```bash
curl -v http://localhost:80/ 2>&1 | grep -E 'X-(Country|City|ASN)'
```

## Configuration Reference

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `databases` | array of tables | *(required)* | List of MMDB databases to load. Each entry has `type` and `path`. |
| `locale` | string | `"en"` | Locale for name fields (city, country, continent, subdivision). |
| `fields` | array of strings | *(all)* | Optional allowlist to restrict which variables are returned. |

### Database Types

| Type | Databases | Description |
|------|-----------|-------------|
| `city` | GeoIP2-City, GeoLite2-City | Country, city, coordinates, postal code, subdivision, timezone |
| `country` | GeoIP2-Country, GeoLite2-Country | Country and continent only |
| `asn` | GeoLite2-ASN | Autonomous system number and organization |
| `anonymous` | GeoIP2-Anonymous-IP | Proxy, VPN, Tor, and hosting provider detection |
| `isp` | GeoIP2-ISP | ISP name, organization, and ASN |
| `conntype` | GeoIP2-Connection-Type | Cable/DSL, Cellular, Corporate, Satellite |
| `domain` | GeoIP2-Domain | Second-level domain |

### Supported Locales

`de`, `en`, `es`, `fr`, `ja`, `pt-BR`, `ru`, `zh-CN`. Falls back to `en` when the requested locale is not available for a given record.

### Output Variables

All variables are transaction-scoped and namespaced under the plugin name by the hub.

#### City / Country

| Variable | Description |
|----------|-------------|
| `country_iso_code` | ISO 3166-1 two-letter code (e.g., `US`) |
| `country_name` | Localized country name |
| `continent_code` | Two-letter code (`AF`, `AN`, `AS`, `EU`, `NA`, `OC`, `SA`) |
| `continent_name` | Localized continent name |
| `is_in_european_union` | `true` or `false` |

#### City (additional)

| Variable | Description |
|----------|-------------|
| `city_name` | Localized city name |
| `latitude` | WGS84 latitude |
| `longitude` | WGS84 longitude |
| `postal_code` | Postal/ZIP code |
| `timezone` | IANA time zone (e.g., `America/New_York`) |
| `accuracy_radius` | Accuracy radius in kilometers |
| `subdivision_iso_code` | ISO 3166-2 code of the first subdivision |
| `subdivision_name` | Localized subdivision name |

#### ASN

| Variable | Description |
|----------|-------------|
| `autonomous_system_number` | ASN number |
| `autonomous_system_organization` | Organization name |

#### Anonymous IP

| Variable | Description |
|----------|-------------|
| `is_anonymous` | Any sort of anonymous network |
| `is_anonymous_vpn` | Registered anonymous VPN provider |
| `is_hosting_provider` | Hosting or VPN provider |
| `is_public_proxy` | Public proxy |
| `is_residential_proxy` | Residential proxy |
| `is_tor_exit_node` | Tor exit node |

#### ISP

| Variable | Description |
|----------|-------------|
| `isp` | ISP name |
| `organization` | Organization name |
| `autonomous_system_number` | ASN number |
| `autonomous_system_organization` | Organization name |

#### Connection Type

| Variable | Description |
|----------|-------------|
| `connection_type` | `Cable/DSL`, `Cellular`, `Corporate`, `Satellite` |

#### Domain

| Variable | Description |
|----------|-------------|
| `domain` | Second-level domain (e.g., `example.com`) |

### Restricting Output

Use the `fields` parameter to return only the variables you need:

```toml
[plugins.params]
fields = ["country_iso_code", "city_name", "autonomous_system_number"]
```

When `fields` is omitted, all variables from all configured databases are returned.

### Error Behavior

- **Missing `ip` argument**: Returns an error (logged by the hub).
- **IP not found in database**: Returns empty strings for all fields of that database type.
- **Missing or invalid database path**: Plugin fails to initialize with a descriptive error message.
- **Invalid IP string**: Returns an error (logged by the hub).

## Performance

Plugin computation (Criterion benchmarks, test MMDB fixtures):

| Lookup | Time |
|--------|------|
| City (13 fields) | 3.2 µs |
| ASN (2 fields) | 276 ns |
| City + ASN combined | 4.2 µs |

## Development

### Commands

```bash
cargo build --release            # Build plugin .so
cargo test                       # Unit + integration tests (26 tests)
cargo bench                      # Criterion microbenchmarks
cargo clippy -- -D warnings      # Lint
```

### Project Structure

```
src/
  lib.rs              Module declarations + test helpers
  plugin.rs           SpoePlugin implementation (define_plugin! macro)
  config.rs           Plugin configuration (databases, locale, fields)
  db_type.rs          DatabaseType enum and per-type field extraction
  lookup.rs           Database loading and IP lookup orchestration

tests/
  lookup_test.rs      Lookup tests for all 7 database types, locale, filtering
  plugin_test.rs      Configuration parsing, IP extraction, schema validation
  fixtures/           MaxMind test MMDB files (git submodule)

benches/
  lookup_bench.rs     Criterion microbenchmarks
```

## License

MIT
