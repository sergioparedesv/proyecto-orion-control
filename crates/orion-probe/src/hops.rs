//! Traceroute ligero y detección del primer hop público del ISP (§6.1.4).
//!
//! Los ecos con TTL 1..=max se envían **en paralelo**, así la ruta completa tarda
//! como mucho un `timeout`, no `max × timeout`.

use std::net::Ipv4Addr;
use std::time::Duration;

use futures::future::join_all;

use crate::net::is_public;
use crate::{EchoKind, Prober};

#[derive(Debug, Clone, PartialEq)]
pub struct Hop {
    pub ttl: u8,
    pub addr: Option<Ipv4Addr>,
    pub rtt_ms: Option<f64>,
    pub reached: bool,
}

pub async fn trace(prober: &dyn Prober, dest: Ipv4Addr, max_ttl: u8, timeout: Duration) -> Vec<Hop> {
    let futs = (1..=max_ttl).map(|ttl| async move {
        let r = prober.echo(dest, Some(ttl), timeout).await;
        let reached = r.kind == EchoKind::Reply;
        let ok = matches!(r.kind, EchoKind::Reply | EchoKind::TtlExpired);
        Hop { ttl, addr: if ok { r.from } else { None }, rtt_ms: if ok { r.rtt_ms } else { None }, reached }
    });
    let mut hops = join_all(futs).await;
    if let Some(i) = hops.iter().position(|h| h.reached) {
        hops.truncate(i + 1);
    }
    hops
}

#[derive(Debug, Clone, PartialEq)]
pub struct IspHop {
    pub addr: Ipv4Addr,
    pub ttl: u8,
    /// El hop anterior no respondió: puede que el primer hop público real esté oculto.
    pub approximate: bool,
}

/// Primer hop con IP pública. Si el destino es el primero público, también vale.
pub fn first_public_hop(hops: &[Hop]) -> Option<IspHop> {
    let idx = hops.iter().position(|h| h.addr.is_some_and(is_public))?;
    let approximate = idx > 0 && hops[..idx].last().is_some_and(|h| h.addr.is_none());
    Some(IspHop { addr: hops[idx].addr?, ttl: hops[idx].ttl, approximate })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hop(ttl: u8, addr: Option<&str>) -> Hop {
        Hop { ttl, addr: addr.map(|a| a.parse().unwrap()), rtt_ms: addr.map(|_| 1.0), reached: false }
    }

    #[test]
    fn skips_private_and_cgnat() {
        let hops = vec![hop(1, Some("192.168.1.1")), hop(2, Some("100.64.3.1")), hop(3, Some("190.1.1.1"))];
        let isp = first_public_hop(&hops).unwrap();
        assert_eq!(isp.addr, "190.1.1.1".parse::<Ipv4Addr>().unwrap());
        assert!(!isp.approximate);
    }

    #[test]
    fn marks_approximate_when_previous_is_silent() {
        let hops = vec![hop(1, Some("192.168.1.1")), hop(2, None), hop(3, Some("190.1.1.1"))];
        assert!(first_public_hop(&hops).unwrap().approximate);
    }

    #[test]
    fn none_when_no_public() {
        let hops = vec![hop(1, Some("192.168.1.1")), hop(2, None)];
        assert_eq!(first_public_hop(&hops), None);
    }
}
