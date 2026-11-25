use std::net::IpAddr;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct DiscoveredBridge {
    pub id: String,
    pub ip: IpAddr,
}

const HUE_SERVICE: &str = "_hue._tcp.local.";

/// Looks for bridges on the LAN via mDNS, falling back to the Signify discovery endpoint.
pub async fn discover(timeout: Duration) -> Result<Vec<DiscoveredBridge>> {
    let found = tokio::task::spawn_blocking(move || discover_mdns(timeout))
        .await?
        .unwrap_or_else(|e| {
            tracing::warn!("mDNS discovery failed: {e:#}");
            Vec::new()
        });
    if !found.is_empty() {
        return Ok(found);
    }
    discover_cloud().await
}

fn discover_mdns(timeout: Duration) -> Result<Vec<DiscoveredBridge>> {
    let daemon = mdns_sd::ServiceDaemon::new()?;
    let rx = daemon.browse(HUE_SERVICE)?;
    let deadline = Instant::now() + timeout;
    let mut found: Vec<DiscoveredBridge> = Vec::new();

    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        let Ok(event) = rx.recv_timeout(left) else { break };
        if let mdns_sd::ServiceEvent::ServiceResolved(info) = event {
            let id = info
                .get_property_val_str("bridgeid")
                .unwrap_or(info.get_fullname())
                .to_lowercase();
            let Some(ip) = info.get_addresses_v4().into_iter().next() else { continue };
            if !found.iter().any(|b| b.id == id) {
                found.push(DiscoveredBridge { id, ip: IpAddr::V4(ip) });
            }
        }
    }
    let _ = daemon.shutdown();
    Ok(found)
}

#[derive(Deserialize)]
struct CloudEntry {
    id: String,
    internalipaddress: IpAddr,
}

async fn discover_cloud() -> Result<Vec<DiscoveredBridge>> {
    let entries: Vec<CloudEntry> = reqwest::Client::new()
        .get("https://discovery.meethue.com/")
        .timeout(Duration::from_secs(5))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(entries
        .into_iter()
        .map(|e| DiscoveredBridge { id: e.id.to_lowercase(), ip: e.internalipaddress })
        .collect())
}
