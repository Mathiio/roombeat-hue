# roombeat-hue

Music-reactive Philips Hue lighting with spatial effects and a 3D room editor. Unofficial open-source desktop app (macOS first) built with **Rust** and **Tauri**.

Sync lights to system audio in real time via the Hue **Entertainment API** (DTLS streaming). Plan rooms in 2D/3D, place bulbs in space, and drive layered, position-aware effects from local audio analysis (FFT, beats, BPM).

> Not affiliated with Signify or Philips Hue. Distinct from the official Hue Sync app.

## Status

Early development — **Phase 0** (CLI): bridge discovery, pairing, and entertainment streaming tests are in place. The Tauri UI and full audio pipeline are planned; see [PLAN.md](./PLAN.md).

## Requirements

- Philips Hue Bridge (developed against **Bridge Pro**)
- macOS (Windows/Linux later via Tauri)
- Rust toolchain ([rustup](https://rustup.rs/))

## Quick start (CLI)

```bash
cargo run -p huesync-cli -- discover
cargo run -p huesync-cli -- pair
cargo run -p huesync-cli -- info
cargo run -p huesync-cli -- test --effect sweep
cargo run -p huesync-cli -- stop
```

Bridge credentials are stored under `~/Library/Application Support/huesync/` (not committed).

## Repository layout

```text
crates/hue/   # Bridge discovery, CLIP v2, DTLS HueStream
crates/cli/   # Phase 0 CLI and test effects
PLAN.md       # Architecture and roadmap
```

## Credits

Track BPM lookup in the desktop app uses the [GetSongBPM](https://getsongbpm.com) API ([API documentation](https://getsongbpm.com/api)).

## License

License TBD.