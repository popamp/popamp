//! Audio side of Popamp: decoding, DSP, sound output and the engine thread.

pub mod cover;
pub mod decoder;
pub mod dsp;
mod dylib;
pub mod engine;
pub mod formats;
mod midi;
mod opus;
pub mod output;
pub mod pipe;
pub mod readahead;
pub mod replaygain;
pub mod source;
pub mod stream;
pub mod tags;
mod tracker;

pub use decoder::{AudioSource, StreamInfo};
pub use dsp::EqSettings;
pub use engine::{Engine, EngineCmd, EngineEvent, PlayState, ReplayGainSettings, Shared};
pub use formats::read_meta;
