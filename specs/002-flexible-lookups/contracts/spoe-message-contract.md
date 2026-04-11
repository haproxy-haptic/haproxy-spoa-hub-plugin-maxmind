# SPOE Message Contract: operator-defined lookups

**Feature**: 002-flexible-lookups
**Date**: 2026-04-11
**Supersedes**: 001-maxmind-geoip-plugin/contracts/spoe-message-contract.md

## Overview

The plugin no longer registers a single hard-coded `geoip-lookup` message. Instead, every operator-defined `Lookup` declares a `message` name that the plugin will handle. Multiple lookups MAY share a `message` name; in that case the plugin runs all of them on each incoming message and emits the union of their output and status variables in one response.

## Message Definition

| Property | Value |
|---|---|
| Message name | Operator-defined per `Lookup.message` |
| Event | Operator-defined in HAProxy SPOE config (typically `on-frontend-http-request`) |

## Input Arguments

Each lookup reads exactly one argument from the SPOE message: the value named by `Lookup.input` (default `"ip"`).

| Argument | SPOP Type | Required | Description |
|---|---|---|---|
| `<input>` | IPv4, IPv6, or String | Yes | Value to look up. Operator picks the source via the HAProxy SPOE message definition: `args ip=src`, `args ip=req.hdr(X-Forwarded-For)`, `args ip=src,ipmask(24)`, etc. |

If two lookups bound to the same `message` declare different `input` names, the SPOE message MUST carry both. If they declare the same `input` name, the message carries it once and both lookups read from it.

## Output Variables

For every lookup executed against a message, the plugin emits two `VarScope::Transaction` `SpoeValue::String` variables:

| Variable | Value | When |
|---|---|---|
| `<output_var>` | The leaf value rendered as a string (text verbatim, booleans as `"true"`/`"false"`, numerics in canonical decimal form), or the empty string when the status is not `ok`. | Always |
| `<output_var>_status` | `"ok"`, `"not_found"`, or `"error"`. | Always |

The hub prefixes both variables with the plugin name as today, so `output_var = "geo_country"` reaches HAProxy as `txn.maxmind.geo_country` and `txn.maxmind.geo_country_status`.

## Status semantics

| Status | Meaning |
|---|---|
| `ok` | The lookup found the IP, traversed the configured field path, and rendered a leaf value. The primary variable carries that value. |
| `not_found` | Either the IP is not present in the database, or the configured field path does not exist in the matching record. The primary variable is empty. A warning is emitted at most once per `(lookup_name, path)` per dedup window. |
| `error` | Either the input value was not a valid IP, or the configured field path landed on a non-leaf node (operator typo). The primary variable is empty. A warning is emitted at most once per `(lookup_name, path)` per dedup window. |

The status is in-band so HAProxy ACLs can distinguish "look elsewhere" from "fix the config" without parsing the value. Operators who do not need this signal can ignore the status variable.

## Error Behavior

| Condition | Behavior |
|---|---|
| Plugin receives a message name with no lookups bound | Return error to the hub: `no lookup bound to message '<name>'`. The hub logs it and the SPOE transaction surfaces the error. |
| Lookup's declared `input` argument is missing from the message | Return error to the hub: `lookup '<lookup_name>' missing required argument '<input>'`. This is a configuration mismatch between HAProxy and the hub, not a per-request data issue. |
| `input` argument has an unsupported SPOP type | Return error to the hub: `lookup '<lookup_name>' received unsupported argument type for '<input>'`. |
| `input` argument is a string that is not a valid IP | Set primary variable to empty, status to `error`, warn once. |
| IP not present in any record | Set primary variable to empty, status to `not_found`, warn once. |
| Field path key missing | Set primary variable to empty, status to `not_found`, warn once. |
| Field path lands on a non-leaf (map/array) | Set primary variable to empty, status to `error`, warn once. |

## HAProxy SPOE Configuration Example

A deployment that wants country code, ASN, and a Tor flag — all from one round-trip — defines one SPOE message and binds three lookups to it:

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
    args ip=src
    event on-frontend-http-request
```

A deployment that wants to geolocate the original client behind a proxy uses `req.hdr` instead:

```
spoe-message geoip-enrich
    args ip=req.hdr(X-Forwarded-For)
    event on-frontend-http-request
```

## Hub Configuration Example

```toml
[[plugins]]
name    = "maxmind"
library = "libmaxmind_plugin.so"
messages = ["geoip-enrich"]

[plugins.params.databases]
city = { path = "/data/GeoLite2-City.mmdb" }
asn  = { path = "/data/GeoLite2-ASN.mmdb"  }
anon = { path = "/data/GeoIP2-Anonymous-IP.mmdb" }

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
name       = "is_tor"
message    = "geoip-enrich"
database   = "anon"
path       = ["is_tor_exit_node"]
output_var = "geo_is_tor"
```

## HAProxy Usage Example

```
frontend www
    mode http
    bind *:80
    filter spoe engine maxmind config /etc/haproxy/spoe-maxmind.conf

    http-request set-header X-Country %[var(txn.maxmind.geo_country)]
    http-request set-header X-ASN     %[var(txn.maxmind.geo_asn)]

    # Branch on the status variable to distinguish "not in DB" from "ok"
    http-request set-header X-Geo-Source "miss" \
        if { var(txn.maxmind.geo_country_status) -m str not_found }
    http-request set-header X-Geo-Source "ok" \
        if { var(txn.maxmind.geo_country_status) -m str ok }

    # Geo-based routing — empty value also matches missing, but the status
    # variable lets us be explicit if we care.
    use_backend tor-quarantine if { var(txn.maxmind.geo_is_tor) -m str true }
    use_backend eu-servers     if { var(txn.maxmind.geo_country) -m str -f /etc/haproxy/eu-iso-codes.lst }

    default_backend webservers
```

## Migration from 0.1.0

| 0.1.0 concept | 0.2.0 replacement |
|---|---|
| `databases[].type = "city"` | `[plugins.params.databases]` table with operator-chosen alias and a `path` field. |
| Hard-coded variable bundle (e.g. `country_iso_code`, `city_name`, ...) | One `[[plugins.params.lookups]]` entry per variable you actually want, with explicit `path` and `output_var`. |
| `locale = "de"` | `path = ["city", "names", "de"]` (per-lookup, fully addressable). |
| `fields = ["country_iso_code", "city_name"]` allowlist | Define only the lookups you want — the allowlist is implicit in the lookup definitions. |
| Single message name `geoip-lookup` | Operator-chosen `message` per lookup (multiple lookups may share a message name). |
