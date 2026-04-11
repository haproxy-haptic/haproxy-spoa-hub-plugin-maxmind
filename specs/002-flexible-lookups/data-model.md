# Phase 1 Data Model: Flexible lookup inputs and outputs

**Date**: 2026-04-11
**Branch**: `002-flexible-lookups`

## Configuration entities (parsed from `PluginContext`)

### `PluginConfig`

The root configuration struct produced by `PluginConfig::from_context(&PluginContext)`.

| Field | Type | Required | Notes |
|---|---|---|---|
| `databases` | `BTreeMap<String, DatabaseEntry>` | yes, ≥1 | Operator-chosen alias → database file path. Empty map is rejected. |
| `lookups` | `Vec<Lookup>` | yes, ≥1 | All extractions defined in this deployment. Empty vector is rejected. |

**Validation rules** (enforced in `from_context` before any database is opened):

- Every `Lookup.database` MUST resolve to a key in `databases` — otherwise init fails with `unknown database alias '<name>' referenced by lookup '<lookup_name>'`.
- Every `Lookup.name` MUST be unique across the `lookups` vector — otherwise init fails with `duplicate lookup name '<name>'`.
- Every `Lookup.output_var` MUST be unique across the `lookups` vector — otherwise init fails with `duplicate output variable '<var>' (used by lookups '<a>' and '<b>')`.
- Every `Lookup.path` MUST be non-empty — otherwise init fails with `lookup '<name>' has empty field path`.
- Every `Lookup.message` MUST be non-empty — otherwise init fails with `lookup '<name>' has empty message name`.

### `DatabaseEntry`

| Field | Type | Required | Notes |
|---|---|---|---|
| `path` | `String` | yes | Filesystem path to an MMDB file. Read once at init via `Reader::open_readfile`. |

### `Lookup`

The new central entity. Replaces the implicit "one extractor per database type" mapping from 0.1.0.

| Field | Type | Required | Default | Notes |
|---|---|---|---|---|
| `name` | `String` | yes | — | Internal identifier. Stable key for warn-once dedup and error messages. Unique across `lookups`. |
| `message` | `String` | yes | — | SPOE message name to handle. Multiple lookups MAY share a message name (clarification Q2). |
| `database` | `String` | yes | — | Alias matching a key in `PluginConfig.databases`. |
| `input` | `String` | no | `"ip"` | Name of the SPOE message argument carrying the lookup value. |
| `path` | `Vec<String>` | yes, ≥1 | — | Ordered list of map keys to traverse the MMDB record (clarification Q1). |
| `output_var` | `String` | yes | — | Transaction-scope SPOE variable name receiving the leaf value. |

## Runtime entities (constructed during `init`)

### `LoadedDatabases`

Owns all opened MMDB readers. Built from `PluginConfig.databases`.

| Field | Type | Notes |
|---|---|---|
| `readers` | `BTreeMap<String, Reader<Vec<u8>>>` | Alias → in-memory `maxminddb::Reader`. Each file is opened exactly once regardless of how many lookups reference it (FR-010). |

### `CompiledLookup`

A `Lookup` with its field-path pre-compiled into the format `decode_path` expects, plus a hot-path-friendly key for the warn-once cache. Stored once at init; reused per request.

| Field | Type | Notes |
|---|---|---|
| `name` | `String` | Copied from `Lookup.name`. |
| `database` | `String` | Copied from `Lookup.database`. |
| `input` | `String` | Copied from `Lookup.input` (with default applied). |
| `path` | `Vec<String>` | Owned key strings. `PathElement::Key(&str)` slices are rebuilt per request (a handful of stack writes). |
| `path_display` | `String` | Pre-rendered `"a/b/c"` form used as the warn-once cache key and in log messages. |
| `output_var` | `String` | Copied from `Lookup.output_var`. |
| `status_var` | `String` | Pre-computed `"<output_var>_status"` to avoid per-request concatenation. |

### `LookupRouter`

Routes incoming SPOE messages to the lookups bound to them.

