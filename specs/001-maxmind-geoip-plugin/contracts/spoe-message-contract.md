# SPOE Message Contract: geoip-lookup

**Feature**: 001-maxmind-geoip-plugin
**Date**: 2026-04-09

## Message Definition

**Message name**: `geoip-lookup`
**Event**: `on-frontend-http-request` (typical, configurable by operator)

## Input Arguments

| Argument | SPOP Type | Required | Description |
|----------|-----------|----------|-------------|
| ip | IPv4, IPv6, or String | Yes | IP address to look up. Typically `src` (client IP) or a header value like `req.hdr(X-Forwarded-For)`. |

## Output Variables

All output variables use `VarScope::Transaction` and `SpoeValue::String`.
Variable names are unprefixed — the hub adds the plugin name
automatically (e.g., `country_iso_code` becomes
`txn.maxmind.country_iso_code` in HAProxy).

See [data-model.md](../data-model.md) for the complete variable list
per database type.

## HAProxy SPOE Configuration Example

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

## Hub Configuration Example

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

## Error Behavior

| Condition | Behavior |
|-----------|----------|
| Missing `ip` argument | Return error (logged by hub) |
| IP not found in database | Return empty strings for all fields |
| IP found in one DB but not another | Populated fields from matching DB, empty strings from non-matching |
| Invalid IP string format | Return error (logged by hub) |

## HAProxy Usage Example

```
frontend www
    mode http
    bind *:80
    filter spoe engine maxmind config /etc/haproxy/spoe-maxmind.conf

    http-request set-header X-Country %[var(txn.maxmind.country_iso_code)]
    http-request set-header X-City    %[var(txn.maxmind.city_name)]
    http-request set-header X-ASN     %[var(txn.maxmind.autonomous_system_number)]

    # Geo-based routing
    use_backend eu-servers if { var(txn.maxmind.continent_code) -m str EU }

    default_backend webservers
```
