use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;

use maxmind_plugin::config::{DatabaseEntry, Lookup};
use maxmind_plugin::lookup::{
    CompiledLookup, LoadedDatabases, LookupRouter, LookupStatus, execute_lookup,
};
use maxmind_plugin::warn_log::WarnOnce;

const FIXTURES: &str = "tests/fixtures/MaxMind-DB/test-data";

fn fixture_path(name: &str) -> String {
    Path::new(FIXTURES)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn databases(entries: &[(&str, &str)]) -> LoadedDatabases {
    let mut map = BTreeMap::new();
    for (alias, file) in entries {
        map.insert(
            (*alias).to_string(),
            DatabaseEntry {
                path: fixture_path(file),
            },
        );
    }
    LoadedDatabases::open(&map).unwrap_or_else(|e| panic!("failed to open databases: {e}"))
}

fn lookup(name: &str, db: &str, path: &[&str], output: &str) -> Lookup {
    Lookup {
        name: name.to_string(),
        message: "msg".to_string(),
        database: db.to_string(),
        input: "ip".to_string(),
        path: path.iter().map(|s| s.to_string()).collect(),
        output_var: output.to_string(),
    }
}

fn compile(l: Lookup) -> CompiledLookup {
    let router = LookupRouter::from_lookups(vec![l]);
    // Move the single compiled entry out — easier than re-implementing
    // CompiledLookup construction in the test crate.
    let mut compiled = router.compiled;
    compiled.remove(0)
}

fn warn() -> WarnOnce {
    WarnOnce::new(Duration::from_secs(60))
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

// --- T009: Field-path traversal against the bundled fixtures ---

#[test]
fn extract_country_iso_code_from_city_db_returns_value_and_ok_status() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    let l = compile(lookup(
        "country_code",
        "city",
        &["country", "iso_code"],
        "geo_country",
    ));
    let (value, status) = execute_lookup(&l, &dbs, ip("89.160.20.128"), &warn());
    assert_eq!(value, "SE");
    assert_eq!(status, LookupStatus::Ok);
}

#[test]
fn extract_missing_field_returns_empty_and_not_found_status() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    let l = compile(lookup("noexist", "city", &["nonexistent_key"], "out"));
    let (value, status) = execute_lookup(&l, &dbs, ip("89.160.20.128"), &warn());
    assert!(value.is_empty());
    assert_eq!(status, LookupStatus::NotFound);
}

#[test]
fn extract_non_leaf_field_returns_empty_and_error_status() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    // "country" is a map, not a leaf — decode_path::<AnyLeaf> must reject it.
    let l = compile(lookup("non_leaf", "city", &["country"], "out"));
    let (value, status) = execute_lookup(&l, &dbs, ip("89.160.20.128"), &warn());
    assert!(value.is_empty());
    assert_eq!(status, LookupStatus::Error);
}

#[test]
fn extract_localized_name_via_path_returns_value() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    let l = compile(lookup(
        "country_de",
        "city",
        &["country", "names", "de"],
        "out",
    ));
    let (value, status) = execute_lookup(&l, &dbs, ip("89.160.20.128"), &warn());
    assert_eq!(value, "Schweden");
    assert_eq!(status, LookupStatus::Ok);
}

#[test]
fn extract_boolean_field_renders_true_or_false() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    // 89.160.20.128 → Sweden, in EU → is_in_european_union = true
    let l = compile(lookup(
        "in_eu",
        "city",
        &["country", "is_in_european_union"],
        "out",
    ));
    let (value, status) = execute_lookup(&l, &dbs, ip("89.160.20.128"), &warn());
    assert_eq!(value, "true");
    assert_eq!(status, LookupStatus::Ok);
}

#[test]
fn extract_numeric_field_renders_canonical_decimal() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    let l = compile(lookup("lat", "city", &["location", "latitude"], "out"));
    let (value, status) = execute_lookup(&l, &dbs, ip("89.160.20.128"), &warn());
    assert_eq!(status, LookupStatus::Ok);
    let parsed: f64 = value.parse().expect("latitude should be a parseable float");
    assert!(parsed.is_finite());
}

#[test]
fn extract_asn_number_returns_unsigned_integer_string() {
    let dbs = databases(&[("asn", "GeoLite2-ASN-Test.mmdb")]);
    let l = compile(lookup(
        "as_number",
        "asn",
        &["autonomous_system_number"],
        "out",
    ));
    let (value, status) = execute_lookup(&l, &dbs, ip("1.128.0.0"), &warn());
    assert_eq!(status, LookupStatus::Ok);
    let _: u64 = value
        .parse()
        .unwrap_or_else(|e| panic!("ASN should parse as u64: {e}, got '{value}'"));
}

#[test]
fn lookup_for_ip_not_in_database_returns_empty_and_not_found_status() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    let l = compile(lookup(
        "country_code",
        "city",
        &["country", "iso_code"],
        "out",
    ));
    let (value, status) = execute_lookup(&l, &dbs, ip("127.0.0.1"), &warn());
    assert!(value.is_empty());
    assert_eq!(status, LookupStatus::NotFound);
}

// --- T010: Multi-lookup against the same database loads the file once ---

#[test]
fn multi_lookup_against_same_database_loads_file_once() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    assert_eq!(dbs.readers.len(), 1);

    // Build a router with two lookups against the same alias and verify the
    // database map is still single-entry (the router does not reopen files).
    let lookups = vec![
        lookup("a", "city", &["country", "iso_code"], "out_a"),
        lookup("b", "city", &["city", "names", "en"], "out_b"),
    ];
    let router = LookupRouter::from_lookups(lookups);
    assert_eq!(router.compiled.len(), 2);
    assert_eq!(dbs.readers.len(), 1);
}

#[test]
fn ipv6_lookup_returns_record() {
    let dbs = databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    let l = compile(lookup(
        "country_code",
        "city",
        &["country", "iso_code"],
        "out",
    ));
    let (value, status) = execute_lookup(&l, &dbs, ip("2001:218::"), &warn());
    assert_eq!(value, "JP");
    assert_eq!(status, LookupStatus::Ok);
}
