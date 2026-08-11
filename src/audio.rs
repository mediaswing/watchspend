//! The two sounds the app makes: one when something worked, one when it did
//! not.
//!
//! Sound is a courtesy, never a requirement — a machine with no audio device,
//! or one whose device disappears when a dock is unplugged, should still be a
//! perfectly good budgeting app. So every failure in here is swallowed after
//! being logged once.

use std::io::Cursor;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Source as _};

const SUCCESS_WAV: &[u8] = include_bytes!("../assets/sounds/success.wav");
const ERROR_WAV: &[u8] = include_bytes!("../assets/sounds/error.wav");

/// Both cues are levelled to the same loudness in the asset files, so a single
/// gain keeps them polite without making the failure sound the quiet one.
const GAIN: f32 = 0.7;

pub struct Sounds {
    /// Holds the audio device open; playback stops when this is dropped.
    device: Option<MixerDeviceSink>,
}

impl Sounds {
    pub fn new() -> Self {
        let device = match DeviceSinkBuilder::open_default_sink() {
            Ok(device) => Some(device),
            Err(err) => {
                log::warn!("no audio output, carrying on in silence: {err}");
                None
            }
        };
        Self { device }
    }

    pub fn success(&self) {
        self.play(SUCCESS_WAV);
    }

    pub fn failure(&self) {
        self.play(ERROR_WAV);
    }

    fn play(&self, wav: &'static [u8]) {
        let Some(device) = &self.device else {
            return;
        };
        match Decoder::new_wav(Cursor::new(wav)) {
            // `add` mixes rather than queues, so a quick series of actions
            // does not build up a backlog of sounds playing late.
            Ok(source) => device.mixer().add(source.amplify(GAIN)),
            Err(err) => log::warn!("could not decode a sound effect: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The assets are baked into the binary, so a bad or missing file is a
    /// build-time fact rather than something to discover on the day something
    /// goes wrong and the app has nothing to say about it.
    #[test]
    fn both_cues_decode_to_audible_samples() {
        for (name, wav) in [("success", SUCCESS_WAV), ("error", ERROR_WAV)] {
            let source = Decoder::new_wav(Cursor::new(wav))
                .unwrap_or_else(|e| panic!("{name}.wav does not decode: {e}"));
            assert!(source.count() > 0, "{name}.wav decodes to no samples");
        }
    }
}