| Field | Type | Notes |
|---|---|---|
| `by_message` | `BTreeMap<String, Vec<usize>>` | Message name → indices into `compiled_lookups`. Built once at init. |
| `compiled_lookups` | `Vec<CompiledLookup>` | All compiled lookups, in declaration order. |

### `WarnOnce`

Bounded warning deduplication helper (see research D3).

| Field | Type | Notes |
|---|---|---|
| `window` | `Duration` | Default 5 minutes. |
| `seen` | `Mutex<HashMap<(String, String), Instant>>` | Keyed on `(lookup_name, path_display)`. GC'd opportunistically when size exceeds 1024. |

### `PluginState`

Top-level state stored in `OnceLock<PluginState>` (same shape as 0.1.0).

| Field | Type | Notes |
|---|---|---|
| `databases` | `LoadedDatabases` | |
| `router` | `LookupRouter` | |
| `warn_once` | `WarnOnce` | |

## Status enumeration

`LookupStatus` is an internal enum produced by the lookup execution path; it never appears in config.

| Variant | Rendered string | Triggered when |
|---|---|---|
| `Ok` | `"ok"` | `decode_path` returned `Ok(Some(_))` and the leaf was rendered to a string. |
| `NotFound` | `"not_found"` | `Reader::lookup` could not find the IP, or `decode_path` returned `Ok(None)` (missing key). |
| `Error` | `"error"` | Input value was not a valid IP, or `decode_path` returned `Err(Decoding{..})` (path landed on a non-leaf or type-mismatched node). |

## Output variable model

For every `CompiledLookup` executed against an incoming message, the plugin emits exactly two `TxnVariable`s:

1. **Primary**: name = `output_var`, value = the rendered leaf (`String`) or empty string when status ≠ `Ok`.
2. **Status**: name = `status_var`, value = `"ok"` | `"not_found"` | `"error"`.

Both are `VarScope::Transaction` and `SpoeValue::String`. The hub prefixes them with the plugin name as today (e.g. `output_var = "geo_country"` becomes `txn.maxmind.geo_country` in HAProxy).

## State transitions

There is no per-request state machine. Per-request flow:

1. Plugin receives `SpoeMessage{ name, args, .. }`.
2. `LookupRouter` resolves `name → &[CompiledLookup]`. If no entry, return `Err("no lookup bound to message '<name>'")` (FR-008).
3. For each compiled lookup in the bucket:
   a. Read `args[input]` → `SpoeValue`. If absent, return error with the lookup name.
   b. `spoe_value_to_ip(value)` → `IpAddr` or `LookupStatus::Error`.
   c. `LoadedDatabases.readers[database].lookup(ip)` → `LookupResult`. If miss, status = `NotFound`.
   d. `result.decode_path::<AnyLeaf>(&path_elements)` → `Some(leaf)` (status `Ok`), `None` (status `NotFound`, warn once on `(name, path_display)`), or `Err` (status `Error`, warn once on the same key with the error string).
   e. Emit primary + status `TxnVariable`s.
4. Return the collected variables in a single `ProcessingResult`.

## Schema (JSON Schema sketch for `PluginConfig::config_schema()`)

```json
{
  "type": "object",
  "required": ["databases", "lookups"],
  "properties": {
    "databases": {
      "type": "object",
      "minProperties": 1,
      "additionalProperties": {
        "type": "object",
        "required": ["path"],
        "properties": { "path": { "type": "string" } }
      }
    },
    "lookups": {
      "type": "array",
      "minItems": 1,
      "items": {
        "type": "object",
        "required": ["name", "message", "database", "path", "output_var"],
        "properties": {
          "name":       { "type": "string", "minLength": 1 },
          "message":    { "type": "string", "minLength": 1 },
          "database":   { "type": "string", "minLength": 1 },
          "input":      { "type": "string", "minLength": 1 },
          "path":       { "type": "array", "minItems": 1, "items": { "type": "string", "minLength": 1 } },
          "output_var": { "type": "string", "minLength": 1 }
        }
      }
    }
  }
}
```
