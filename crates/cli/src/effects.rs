use hue::{Channel, ChannelRgb};

pub enum TestEffect {
    /// Every channel alternates red / blue at 2 Hz.
    Blink,
    /// One channel at a time, ordered along x.
    Chase,
    /// A bar moving back and forth along x, using channel positions.
    Sweep,
}

const SWEEP_PERIOD_S: f64 = 3.0;
const SWEEP_WIDTH: f64 = 0.5;

impl TestEffect {
    pub fn render(&mut self, channels: &[Channel], t: f64) -> Vec<ChannelRgb> {
        match self {
            TestEffect::Blink => {
                let rgb = if (t * 2.0) as u64 % 2 == 0 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] };
                channels.iter().map(|c| to_rgb(c.channel_id, rgb, 1.0)).collect()
            }
            TestEffect::Chase => {
                let mut order: Vec<&Channel> = channels.iter().collect();
                order.sort_by(|a, b| a.position.x.total_cmp(&b.position.x));
                let active = (t * 4.0) as usize % order.len();
                order
                    .iter()
                    .enumerate()
                    .map(|(i, c)| to_rgb(c.channel_id, [1.0, 0.6, 0.2], if i == active { 1.0 } else { 0.03 }))
                    .collect()
            }
            TestEffect::Sweep => {
                // Triangle wave from -1 to 1 and back.
                let phase = (t / SWEEP_PERIOD_S).fract();
                let pos = if phase < 0.5 { phase * 4.0 - 1.0 } else { 3.0 - phase * 4.0 };
                channels
                    .iter()
                    .map(|c| {
                        let d = (c.position.x - pos).abs();
                        let level = (1.0 - d / SWEEP_WIDTH).max(0.0);
                        to_rgb(c.channel_id, [0.0, 0.8, 1.0], 0.02 + 0.98 * level)
                    })
                    .collect()
            }
        }
    }
}

fn to_rgb(channel_id: u8, rgb: [f64; 3], level: f64) -> ChannelRgb {
    let q = |v: f64| ((v * level).clamp(0.0, 1.0) * u16::MAX as f64).round() as u16;
    ChannelRgb { channel_id, rgb: [q(rgb[0]), q(rgb[1]), q(rgb[2])] }
}
