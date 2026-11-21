//! `orion-cli`: herramienta de consola de ORION.
//!
//! - `spike ...`: validaciones técnicas de la fase F0 (ver docs/f0-resultados.md).
//! - `monitor`: monitoreo en vivo con diagnóstico y semáforo (F1).

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use futures::future::join_all;
use orion_engine::{Engine, EngineConfig, Readiness, TargetStats, TargetStatus};
use orion_probe::catalog::{Catalog, CatalogSource, DEFAULT_WATCH, SDR_URL};
use orion_probe::hops::{first_public_hop, trace};
use orion_probe::monitor::{self, MonitorConfig, MonitorDeps, MonitorEvent};
use orion_probe::{NetEnv, Prober, SystemEnv};

#[derive(Parser)]
#[command(name = "orion-cli", version, about = "ORION NetControl — diagnóstico de red para Dota 2")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Validaciones técnicas de F0.
    Spike {
        #[command(subcommand)]
        what: Spike,
    },
    /// Monitoreo en vivo: gateway, ISP y relay, con diagnóstico y semáforo.
    Monitor {
        /// Código del POP activo (lim, scl, eze, gru, iad, ...).
        #[arg(long, default_value = "lim")]
        region: String,
        /// Modo DEMO con datos simulados: calm | spike | wifi | isp | route.
        #[arg(long)]
        mock: Option<String>,
    },
}

#[derive(Subcommand)]
enum Spike {
    /// Eco ICMP nativo (sin administrador). Compárese con `ping.exe -n N ip`.
    Icmp {
        ip: Ipv4Addr,
        #[arg(short, default_value_t = 10)]
        n: u32,
    },
    /// Interfaz por defecto y gateway.
    Gateway,
    /// Traceroute ligero y primer hop público.
    Hops {
        #[arg(default_value = "1.1.1.1")]
        ip: Ipv4Addr,
        #[arg(long, default_value_t = 12)]
        max: u8,
    },
    /// Descarga GetSDRConfig (appid 570) y mide qué relays responden ICMP.
    Sdr {
        /// Guarda la respuesta cruda (p. ej. docs/samples/sdr_config.json).
        #[arg(long)]
        out: Option<PathBuf>,
        /// POP a medir (por defecto LATAM + EE. UU. este/sur).
        #[arg(long, value_delimiter = ',')]
        pops: Option<Vec<String>>,
        /// Ecos por relay.
        #[arg(short, default_value_t = 5)]
        n: u32,
    },
    /// Información del Wi-Fi activo (RSSI, canal, banda, velocidad).
    Wifi,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()))
        .init();
    match Cli::parse().cmd {
        Cmd::Spike { what } => spike(what).await,
        Cmd::Monitor { region, mock } => run_monitor(region, mock).await,
    }
}

const TIMEOUT: Duration = Duration::from_millis(1000);

fn fmt_ms(v: Option<f64>) -> String {
    v.map_or_else(|| "   —  ".into(), |v| format!("{v:6.1}"))
}

