use std::net::IpAddr;

use maxminddb::Reader;
use maxminddb::geoip2;

/// Supported MaxMind database types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseType {
    City,
    Country,
    Asn,
    Anonymous,
    Isp,
    ConnType,
    Domain,
}

impl DatabaseType {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "city" => Some(Self::City),
            "country" => Some(Self::Country),
            "asn" => Some(Self::Asn),
            "anonymous" => Some(Self::Anonymous),
            "isp" => Some(Self::Isp),
            "conntype" => Some(Self::ConnType),
            "domain" => Some(Self::Domain),
            _ => None,
        }
    }

    /// Extract fields from a database lookup for the given IP.
    /// Returns a list of (variable_name, value) pairs.
    /// On lookup miss, returns empty strings for all fields.
    pub fn extract_fields(
        &self,
        reader: &Reader<Vec<u8>>,
        ip: IpAddr,
        locale: &str,
    ) -> Vec<(String, String)> {
        match self {
            Self::City => extract_city(reader, ip, locale),
            Self::Country => extract_country(reader, ip, locale),
            Self::Asn => extract_asn(reader, ip),
            Self::Anonymous => extract_anonymous(reader, ip),
            Self::Isp => extract_isp(reader, ip),
            Self::ConnType => extract_conntype(reader, ip),
            Self::Domain => extract_domain(reader, ip),
        }
    }
}

fn get_name(names: &geoip2::Names, locale: &str) -> String {
    let name = match locale {
        "de" => names.german,
        "en" => names.english,
        "es" => names.spanish,
        "fr" => names.french,
        "ja" => names.japanese,
        "pt-BR" => names.brazilian_portuguese,
        "ru" => names.russian,
        "zh-CN" => names.simplified_chinese,
        _ => names.english,
    };
    name.or(names.english).unwrap_or("").to_string()
}

/// Build a vec of (name, "") pairs for the given field names.
fn empty_fields(names: &[&str]) -> Vec<(String, String)> {
    names
        .iter()
        .map(|name| ((*name).into(), String::new()))
        .collect()
}

/// Perform lookup and decode, returning None on address-not-found.
fn lookup_decode<'a, T: serde::Deserialize<'a>>(
    reader: &'a Reader<Vec<u8>>,
    ip: IpAddr,
    db_name: &str,
) -> Option<T> {
    match reader.lookup(ip) {
        Ok(result) => match result.decode::<T>() {
            Ok(data) => data,
            Err(e) => {
                log::warn!("{db_name} decode failed for {ip}: {e}");
                None
            }
        },
        Err(e) => {
            log::warn!("{db_name} lookup failed for {ip}: {e}");
            None
        }
    }
}

/// Extract the 5 country/continent fields shared by City and Country databases.
fn country_continent_fields(
    country: &geoip2::country::Country,
    continent: &geoip2::country::Continent,
    locale: &str,
) -> Vec<(String, String)> {
    vec![
        (
            "country_iso_code".into(),
            country.iso_code.unwrap_or("").into(),
        ),
        ("country_name".into(), get_name(&country.names, locale)),
        ("continent_code".into(), continent.code.unwrap_or("").into()),
        ("continent_name".into(), get_name(&continent.names, locale)),
        (
            "is_in_european_union".into(),
            country
                .is_in_european_union
                .map(|b| b.to_string())
                .unwrap_or_default(),
        ),
    ]
}

const COUNTRY_FIELDS: &[&str] = &[
    "country_iso_code",
    "country_name",
    "continent_code",
    "continent_name",
    "is_in_european_union",
];

const CITY_EXTRA_FIELDS: &[&str] = &[
    "city_name",
    "latitude",
    "longitude",
    "postal_code",
    "timezone",
    "accuracy_radius",
    "subdivision_iso_code",
    "subdivision_name",
];

fn extract_city(reader: &Reader<Vec<u8>>, ip: IpAddr, locale: &str) -> Vec<(String, String)> {
    let Some(city) = lookup_decode::<geoip2::City>(reader, ip, "City") else {
        return empty_fields(&[COUNTRY_FIELDS, CITY_EXTRA_FIELDS].concat());
    };

    let subdivision = city.subdivisions.first();

    let mut fields = country_continent_fields(&city.country, &city.continent, locale);
    fields.extend([
        ("city_name".into(), get_name(&city.city.names, locale)),
        (
            "latitude".into(),
            city.location
                .latitude
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ),
        (
            "longitude".into(),
            city.location
                .longitude
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ),
        ("postal_code".into(), city.postal.code.unwrap_or("").into()),
        (
            "timezone".into(),
            city.location.time_zone.unwrap_or("").into(),
        ),
        (
            "accuracy_radius".into(),
            city.location
                .accuracy_radius
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ),
        (
            "subdivision_iso_code".into(),
            subdivision.and_then(|s| s.iso_code).unwrap_or("").into(),
        ),
        (
            "subdivision_name".into(),
            subdivision
                .map(|s| get_name(&s.names, locale))
                .unwrap_or_default(),
        ),
    ]);
    fields
}

