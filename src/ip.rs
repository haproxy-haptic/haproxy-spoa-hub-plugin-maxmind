use std::net::IpAddr;

use haproxy_spoa_hub_plugin_api::SpoeValue;

/// Convert an SPOE typed argument value to an `IpAddr`.
///
/// Accepts the three types HAProxy can send for an IP-shaped sample
/// expression: native IPv4, native IPv6, and a string that parses as either.
/// Any other SPOP type, or a string that fails to parse, returns an error.
pub fn spoe_value_to_ip(
    value: &SpoeValue,
) -> Result<IpAddr, Box<dyn std::error::Error + Send + Sync>> {
    match value {
        SpoeValue::Ipv4(bytes) => Ok(IpAddr::V4((*bytes).into())),
        SpoeValue::Ipv6(bytes) => Ok(IpAddr::V6((*bytes).into())),
        SpoeValue::String(s) => s
            .as_str()
            .parse::<IpAddr>()
            .map_err(|e| format!("invalid IP string '{}': {e}", s.as_str()).into()),
        other => Err(format!(
            "expected IPv4, IPv6, or String for IP argument, got {:?}",
            std::mem::discriminant(other)
        )
        .into()),
    }
}