async fn spike(what: Spike) -> Result<()> {
    let prober = orion_probe::system_prober()?;
    match what {
        Spike::Icmp { ip, n } => {
            let mut rtts = Vec::new();
            for seq in 1..=n {
                let r = prober.echo(ip, None, TIMEOUT).await;
                println!("{seq:>3}  {ip}  {:?}  rtt={} ms", r.kind, fmt_ms(r.rtt_ms));
                rtts.extend(r.reply_rtt());
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            rtts.sort_by(f64::total_cmp);
            let lost = n as usize - rtts.len();
            println!(
                "\nenviados={n} recibidos={} pérdida={:.0}%  min={} mediana={} max={}",
                rtts.len(),
                lost as f64 * 100.0 / n as f64,
                fmt_ms(rtts.first().copied()),
                fmt_ms(rtts.get(rtts.len() / 2).copied()),
                fmt_ms(rtts.last().copied()),
            );
        }
        Spike::Gateway => {
            let net = SystemEnv.local_network()?;
            println!("interfaz : {} ({}) idx={}", net.iface_label, net.iface_name, net.iface_index);
            println!("IPv4     : {}", net.local_ipv4.map_or("—".into(), |i| i.to_string()));
            println!("gateway  : {}", net.gateway);
            println!("MAC gw   : {}", net.gateway_mac.as_deref().unwrap_or("—"));
            let r = prober.echo(net.gateway, None, TIMEOUT).await;
            println!("eco gw   : {:?} rtt={} ms", r.kind, fmt_ms(r.rtt_ms));
        }
        Spike::Hops { ip, max } => {
            let hops = trace(prober.as_ref(), ip, max, TIMEOUT).await;
            for h in &hops {
                let addr = h.addr.map_or("*".into(), |a| a.to_string());
                let tag = if h.addr.is_some_and(orion_probe::net::is_public) { "público" } else { "" };
                println!("{:>2}  {addr:<16} {} ms  {tag}", h.ttl, fmt_ms(h.rtt_ms));
            }
            match first_public_hop(&hops) {
                Some(h) => println!("\nprimer hop público: {} (TTL {}){}", h.addr, h.ttl, if h.approximate { " — aproximado" } else { "" }),
                None => println!("\nno se encontró un hop público en {max} saltos"),
            }
        }
        Spike::Sdr { out, pops, n } => {
            let http = reqwest::Client::new();
            let text = http.get(SDR_URL).timeout(Duration::from_secs(15)).send().await?.error_for_status()?.text().await?;
            if let Some(p) = &out {
                std::fs::write(p, &text).with_context(|| format!("escribiendo {}", p.display()))?;
                println!("guardado en {}", p.display());
            }
            let catalog = Catalog::parse(&text, CatalogSource::Remote)?;
            println!("revisión {:?}, {} POP con relays\n", catalog.revision, catalog.pops.len());
            let codes = pops.unwrap_or_else(|| DEFAULT_WATCH.iter().map(|s| s.to_string()).collect());
            println!("| POP | Descripción | Relay | Respuestas | Mejor RTT (ms) |");
            println!("|---|---|---|---|---|");
            for code in codes {
                let Some(pop) = catalog.pop(&code) else {
                    println!("| {code} | (no está en el catálogo) | | | |");
                    continue;
                };
                let rows = join_all(pop.relays.iter().map(|&addr| probe_n(prober.clone(), addr, n))).await;
                for (addr, ok, best) in rows {
                    println!("| {code} | {} | {addr} | {ok}/{n} | {} |", pop.desc, fmt_ms(best).trim());
                }
            }
        }
        Spike::Wifi => {
            let net = SystemEnv.local_network()?;
            match SystemEnv.wifi(&net) {
                Some(w) => println!("{}", serde_json::to_string_pretty(&w)?),
                None => println!("Sin Wi-Fi activo en la interfaz por defecto ({}), o la plataforma no lo soporta.", net.iface_label),
            }
        }
    }
    Ok(())
}

async fn probe_n(prober: Arc<dyn Prober>, addr: Ipv4Addr, n: u32) -> (Ipv4Addr, u32, Option<f64>) {
    let mut ok = 0;
    let mut best: Option<f64> = None;
    for _ in 0..n {
        if let Some(r) = prober.echo(addr, None, TIMEOUT).await.reply_rtt() {
            ok += 1;
            best = Some(best.map_or(r, |b| b.min(r)));
        }
    }
    (addr, ok, best)
}

fn status_icon(s: TargetStatus) -> &'static str {
    match s {
        TargetStatus::Ok => "OK  ",
        TargetStatus::Degraded => "DEGR",
        TargetStatus::Down => "CAÍDO",
        TargetStatus::IcmpBlocked => "ICMP✗",
        TargetStatus::NoData => "s/d ",
    }
}

fn readiness_icon(r: Readiness) -> &'static str {
    match r {
        Readiness::Ready => "🟢",
        Readiness::Caution => "🟡",
        Readiness::NoGo => "🔴",
        Readiness::NoData => "⚪",
    }
}

