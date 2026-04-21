use std::net::Ipv4Addr;
use std::path::Path;
use std::time::Duration;

use haproxy_spoa_hub_plugin_api::{
    ConfigValue, PluginContext, RHashMap, RString, RVec, SpoeMessage, SpoeValue, VarScope,
};

use maxmind_plugin::config::PluginConfig;
use maxmind_plugin::lookup::{LookupRouter, load};
use maxmind_plugin::plugin::process_message;
use maxmind_plugin::warn_log::WarnOnce;

const FIXTURES: &str = "tests/fixtures/MaxMind-DB/test-data";

fn fixture_path(name: &str) -> String {
    Path::new(FIXTURES)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn make_context(params: &[(&str, ConfigValue)]) -> PluginContext {
    let mut config = RHashMap::new();
    for (k, v) in params {
        config.insert(RString::from(*k), v.clone());
    }
    PluginContext {
        name: RString::from("maxmind"),
        config,
    }
}

fn databases_table(entries: &[(&str, &str)]) -> ConfigValue {
    let mut table = RHashMap::new();
    for (alias, path) in entries {
        let mut entry = RHashMap::new();
        entry.insert(
            RString::from("path"),
            ConfigValue::String(RString::from(*path)),
        );
        table.insert(RString::from(*alias), ConfigValue::Table(entry));
    }
    ConfigValue::Table(table)
}

fn lookup_value(
    name: &str,
    message: &str,
    database: &str,
    input: Option<&str>,
    path: &[&str],
    output_var: &str,
) -> ConfigValue {
    let mut t = RHashMap::new();
    t.insert(
        RString::from("name"),
        ConfigValue::String(RString::from(name)),
    );
    t.insert(
        RString::from("message"),
        ConfigValue::String(RString::from(message)),
    );
    t.insert(
        RString::from("database"),
        ConfigValue::String(RString::from(database)),
    );
    if let Some(i) = input {
        t.insert(
            RString::from("input"),
            ConfigValue::String(RString::from(i)),
        );
    }
    let path_arr: RVec<ConfigValue> = path
        .iter()
        .map(|s| ConfigValue::String(RString::from(*s)))
        .collect();
    t.insert(RString::from("path"), ConfigValue::Array(path_arr));
    t.insert(
        RString::from("output_var"),
        ConfigValue::String(RString::from(output_var)),
    );
    ConfigValue::Table(t)
}

fn lookups_array(values: Vec<ConfigValue>) -> ConfigValue {
    ConfigValue::Array(values.into())
}

fn make_message(name: &str, args: &[(&str, SpoeValue)]) -> SpoeMessage {
    let mut a = RHashMap::new();
    for (k, v) in args {
        a.insert(RString::from(*k), v.clone());
    }
    SpoeMessage {
        name: RString::from(name),
        args: a,
        stream_id: 1,
        frame_id: 1,
    }
}

// =============================================================================
// IP extraction (US1 + carry-over from 0.1.0)
// =============================================================================

#[test]
fn extract_ip_from_ipv4_value() {
    let ip_bytes: [u8; 4] = Ipv4Addr::new(89, 160, 20, 128).octets();
    let value = SpoeValue::Ipv4(ip_bytes);
    let ip = maxmind_plugin::ip::spoe_value_to_ip(&value).unwrap();
    assert_eq!(ip.to_string(), "89.160.20.128");
}

#[test]
fn extract_ip_from_ipv6_value() {
    let ip: std::net::Ipv6Addr = "2001:218::".parse().unwrap();
    let value = SpoeValue::Ipv6(ip.octets());
    let result = maxmind_plugin::ip::spoe_value_to_ip(&value).unwrap();
    assert_eq!(result.to_string(), "2001:218::");
}

#[test]
fn extract_ip_from_string_value() {
    let value = SpoeValue::String(RString::from("89.160.20.128"));
    let ip = maxmind_plugin::ip::spoe_value_to_ip(&value).unwrap();
    assert_eq!(ip.to_string(), "89.160.20.128");
}

#[test]
fn extract_ip_from_invalid_string_returns_error() {
    let value = SpoeValue::String(RString::from("not-an-ip"));
    let result = maxmind_plugin::ip::spoe_value_to_ip(&value);
    assert!(result.is_err());
}

#[test]
fn extract_ip_from_unsupported_type_returns_error() {
    let value = SpoeValue::Int32(42);
    let result = maxmind_plugin::ip::spoe_value_to_ip(&value);
    assert!(result.is_err());
}

// =============================================================================
// T008: Config parsing — alias-and-lookup shape
// =============================================================================

#[test]
fn config_with_databases_map_and_lookups_parses_correctly() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "country_code",
                "geoip-enrich",
                "city",
                None,
                &["country", "iso_code"],
                "geo_country",
            )]),
        ),
    ]);
    let config = PluginConfig::from_context(&ctx).unwrap();
    assert_eq!(config.databases.len(), 1);
    assert_eq!(config.lookups.len(), 1);
    assert_eq!(config.lookups[0].input, "input");
    assert_eq!(config.lookups[0].path, vec!["country", "iso_code"]);
}

