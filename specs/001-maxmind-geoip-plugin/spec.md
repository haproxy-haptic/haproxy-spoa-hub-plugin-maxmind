# Feature Specification: MaxMind GeoIP SPOA Plugin

**Feature Branch**: `001-maxmind-geoip-plugin`
**Created**: 2026-04-09
**Status**: Draft
**Input**: User description: "Implement a haproxy-spoa-hub plugin for MaxMind databases with equivalent feature set and API to the HAProxy Enterprise MaxMind module"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - GeoIP Lookup by Client IP (Priority: P1)

An operator configures HAProxy with an SPOE filter that sends the client
source IP to the SPOA hub. The MaxMind plugin receives the IP, looks it
up in one or more configured MMDB databases, and returns geolocation
variables (country, city, coordinates, ASN, etc.) that HAProxy can use
in routing decisions, headers, or logging.

**Why this priority**: This is the core value proposition of the plugin.
Without IP-based GeoIP lookups, no other feature is useful.

**Independent Test**: Send an SPOE message with a known IP address and
verify the returned variables match expected MaxMind data for that IP.

**Acceptance Scenarios**:

1. **Given** a configured City database and a known US IP,
   **When** the plugin receives a geoip-lookup message with that IP,
   **Then** it returns country_iso_code="US", city name, latitude,
   longitude, and timezone variables.

2. **Given** a configured ASN database and a known IP,
   **When** the plugin receives a geoip-lookup message,
   **Then** it returns autonomous_system_number and
   autonomous_system_organization variables.

3. **Given** multiple databases configured (City + ASN),
   **When** the plugin receives a geoip-lookup message,
   **Then** it returns variables from all configured databases in a
   single response.

4. **Given** a private/unroutable IP (e.g., 127.0.0.1),
   **When** the plugin receives a geoip-lookup message,
   **Then** it returns empty string values for all fields and does not
   error.

5. **Given** an IPv6 address,
   **When** the plugin receives a geoip-lookup message,
   **Then** it performs the lookup correctly and returns matching data.

---

### User Story 2 - Flexible Database Configuration (Priority: P2)

An operator configures one or more MaxMind databases by path and type
keyword in the hub TOML configuration. The plugin loads all specified
databases at startup, validates they exist and are readable, and uses
them for lookups. The operator can choose which database types to load
based on their licensing and use case.

**Why this priority**: Operators have different MaxMind licenses
(free GeoLite2 vs commercial GeoIP2) and different needs (some only
want country, others want full city + ASN + ISP). Flexible database
configuration is essential for real-world deployment.

**Independent Test**: Configure the plugin with various database
combinations and verify it loads successfully and returns appropriate
variables for each.

**Acceptance Scenarios**:

1. **Given** a hub config with a single City database entry,
   **When** the plugin initializes,
   **Then** it loads the City database and serves city-related variables.

2. **Given** a hub config with multiple databases (City, ASN, Country),
   **When** the plugin initializes,
   **Then** all databases are loaded and variables from each are returned.

3. **Given** a configured database path that does not exist,
   **When** the plugin initializes,
   **Then** it fails with a clear error message naming the missing file.

4. **Given** a configured database path pointing to a corrupt file,
   **When** the plugin initializes,
   **Then** it fails with a clear error identifying the file and problem.

---

### User Story 3 - Configurable Output Fields and Locale (Priority: P3)

An operator specifies which fields to extract from the database and
which locale to use for name fields. This allows reducing the number
of variables returned (performance) and localizing city/country names
to the operator's preferred language.

**Why this priority**: Reduces unnecessary variable overhead and
supports internationalized deployments, but the plugin is fully
functional without this (defaulting to all available fields in English).

**Independent Test**: Configure the plugin with a restricted field
list and a non-English locale, then verify only the specified fields
are returned and names are in the requested language.

**Acceptance Scenarios**:

1. **Given** a config with `locale = "de"` and a City database,
   **When** looking up a known German IP,
   **Then** city and country names are returned in German.

2. **Given** a config with a field allowlist of only country_iso_code
   and asn,
   **When** looking up an IP,
   **Then** only country_iso_code and asn variables are returned (no
   city, coordinates, etc.).

3. **Given** a locale for which a name translation does not exist,
   **When** looking up an IP,
   **Then** the plugin falls back to the English name.

---

### Edge Cases

- What happens when the MMDB file is replaced on disk while the plugin
  is running? The plugin continues using the in-memory reader; a reload
  mechanism is out of scope for v1 but the design MUST NOT prevent
  adding it later.
- What happens when the IP argument is missing from the SPOE message?
  The plugin returns an error result indicating the missing argument.
- What happens when the IP argument is a string (e.g., from
  X-Forwarded-For) instead of a native IPv4/IPv6 type? The plugin
  MUST parse string-formatted IPs in addition to native SPOP IP types.