fn extract_country(reader: &Reader<Vec<u8>>, ip: IpAddr, locale: &str) -> Vec<(String, String)> {
    let Some(data) = lookup_decode::<geoip2::Country>(reader, ip, "Country") else {
        return empty_fields(COUNTRY_FIELDS);
    };

    country_continent_fields(&data.country, &data.continent, locale)
}

fn extract_asn(reader: &Reader<Vec<u8>>, ip: IpAddr) -> Vec<(String, String)> {
    const FIELDS: &[&str] = &["autonomous_system_number", "autonomous_system_organization"];
    let Some(data) = lookup_decode::<geoip2::Asn>(reader, ip, "ASN") else {
        return empty_fields(FIELDS);
    };

    vec![
        (
            "autonomous_system_number".into(),
            data.autonomous_system_number
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "autonomous_system_organization".into(),
            data.autonomous_system_organization.unwrap_or("").into(),
        ),
    ]
}

fn extract_anonymous(reader: &Reader<Vec<u8>>, ip: IpAddr) -> Vec<(String, String)> {
    const FIELDS: &[&str] = &[
        "is_anonymous",
        "is_anonymous_vpn",
        "is_hosting_provider",
        "is_public_proxy",
        "is_residential_proxy",
        "is_tor_exit_node",
    ];
    let Some(data) = lookup_decode::<geoip2::AnonymousIp>(reader, ip, "Anonymous IP") else {
        return empty_fields(FIELDS);
    };

    vec![
        (
            "is_anonymous".into(),
            data.is_anonymous.map(|b| b.to_string()).unwrap_or_default(),
        ),
        (
            "is_anonymous_vpn".into(),
            data.is_anonymous_vpn
                .map(|b| b.to_string())
                .unwrap_or_default(),
        ),
        (
            "is_hosting_provider".into(),
            data.is_hosting_provider
                .map(|b| b.to_string())
                .unwrap_or_default(),
        ),
        (
            "is_public_proxy".into(),
            data.is_public_proxy
                .map(|b| b.to_string())
                .unwrap_or_default(),
        ),
        (
            "is_residential_proxy".into(),
            data.is_residential_proxy
                .map(|b| b.to_string())
                .unwrap_or_default(),
        ),
        (
            "is_tor_exit_node".into(),
            data.is_tor_exit_node
                .map(|b| b.to_string())
                .unwrap_or_default(),
        ),
    ]
}

fn extract_isp(reader: &Reader<Vec<u8>>, ip: IpAddr) -> Vec<(String, String)> {
    const FIELDS: &[&str] = &[
        "isp",
        "organization",
        "autonomous_system_number",
        "autonomous_system_organization",
    ];
    let Some(data) = lookup_decode::<geoip2::Isp>(reader, ip, "ISP") else {
        return empty_fields(FIELDS);
    };

    vec![
        ("isp".into(), data.isp.unwrap_or("").into()),
        (
            "organization".into(),
            data.organization.unwrap_or("").into(),
        ),
        (
            "autonomous_system_number".into(),
            data.autonomous_system_number
                .map(|n| n.to_string())
                .unwrap_or_default(),
        ),
        (
            "autonomous_system_organization".into(),
            data.autonomous_system_organization.unwrap_or("").into(),
        ),
    ]
}

fn extract_conntype(reader: &Reader<Vec<u8>>, ip: IpAddr) -> Vec<(String, String)> {
    let Some(data) = lookup_decode::<geoip2::ConnectionType>(reader, ip, "Connection type") else {
        return empty_fields(&["connection_type"]);
    };

    vec![(
        "connection_type".into(),
        data.connection_type.unwrap_or("").into(),
    )]
}

fn extract_domain(reader: &Reader<Vec<u8>>, ip: IpAddr) -> Vec<(String, String)> {
    let Some(data) = lookup_decode::<geoip2::Domain>(reader, ip, "Domain") else {
        return empty_fields(&["domain"]);
    };

    vec![("domain".into(), data.domain.unwrap_or("").into())]
}
