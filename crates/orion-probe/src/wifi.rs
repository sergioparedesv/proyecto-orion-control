//! Información de Wi-Fi (§6.1.6) con la Native Wifi API de Windows.
//! En otras plataformas devuelve `None` (la interfaz muestra "no disponible").
//!
//! El SSID nunca sale de este módulo en claro: solo su hash.

use orion_engine::WifiInfo;

use crate::LocalNetwork;

#[cfg(not(windows))]
pub fn query(_net: &LocalNetwork) -> Option<WifiInfo> {
    None
}

#[cfg(windows)]
pub fn query(net: &LocalNetwork) -> Option<WifiInfo> {
    imp::query(net)
}

/// Banda a partir del canal. Los canales 1-14 son 2,4 GHz; el resto se asume 5 GHz.
/// (6 GHz reutiliza números de canal y no se puede distinguir solo con el canal: pendiente F0.)
pub fn band_from_channel(channel: u32) -> orion_engine::WifiBand {
    use orion_engine::WifiBand;
    match channel {
        1..=14 => WifiBand::G2_4,
        32..=177 => WifiBand::G5,
        _ => WifiBand::Unknown,
    }
}

/// Aproximación estándar de Microsoft: calidad 0-100 ↔ RSSI −100..−50 dBm.
pub fn quality_to_rssi(quality: u32) -> i32 {
    (quality.min(100) as i32) / 2 - 100
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    use orion_engine::WifiInfo;

    use super::{band_from_channel, quality_to_rssi};
    use crate::net::short_hash;
    use crate::LocalNetwork;

    type Handle = isize;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    struct WlanInterfaceInfo {
        guid: Guid,
        description: [u16; 256],
        state: u32,
    }

    #[repr(C)]
    struct WlanInterfaceInfoList {
        count: u32,
        index: u32,
        items: [WlanInterfaceInfo; 1],
    }

    #[repr(C)]
    struct Dot11Ssid {
        len: u32,
        ssid: [u8; 32],
    }

    #[repr(C)]
    struct WlanAssociationAttributes {
        ssid: Dot11Ssid,
        bss_type: u32,
        bssid: [u8; 6],
        phy_type: u32,
        phy_index: u32,
        signal_quality: u32,
        rx_rate_kbps: u32,
        tx_rate_kbps: u32,
    }

    #[repr(C)]
    struct WlanSecurityAttributes {
        security_enabled: i32,
        one_x_enabled: i32,
        auth_algorithm: u32,
        cipher_algorithm: u32,
    }

    #[repr(C)]
    struct WlanConnectionAttributes {
        state: u32,
        mode: u32,
        profile_name: [u16; 256],
        assoc: WlanAssociationAttributes,
        security: WlanSecurityAttributes,
    }

    const WLAN_INTERFACE_STATE_CONNECTED: u32 = 1;
    const OPCODE_CURRENT_CONNECTION: u32 = 7;
    const OPCODE_CHANNEL_NUMBER: u32 = 8;
    const OPCODE_RSSI: u32 = 0x1000_0102;

    #[link(name = "wlanapi")]
    extern "system" {
        fn WlanOpenHandle(version: u32, reserved: *const c_void, negotiated: *mut u32, handle: *mut Handle) -> u32;
        fn WlanCloseHandle(handle: Handle, reserved: *const c_void) -> u32;
        fn WlanEnumInterfaces(handle: Handle, reserved: *const c_void, list: *mut *mut WlanInterfaceInfoList) -> u32;
        fn WlanQueryInterface(
            handle: Handle,
            guid: *const Guid,
            opcode: u32,
            reserved: *const c_void,
            data_size: *mut u32,
            data: *mut *mut c_void,
            value_type: *mut u32,
        ) -> u32;
        fn WlanFreeMemory(memory: *mut c_void);
    }

    fn guid_string(g: &Guid) -> String {
        format!(
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            g.data1, g.data2, g.data3, g.data4[0], g.data4[1], g.data4[2], g.data4[3], g.data4[4], g.data4[5], g.data4[6], g.data4[7]
        )
    }

    unsafe fn query_u32(h: Handle, guid: &Guid, opcode: u32) -> Option<u32> {
        let mut size = 0u32;
        let mut data: *mut c_void = null_mut();
        let mut vt = 0u32;
        if WlanQueryInterface(h, guid, opcode, null(), &mut size, &mut data, &mut vt) != 0 || data.is_null() {
            return None;
        }
        let v = (size as usize >= 4).then(|| *(data as *const u32));
        WlanFreeMemory(data);
        v
    }

    struct WlanHandle(Handle);
    impl Drop for WlanHandle {
        fn drop(&mut self) {
            // SAFETY: handle obtenido de WlanOpenHandle.
            unsafe { WlanCloseHandle(self.0, null()) };
        }
    }

    pub fn query(net: &LocalNetwork) -> Option<WifiInfo> {
        // SAFETY: FFI a wlanapi; toda la memoria devuelta se libera con WlanFreeMemory.
        unsafe {
            let mut negotiated = 0u32;
            let mut raw: Handle = 0;
            if WlanOpenHandle(2, null(), &mut negotiated, &mut raw) != 0 {
                return None; // servicio WLAN apagado o equipo sin Wi-Fi
            }
            let h = WlanHandle(raw);
            let mut list: *mut WlanInterfaceInfoList = null_mut();
            if WlanEnumInterfaces(h.0, null(), &mut list) != 0 || list.is_null() {
                return None;
            }
            let count = (*list).count as usize;
            let items = std::slice::from_raw_parts((*list).items.as_ptr(), count);
            // En Windows, netdev expone como `name` el GUID del adaptador: así sabemos si el
            // Wi-Fi conectado es la interfaz por defecto (y no un Wi-Fi secundario con cable activo).
            let default_is_guid = net.iface_name.starts_with('{');
            let target = items.iter().find(|i| {
                i.state == WLAN_INTERFACE_STATE_CONNECTED
                    && (!default_is_guid || guid_string(&i.guid).eq_ignore_ascii_case(&net.iface_name))
            });
            let Some(iface) = target else {
                WlanFreeMemory(list.cast());
                return None;
            };
            let guid = iface.guid;
            WlanFreeMemory(list.cast());

            let mut size = 0u32;
            let mut data: *mut c_void = null_mut();
            let mut vt = 0u32;
            if WlanQueryInterface(h.0, &guid, OPCODE_CURRENT_CONNECTION, null(), &mut size, &mut data, &mut vt) != 0
                || data.is_null()
            {
                return None;
            }
            let attrs = &*(data as *const WlanConnectionAttributes);
            let ssid_len = (attrs.assoc.ssid.len as usize).min(32);
            let ssid_hash = short_hash(&attrs.assoc.ssid.ssid[..ssid_len]);
            let quality = attrs.assoc.signal_quality;
            let rx_kbps = attrs.assoc.rx_rate_kbps;
            WlanFreeMemory(data);

            let channel = query_u32(h.0, &guid, OPCODE_CHANNEL_NUMBER);
            let rssi = query_u32(h.0, &guid, OPCODE_RSSI).map(|v| v as i32).filter(|v| (-120..0).contains(v));

            Some(WifiInfo {
                ssid_hash,
                rssi_dbm: rssi.or(Some(quality_to_rssi(quality))),
                signal_quality: Some(quality),
                band: channel.map(band_from_channel).unwrap_or(orion_engine::WifiBand::Unknown),
                channel,
                link_mbps: (rx_kbps > 0).then(|| rx_kbps as f64 / 1000.0),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orion_engine::WifiBand;

    #[test]
    fn bands_and_quality() {
        assert_eq!(band_from_channel(6), WifiBand::G2_4);
        assert_eq!(band_from_channel(149), WifiBand::G5);
        assert_eq!(quality_to_rssi(100), -50);
        assert_eq!(quality_to_rssi(0), -100);
        assert_eq!(quality_to_rssi(60), -70);
    }
}