- What happens with an IP that exists in one configured database but
  not another? Fields from the database without a match are returned
  as empty strings; fields from the matching database are populated
  normally.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The plugin MUST implement the `SpoePlugin` trait from
  `haproxy-spoa-hub-plugin-api` using the `define_plugin!` macro.
- **FR-002**: The plugin MUST accept an IP address argument from the
  SPOE message (as IPv4, IPv6, or string-formatted IP).
- **FR-003**: The plugin MUST support loading one or more MaxMind MMDB
  databases (GeoLite2-City, GeoLite2-Country, GeoLite2-ASN, and their
  commercial GeoIP2 equivalents).
- **FR-004**: The plugin MUST return geolocation variables scoped to
  the transaction (`VarScope::Transaction`).
- **FR-005**: For City/Country databases, the plugin MUST return at
  minimum: `country_iso_code`, `country_name`, `continent_code`,
  `continent_name`.
- **FR-006**: For City databases, the plugin MUST additionally return:
  `city_name`, `latitude`, `longitude`, `postal_code`, `timezone`,
  `accuracy_radius`, `subdivision_iso_code`, `subdivision_name`,
  `is_in_european_union`.
- **FR-007**: For ASN databases, the plugin MUST return:
  `autonomous_system_number`, `autonomous_system_organization`.
- **FR-008**: For Anonymous IP databases, the plugin MUST return:
  `is_anonymous`, `is_anonymous_vpn`, `is_hosting_provider`,
  `is_public_proxy`, `is_residential_proxy`, `is_tor_exit_node`.
- **FR-009**: For ISP databases, the plugin MUST return: `isp`,
  `organization` (in addition to ASN fields).
- **FR-010**: For Connection Type databases, the plugin MUST return:
  `connection_type`.
- **FR-011**: For Domain databases, the plugin MUST return: `domain`.
- **FR-012**: The plugin MUST support a configurable locale for name
  fields (city, country, continent, subdivision names), defaulting to
  `"en"`.
- **FR-013**: The plugin MUST support an optional field allowlist to
  restrict which variables are returned.
- **FR-014**: The plugin MUST load MMDB databases into memory at
  initialization and use an in-memory reader for all lookups (no
  per-request disk I/O).
- **FR-015**: The plugin MUST return empty string values (not errors)
  when an IP has no match in a database.
- **FR-016**: The plugin MUST compile as `cdylib` (and `rlib` for
  testing) compatible with haproxy-spoa-hub's plugin loading.
- **FR-017**: The plugin MUST provide a JSON Schema for its
  configuration via `config_schema()`.
- **FR-018**: The plugin MUST fail initialization with a descriptive
  error if any configured database path is missing or unreadable.

### Key Entities

- **Database**: Represents a loaded MaxMind MMDB file with a type
  keyword (city, country, asn, anonymous, isp, conntype, domain)
  and an in-memory reader instance.
- **LookupResult**: The collection of extracted fields from one or
  more database lookups for a single IP address, converted to
  transaction variable entries.
- **PluginConfig**: The parsed configuration containing database
  paths/types, locale preference, and optional field allowlist.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The plugin correctly resolves known IP addresses against
  all supported database types, matching the reference data from
  MaxMind's test databases.
- **SC-002**: Lookup latency per SPOE message remains below 1 ms at
  p99 for single-database configurations under sustained load.
- **SC-003**: The plugin loads and initializes within 5 seconds for
  databases up to 100 MB in size.
- **SC-004**: Memory usage after initialization grows by no more than
  the size of the loaded MMDB files plus 10% overhead.
- **SC-005**: Operators can configure and deploy the plugin using only
  the hub TOML configuration file, without modifying plugin source
  code.
- **SC-006**: All supported database types (City, Country, ASN,
  Anonymous, ISP, Connection Type, Domain) produce correct output
  verified against known test data.

## Assumptions

- Operators provide their own MaxMind MMDB database files (either
  free GeoLite2 or commercial GeoIP2). The plugin does not download
  or update databases.
- The plugin targets the same Rust edition (2024), MSRV (1.86), and
  `haproxy-spoa-hub-plugin-api` version (0.1) as the fingerprinting
  plugin.
- The `maxmind-db` crate (or equivalent pure-Rust MMDB reader) is
  available and suitable for in-memory lookups without C dependencies.
- Database hot-reload (replacing the MMDB file while the plugin is
  running and picking up changes without restart) is out of scope for
  this initial version.
- The repository structure, Cargo.toml profile settings (LTO, single
  codegen unit), and CI patterns mirror the fingerprinting plugin.
- The SPOE message name defaults to `geoip-lookup` and the IP
  argument name defaults to `ip`, both matching HAProxy Enterprise
  conventions.
