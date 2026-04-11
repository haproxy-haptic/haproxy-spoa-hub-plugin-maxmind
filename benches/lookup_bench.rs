use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

use abi_stable::std_types::{RHashMap, RString};
use criterion::{Criterion, criterion_group, criterion_main};
use haproxy_spoa_hub_plugin_api::{SpoeMessage, SpoeValue};
use maxmind_plugin::config::{DatabaseEntry, Lookup};
use maxmind_plugin::lookup::{CompiledLookup, LoadedDatabases, LookupRouter, execute_lookup};
use maxmind_plugin::plugin::process_message;
use maxmind_plugin::warn_log::WarnOnce;

fn fixture(name: &str) -> String {
    format!("tests/fixtures/MaxMind-DB/test-data/{name}")
}

fn open_databases(entries: &[(&str, &str)]) -> LoadedDatabases {
    let mut map = BTreeMap::new();
    for (alias, file) in entries {
        map.insert(
            (*alias).to_string(),
            DatabaseEntry {
                path: fixture(file),
            },
        );
    }
    LoadedDatabases::open(&map).unwrap()
}

fn lookup(name: &str, db: &str, message: &str, path: &[&str], output: &str) -> Lookup {
    Lookup {
        name: name.to_string(),
        message: message.to_string(),
        database: db.to_string(),
        input: "ip".to_string(),
        path: path.iter().map(|s| s.to_string()).collect(),
        output_var: output.to_string(),
    }
}

fn compile_one(l: Lookup) -> CompiledLookup {
    let mut compiled = LookupRouter::from_lookups(vec![l]).compiled;
    compiled.remove(0)
}

fn make_message(name: &str, ip: [u8; 4]) -> SpoeMessage {
    let mut args = RHashMap::new();
    args.insert(RString::from("ip"), SpoeValue::Ipv4(ip));
    SpoeMessage {
        name: RString::from(name),
        args,
        stream_id: 1,
        frame_id: 1,
    }
}

fn bench_lookups(c: &mut Criterion) {
    let warn = WarnOnce::new(Duration::from_secs(60));
    let ip: IpAddr = "89.160.20.128".parse().unwrap();

    // Single lookup against the City fixture — the dominant per-request cost.
    let city_dbs = open_databases(&[("city", "GeoIP2-City-Test.mmdb")]);
    let country_lookup = compile_one(lookup(
        "country_code",
        "city",
        "msg",
        &["country", "iso_code"],
        "geo_country",
    ));
    c.bench_function("single_lookup_country_iso_code", |b| {
        b.iter(|| execute_lookup(&country_lookup, &city_dbs, ip, &warn));
    });

    // Single deep-path lookup (4 nesting levels).
    let deep_lookup = compile_one(lookup(
        "country_de",
        "city",
        "msg",
        &["country", "names", "de"],
        "geo_country_de",
    ));
    c.bench_function("single_lookup_localized_name_path", |b| {
        b.iter(|| execute_lookup(&deep_lookup, &city_dbs, ip, &warn));
    });

    // ASN single-key lookup.
    let asn_dbs = open_databases(&[("asn", "GeoLite2-ASN-Test.mmdb")]);
    let asn_lookup = compile_one(lookup(
        "as_number",
        "asn",
        "msg",
        &["autonomous_system_number"],
        "geo_asn",
    ));
    c.bench_function("single_lookup_asn_number", |b| {
        b.iter(|| execute_lookup(&asn_lookup, &asn_dbs, ip, &warn));
    });

    // Three lookups bound to the same SPOE message — measures the union path
    // through process_message, including arg parsing and TxnVariable construction.
    let combined_dbs = open_databases(&[
        ("city", "GeoIP2-City-Test.mmdb"),
        ("asn", "GeoLite2-ASN-Test.mmdb"),
    ]);
    let combined_router = LookupRouter::from_lookups(vec![
        lookup(
            "country_code",
            "city",
            "geoip-enrich",
            &["country", "iso_code"],
            "geo_country",
        ),
        lookup(
            "city_name",
            "city",
            "geoip-enrich",
            &["city", "names", "en"],
            "geo_city",
        ),
        lookup(
            "as_number",
            "asn",
            "geoip-enrich",
            &["autonomous_system_number"],
            "geo_asn",
        ),
    ]);
    let msg = make_message("geoip-enrich", [89, 160, 20, 128]);
    c.bench_function("three_lookups_one_message", |b| {
        b.iter(|| process_message(&combined_dbs, &combined_router, &warn, &msg));
    });
}

criterion_group!(benches, bench_lookups);
criterion_main!(benches);