#[test]
fn config_lookup_input_defaults_to_input() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "msg",
                "city",
                None,
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let config = PluginConfig::from_context(&ctx).unwrap();
    assert_eq!(config.lookups[0].input, "input");
}

#[test]
fn config_lookup_input_can_be_overridden() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "msg",
                "city",
                Some("client_ip"),
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let config = PluginConfig::from_context(&ctx).unwrap();
    assert_eq!(config.lookups[0].input, "client_ip");
}

#[test]
fn config_missing_databases_returns_error() {
    let ctx = make_context(&[(
        "lookups",
        lookups_array(vec![lookup_value(
            "lk",
            "msg",
            "x",
            None,
            &["country"],
            "out",
        )]),
    )]);
    let err = PluginConfig::from_context(&ctx).unwrap_err();
    assert!(
        err.contains("databases"),
        "error should mention 'databases': {err}"
    );
}

#[test]
fn config_missing_lookups_returns_error() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[("databases", databases_table(&[("city", &city)]))]);
    let err = PluginConfig::from_context(&ctx).unwrap_err();
    assert!(
        err.contains("lookups"),
        "error should mention 'lookups': {err}"
    );
}

#[test]
fn config_unknown_database_alias_returns_error() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "country_code",
                "msg",
                "nonexistent",
                None,
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let err = PluginConfig::from_context(&ctx).unwrap_err();
    assert!(err.contains("nonexistent"));
    assert!(err.contains("country_code"));
}

#[test]
fn config_duplicate_output_var_returns_error() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![
                lookup_value("a", "msg", "city", None, &["country", "iso_code"], "dup"),
                lookup_value("b", "msg", "city", None, &["city", "names", "en"], "dup"),
            ]),
        ),
    ]);
    let err = PluginConfig::from_context(&ctx).unwrap_err();
    assert!(err.contains("duplicate output variable"));
    assert!(err.contains("dup"));
}

#[test]
fn config_duplicate_lookup_name_returns_error() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![
                lookup_value("same", "msg", "city", None, &["country", "iso_code"], "a"),
                lookup_value("same", "msg", "city", None, &["country", "iso_code"], "b"),
            ]),
        ),
    ]);
    let err = PluginConfig::from_context(&ctx).unwrap_err();
    assert!(err.contains("duplicate lookup name"));
    assert!(err.contains("same"));
}

#[test]
fn config_legacy_locale_key_returns_migration_error() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        ("locale", ConfigValue::String(RString::from("de"))),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "msg",
                "city",
                None,
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let err = PluginConfig::from_context(&ctx).unwrap_err();
    assert!(err.contains("locale"));
    assert!(err.contains("Migration"));
}

