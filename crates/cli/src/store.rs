use std::net::IpAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use hue::Credentials;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct StoredBridge {
    pub bridge_id: String,
    pub ip: IpAddr,
    pub credentials: Credentials,
}

fn path() -> Result<PathBuf> {
    Ok(dirs::config_dir().context("no config directory")?.join("huesync").join("bridge.json"))
}

pub fn save(bridge: &StoredBridge) -> Result<PathBuf> {
    let path = path()?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, serde_json::to_vec_pretty(bridge)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(path)
}

pub fn load() -> Result<StoredBridge> {
    let path = path()?;
    let data = std::fs::read(&path)
        .with_context(|| format!("{} introuvable : lance d'abord `huesync-cli pair`", path.display()))?;
    Ok(serde_json::from_slice(&data)?)
}
