# HAProxy SPOA Hub — MaxMind Plugin

A [haproxy-spoa-hub](https://gitlab.com/haproxy-haptic/haproxy-spoa-hub) plugin that performs MaxMind GeoIP lookups. Supports every MaxMind MMDB database type — City, Country, ASN, Anonymous IP, ISP, Connection Type, Domain — by addressing arbitrary field paths inside the record.

## Overview

The plugin loads MaxMind MMDB files into memory at startup and exposes their records as transaction-scoped HAProxy variables. Operators define **named lookups**, each binding one HAProxy SPOE message to one field path inside one database, producing one transaction variable plus a sibling status variable. This brings the flexibility of [HAProxy Enterprise's MaxMind module](https://www.haproxy.com/documentation/haproxy-enterprise/enterprise-modules/geolocation/maxmind/) and its `maxmind-lookup` converter to HAProxy Community Edition.

Databases are read once at init via the in-memory MMDB reader — there is no per-request disk I/O. A single lookup completes in single-digit microseconds.

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
name     = "maxmind"
library  = "libmaxmind_plugin.so"
messages = ["geoip-enrich"]

# Load every MMDB file under an operator-chosen alias.
[plugins.params.databases]
city = { path = "/data/GeoLite2-City.mmdb" }
asn  = { path = "/data/GeoLite2-ASN.mmdb"  }

# Define exactly the extractions you want. Multiple lookups MAY share a
# message name; the plugin runs all of them in one round-trip and returns
# the union of their output variables.
[[plugins.params.lookups]]
name       = "country_code"
message    = "geoip-enrich"
database   = "city"
path       = ["country", "iso_code"]
output_var = "geo_country"

[[plugins.params.lookups]]
name       = "as_number"
message    = "geoip-enrich"
database   = "asn"
path       = ["autonomous_system_number"]
output_var = "geo_asn"

[[plugins.params.lookups]]
name       = "forwarded_country"
message    = "geoip-enrich"
database   = "city"
input      = "client_ip"           # override; defaults to "ip" when omitted
path       = ["country", "iso_code"]
output_var = "geo_fwd_country"
```

### Configure HAProxy

#### SPOE agent definition (`spoe-maxmind.conf`)

```
[maxmind]
spoe-agent maxmind-agent
    messages    geoip-enrich
    option var-prefix maxmind
    option continue-on-error
    timeout hello      2s
    timeout idle       5m
    timeout processing 10ms
    use-backend spoa-hub-backend

spoe-message geoip-enrich
    args ip=src client_ip=req.hdr(X-Forwarded-For)
    event on-frontend-http-request
```

Each lookup's `input` field names the SPOE argument it reads (default `ip`); the HAProxy `args` line binds each argument name to a sample expression with `args <name>=<sample-expression>`. The names on both sides must match — in the example above, `country_code` and `as_number` read the `ip` argument bound to `src`, while `forwarded_country` reads the `client_ip` argument bound to `req.hdr(X-Forwarded-For)`. Any HAProxy sample expression works on the right side of the binding (`src`, `req.hdr(...)`, `var(...)`, `src,ipmask(24)`, etc.).

#### HAProxy frontend (`haproxy.cfg`)

```haproxy
frontend www
    bind *:80
    filter spoe engine maxmind config /etc/haproxy/spoe-maxmind.conf

    http-request set-header X-Country %[var(txn.maxmind.geo_country)]
    http-request set-header X-ASN     %[var(txn.maxmind.geo_asn)]

    # Branch on the status variable to distinguish "not in DB" from "ok".
    use_backend eu-servers \
        if { var(txn.maxmind.geo_country_status) -m str ok } \
           { var(txn.maxmind.geo_country) -m str -f /etc/haproxy/eu-iso-codes.lst }

    default_backend app

backend spoa-hub-backend
    mode spop
    server hub /var/run/spoe/hub.sock
```

### Verify

```bash
curl -v http://localhost:80/ 2>&1 | grep -E 'X-(Country|ASN)'
```

## Configuration Reference

### Top-level

| Parameter | Type | Description |
|---|---|---|
| `databases` | table of tables | Operator-chosen alias → `{ path = "..." }`. Required, must contain at least one entry. |
| `lookups` | array of tables | Named extractions. Required, must contain at least one entry. |

### Per-lookup

| Field | Type | Default | Description |
|---|---|---|---|
| `name` | string | *(required)* | Internal identifier. Must be unique across all lookups. Used in log messages and warning deduplication keys. |
| `message` | string | *(required)* | SPOE message name this lookup answers to. Multiple lookups may share a message name. |
| `database` | string | *(required)* | Alias of a database loaded under `[plugins.params.databases]`. |
| `input` | string | `"ip"` | Name of the SPOE message argument carrying the lookup value. Match this in the HAProxy `spoe-message` definition (e.g. `args ip=src` or `args client_ip=req.hdr(X-Forwarded-For)`). |
| `path` | array of strings | *(required)* | Ordered list of map keys traversing the MMDB record to a leaf value. |
| `output_var` | string | *(required)* | Transaction-scope variable receiving the leaf value. The plugin also emits `<output_var>_status` carrying `ok`, `not_found`, or `error`. |

### How field paths work

A field path is an ordered list of map keys, just like the path arguments to the HAProxy Enterprise `maxmind-lookup` converter. To find valid paths, consult the MaxMind MMDB schema for your database — for example by running `mmdblookup` against your file and reading the JSON output. Common paths:

| Path | Database | Leaf type | Example value |
|---|---|---|---|
| `["country", "iso_code"]` | City / Country | string | `SE` |
| `["country", "names", "de"]` | City / Country | string | `Schweden` |
| `["country", "is_in_european_union"]` | City / Country | bool | `true` |
| `["city", "names", "en"]` | City | string | `Stockholm` |
| `["location", "latitude"]` | City | float | `59.3293` |
| `["location", "time_zone"]` | City | string | `Europe/Stockholm` |
| `["postal", "code"]` | City | string | `11122` |
| `["autonomous_system_number"]` | ASN / ISP | int | `29518` |
| `["autonomous_system_organization"]` | ASN / ISP | string | `Bredband2 AB` |
| `["isp"]` | ISP | string | `Comhem` |
| `["is_tor_exit_node"]` | Anonymous IP | bool | `true` |
| `["connection_type"]` | Connection Type | string | `Cable/DSL` |
| `["domain"]` | Domain | string | `example.com` |

Strings render verbatim, integers and floats use their canonical decimal form, and booleans become the literal `"true"` or `"false"`. Missing keys and addresses-not-in-database both yield an empty string with status `not_found`. A path that lands on a non-leaf node (map or array) yields an empty string with status `error` and a one-time warning naming the offending lookup.

### Status variable

For every lookup the plugin emits a sibling `<output_var>_status` variable carrying one of:

| Value | Meaning |
|---|---|
| `ok` | Leaf value extracted successfully — the primary variable carries it. |
| `not_found` | Either the IP is not present in the database, or the configured field path does not exist in the matching record. The primary variable is empty. |
| `error` | The input value was not a valid IP, or the field path landed on a non-leaf node (operator typo). The primary variable is empty. |

Operators who don't need this signal can ignore it; operators who do can branch on it from HAProxy ACLs without parsing the primary value.

### Error behavior

- **Plugin receives a message name with no lookups bound**: Returns a hub-level error.
- **Lookup's declared `input` argument is missing from the message**: Returns a hub-level error naming both the lookup and the missing argument — this is a configuration mismatch between HAProxy and the hub.
- **Invalid configuration at startup** (unknown database alias, duplicate output variable, duplicate lookup name, missing required field, legacy 0.1.0 keys): Plugin fails to initialize with a descriptive error.
- **Per-request data anomalies** (IP not in DB, malformed input value, missing field path, non-leaf path): Sets the output variable to empty, sets the status variable accordingly, and emits a warning at most once per `(lookup_name, path)` per dedup window. The SPOE transaction is never aborted by data anomalies.

## Migration from 0.1.0

The 0.2.0 configuration shape replaces the 0.1.0 type-based model entirely. Upgrading is a one-time edit of your hub config and HAProxy variable references.

| 0.1.0 | 0.2.0 |
|---|---|
| `[[plugins.params.databases]]` array with `type = "city"` | `[plugins.params.databases]` table with operator-chosen alias and a `path` field |
| Hard-coded variable bundle (`country_iso_code`, `city_name`, …) | One `[[plugins.params.lookups]]` entry per variable you actually want, with explicit `path` and `output_var` |
| `locale = "de"` | `path = ["city", "names", "de"]` (per-lookup, fully addressable) |
| `fields = [...]` allowlist | Define only the lookups you want — the allowlist is implicit in the lookup definitions |
| Single message name `geoip-lookup` | Operator-chosen `message` per lookup; multiple lookups may share a message name |

The 0.1.0 keys (`type`, `locale`, `fields`) are not accepted by 0.2.0 — the plugin fails fast at init with an error pointing back at this section.

## Performance

Single-database lookup against a loaded MMDB stays well under 1 ms p99 on commodity hardware. Adding a second lookup against the same already-loaded database carries no additional load cost; the file is opened once regardless of how many lookups reference it.

## Development

### Commands

```bash
cargo build --release            # Build plugin .so
cargo test                       # Unit + integration tests
cargo bench                      # Criterion microbenchmarks
cargo clippy -- -D warnings      # Lint
```

### Project Structure

```
src/
  lib.rs              Module declarations
  plugin.rs           SpoePlugin glue + process_message routing
  config.rs           PluginConfig, DatabaseEntry, Lookup parsing & validation
  lookup.rs           LoadedDatabases, LookupRouter, execute_lookup
  path.rs             AnyLeaf decoder + field-path compilation
  ip.rs               SpoeValue → IpAddr conversion
  warn_log.rs         Bounded warn-once helper

tests/
  lookup_test.rs      Field-path traversal across all primitive leaf types
  plugin_test.rs      Config parsing, IP extraction, schema, end-to-end routing
  fixtures/           MaxMind test MMDB files (git submodule)

benches/
  lookup_bench.rs     Criterion microbenchmarks
```

## License

MIT
