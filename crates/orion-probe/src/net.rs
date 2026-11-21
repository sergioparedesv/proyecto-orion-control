//! Utilidades de direcciones e identificador de red.

use std::net::Ipv4Addr;

use sha2::{Digest, Sha256};

/// IP pública "de verdad": excluye RFC1918, CGNAT (100.64.0.0/10), loopback, link-local,
/// documentación, multicast, broadcast y no especificada.
pub fn is_public(ip: Ipv4Addr) -> bool {
    let o = ip.octets();
    let cgnat = o[0] == 100 && (o[1] & 0b1100_0000) == 64;
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_documentation()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_unspecified()
        || cgnat
        || o[0] == 0
        || o[0] >= 240)
}

/// Hash corto y estable (hex) para no guardar identificadores en claro (p. ej. el SSID).
pub fn short_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Identificador de red estable: gateway + MAC del gateway + hash del SSID.
/// Positivo para que sea cómodo como clave en SQLite.
pub fn network_id(gateway: Ipv4Addr, gateway_mac: Option<&str>, ssid_hash: Option<&str>) -> i64 {
    let mut h = Sha256::new();
    h.update(gateway.octets());
    h.update(gateway_mac.unwrap_or("").as_bytes());
    h.update(b"|");
    h.update(ssid_hash.unwrap_or("").as_bytes());
    let d = h.finalize();
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[..8]);
    (u64::from_be_bytes(b) & (i64::MAX as u64)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_ranges() {
        for ip in ["192.168.1.1", "10.0.0.1", "172.16.5.4", "100.64.0.1", "100.127.255.254", "169.254.1.1", "127.0.0.1"] {
            assert!(!is_public(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["100.128.0.1", "190.24.1.1", "155.133.244.35", "1.1.1.1"] {
            assert!(is_public(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn network_id_is_stable_and_positive() {
        let gw: Ipv4Addr = "192.168.1.1".parse().unwrap();
        let a = network_id(gw, Some("aa:bb"), Some("x"));
        assert_eq!(a, network_id(gw, Some("aa:bb"), Some("x")));
        assert_ne!(a, network_id(gw, Some("aa:bb"), Some("y")));
        assert!(a >= 0);
    }
}
