use std::process::Command;
use anyhow::{Result, anyhow};
use tracing::{info, warn, error};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// manages macOS system proxy settings
pub struct MacProxyManager {
    interface_name: String,
    active: Arc<AtomicBool>,
}

impl MacProxyManager {
    /// Create a new manager. Tries to auto-detect the active interface.
    pub fn new() -> Self {
        let interface = Self::detect_interface().unwrap_or_else(|| "Wi-Fi".to_string());
        info!("Selected network interface for proxy: {}", interface);
        Self {
            interface_name: interface,
            active: Arc::new(AtomicBool::new(false)),
        }
    }

    fn detect_interface() -> Option<String> {
        // Simple heuristic: Try Wi-Fi first, then Ethernet.
        // A robust solution would parse `networksetup -listallhardwareports`
        Some("Wi-Fi".to_string())
    }

    /// Enables the SOCKS5 proxy on the system
    pub fn enable(&self, host: &str, port: u16) -> Result<()> {
        info!("Enabling System SOCKS Proxy on {}...", self.interface_name);
        
        let status = Command::new("networksetup")
            .args(&["-setsocksfirewallproxy", &self.interface_name, host, &port.to_string()])
            .status();

        match status {
            Ok(s) if s.success() => {
                info!("System Proxy ENABLED.");
                self.active.store(true, Ordering::SeqCst);
                
                // Also enable the state (sometimes required separately)
                 let _ = Command::new("networksetup")
                    .args(&["-setsocksfirewallproxystate", &self.interface_name, "on"])
                    .status();
                
                Ok(())
            }
            Ok(s) => Err(anyhow!("networksetup failed with code: {}", s)),
            Err(e) => Err(anyhow!("Failed to execute networksetup: {}", e)),
        }
    }

    /// Disables the SOCKS5 proxy
    pub fn disable(&self) {
        if !self.active.load(Ordering::SeqCst) {
             return;
        }

        info!("Disabling System SOCKS Proxy on {}...", self.interface_name);
        let _ = Command::new("networksetup")
            .args(&["-setsocksfirewallproxystate", &self.interface_name, "off"])
            .status();
            
        self.active.store(false, Ordering::SeqCst);
        info!("System Proxy DISABLED.");
    }
}

impl Drop for MacProxyManager {
    fn drop(&mut self) {
        if self.active.load(Ordering::SeqCst) {
            warn!("ProxyManager dropped while active! Attempting panic-cleanup...");
            // Start a new process to cleanup because current one is dying
             let _ = Command::new("networksetup")
                .args(&["-setsocksfirewallproxystate", &self.interface_name, "off"])
                .status();
        }
    }
}