#[test]
fn config_legacy_type_key_returns_migration_error() {
    // 0.1.0-style: databases as array of {type, path}
    let mut entry = RHashMap::new();
    entry.insert(
        RString::from("type"),
        ConfigValue::String(RString::from("city")),
    );
    entry.insert(
        RString::from("path"),
        ConfigValue::String(RString::from(fixture_path("GeoIP2-City-Test.mmdb"))),
    );
    let arr: RVec<ConfigValue> = vec![ConfigValue::Table(entry)].into();
    let ctx = make_context(&[
        ("databases", ConfigValue::Array(arr)),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "msg",
                "city",
                None,
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let err = PluginConfig::from_context(&ctx).unwrap_err();
    assert!(err.contains("Migration") || err.contains("table"));
}

#[test]
fn config_schema_is_valid_json() {
    let schema = PluginConfig::config_schema();
    let parsed: serde_json::Value =
        serde_json::from_str(schema).expect("config_schema() must return valid JSON");
    assert_eq!(parsed["type"], "object");
    assert!(parsed["properties"]["databases"].is_object());
    assert!(parsed["properties"]["lookups"].is_object());
}

// =============================================================================
// US1: end-to-end via process_message
// =============================================================================

fn build_state(
    ctx: &PluginContext,
) -> (
    maxmind_plugin::lookup::LoadedDatabases,
    LookupRouter,
    WarnOnce,
) {
    let config = PluginConfig::from_context(ctx).unwrap();
    let (dbs, router) = load(config).unwrap();
    (dbs, router, WarnOnce::new(Duration::from_secs(60)))
}

fn assert_var(vars: &[haproxy_spoa_hub_plugin_api::TxnVariable], name: &str, expected: &str) {
    let v = vars
        .iter()
        .find(|v| v.name.as_str() == name)
        .unwrap_or_else(|| panic!("variable '{name}' not found"));
    assert_eq!(v.scope, VarScope::Transaction);
    let SpoeValue::String(s) = &v.value else {
        panic!("variable '{name}' is not a string");
    };
    assert_eq!(s.as_str(), expected, "variable '{name}'");
}

#[test]
fn process_message_emits_primary_and_status_variables() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "country_code",
                "geoip-enrich",
                "city",
                None,
                &["country", "iso_code"],
                "geo_country",
            )]),
        ),
    ]);
    let (dbs, router, warn) = build_state(&ctx);
    let msg = make_message(
        "geoip-enrich",
        &[("input", SpoeValue::Ipv4([89, 160, 20, 128]))],
    );
    let vars = process_message(&dbs, &router, &warn, &msg).unwrap();
    assert_eq!(vars.len(), 2);
    assert_var(&vars, "geo_country", "SE");
    assert_var(&vars, "geo_country_status", "ok");
}

#[test]
fn process_message_for_unknown_message_returns_error() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "geoip-enrich",
                "city",
                None,
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let (dbs, router, warn) = build_state(&ctx);
    let msg = make_message(
        "something-else",
        &[("input", SpoeValue::Ipv4([1, 1, 1, 1]))],
    );
    let err = process_message(&dbs, &router, &warn, &msg).unwrap_err();
    assert!(err.contains("something-else"));
}

#[test]
fn process_message_for_private_ip_emits_not_found_status() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "msg",
                "city",
                None,
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let (dbs, router, warn) = build_state(&ctx);
    let msg = make_message("msg", &[("input", SpoeValue::Ipv4([127, 0, 0, 1]))]);
    let vars = process_message(&dbs, &router, &warn, &msg).unwrap();
    assert_var(&vars, "out", "");
    assert_var(&vars, "out_status", "not_found");
}

#[test]
fn process_message_for_invalid_ip_string_emits_error_status() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "msg",
                "city",
                None,
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let (dbs, router, warn) = build_state(&ctx);
    let msg = make_message(
        "msg",
        &[("input", SpoeValue::String(RString::from("not-an-ip")))],
    );
    let vars = process_message(&dbs, &router, &warn, &msg).unwrap();
    assert_var(&vars, "out", "");
    assert_var(&vars, "out_status", "error");
}

#[test]
fn process_message_with_missing_input_arg_returns_hub_error() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "msg",
                "city",
                Some("client_ip"),
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let (dbs, router, warn) = build_state(&ctx);
    // Send a message that's missing the configured input arg.
    let msg = make_message("msg", &[("other", SpoeValue::Ipv4([1, 1, 1, 1]))]);
    let err = process_message(&dbs, &router, &warn, &msg).unwrap_err();
    assert!(err.contains("lk"));
    assert!(err.contains("client_ip"));
}

