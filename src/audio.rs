//! Short synthesized sound effects, loaded as WAV bytes.
//!
//! Nothing here is recorded audio. Each cue is a few sine tones and noise
//! bursts so the game can ship without an audio asset pack.

use crate::cues::Cue;
use macroquad::audio::{load_sound_from_bytes, play_sound, PlaySoundParams, Sound};
use std::f32::consts::TAU;

const RATE: f32 = 22_050.0;

pub struct Bank {
    sounds: Vec<Option<Sound>>,
}

impl Bank {
    pub async fn load() -> Self {
        let mut sounds = Vec::with_capacity(Cue::ALL.len());
        for cue in Cue::ALL {
            let bytes = synthesize(cue);
            match load_sound_from_bytes(&bytes).await {
                Ok(sound) => sounds.push(Some(sound)),
                Err(err) => {
                    eprintln!("could not load {cue:?} sound: {err}");
                    sounds.push(None);
                }
            }
        }
        Self { sounds }
    }

    pub fn play(&self, cues: &[Cue], volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        if volume <= 0.001 {
            return;
        }
        for cue in cues {
            if let Some(Some(sound)) = self.sounds.get(cue.index()) {
                play_sound(sound, PlaySoundParams { looped: false, volume });
            }
        }
    }
}

pub fn synthesize(cue: Cue) -> Vec<u8> {
    let samples = match cue {
        Cue::War => mix(&[
            tone(73.4, 0.42, 5.0, 0.55),
            tone(146.8, 0.42, 4.5, 0.35),
            tone(155.6, 0.42, 4.5, 0.28),
        ]),
        Cue::Alliance => mix(&[tone(523.25, 0.32, 6.0, 0.34), tone(659.25, 0.32, 6.0, 0.30)]),
        Cue::Break => mix(&[tone(493.88, 0.16, 8.0, 0.40), delay(0.11, tone(369.99, 0.22, 6.0, 0.40))]),
        Cue::Peace => mix(&[
            tone(261.63, 0.48, 3.5, 0.26),
            tone(329.63, 0.48, 3.5, 0.24),
            tone(392.00, 0.48, 3.5, 0.22),
        ]),
        Cue::Annex => mix(&[tone(98.0, 0.28, 9.0, 0.70), burst(0.06, 36.0, 0.35, 3)]),
        Cue::Fanfare => mix(&[
            tone(523.25, 0.20, 8.0, 0.36),
            delay(0.10, tone(659.25, 0.20, 8.0, 0.36)),
            delay(0.20, tone(783.99, 0.22, 7.0, 0.36)),
            delay(0.30, tone(1046.50, 0.26, 6.0, 0.32)),
        ]),
        Cue::Election => mix(&[tone(880.0, 0.36, 5.0, 0.32), tone(1760.0, 0.22, 9.0, 0.12)]),
        Cue::Disaster => mix(&[tone(48.0, 0.50, 4.0, 0.62), burst(0.45, 6.0, 0.28, 11)]),
        Cue::Flood => flood(),
        Cue::Pirates => mix(&[
            tone(233.08, 0.24, 10.0, 0.36),
            tone(277.18, 0.24, 10.0, 0.30),
            tone(349.23, 0.24, 10.0, 0.28),
        ]),
        Cue::Founded => mix(&[
            burst(0.04, 50.0, 0.30, 5),
            delay(0.03, tone(523.25, 0.16, 10.0, 0.32)),
            delay(0.12, tone(783.99, 0.20, 8.0, 0.30)),
        ]),
        Cue::Built => mix(&[burst(0.035, 55.0, 0.45, 7), delay(0.09, burst(0.04, 50.0, 0.40, 9))]),
        Cue::Puppet => tone(196.0, 0.30, 7.0, 0.46),
        Cue::Freedom => mix(&[
            tone(392.0, 0.14, 9.0, 0.34),
            delay(0.08, tone(523.25, 0.14, 9.0, 0.34)),
            delay(0.16, tone(659.25, 0.20, 7.0, 0.34)),
        ]),
        Cue::Eliminated => mix(&[
            tone(392.0, 0.22, 6.0, 0.36),
            delay(0.14, tone(349.23, 0.22, 6.0, 0.36)),
            delay(0.28, tone(311.13, 0.22, 5.5, 0.36)),
            delay(0.42, tone(261.63, 0.28, 4.5, 0.38)),
        ]),
        Cue::Surrender => chirp(349.0, 174.0, 0.42, 0.46),
        Cue::Occupy => mix(&[tone(587.33, 0.16, 12.0, 0.40), tone(880.0, 0.10, 16.0, 0.18)]),
        Cue::Battle => mix(&[
            burst(0.05, 40.0, 0.55, 2),
            tone(70.0, 0.34, 7.0, 0.62),
            tone(140.0, 0.18, 10.0, 0.22),
        ]),
        Cue::Siege => mix(&[tone(55.0, 0.46, 5.5, 0.72), burst(0.12, 14.0, 0.30, 4)]),
        Cue::Shot => mix(&[burst(0.045, 48.0, 0.60, 8), tone(180.0, 0.10, 22.0, 0.28)]),
        Cue::Select => mix(&[tone(660.0, 0.07, 28.0, 0.32), delay(0.06, tone(990.0, 0.09, 22.0, 0.30))]),
        Cue::Click => tone(1400.0, 0.035, 70.0, 0.40),
    };
    wav(&samples)
}

