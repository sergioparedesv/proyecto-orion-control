//! Descubrimiento de la interfaz por defecto y su gateway (§6.1.3), vía `netdev`.
//! (Pendiente F0: comparar con `GetBestRoute2` en Windows; ver docs/f0-resultados.md.)

use crate::{LocalNetwork, ProbeError};

pub fn discover() -> Result<LocalNetwork, ProbeError> {
    let iface = netdev::get_default_interface().map_err(ProbeError::NoDefaultRoute)?;
    let gw = iface.gateway.as_ref().ok_or(ProbeError::NoGateway)?;
    let gateway = *gw.ipv4.first().ok_or(ProbeError::NoGateway)?;
    let mac = gw.mac_addr.to_string();
    let gateway_mac = (mac != "00:00:00:00:00:00").then_some(mac);
    let iface_label = iface
        .friendly_name
        .clone()
        .or_else(|| iface.description.clone())
        .unwrap_or_else(|| iface.name.clone());
    Ok(LocalNetwork {
        iface_name: iface.name.clone(),
        iface_label,
        iface_index: iface.index,
        local_ipv4: iface.ipv4.first().map(|n| n.addr()),
        gateway,
        gateway_mac,
    })
}
