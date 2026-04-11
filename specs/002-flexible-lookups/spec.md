# Feature Specification: Flexible lookup inputs and outputs

**Feature Branch**: `002-flexible-lookups`
**Created**: 2026-04-11
**Status**: Draft
**Input**: User description: "I want this plugin to be as flexible as https://www.haproxy.com/documentation/haproxy-enterprise/enterprise-modules/geolocation/maxmind/ and its maxmind-lookup converter. There we can fully customize which fields are the lookup input and which fields are extracted from the DB."

## Clarifications

### Session 2026-04-11

- Q: How should operators express a field path in configuration? → A: Array of strings, e.g. `path = ["country", "iso_code"]` — mirrors the HAProxy Enterprise converter's component-wise call shape, handles MMDB keys that contain dots, and maps cleanly to TOML/YAML/JSON lists.
- Q: Can multiple lookups share the same SPOE message name? → A: Yes — any number of lookups may bind to the same message name. On each incoming message the plugin runs every lookup bound to it and returns the union of their output variables in a single response, minimizing HAProxy↔plugin round-trips when several extractions are needed for the same request.
- Q: How should HAProxy distinguish "IP not in DB" from "field path missing" or "input malformed"? → A: Keep the unified empty-string output, and additionally emit a sibling status variable per lookup (`<output>_status` ∈ `ok` | `not_found` | `error`). Operators who don't care continue using the empty-string contract; operators who do can branch on the status variable in HAProxy ACLs without parsing the value.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operator picks exactly which database fields to extract (Priority: P1)

A site reliability engineer wants to enrich incoming requests with just the country ISO code and the ASN number — nothing else — and is willing to define exactly which MaxMind database fields supply those values. They configure the plugin so that one named lookup returns the country ISO code from a City database and another named lookup returns the AS number from an ASN database, each as a single transaction variable. The operator never has to accept (or pay for serializing) fields they don't intend to use, and they can later add a new lookup by editing configuration without code changes.

**Why this priority**: This is the core request. Today the plugin returns a fixed bag of variables per database type. Letting the operator address any field path inside any MMDB record — and decide what each lookup is named — is what makes the plugin "as flexible as the HAProxy Enterprise module". Without this, the other stories don't matter.

**Independent Test**: Configure two named lookups against the supplied test fixtures (one targeting `country.iso_code` in a City database, one targeting `autonomous_system_number` in an ASN database), send a request with a known IP, and assert that exactly the two requested transaction variables are populated with the expected values and no other variables are set.

**Acceptance Scenarios**:

1. **Given** a City database is configured under an alias and a lookup named `country_code` is defined with field path `country.iso_code` writing to variable `geo_country`, **When** a request arrives carrying a public IP that resolves to a known country in the test fixture, **Then** the plugin sets a single transaction variable `geo_country` to that country's ISO code and sets no other variables for that lookup.
2. **Given** the same configuration as above, **When** a request arrives whose IP is not present in the database, **Then** the plugin sets `geo_country` to an empty string and does not raise an error that would interrupt request processing.
3. **Given** a lookup is defined with a field path that does not exist in the database's schema, **When** any request triggers that lookup, **Then** the plugin sets the configured output variable to an empty string and emits a warning log entry once per unique missing path so operators can detect misconfiguration without flooding logs.
4. **Given** a lookup targets a field whose value is not a primitive (object, array), **When** the lookup runs, **Then** the plugin sets the output variable to an empty string and warns that the path does not address a leaf value.

---

### User Story 2 - Operator chooses which client value to look up (Priority: P2)

A platform team in front of an upstream proxy needs to geolocate the *original* client, not the immediate TCP peer. They want to send the value of an `X-Forwarded-For` header (or any other sample expression they choose in HAProxy) into the plugin and have it perform the lookup against that value. They expect to configure this once on the HAProxy side and once in the hub, without the plugin caring whether the source is the connection IP, a request header, or a stick-table key.

**Why this priority**: The HAProxy Enterprise converter takes its input from whichever sample expression the user puts in front of `,maxmind-lookup(...)`. Matching that expressiveness is the second half of "fully customize which fields are the lookup input". This is independent of US1: even with a fixed City→country lookup hard-coded, being able to feed it a different input is already valuable.

**Independent Test**: With a single lookup configured, send two requests through HAProxy — one whose connection IP differs from an `X-Forwarded-For` header value — and verify that the plugin geolocates whichever value the SPOE configuration was instructed to send, not the other one.

**Acceptance Scenarios**:

