use std::net::IpAddr;

use abi_stable::std_types::Tuple2;
use haproxy_spoa_hub_plugin_api::{SpoeMessage, SpoeValue};

/// Extract the IP address from a geoip-lookup SPOE message.
pub fn extract_ip(
    message: &SpoeMessage,
) -> Result<IpAddr, Box<dyn std::error::Error + Send + Sync>> {
    for Tuple2(k, v) in message.args.iter() {
        if k.as_str() == "ip" {
            return spoe_value_to_ip(v);
        }
    }
    Err("missing required 'ip' argument in geoip-lookup message".into())
}

/// Convert a SPOP typed value to an IpAddr.
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
            "expected IPv4, IPv6, or String for 'ip' argument, got {:?}",
            std::mem::discriminant(other)
        )
        .into()),
    }
}
