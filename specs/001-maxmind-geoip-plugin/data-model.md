# Data Model: MaxMind GeoIP SPOA Plugin

**Feature**: 001-maxmind-geoip-plugin
**Date**: 2026-04-09

## Entities

### DatabaseType

Enumeration of supported MaxMind database types. Determines which
fields are extracted from a lookup result.

| Variant | MMDB Files | Description |
|---------|------------|-------------|
| City | GeoIP2-City, GeoLite2-City | Full geolocation (country, city, coordinates, postal, subdivisions) |
| Country | GeoIP2-Country, GeoLite2-Country | Country and continent only |
| Asn | GeoLite2-ASN | Autonomous system number and organization |
| Anonymous | GeoIP2-Anonymous-IP | Anonymizer/proxy/VPN/Tor detection flags |
| Isp | GeoIP2-ISP | ISP, organization, ASN, mobile codes |
| ConnType | GeoIP2-Connection-Type | Connection type (Cable/DSL, Cellular, etc.) |
| Domain | GeoIP2-Domain | Second-level domain |

**Config string mapping**: `"city"`, `"country"`, `"asn"`, `"anonymous"`,
`"isp"`, `"conntype"`, `"domain"`.

### LoadedDatabase

A loaded MMDB database ready for lookups.

| Field | Type | Description |
|-------|------|-------------|
| db_type | DatabaseType | Which type of database this is |
| reader | Reader<Vec<u8>> | In-memory MMDB reader instance |

Created during plugin initialization. Immutable after init. Shared
across all concurrent lookups via `&self`.

### PluginConfig

Parsed plugin configuration from hub TOML.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| databases | Vec<DatabaseEntry> | (required) | List of databases to load |
| locale | String | "en" | Locale for name fields |
| fields | Option<Vec<String>> | None (all) | Optional field allowlist |

### DatabaseEntry

Single database entry from configuration.

| Field | Type | Description |
|-------|------|-------------|
| db_type | String | Type keyword ("city", "asn", etc.) |
| path | String | Filesystem path to the MMDB file |

### Output Variables by Database Type

#### City / Country databases

| Variable Name | SPOP Type | Source Path |
|---------------|-----------|-------------|
| country_iso_code | String | country.iso_code |
| country_name | String | country.names.{locale} |
| continent_code | String | continent.code |
| continent_name | String | continent.names.{locale} |
| is_in_european_union | String | country.is_in_european_union (as "true"/"false") |

#### City databases (additional)

| Variable Name | SPOP Type | Source Path |
|---------------|-----------|-------------|
| city_name | String | city.names.{locale} |
| latitude | String | location.latitude (formatted) |
| longitude | String | location.longitude (formatted) |
| postal_code | String | postal.code |
| timezone | String | location.time_zone |
| accuracy_radius | String | location.accuracy_radius (formatted) |
| subdivision_iso_code | String | subdivisions[0].iso_code |
| subdivision_name | String | subdivisions[0].names.{locale} |

#### ASN database

| Variable Name | SPOP Type | Source Path |
|---------------|-----------|-------------|
| autonomous_system_number | String | autonomous_system_number (formatted) |
| autonomous_system_organization | String | autonomous_system_organization |

#### Anonymous IP database

| Variable Name | SPOP Type | Source Path |
|---------------|-----------|-------------|
| is_anonymous | String | is_anonymous (as "true"/"false") |
| is_anonymous_vpn | String | is_anonymous_vpn (as "true"/"false") |
| is_hosting_provider | String | is_hosting_provider (as "true"/"false") |
| is_public_proxy | String | is_public_proxy (as "true"/"false") |
| is_residential_proxy | String | is_residential_proxy (as "true"/"false") |
| is_tor_exit_node | String | is_tor_exit_node (as "true"/"false") |

#### ISP database

| Variable Name | SPOP Type | Source Path |
|---------------|-----------|-------------|
| isp | String | isp |
| organization | String | organization |
| autonomous_system_number | String | autonomous_system_number (formatted) |
| autonomous_system_organization | String | autonomous_system_organization |

#### Connection Type database

| Variable Name | SPOP Type | Source Path |
|---------------|-----------|-------------|
| connection_type | String | connection_type |

#### Domain database

| Variable Name | SPOP Type | Source Path |
|---------------|-----------|-------------|
| domain | String | domain |

## Data Flow

```text
HAProxy                    SPOA Hub              MaxMind Plugin
  │                          │                        │
  │── SPOE msg ──────────────│                        │
  │   (geoip-lookup,         │                        │
  │    ip=<client IP>)       │── process(message) ───│
  │                          │                        │── parse IP from args
  │                          │                        │── for each LoadedDatabase:
  │                          │                        │     reader.lookup(ip)
  │                          │                        │     extract fields by db_type
  │                          │                        │     apply locale for names
  │                          │                        │     apply field allowlist
  │                          │                        │── collect TxnVariables
  │                          │←─ ProcessingResult ───│
  │←─ ACK (SET-VAR) ────────│                        │
  │                          │                        │
  │  txn.maxmind.country_iso_code = "US"              │
  │  txn.maxmind.city_name = "Minneapolis"            │
  │  txn.maxmind.latitude = "44.9759"                 │
  │  ...                                              │
```

## State Lifecycle

1. **Init**: Load config → parse database entries → open each MMDB
   file into `Reader<Vec<u8>>` → store in `OnceLock`
2. **Process**: Read IP from message args → lookup in each database →
   extract fields → return `ProcessingResult` with `TxnVariable` vec
3. **Shutdown**: Drop readers (memory freed automatically)

No mutable state after initialization. All `process()` calls are
read-only against the loaded databases.