// =============================================================================
// US2: configurable input arg name
// =============================================================================

#[test]
fn lookup_reads_custom_input_arg_name() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", databases_table(&[("city", &city)])),
        (
            "lookups",
            lookups_array(vec![lookup_value(
                "lk",
                "msg",
                "city",
                Some("client_ip"),
                &["country", "iso_code"],
                "out",
            )]),
        ),
    ]);
    let (dbs, router, warn) = build_state(&ctx);
    let msg = make_message(
        "msg",
        &[
            ("input", SpoeValue::Ipv4([127, 0, 0, 1])),
            ("client_ip", SpoeValue::Ipv4([89, 160, 20, 128])),
        ],
    );
    let vars = process_message(&dbs, &router, &warn, &msg).unwrap();
    // Must use client_ip, not ip — so the result is SE, not empty.
    assert_var(&vars, "out", "SE");
    assert_var(&vars, "out_status", "ok");
}

// =============================================================================
// US3: multiple lookups per message
// =============================================================================

#[test]
fn multi_lookup_per_message_executes_all_and_returns_union() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let asn = fixture_path("GeoLite2-ASN-Test.mmdb");
    let ctx = make_context(&[
        (
            "databases",
            databases_table(&[("city", &city), ("asn", &asn)]),
        ),
        (
            "lookups",
            lookups_array(vec![
                lookup_value(
                    "country",
                    "geoip-enrich",
                    "city",
                    None,
                    &["country", "iso_code"],
                    "geo_country",
                ),
                lookup_value(
                    "as_number",
                    "geoip-enrich",
                    "asn",
                    None,
                    &["autonomous_system_number"],
                    "geo_asn",
                ),
            ]),
        ),
    ]);
    let (dbs, router, warn) = build_state(&ctx);
    let msg = make_message(
        "geoip-enrich",
        &[("input", SpoeValue::Ipv4([89, 160, 20, 128]))],
    );
    let vars = process_message(&dbs, &router, &warn, &msg).unwrap();
    assert_eq!(
        vars.len(),
        4,
        "expected primary + status for each of 2 lookups"
    );
    assert_var(&vars, "geo_country", "SE");
    assert_var(&vars, "geo_country_status", "ok");
    assert_var(&vars, "geo_asn_status", "ok");
}

#[test]
fn removing_a_lookup_removes_only_its_variables() {
    let city = fixture_path("GeoIP2-City-Test.mmdb");
    let asn = fixture_path("GeoLite2-ASN-Test.mmdb");

    let three_lookups = lookups_array(vec![
        lookup_value("a", "m", "city", None, &["country", "iso_code"], "var_a"),
        lookup_value("b", "m", "city", None, &["city", "names", "en"], "var_b"),
        lookup_value(
            "c",
            "m",
            "asn",
            None,
            &["autonomous_system_number"],
            "var_c",
        ),
    ]);
    let two_lookups = lookups_array(vec![
        lookup_value("a", "m", "city", None, &["country", "iso_code"], "var_a"),
        lookup_value(
            "c",
            "m",
            "asn",
            None,
            &["autonomous_system_number"],
            "var_c",
        ),
    ]);

    let dbs_table = databases_table(&[("city", &city), ("asn", &asn)]);
    let msg = make_message("m", &[("input", SpoeValue::Ipv4([89, 160, 20, 128]))]);

    let ctx3 = make_context(&[("databases", dbs_table.clone()), ("lookups", three_lookups)]);
    let (dbs3, r3, w3) = build_state(&ctx3);
    let vars3 = process_message(&dbs3, &r3, &w3, &msg).unwrap();
    assert_eq!(vars3.len(), 6);

    let ctx2 = make_context(&[("databases", dbs_table), ("lookups", two_lookups)]);
    let (dbs2, r2, w2) = build_state(&ctx2);
    let vars2 = process_message(&dbs2, &r2, &w2, &msg).unwrap();
    assert_eq!(vars2.len(), 4);
    assert!(vars2.iter().all(|v| v.name.as_str() != "var_b"));
    assert!(vars2.iter().all(|v| v.name.as_str() != "var_b_status"));
}