fn print_target(t: &TargetStats) {
    println!(
        "  {:<5} {:<34} last {} | p95 {} | jit {} | loss {:5.1}% | n={}",
        status_icon(t.status),
        format!("{}{}", t.label, if t.approximate { " ~" } else { "" }),
        fmt_ms(t.last_ms),
        fmt_ms(t.p95_ms),
        fmt_ms(t.jitter_ms),
        t.loss_pct,
        t.valid
    );
}

async fn run_monitor(region: String, mock: Option<String>) -> Result<()> {
    let engine = Arc::new(Mutex::new(Engine::new(EngineConfig::default())));
    let (prober, env, catalog, demo): (Arc<dyn Prober>, Arc<dyn NetEnv>, Catalog, bool) = match mock {
        Some(s) => {
            let sc: orion_probe::mock::Scenario = s.parse().map_err(anyhow::Error::msg)?;
            (
                Arc::new(orion_probe::mock::MockProber::new(sc)) as Arc<dyn Prober>,
                Arc::new(orion_probe::mock::MockEnv { scenario: sc }) as Arc<dyn NetEnv>,
                Catalog::bundled(),
                true,
            )
        }
        None => {
            let (c, notes) = Catalog::load(None, &reqwest::Client::new()).await;
            for n in notes {
                eprintln!("aviso: {n}");
            }
            (orion_probe::system_prober()?, Arc::new(SystemEnv) as Arc<dyn NetEnv>, c, false)
        }
    };
    if demo {
        println!("*** MODO DEMO: datos simulados, no son mediciones reales ***");
    }
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = monitor::spawn(MonitorDeps { prober, env, catalog, engine, demo }, MonitorConfig::default(), region, tx);

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            ev = rx.recv() => match ev {
                None => break,
                Some(MonitorEvent::Log { level, message }) => eprintln!("[{level:?}] {message}"),
                Some(MonitorEvent::Network(n)) => {
                    println!("\n== Red: {} | gateway {} | ISP {}{} | Wi-Fi {}",
                        n.iface_label,
                        n.gateway.as_deref().unwrap_or("—"),
                        n.isp_hop.as_deref().unwrap_or("—"),
                        if n.isp_approximate { " (aprox.)" } else { "" },
                        n.wifi.as_ref().map_or("no".into(), |w| format!("{:?} canal {:?} RSSI {:?} dBm", w.band, w.channel, w.rssi_dbm)));
                }
                Some(MonitorEvent::Tick(out)) => {
                    let t = &out.tick;
                    println!("\n{} {} — {}", readiness_icon(t.readiness), t.readiness.label(), t.readiness_reason);
                    let active = t.active_relay.as_deref();
                    for s in t.targets.iter().filter(|s| s.id == "gateway" || s.id == "isp-hop" || Some(s.id.as_str()) == active) {
                        print_target(s);
                    }
                    println!("  Diagnóstico: {:?} — {}", t.diagnosis.verdict, t.diagnosis.message);
                    for e in &t.diagnosis.evidence {
                        println!("    · {} {} = {:.1} (umbral {:.1})", e.target, e.metric, e.value, e.threshold);
                    }
                    if let Some(b) = &t.baseline {
                        if b.converged {
                            println!("  Línea base: μ={:.1} σ={:.1} k={} umbral={:.1} ms anomalía={:.2}", b.mu_ms, b.sigma_ms, b.k, b.threshold_ms, b.anomaly_score);
                        } else {
                            println!("  Línea base: calibrando ({}/{})", b.samples, b.converge_after);
                        }
                    }
                    for r in &t.recommendations {
                        println!("  → {}", r.message);
                    }
                    if let Some(c) = &out.readiness_change {
                        println!("  *** CAMBIO: {} → {} ***", c.from.label(), c.to.label());
                    }
                }
            }
        }
    }
    handle.stop();
    Ok(())
}
