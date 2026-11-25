mod effects;
mod store;

use std::net::IpAddr;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use hue::{BridgeClient, EntertainmentConfiguration, EntertainmentStream};

use crate::effects::TestEffect;
use crate::store::StoredBridge;

#[derive(Parser)]
#[command(name = "huesync-cli", about = "HueSync phase 0: bridge pairing and entertainment streaming test")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Find Hue bridges on the network
    Discover,
    /// Pair with a bridge (press its link button when asked)
    Pair {
        /// Bridge IP (auto-discovered if omitted)
        #[arg(long)]
        ip: Option<IpAddr>,
    },
    /// Show lights and entertainment areas of the paired bridge
    Info,
    /// Stream a test effect to an entertainment area
    Test {
        /// Entertainment area id or name (first one if omitted)
        #[arg(long)]
        area: Option<String>,
        #[arg(long, value_enum, default_value_t = EffectArg::Sweep)]
        effect: EffectArg,
        #[arg(long, default_value_t = 20)]
        seconds: u64,
        /// Frames sent per second
        #[arg(long, default_value_t = 50)]
        fps: u32,
    },
    /// Force-stop streaming on an entertainment area
    Stop {
        #[arg(long)]
        area: Option<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum EffectArg {
    Blink,
    Chase,
    Sweep,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    match Cli::parse().command {
        Command::Discover => discover().await,
        Command::Pair { ip } => pair(ip).await,
        Command::Info => info().await,
        Command::Test { area, effect, seconds, fps } => test(area, effect, seconds, fps).await,
        Command::Stop { area } => stop(area).await,
    }
}

async fn discover() -> Result<()> {
    println!("Recherche des bridges (mDNS, puis discovery.meethue.com)...");
    let bridges = hue::discover(Duration::from_secs(4)).await?;
    if bridges.is_empty() {
        println!("Aucun bridge trouvé.");
    }
    for b in bridges {
        let cfg = BridgeClient::new(b.ip, None)?.config().await;
        match cfg {
            Ok(c) => println!("- {} @ {}  ({}, modèle {}, firmware {})", b.id, b.ip, c.name, c.modelid, c.swversion),
            Err(e) => println!("- {} @ {}  (config injoignable: {e:#})", b.id, b.ip),
        }
    }
    Ok(())
}

async fn pair(ip: Option<IpAddr>) -> Result<()> {
    let ip = match ip {
        Some(ip) => ip,
        None => {
            let bridges = hue::discover(Duration::from_secs(4)).await?;
            match bridges.as_slice() {
                [] => bail!("aucun bridge trouvé, relance avec --ip"),
                [b] => b.ip,
                _ => bail!("plusieurs bridges trouvés, choisis avec --ip : {bridges:?}"),
            }
        }
    };
    let client = BridgeClient::new(ip, None)?;
    let cfg = client.config().await.context("bridge injoignable")?;
    println!("Bridge « {} » ({}) @ {ip}", cfg.name, cfg.modelid);
    println!(">>> Appuie sur le bouton du bridge (60 s max)...");

    let deadline = Instant::now() + Duration::from_secs(60);
    let creds = loop {
        if let Some(creds) = client.try_pair("huesync#mac").await? {
            break creds;
        }
        if Instant::now() > deadline {
            bail!("délai dépassé : bouton non pressé");
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };

    let path = store::save(&StoredBridge { bridge_id: cfg.bridgeid.to_lowercase(), ip, credentials: creds })?;
    println!("Appairé ! Identifiants enregistrés dans {}", path.display());
    Ok(())
}

async fn connect() -> Result<(StoredBridge, BridgeClient)> {
    let stored = store::load()?;
    let client = BridgeClient::new(stored.ip, Some(stored.credentials.username.clone()))?;
    Ok((stored, client))
}

async fn info() -> Result<()> {
    let (_, client) = connect().await?;
    let (lights, ents, areas) = tokio::try_join!(
        client.lights(),
        client.entertainments(),
        client.entertainment_configurations()
    )?;

    println!("Lampes ({}) :", lights.len());
    for l in &lights {
        let color = if l.supports_color() { "couleur" } else { "blanc" };
        println!("  - {:<28} [{color}]  {}", l.metadata.name, l.id);
    }

    println!("\nZones de divertissement ({}) :", areas.len());
    if areas.is_empty() {
        println!("  Aucune. Crée-en une dans l'app Hue : Paramètres > Zones de divertissement.");
    }
    for a in &areas {
        println!("  - « {} » [{}] statut={}  {}", a.metadata.name, a.configuration_type, a.status, a.id);
        for ch in &a.channels {
            let names: Vec<_> = hue::channel_lights(ch, &ents, &lights)
                .iter()
                .map(|l| l.metadata.name.as_str())
                .collect();
            let p = ch.position;
            println!(
                "      canal {:>2}  x={:+.2} y={:+.2} z={:+.2}  {}",
                ch.channel_id,
                p.x,
                p.y,
                p.z,
                names.join(", ")
            );
        }
    }
    Ok(())
}

fn pick_area(areas: Vec<EntertainmentConfiguration>, wanted: Option<&str>) -> Result<EntertainmentConfiguration> {
    match wanted {
        None => areas.into_iter().next().context("aucune zone de divertissement sur le bridge"),
        Some(w) => areas
            .into_iter()
            .find(|a| a.id == w || a.metadata.name.eq_ignore_ascii_case(w))
            .with_context(|| format!("zone « {w} » introuvable")),
    }
}

async fn test(area: Option<String>, effect: EffectArg, seconds: u64, fps: u32) -> Result<()> {
    let (stored, client) = connect().await?;
    let area = pick_area(client.entertainment_configurations().await?, area.as_deref())?;
    if area.channels.is_empty() {
        bail!("la zone « {} » n'a aucun canal", area.metadata.name);
    }
    println!("Zone « {} » : {} canaux", area.metadata.name, area.channels.len());

    client.set_streaming(&area.id, true).await.context("démarrage du streaming")?;
    let result = run_stream(&stored, &area, effect, seconds, fps).await;
    let stopped = client.set_streaming(&area.id, false).await;
    result?;
    stopped.context("arrêt du streaming")?;
    println!("Streaming arrêté.");
    Ok(())
}

async fn run_stream(
    stored: &StoredBridge,
    area: &EntertainmentConfiguration,
    effect: EffectArg,
    seconds: u64,
    fps: u32,
) -> Result<()> {
    let connect_start = Instant::now();
    let mut stream = EntertainmentStream::connect(stored.ip, &stored.credentials, &area.id).await?;
    println!("DTLS connecté en {:?}. Ctrl+C pour arrêter.", connect_start.elapsed());

    let mut fx = match effect {
        EffectArg::Blink => TestEffect::Blink,
        EffectArg::Chase => TestEffect::Chase,
        EffectArg::Sweep => TestEffect::Sweep,
    };

    let mut ticker = tokio::time::interval(Duration::from_secs_f64(1.0 / fps.max(1) as f64));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let start = Instant::now();
    let end = start + Duration::from_secs(seconds);
    let mut frames: u64 = 0;
    let mut worst_send = Duration::ZERO;

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        tokio::select! {
            _ = &mut ctrl_c => break,
            _ = ticker.tick() => {}
        }
        let now = Instant::now();
        if now >= end {
            break;
        }
        let colors = fx.render(&area.channels, (now - start).as_secs_f64());
        stream.send_rgb(&colors).await?;
        worst_send = worst_send.max(now.elapsed());
        frames += 1;
    }

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "{frames} images en {elapsed:.1} s ({:.1} img/s), envoi le plus lent : {worst_send:?}",
        frames as f64 / elapsed
    );
    stream.close().await
}

async fn stop(area: Option<String>) -> Result<()> {
    let (_, client) = connect().await?;
    let area = pick_area(client.entertainment_configurations().await?, area.as_deref())?;
    client.set_streaming(&area.id, false).await?;
    println!("Streaming arrêté sur « {} ».", area.metadata.name);
    Ok(())
}
