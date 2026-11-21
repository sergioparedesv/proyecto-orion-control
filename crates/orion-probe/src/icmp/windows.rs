//! ICMP en Windows con la IP Helper API (`IcmpSendEcho`), sin privilegios de administrador.
//!
//! Se declara la FFI a mano (estructuras de `icmpapi.h` / `ipexport.h`): es ABI estable de
//! Windows y evita depender de los cambios de firma entre versiones del crate `windows`.
//! `IcmpSendEcho` es bloqueante, así que cada eco corre en `spawn_blocking`.
//!
//! El RTT se mide con `Instant` alrededor de la llamada (resolución de µs). El campo
//! `RoundTripTime` del sistema tiene resolución de 1 ms y se usa solo como cota de cordura.

use std::ffi::c_void;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use crate::{EchoKind, EchoResult, Prober};

type Handle = isize;
const INVALID_HANDLE_VALUE: Handle = -1;

#[repr(C)]
struct IpOptionInformation {
    ttl: u8,
    tos: u8,
    flags: u8,
    options_size: u8,
    options_data: *mut u8,
}

#[repr(C)]
struct IcmpEchoReply {
    address: u32,
    status: u32,
    round_trip_time: u32,
    data_size: u16,
    reserved: u16,
    data: *mut c_void,
    options: IpOptionInformation,
}

#[link(name = "iphlpapi")]
extern "system" {
    fn IcmpCreateFile() -> Handle;
    fn IcmpCloseHandle(handle: Handle) -> i32;
    fn IcmpSendEcho(
        handle: Handle,
        destination: u32,
        request_data: *const c_void,
        request_size: u16,
        request_options: *const IpOptionInformation,
        reply_buffer: *mut c_void,
        reply_size: u32,
        timeout_ms: u32,
    ) -> u32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetLastError() -> u32;
}

// Códigos IP_STATUS de ipexport.h
const IP_SUCCESS: u32 = 0;
const IP_DEST_NET_UNREACHABLE: u32 = 11002;
const IP_DEST_HOST_UNREACHABLE: u32 = 11003;
const IP_DEST_PROT_UNREACHABLE: u32 = 11004;
const IP_DEST_PORT_UNREACHABLE: u32 = 11005;
const IP_REQ_TIMED_OUT: u32 = 11010;
const IP_TTL_EXPIRED_TRANSIT: u32 = 11013;
const IP_TTL_EXPIRED_REASSEM: u32 = 11014;

const PAYLOAD: [u8; 32] = [0x4f; 32];
const REPLY_BUF_WORDS: usize = 128; // 1024 bytes, alineado a 8

pub struct WindowsIcmp;

fn echo_blocking(dest: Ipv4Addr, ttl: Option<u8>, timeout: Duration) -> EchoResult {
    // SAFETY: llamadas FFI a la IP Helper API con buffers propios del tamaño declarado.
    unsafe {
        let handle = IcmpCreateFile();
        if handle == INVALID_HANDLE_VALUE || handle == 0 {
            return EchoResult::lost(EchoKind::Error(format!("IcmpCreateFile falló ({})", GetLastError())));
        }
        let options = IpOptionInformation {
            ttl: ttl.unwrap_or(128),
            tos: 0,
            flags: 0,
            options_size: 0,
            options_data: std::ptr::null_mut(),
        };
        let mut buf = [0u64; REPLY_BUF_WORDS];
        let timeout_ms = timeout.as_millis().clamp(1, u32::MAX as u128) as u32;
        let start = Instant::now();
        let n = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(dest.octets()),
            PAYLOAD.as_ptr().cast(),
            PAYLOAD.len() as u16,
            &options,
            buf.as_mut_ptr().cast(),
            std::mem::size_of_val(&buf) as u32,
            timeout_ms,
        );
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        let last_error = if n == 0 { GetLastError() } else { 0 };
        IcmpCloseHandle(handle);

        let reply = &*(buf.as_ptr() as *const IcmpEchoReply);
        let from = (reply.address != 0).then(|| Ipv4Addr::from(reply.address.to_ne_bytes()));
        // Con TTL excedido algunas versiones devuelven 0 y dejan el estado en GetLastError.
        let status = if n > 0 { reply.status } else if reply.status != 0 { reply.status } else { last_error };

        match status {
            IP_SUCCESS if n > 0 => {
                // Cordura: si Windows reporta un RTT mucho mayor, se usa el del sistema.
                let sys = reply.round_trip_time as f64;
                let rtt = if sys > elapsed_ms + 1.0 { sys } else { elapsed_ms };
                EchoResult { kind: EchoKind::Reply, rtt_ms: Some(rtt), from }
            }
            IP_TTL_EXPIRED_TRANSIT | IP_TTL_EXPIRED_REASSEM => {
                EchoResult { kind: EchoKind::TtlExpired, rtt_ms: Some(elapsed_ms), from }
            }
            IP_REQ_TIMED_OUT => EchoResult::lost(EchoKind::Timeout),
            IP_DEST_NET_UNREACHABLE | IP_DEST_HOST_UNREACHABLE | IP_DEST_PROT_UNREACHABLE | IP_DEST_PORT_UNREACHABLE => {
                EchoResult { kind: EchoKind::Unreachable, rtt_ms: None, from }
            }
            other => EchoResult::lost(EchoKind::Error(format!("IP_STATUS {other}"))),
        }
    }
}

#[async_trait::async_trait]
impl Prober for WindowsIcmp {
    async fn echo(&self, dest: Ipv4Addr, ttl: Option<u8>, timeout: Duration) -> EchoResult {
        match tokio::task::spawn_blocking(move || echo_blocking(dest, ttl, timeout)).await {
            Ok(r) => r,
            Err(e) => EchoResult::lost(EchoKind::Error(format!("tarea ICMP: {e}"))),
        }
    }
}
