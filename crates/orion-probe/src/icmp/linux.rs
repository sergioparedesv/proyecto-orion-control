//! ICMP en Linux con `surge-ping` (socket DGRAM; requiere `net.ipv4.ping_group_range`
//! que incluya al usuario, o `CAP_NET_RAW`). Plataforma secundaria: sin traceroute en el MVP.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use surge_ping::{Client, Config, PingIdentifier, PingSequence, SurgeError};

use crate::{EchoKind, EchoResult, ProbeError, Prober};

pub struct LinuxIcmp {
    client: Client,
    ident: AtomicU16,
}

impl LinuxIcmp {
    pub fn new() -> Result<Self, ProbeError> {
        let client = Client::new(&Config::default())?;
        Ok(Self { client, ident: AtomicU16::new(std::process::id() as u16) })
    }
}

#[async_trait::async_trait]
impl Prober for LinuxIcmp {
    async fn echo(&self, dest: Ipv4Addr, ttl: Option<u8>, timeout: Duration) -> EchoResult {
        if ttl.is_some() {
            return EchoResult::lost(EchoKind::Error("traceroute no implementado en Linux (MVP)".into()));
        }
        let id = self.ident.fetch_add(1, Ordering::Relaxed);
        let mut pinger = self.client.pinger(IpAddr::V4(dest), PingIdentifier(id)).await;
        pinger.timeout(timeout);
        match pinger.ping(PingSequence(0), &[0x4f; 32]).await {
            Ok((_, d)) => EchoResult { kind: EchoKind::Reply, rtt_ms: Some(d.as_secs_f64() * 1000.0), from: Some(dest) },
            Err(SurgeError::Timeout { .. }) => EchoResult::lost(EchoKind::Timeout),
            Err(e) => EchoResult::lost(EchoKind::Error(e.to_string())),
        }
    }
}
