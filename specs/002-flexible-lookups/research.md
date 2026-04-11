# Phase 0 Research: Flexible lookup inputs and outputs

**Date**: 2026-04-11
**Branch**: `002-flexible-lookups`

## Decisions

### D1. Generic field-path decoding strategy

**Decision**: Use `maxminddb::LookupResult::decode_path::<T>(&[PathElement])` from `maxminddb` 0.27 directly. Pre-compile each lookup's operator-supplied field path into `Vec<PathElement<'static>>` at config load time and reuse on every request.

**Rationale**: `decode_path` walks the on-disk MMDB binary tree against the mmap'd `&[u8]`, calls `skip_value` on non-matching map entries, and only deserializes the terminal node. There is no intermediate allocation of any container types — exactly what the <1 ms p99 budget needs. It also returns `Ok(None)` for missing keys (perfect for the "field path missing → empty string + warn" semantics) and surfaces type mismatches as `Err(Decoding {..})` (perfect for distinguishing "operator typo addressing a non-leaf" from "data anomaly").

**Alternatives considered**:

- Decode the whole record into `serde_json::Value` (or `BTreeMap<String, …>`) and walk it manually. Rejected: forces allocation of every nested map and string in the record (a City record has ~30 fields across several sub-maps), measurably slower, and adds an unnecessary `serde_json` dependency.
- Use `Reader::within()` and iterate. Rejected: that API is for range scans, not point lookups.
- Keep the typed `geoip2::City`/`Country`/etc. structs and add a runtime field-name dispatch table. Rejected: defeats the entire point of the rewrite; still hard-codes the field universe.

### D2. Leaf-value rendering across heterogeneous types

**Decision**: Implement a tiny `AnyLeaf` enum with a custom `serde::de::Visitor` that accepts `visit_str` / `visit_borrowed_str` / `visit_i64` / `visit_u64` / `visit_f64` / `visit_bool` and stringifies eagerly. Use it as the `T` parameter to `decode_path::<AnyLeaf>`.

**Rationale**: Operators expect to address heterogeneous fields with one mechanism. `decode_path` is generic over `T: Deserialize`, so a single visitor that handles every primitive type lets the operator point at `country.iso_code` (string), `location.latitude` (float), or `country.is_in_european_union` (bool) with no per-field code. Booleans render as `"true"`/`"false"`; numerics render via Rust's `Display` (canonical decimal). Non-primitive landings (map/array at the terminal) bubble up as a `Decoding` error which we map to status `error` per FR-009/FR-009a.

**Alternatives considered**:

- Try `decode_path::<String>` then fall back to `i64` then `f64` then `bool`. Rejected: four traversals per failed branch is wasteful and clutters error handling.
- Decode into `serde_json::Value` and stringify post-hoc. Rejected: same reasons as D1.

### D3. Bounded warning deduplication

**Decision**: Add a small in-process `WarnOnce` helper backed by `std::sync::Mutex<HashMap<(String, String), Instant>>` keyed on `(lookup_name, path_string)`. Emit the warning only if the key is absent or its last-seen timestamp is older than a 5-minute window. Run an opportunistic `retain` GC when the map exceeds 1024 entries to bound memory without a background task. No new dependency.

**Rationale**: Satisfies FR-012 (bounded log emission per unique missing path) without dragging in `tracing` / `metrics` / `governor`. `Instant` is monotonic so there is no clock-skew risk. The critical section is O(1) and the call rate is bounded by request rate × number of misconfigured lookups, which in practice is negligible.

**Alternatives considered**:

- Crate-based rate limiter (`governor`, `ratelimit_meter`). Rejected: extra dependency for a 30-line problem.
- Always-log-but-aggregate via `log` filter. Rejected: pushes the responsibility onto operators' log infrastructure and doesn't solve the in-process flood under traffic.
- Once-per-process (no time window). Rejected: a transient misconfiguration that gets fixed and reintroduced wouldn't re-warn.

### D4. Hub configuration shape

**Decision**: Two top-level keys.

```toml
# Aliases — operator picks the names; values are filesystem paths.
[databases]
city  = "/var/lib/maxmind/GeoIP2-City.mmdb"
asn   = "/var/lib/maxmind/GeoLite2-ASN.mmdb"

# Lookups — array of tables, each binding one extraction to a message.
[[lookups]]
name        = "country_code"
message     = "geoip-country"
database    = "city"
input       = "ip"                       # default if omitted
path        = ["country", "iso_code"]
output_var  = "geo_country"

[[lookups]]
name        = "as_number"
message     = "geoip-country"            # same message → returned together
database    = "asn"
path        = ["autonomous_system_number"]
output_var  = "geo_asn"
```

**Rationale**: `databases` as a flat alias→path map is the minimum the operator needs to express. `lookups` as an array of tables keeps every binding explicit, supports many lookups per message (clarification Q2: yes), and uses field-path-as-array (clarification Q1: array of strings). The `name` field is internal-facing (logs, error messages, status-variable de-duplication) and is required to give the warn-once dedup helper a stable key.

**Alternatives considered**:

- Nest lookups under their database: `[databases.city.lookups]`. Rejected: makes "many lookups, same message, different databases" awkward.
- Inline path as a dot-string: `path = "country.iso_code"`. Rejected by clarification Q1.
- Make `input` mandatory. Rejected: every existing call site uses `ip`, defaulting matches the 0.1.0 ergonomics.

### D5. Status variable naming

**Decision**: Sibling variable named `<output_var>_status`, value ∈ {`ok`, `not_found`, `error`}, always emitted alongside the primary variable. Operators who want to be strict can `acl missing var(txn.geo_country_status) -m str not_found`.

**Rationale**: Matches clarification Q3. Suffix `_status` avoids collision with operator-chosen names. Three discrete strings give HAProxy ACLs an obvious branching primitive without exposing internal error categories.

**Alternatives considered**:

- Numeric status code. Rejected: harder to read in HAProxy ACLs.
- Single sentinel value embedded in the primary variable (e.g. empty for not_found, `__err__` for error). Rejected by clarification Q3 (Option C) — pollutes the value space and risks collisions with real DB strings.

### D6. Backwards compatibility

**Decision**: No backwards compatibility shim. The 0.1.0 config keys (`databases[].type`, `locale`, `fields`) are removed entirely. Document the migration in `CHANGELOG.md` and `README.md`.

**Rationale**: Pre-1.0 project with no stable downstream consumers; carrying two config models would double the surface area and confuse the schema. Operators upgrading from 0.1.0 receive a clear init-time error if they try to load an old config.

## Open questions deferred to implementation

- **Exact dedup window**: 5 minutes is the chosen default; whether to expose it as a config knob is left to the implementer's judgment. Default-only is acceptable for v0.2.
- **`name` uniqueness across the `lookups` array**: must be enforced at config load (alongside the existing duplicate-`output_var` check) so the warn-once cache key is stable. Implementer to add the check.
