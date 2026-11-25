//! Subset of the CLIP v2 resources used by HueSync.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceRef {
    pub rid: String,
    pub rtype: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Light {
    pub id: String,
    pub owner: ResourceRef,
    #[serde(default)]
    pub metadata: Metadata,
    /// Present only on color-capable lights.
    #[serde(default)]
    pub color: Option<serde_json::Value>,
}

impl Light {
    pub fn supports_color(&self) -> bool {
        self.color.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entertainment {
    pub id: String,
    pub owner: ResourceRef,
    #[serde(default)]
    pub renderer: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMember {
    pub service: ResourceRef,
    #[serde(default)]
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub channel_id: u8,
    pub position: Position,
    #[serde(default)]
    pub members: Vec<ChannelMember>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntertainmentConfiguration {
    pub id: String,
    #[serde(default)]
    pub metadata: Metadata,
    #[serde(default)]
    pub configuration_type: String,
    /// "active" while someone is streaming, "inactive" otherwise.
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub active_streamer: Option<ResourceRef>,
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub light_services: Vec<ResourceRef>,
}

/// Unauthenticated `/api/0/config` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgeConfig {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub bridgeid: String,
    #[serde(default)]
    pub modelid: String,
    #[serde(default)]
    pub swversion: String,
    #[serde(default)]
    pub apiversion: String,
}

/// Resolves the light behind each channel of an entertainment configuration
/// (channel → entertainment service → device → light).
pub fn channel_lights<'a>(
    channel: &Channel,
    entertainments: &[Entertainment],
    lights: &'a [Light],
) -> Vec<&'a Light> {
    channel
        .members
        .iter()
        .filter_map(|m| entertainments.iter().find(|e| e.id == m.service.rid))
        .filter_map(|e| lights.iter().find(|l| l.owner.rid == e.owner.rid))
        .collect()
}
