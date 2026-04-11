use std::net::IpAddr;

use criterion::{Criterion, criterion_group, criterion_main};
use maxmind_plugin::db_type::DatabaseType;
use maxmind_plugin::lookup::{LoadedDatabase, lookup_ip};
use maxminddb::Reader;

fn load_fixture(name: &str, db_type: DatabaseType) -> LoadedDatabase {
    let path = format!("tests/fixtures/MaxMind-DB/test-data/{name}");
    LoadedDatabase {
        db_type,
        reader: Reader::open_readfile(&path).unwrap(),
    }
}

fn bench_lookups(c: &mut Criterion) {
    let city_db = load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City);
    let asn_db = load_fixture("GeoLite2-ASN-Test.mmdb", DatabaseType::Asn);
    let ip: IpAddr = "89.160.20.128".parse().unwrap();

    c.bench_function("city_lookup", |b| {
        b.iter(|| city_db.db_type.extract_fields(&city_db.reader, ip, "en"));
    });

    c.bench_function("asn_lookup", |b| {
        b.iter(|| asn_db.db_type.extract_fields(&asn_db.reader, ip, "en"));
    });

    let both = vec![
        load_fixture("GeoIP2-City-Test.mmdb", DatabaseType::City),
        load_fixture("GeoLite2-ASN-Test.mmdb", DatabaseType::Asn),
    ];

    c.bench_function("city+asn_lookup", |b| {
        b.iter(|| lookup_ip(ip, &both, "en", None));
    });
}

criterion_group!(benches, bench_lookups);
criterion_main!(benches);