1. **Given** an operator configures HAProxy's SPOE to send the value of `req.hdr(X-Forwarded-For)` as the lookup input for a specific message, **When** a request arrives with that header set to an IPv4 address present in the database, **Then** the plugin geolocates the header value and not the TCP source.
2. **Given** the same setup, **When** the lookup input arrives as an IPv6 address, **Then** the plugin handles it identically and returns the IPv6 record's data.
3. **Given** the lookup input value is malformed (not a valid IP), **When** the message is processed, **Then** the plugin sets the configured output variable to an empty string and logs a warning, without aborting the SPOE transaction.
4. **Given** the SPOE message arrives without the expected input argument at all, **When** the plugin processes it, **Then** the plugin returns an error that HAProxy can surface, because this indicates a configuration mismatch rather than a data anomaly.

---

### User Story 3 - Operator composes several independent lookups in the same deployment (Priority: P3)

A security team wants to run several enrichments side-by-side: country code from a City database, AS number from an ASN database, and a flag from an Anonymous IP database — each as its own variable, each potentially using a different input expression and a different output variable name. They want to add or remove individual lookups without disturbing the others, and they want each lookup to be addressable individually from HAProxy so they can conditionally invoke them.

**Why this priority**: Multi-database support already exists in 0.1.0, so this story is mostly about preserving that capability under the new flexible model and proving the new model composes cleanly. It is lower priority than US1/US2 because it adds no new fundamental capability — it validates that the new shape doesn't regress.

**Independent Test**: Configure three named lookups against three different database aliases and three different output variable names, exercise each one with the same fixture IP, and assert all three output variables are set independently and that disabling one in configuration removes only its variable from the response.

**Acceptance Scenarios**:

1. **Given** three lookups are configured against three different database aliases, **When** HAProxy invokes all three for a single request, **Then** all three corresponding transaction variables are set independently and carry the correct values.
2. **Given** the same configuration, **When** the operator removes one lookup from the hub configuration and reloads, **Then** the corresponding variable is no longer produced and the other two continue to work unchanged.
3. **Given** two lookups address the same database alias with different field paths, **When** they run for the same input, **Then** the plugin reuses the loaded database (no duplicate file load) and returns both values.

---

### Edge Cases

- A field path traverses a localized name table (e.g. `city.names.de`): the plugin must address the requested locale directly via the field path, with no implicit locale fallback (the operator decides whether to fall back, by configuring additional lookups if they want to).
- A field path addresses a boolean (`country.is_in_european_union`): the plugin renders it as the string `"true"` or `"false"` so HAProxy ACLs can compare it textually.
- A field path addresses a numeric value (`location.latitude`): the plugin renders the canonical decimal string of the number so it can round-trip through HAProxy variables.
- Two lookups declare the same output variable name: configuration loading must reject this at startup so operators get a deterministic error rather than an order-dependent overwrite.
- A configured database alias is not referenced by any lookup: this is allowed (operators may stage databases in advance) but produces an info-level log line so unused loads are visible.
- A lookup references a database alias that is not loaded: configuration loading must reject this at startup.
- The operator changes the field path of an existing lookup without renaming it: on reload the new path takes effect immediately for subsequent requests.
- Multiple SPOE messages map to different lookups in the same deployment: the plugin must be able to route each message to its configured lookup based on the message name alone.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The plugin MUST allow operators to configure databases under operator-chosen aliases, decoupled from any fixed database "type" enum, so that any MaxMind MMDB file can be loaded and addressed by name.
- **FR-002**: The plugin MUST allow operators to configure an arbitrary number of named lookups, each specifying at minimum: a message name to handle, a database alias to query, a field path to extract, and an output transaction variable name.
- **FR-003**: The plugin MUST extract values from MMDB records by following an operator-supplied field path that can traverse arbitrarily nested map keys, matching the addressing model of the HAProxy Enterprise `maxmind-lookup` converter. The field path MUST be expressed in configuration as an ordered list (array) of string keys, one per nesting level, so that keys containing literal dots are unambiguous.
- **FR-004**: The plugin MUST render extracted leaf values as strings: text values verbatim, booleans as `"true"`/`"false"`, integers and floats in their canonical decimal form, and missing or non-leaf values as the empty string.
- **FR-005**: The plugin MUST accept the value to look up as an SPOE message argument whose name is configurable per lookup (defaulting to `input`), so operators can pass any HAProxy sample expression — connection source, request header, stick-table key, etc. — into the lookup.
- **FR-006**: The plugin MUST accept lookup inputs presented as IPv4, IPv6, or string values, parsing strings as textual IP addresses and rejecting unsupported argument types with a clear error.
- **FR-007**: The plugin MUST reject configuration at startup if a lookup references an unknown database alias, if two lookups share the same output variable name, or if a lookup omits any required field, so misconfigurations fail loudly before serving traffic.
- **FR-008**: The plugin MUST route incoming SPOE messages to lookups by the message name declared in configuration, supporting multiple distinct message names in the same deployment. Multiple lookups MAY share the same message name; when this happens the plugin MUST execute every lookup bound to that name on each incoming message and emit the union of their output variables in a single response. The plugin MUST return an error if it receives a message name that no lookup handles.
- **FR-009**: The plugin MUST tolerate runtime data anomalies — IP not present in the database, malformed input value, field path that does not exist, field path that addresses a non-leaf — by setting the output variable to an empty string and logging a warning, without aborting the SPOE transaction.
- **FR-009a**: For every lookup the plugin MUST emit a sibling status transaction variable named `<output>_status` whose value is `ok` when a leaf value was successfully extracted, `not_found` when the IP was not present in the database, or `error` when the input was malformed or the field path did not address a leaf. Operators who do not need this signal can ignore the variable; operators who do can branch on it from HAProxy ACLs without parsing the primary value.
- **FR-010**: The plugin MUST load each configured database file exactly once, in memory, regardless of how many lookups reference it, so adding additional lookups against an already-loaded database carries no additional load cost.
- **FR-011**: The plugin MUST publish a configuration schema that documents the new database-alias and lookup shape so the hub can validate operator input before initialization.
- **FR-012**: The plugin MUST emit warning log entries for missing field paths in a way that is bounded (deduplicated per unique path within a reasonable window) so a misconfigured field cannot flood logs under traffic.
- **FR-013**: The plugin MUST preserve the per-request lookup latency budget established in 0.1.0 (single-database lookup completes well under one millisecond at p99 on commodity hardware) under the new flexible model.

