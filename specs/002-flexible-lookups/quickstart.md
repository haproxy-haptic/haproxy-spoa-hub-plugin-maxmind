# Quickstart: Flexible MaxMind lookups

**Feature**: 002-flexible-lookups
**Date**: 2026-04-11

This walks through enabling the new alias-and-lookup model from a fresh build. It assumes you already have a working `haproxy-spoa-hub` and at least one MMDB file on disk.

## 1. Build the plugin

```sh
git checkout 002-flexible-lookups
cargo build --release
# → target/release/libmaxmind_plugin.so
```

For multi-arch builds, the existing GitLab CI matrix produces `libmaxmind_plugin-{amd64,arm64,armv7}-glibc2.36.so` artifacts on every push.

## 2. Configure the hub

Edit your hub config (e.g. `/etc/haproxy-spoa-hub/hub.toml`):

```toml
[[plugins]]
name     = "maxmind"
library  = "/usr/local/lib/libmaxmind_plugin.so"
messages = ["geoip-enrich"]

# Load every MMDB file under an operator-chosen alias.
[plugins.params.databases]
city = { path = "/var/lib/maxmind/GeoLite2-City.mmdb" }
asn  = { path = "/var/lib/maxmind/GeoLite2-ASN.mmdb"  }

# Define exactly the extractions you want.
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
```

Both lookups bind to `geoip-enrich`, so HAProxy makes one round-trip per request and receives both variables (plus their `_status` siblings).

## 3. Configure HAProxy SPOE

`/etc/haproxy/spoe-maxmind.conf`:

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

To geolocate the original client behind a forwarding proxy, swap `args ip=src` for `args ip=req.hdr(X-Forwarded-For)`.

## 4. Use the variables in HAProxy

```
frontend www
    mode http
    bind *:80
    filter spoe engine maxmind config /etc/haproxy/spoe-maxmind.conf

    http-request set-header X-Country %[var(txn.maxmind.geo_country)]
    http-request set-header X-ASN     %[var(txn.maxmind.geo_asn)]

    # Strict branching: only act on confirmed lookups
    use_backend eu-servers \
        if { var(txn.maxmind.geo_country_status) -m str ok } \
           { var(txn.maxmind.geo_country) -m str -f /etc/haproxy/eu-iso-codes.lst }

    default_backend webservers
```

## 5. Verify

Send a test request from a known IP and inspect the SPOE response (or `txn.*` variables in HAProxy logs):

```sh
curl -H 'X-Forwarded-For: 89.160.20.128' http://localhost/
```

You should see two pairs of variables populated:

```
txn.maxmind.geo_country         = "SE"
txn.maxmind.geo_country_status  = "ok"
txn.maxmind.geo_asn             = "29518"
txn.maxmind.geo_asn_status      = "ok"
```

A miss (private IP) yields:

```
txn.maxmind.geo_country         = ""
txn.maxmind.geo_country_status  = "not_found"
```

A typo in the field path yields the same empty value but `_status = "error"` and a one-time warning in the plugin logs naming the lookup and the bad path.

## 6. Add another extraction

Adding the city name does not require any code change:

```toml
[[plugins.params.lookups]]
name       = "city_name_de"
message    = "geoip-enrich"
database   = "city"
path       = ["city", "names", "de"]
output_var = "geo_city_de"
```

Reload the hub. Subsequent requests to `geoip-enrich` will additionally populate `txn.maxmind.geo_city_de` and `txn.maxmind.geo_city_de_status`. The City database is still loaded only once — adding lookups against an already-loaded database is free.

## Migration from 0.1.0

If you are upgrading from 0.1.0:

1. Remove `locale`, `fields`, and the `databases[].type` keys.
2. Convert each variable you used in HAProxy into a `[[plugins.params.lookups]]` entry.
3. Pick a `message` name (you can keep the old `geoip-lookup` if you want — the plugin no longer treats it specially).
4. Update HAProxy `var(txn.maxmind.<old_name>)` references to the new `output_var` names you chose, and add `_status` checks where you want strict branching.

The 0.1.0 config keys are not accepted by 0.2.0 — the plugin fails fast at init with a clear error if it sees them.
