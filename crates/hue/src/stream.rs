//! Hue Entertainment streaming: DTLS 1.2 PSK over UDP/2100, `HueStream` v2 frames.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::net::UdpSocket;
use webrtc_dtls::cipher_suite::CipherSuiteId;
use webrtc_dtls::config::{Config, ExtendedMasterSecretType};
use webrtc_dtls::conn::DTLSConn;

use crate::client::Credentials;

pub const STREAM_PORT: u16 = 2100;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const CONFIG_ID_LEN: usize = 36;
const HEADER_LEN: usize = 16;
const CHANNEL_LEN: usize = 7;
const COLOR_SPACE_RGB: u8 = 0x00;

/// One channel's color, 16 bits per component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelRgb {
    pub channel_id: u8,
    pub rgb: [u16; 3],
}

pub struct EntertainmentStream {
    conn: DTLSConn,
    config_id: String,
    seq: u8,
    buf: Vec<u8>,
}

impl EntertainmentStream {
    /// Streaming must already be started on `config_id` through the REST API.
    pub async fn connect(ip: IpAddr, creds: &Credentials, config_id: &str) -> Result<Self> {
        if config_id.len() != CONFIG_ID_LEN {
            bail!("entertainment configuration id must be {CONFIG_ID_LEN} chars");
        }
        crate::ensure_crypto_provider();
        let psk = hex::decode(&creds.clientkey).context("clientkey is not valid hex")?;

        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(SocketAddr::new(ip, STREAM_PORT)).await?;

        let config = Config {
            cipher_suites: vec![CipherSuiteId::Tls_Psk_With_Aes_128_Gcm_Sha256],
            psk: Some(Arc::new(move |_hint: &[u8]| Ok(psk.clone()))),
            psk_identity_hint: Some(creds.username.as_bytes().to_vec()),
            extended_master_secret: ExtendedMasterSecretType::Request,
            server_name: ip.to_string(),
            ..Default::default()
        };

        let conn = tokio::time::timeout(
            HANDSHAKE_TIMEOUT,
            DTLSConn::new(Arc::new(socket), config, true, None),
        )
        .await
        .context("DTLS handshake timed out")?
        .context("DTLS handshake failed")?;

        Ok(Self {
            conn,
            config_id: config_id.to_owned(),
            seq: 0,
            buf: Vec::with_capacity(HEADER_LEN + CONFIG_ID_LEN + 20 * CHANNEL_LEN),
        })
    }

    pub async fn send_rgb(&mut self, channels: &[ChannelRgb]) -> Result<()> {
        encode_rgb_frame(&mut self.buf, self.seq, &self.config_id, channels);
        self.seq = self.seq.wrapping_add(1);
        self.conn.write(&self.buf, None).await?;
        Ok(())
    }

    pub async fn close(self) -> Result<()> {
        self.conn.close().await?;
        Ok(())
    }
}

pub fn encode_rgb_frame(buf: &mut Vec<u8>, seq: u8, config_id: &str, channels: &[ChannelRgb]) {
    buf.clear();
    buf.extend_from_slice(b"HueStream");
    // version 2.0, sequence, 2 reserved, color space, 1 reserved
    buf.extend_from_slice(&[0x02, 0x00, seq, 0x00, 0x00, COLOR_SPACE_RGB, 0x00]);
    buf.extend_from_slice(config_id.as_bytes());
    for c in channels {
        buf.push(c.channel_id);
        for v in c.rgb {
            buf.extend_from_slice(&v.to_be_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_layout() {
        let id = "1a8d99cc-967b-44f2-9202-43f976c0fa6b";
        let mut buf = Vec::new();
        encode_rgb_frame(
            &mut buf,
            7,
            id,
            &[ChannelRgb { channel_id: 3, rgb: [0xffff, 0x0102, 0] }],
        );
        assert_eq!(buf.len(), HEADER_LEN + CONFIG_ID_LEN + CHANNEL_LEN);
        assert_eq!(&buf[..9], b"HueStream");
        assert_eq!(&buf[9..16], &[2, 0, 7, 0, 0, 0, 0]);
        assert_eq!(&buf[16..52], id.as_bytes());
        assert_eq!(&buf[52..], &[3, 0xff, 0xff, 0x01, 0x02, 0, 0]);
    }
}
