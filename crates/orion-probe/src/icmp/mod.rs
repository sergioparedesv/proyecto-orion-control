//! Eco ICMP nativo (§6.1.2). Prohibido invocar `ping.exe`.

use std::sync::Arc;

use crate::{ProbeError, Prober};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub fn system_prober() -> Result<Arc<dyn Prober>, ProbeError> {
    Ok(Arc::new(windows::WindowsIcmp))
}

#[cfg(target_os = "linux")]
pub fn system_prober() -> Result<Arc<dyn Prober>, ProbeError> {
    Ok(Arc::new(linux::LinuxIcmp::new()?))
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn system_prober() -> Result<Arc<dyn Prober>, ProbeError> {
    Err(ProbeError::Unsupported("ICMP solo está implementado en Windows y Linux"))
}
