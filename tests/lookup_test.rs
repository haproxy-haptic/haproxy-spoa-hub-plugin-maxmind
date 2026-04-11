use std::net::IpAddr;
use std::path::Path;

use maxmind_plugin::db_type::DatabaseType;
use maxmind_plugin::lookup::{LoadedDatabase, lookup_ip};
use maxminddb::Reader;

const FIXTURES: &str = "tests/fixtures/MaxMind-DB/test-data";

fn fixture_path(name: &str) -> String {
    Path::new(FIXTURES)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn load_fixture(name: &str, db_type: DatabaseType) -> LoadedDatabase {
    let path = fixture_path(name);
    LoadedDatabase {
        db_type,
        reader: Reader::open_readfile(&path)
            .unwrap_or_else(|e| panic!("failed to open {path}: {e}")),
    }
}

fn find_field<'a>(fields: &'a [(String, String)], name: &str) -> &'a str {
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("field '{name}' not found in results"))
}

// --- T009: City database lookup with known IP ---

#[test]
fn city_lookup_known_ip_returns_expected_fields() {
    let db = load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City);
    let ip: IpAddr = "89.160.20.128".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    assert_eq!(find_field(&fields, "country_iso_code"), "SE");
    assert_eq!(find_field(&fields, "country_name"), "Sweden");
    assert_eq!(find_field(&fields, "continent_code"), "EU");
    assert_eq!(find_field(&fields, "continent_name"), "Europe");
    assert!(!find_field(&fields, "city_name").is_empty());
    assert!(!find_field(&fields, "latitude").is_empty());
    assert!(!find_field(&fields, "longitude").is_empty());
    assert!(!find_field(&fields, "timezone").is_empty());
}

// --- T010: ASN database lookup ---

#[test]
fn asn_lookup_known_ip_returns_expected_fields() {
    let db = load_fixture("GeoLite2-ASN-Test.mmdb", DatabaseType::Asn);
    let ip: IpAddr = "1.128.0.0".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    let asn = find_field(&fields, "autonomous_system_number");
    assert!(!asn.is_empty(), "ASN should not be empty");
    let asn_num: u32 = asn.parse().expect("ASN should be a number");
    assert!(asn_num > 0);

    let org = find_field(&fields, "autonomous_system_organization");
    assert!(!org.is_empty(), "ASN organization should not be empty");
}

// --- T011: Private IP returns empty strings ---

#[test]
fn city_lookup_private_ip_returns_empty_fields() {
    let db = load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City);
    let ip: IpAddr = "127.0.0.1".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    for (name, value) in &fields {
        assert!(
            value.is_empty(),
            "field '{name}' should be empty for private IP, got '{value}'"
        );
    }
}

// --- T012: IPv6 lookup ---

#[test]
fn city_lookup_ipv6_returns_correct_data() {
    let db = load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City);
    let ip: IpAddr = "2001:218::".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    let country = find_field(&fields, "country_iso_code");
    assert_eq!(country, "JP");
}

// --- T013: Multiple databases return variables from both ---

#[test]
fn lookup_ip_multiple_databases_returns_combined_fields() {
    let dbs = vec![
        load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City),
        load_fixture("GeoLite2-ASN-Test.mmdb", DatabaseType::Asn),
    ];
    let ip: IpAddr = "89.160.20.128".parse().unwrap();
    let fields = lookup_ip(ip, &dbs, "en", None);

    // Should have city fields
    assert!(fields.iter().any(|(n, _)| n == "country_iso_code"));
    assert!(fields.iter().any(|(n, _)| n == "city_name"));
    // Should have ASN fields
    assert!(fields.iter().any(|(n, _)| n == "autonomous_system_number"));
}

// --- T030: Additional database type tests ---

#[test]
fn country_lookup_returns_country_fields() {
    let db = load_fixture("GeoIP2-Country-Test.mmdb", DatabaseType::Country);
    let ip: IpAddr = "89.160.20.128".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    assert_eq!(find_field(&fields, "country_iso_code"), "SE");
    assert_eq!(find_field(&fields, "continent_code"), "EU");
    // Country DB should not have city fields
    assert!(fields.iter().all(|(n, _)| n != "city_name"));
}

#[test]
fn anonymous_ip_lookup_returns_boolean_fields() {
    let db = load_fixture("GeoIP2-Anonymous-IP-Test.mmdb", DatabaseType::Anonymous);
    let ip: IpAddr = "1.2.0.1".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    let anon = find_field(&fields, "is_anonymous");
    assert!(anon == "true" || anon == "false" || anon.is_empty());
}

#[test]
fn isp_lookup_returns_provider_fields() {
    let db = load_fixture("GeoIP2-ISP-Test.mmdb", DatabaseType::Isp);
    let ip: IpAddr = "1.128.0.0".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    assert!(fields.iter().any(|(n, _)| n == "isp"));
    assert!(fields.iter().any(|(n, _)| n == "organization"));
    assert!(fields.iter().any(|(n, _)| n == "autonomous_system_number"));
}

#[test]
fn connection_type_lookup_returns_type_field() {
    let db = load_fixture("GeoIP2-Connection-Type-Test.mmdb", DatabaseType::ConnType);
    let ip: IpAddr = "1.0.1.0".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    assert!(fields.iter().any(|(n, _)| n == "connection_type"));
}

#[test]
fn domain_lookup_returns_domain_field() {
    let db = load_fixture("GeoIP2-Domain-Test.mmdb", DatabaseType::Domain);
    let ip: IpAddr = "1.2.0.0".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "en");

    assert!(fields.iter().any(|(n, _)| n == "domain"));
}

// --- T031-T034: Locale and field filtering ---

#[test]
fn city_lookup_locale_de_returns_german_names() {
    let db = load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City);
    let ip: IpAddr = "89.160.20.128".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "de");

    let country_name = find_field(&fields, "country_name");
    assert_eq!(country_name, "Schweden");
}

#[test]
fn city_lookup_unknown_locale_falls_back_to_english() {
    let db = load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City);
    let ip: IpAddr = "89.160.20.128".parse().unwrap();
    let fields = db.db_type.extract_fields(&db.reader, ip, "xx");

    let country_name = find_field(&fields, "country_name");
    assert_eq!(country_name, "Sweden");
}

#[test]
fn field_allowlist_filters_output() {
    let dbs = vec![
        load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City),
        load_fixture("GeoLite2-ASN-Test.mmdb", DatabaseType::Asn),
    ];
    let ip: IpAddr = "89.160.20.128".parse().unwrap();
    let allowed = vec![
        "country_iso_code".to_string(),
        "autonomous_system_number".to_string(),
    ];
    let fields = lookup_ip(ip, &dbs, "en", Some(&allowed));

    assert_eq!(fields.len(), 2);
    assert!(fields.iter().any(|(n, _)| n == "country_iso_code"));
    assert!(fields.iter().any(|(n, _)| n == "autonomous_system_number"));
}

#[test]
fn default_config_returns_all_fields_in_english() {
    let dbs = vec![load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City)];
    let ip: IpAddr = "89.160.20.128".parse().unwrap();
    let fields = lookup_ip(ip, &dbs, "en", None);

    // City DB should return 13 fields
    assert_eq!(fields.len(), 13);
    assert_eq!(find_field(&fields, "country_name"), "Sweden");
}