fn tone(freq: f32, dur: f32, decay: f32, vol: f32) -> Vec<f32> {
    let n = samples(dur);
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            (t * freq * TAU).sin() * (-t * decay).exp() * vol
        })
        .collect()
}

fn chirp(f0: f32, f1: f32, dur: f32, vol: f32) -> Vec<f32> {
    let n = samples(dur);
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE;
            let f = f0 + (f1 - f0) * (t / dur);
            phase += f / RATE * TAU;
            phase.sin() * (1.0 - t / dur).powi(2) * vol
        })
        .collect()
}

fn burst(dur: f32, decay: f32, vol: f32, seed: u32) -> Vec<f32> {
    let n = samples(dur);
    let mut x = seed | 1;
    (0..n)
        .map(|i| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let s = ((x >> 16) as i16 as f32) / 32_768.0;
            let t = i as f32 / RATE;
            s * (-t * decay).exp() * vol
        })
        .collect()
}

/// Soft noise swept downward, so a flood does not sound like a cannon.
fn flood() -> Vec<f32> {
    let dur = 0.46;
    let n = samples(dur);
    let mut x = 13u32;
    (0..n)
        .map(|i| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let s = ((x >> 16) as i16 as f32) / 32_768.0;
            let t = i as f32 / RATE;
            let swell = (t * 8.0).min(1.0) * (1.0 - t / dur);
            let wobble = 0.65 + 0.35 * (t * 18.0 * TAU).sin();
            s * swell * wobble * 0.42
        })
        .collect()
}

fn delay(seconds: f32, mut samples: Vec<f32>) -> Vec<f32> {
    let lead = (RATE * seconds) as usize;
    let mut out = vec![0.0; lead];
    out.append(&mut samples);
    out
}

fn mix(tracks: &[Vec<f32>]) -> Vec<f32> {
    let n = tracks.iter().map(|t| t.len()).max().unwrap_or(0);
    let mut out = vec![0.0; n];
    for track in tracks {
        for (i, sample) in track.iter().enumerate() {
            out[i] += sample;
        }
    }
    let peak = out.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    if peak > 0.95 {
        let gain = 0.95 / peak;
        for sample in &mut out {
            *sample *= gain;
        }
    }
    out
}

fn samples(dur: f32) -> usize {
    (RATE * dur).round().max(1.0) as usize
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let data_bytes = (samples.len() * 2) as u32;
    let rate = RATE as u32;
    let mut data = Vec::with_capacity(44 + data_bytes as usize);
    data.extend_from_slice(b"RIFF");
    data.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    data.extend_from_slice(b"WAVEfmt ");
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&rate.to_le_bytes());
    data.extend_from_slice(&(rate * 2).to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(b"data");
    data.extend_from_slice(&data_bytes.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32_767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cue_is_a_short_audible_wav() {
        let mut previous = Vec::new();
        for cue in Cue::ALL {
            let bytes = synthesize(cue);
            assert_eq!(&bytes[..4], b"RIFF", "{cue:?}");
            assert_eq!(&bytes[8..12], b"WAVE", "{cue:?}");
            assert!(bytes.len() > 44, "{cue:?}");
            // Under a second of 16-bit mono, so several can overlap.
            assert!(bytes.len() < (RATE as usize) * 2 + 44, "{cue:?} is too long");
            let mut peak = 0u16;
            for frame in bytes[44..].chunks_exact(2) {
                let sample = i16::from_le_bytes([frame[0], frame[1]]);
                peak = peak.max(sample.unsigned_abs());
            }
            assert!(peak > 1000, "{cue:?} is silent");
            assert_ne!(bytes, previous, "{cue:?} repeats the previous sound");
            previous = bytes;
        }
    }
}
