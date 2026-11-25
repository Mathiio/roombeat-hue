use std::net::IpAddr;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{BridgeConfig, Entertainment, EntertainmentConfiguration, Light};

/// Credentials returned by the bridge when pairing with `generateclientkey`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credentials {
    /// Sent as `hue-application-key` and used as the DTLS PSK identity.
    pub username: String,
    /// Hex-encoded 16-byte DTLS pre-shared key.
    pub clientkey: String,
}

#[derive(Debug, Deserialize)]
struct V2Response<T> {
    #[serde(default)]
    errors: Vec<V2Error>,
    #[serde(default = "Vec::new")]
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct V2Error {
    description: String,
}

pub struct BridgeClient {
    http: reqwest::Client,
    ip: IpAddr,
    app_key: Option<String>,
}

const LINK_BUTTON_NOT_PRESSED: i64 = 101;

impl BridgeClient {
    pub fn new(ip: IpAddr, app_key: Option<String>) -> Result<Self> {
        crate::ensure_crypto_provider();
        // The bridge certificate is signed by the Signify private CA and its CN
        // is the bridge id, not the IP, so default verification cannot succeed.
        // TODO: pin the Signify root CA and verify CN == bridge id.
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(5))
            .build()?;
        Ok(Self { http, ip, app_key })
    }

    pub fn ip(&self) -> IpAddr {
        self.ip
    }

    fn url(&self, path: &str) -> String {
        format!("https://{}{}", self.ip, path)
    }

    pub async fn config(&self) -> Result<BridgeConfig> {
        Ok(self
            .http
            .get(self.url("/api/0/config"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    /// One pairing attempt. Returns `Ok(None)` while the link button has not been pressed.
    pub async fn try_pair(&self, devicetype: &str) -> Result<Option<Credentials>> {
        let resp: Vec<Value> = self
            .http
            .post(self.url("/api"))
            .json(&json!({ "devicetype": devicetype, "generateclientkey": true }))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let first = resp.into_iter().next().ok_or_else(|| anyhow!("empty pairing response"))?;
        if let Some(success) = first.get("success") {
            return Ok(Some(serde_json::from_value(success.clone())?));
        }
        if let Some(err) = first.get("error") {
            if err.get("type").and_then(Value::as_i64) == Some(LINK_BUTTON_NOT_PRESSED) {
                return Ok(None);
            }
            bail!("pairing error: {err}");
        }
        bail!("unexpected pairing response: {first}")
    }

    fn key(&self) -> Result<&str> {
        self.app_key.as_deref().context("bridge not paired (no application key)")
    }

    async fn get_resources<T: DeserializeOwned>(&self, resource: &str) -> Result<Vec<T>> {
        let resp: V2Response<T> = self
            .http
            .get(self.url(&format!("/clip/v2/resource/{resource}")))
            .header("hue-application-key", self.key()?)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .with_context(|| format!("decoding {resource}"))?;
        if let Some(e) = resp.errors.first() {
            bail!("{resource}: {}", e.description);
        }
        Ok(resp.data)
    }

    async fn put_resource(&self, resource: &str, id: &str, body: Value) -> Result<()> {
        let resp: V2Response<Value> = self
            .http
            .put(self.url(&format!("/clip/v2/resource/{resource}/{id}")))
            .header("hue-application-key", self.key()?)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if let Some(e) = resp.errors.first() {
            bail!("{resource}/{id}: {}", e.description);
        }
        Ok(())
    }

    pub async fn lights(&self) -> Result<Vec<Light>> {
        self.get_resources("light").await
    }

    pub async fn entertainments(&self) -> Result<Vec<Entertainment>> {
        self.get_resources("entertainment").await
    }

    pub async fn entertainment_configurations(&self) -> Result<Vec<EntertainmentConfiguration>> {
        self.get_resources("entertainment_configuration").await
    }

    pub async fn set_streaming(&self, config_id: &str, active: bool) -> Result<()> {
        let action = if active { "start" } else { "stop" };
        self.put_resource("entertainment_configuration", config_id, json!({ "action": action }))
            .await
    }
}