### Key Entities *(include if feature involves data)*

- **Database alias**: An operator-chosen name (e.g. `city`, `asn`, `anon`) bound to a single MMDB file path. The alias is the only handle that lookups use to refer to a database; the plugin no longer assumes a fixed type per database.
- **Lookup**: An operator-defined named extraction. Holds the SPOE message name it answers to, the database alias it queries, the input argument name it reads from the message, the field path it traverses inside the MMDB record, and the output transaction variable name it writes.
- **Field path**: An ordered list of map keys that traverses the nested record returned by an MMDB lookup, terminating at a leaf value. Operators determine valid paths by inspecting the schema of the MMDB file they loaded (the same way they would when using the HAProxy Enterprise converter).
- **Output variable**: A transaction-scope SPOE variable produced by one lookup, whose name is set by the operator in the lookup definition. Each lookup also produces a sibling status variable (`<output>_status`) carrying `ok`, `not_found`, or `error`, so two transaction variables exist per lookup invocation.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator can add a brand-new field extraction (e.g. "give me the timezone of the connecting client") without recompiling the plugin and without needing any plugin code change — only by editing hub configuration and the HAProxy SPOE message definition.
- **SC-002**: An operator can extract any leaf value present in any of the seven currently supported MaxMind database schemas (City, Country, ASN, Anonymous IP, ISP, Connection Type, Domain) using only configuration, with no plugin code referencing the field by name.
- **SC-003**: For a single lookup against a loaded database on commodity hardware, the per-request processing time stays under one millisecond at the 99th percentile, matching the 0.1.0 baseline.
- **SC-004**: A deployment running three independent lookups (different databases, different field paths, different output variables) returns all three values for the same request and produces no extra variables beyond those three plus their three sibling `_status` variables.
- **SC-005**: A configuration that references an unknown database alias or duplicates an output variable name fails plugin initialization with an error message that names the offending lookup, before any traffic is served.
- **SC-006**: A configuration whose field path does not exist in the database serves traffic without errors and produces a warning log entry that names the lookup and the missing path; the warning rate per unique path is bounded so a misconfigured field cannot dominate the logs under sustained traffic.

## Assumptions

- The current 0.1.0 configuration shape (database `type` enum + global `fields` allowlist) will be replaced by the new alias-and-lookup shape rather than supported in parallel. This is a breaking change, acceptable because the project is pre-1.0 and the existing release has not been promoted into stable downstream pipelines.
- Operators determine valid field paths by consulting MaxMind's documented MMDB schemas (or by running `mmdblookup` against their database file), exactly as they do for the HAProxy Enterprise converter. The plugin does not need to ship a hard-coded catalog of valid paths.
- Locale-aware name extraction (`get_name` and the `locale` config option) goes away as a first-class plugin feature: operators who want a specific localized name configure a lookup whose field path explicitly addresses that locale (e.g. `city.names.de`). This is a deliberate simplification — the new model is strictly more expressive, since any locale can be addressed by path.
- An SPOE message can be bound to one or many lookups. Operators who want several extractions for the same request typically bind several lookups to a single message name so HAProxy issues one round-trip per request; binding several messages remains valid when an operator wants the extractions to be conditionally invoked from different points in the HAProxy config.
- The plugin continues to load databases from local filesystem paths at init time (no hot reload, no remote fetch); flexibility is added to lookup behaviour, not to database lifecycle management.
- Field-path traversal handles only map keys; addressing array elements by index is out of scope for this iteration because none of the seven supported MaxMind schemas require indexed access for the values operators typically extract (subdivisions, the only array, can be exposed via a follow-up if needed).
