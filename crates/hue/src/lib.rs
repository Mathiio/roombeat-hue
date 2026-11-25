pub mod client;
pub mod discovery;
pub mod model;
pub mod stream;

pub use client::{BridgeClient, Credentials};
pub use discovery::{DiscoveredBridge, discover};
pub use model::*;
pub use stream::{ChannelRgb, EntertainmentStream};

/// Both `ring` (via webrtc-dtls) and `aws-lc-rs` (via reqwest) end up enabled, so rustls
/// cannot pick a provider on its own.
pub(crate) fn ensure_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}
