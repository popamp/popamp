//! The sound card: ALSA on Linux, WASAPI on Windows, CoreAudio on macOS.
//! Each system's `Pcm` is a blocking, interleaved float stream with the
//! same calls.

#[cfg(linux_like)]
mod alsa;
#[cfg(linux_like)]
use alsa as system;
#[cfg(linux_like)]
pub use alsa::{available, AlsaPcm as Pcm};
#[cfg(target_os = "macos")]
mod coreaudio;
#[cfg(target_os = "macos")]
use coreaudio as system;
#[cfg(target_os = "macos")]
pub use coreaudio::{available, CoreAudioPcm as Pcm};
#[cfg(windows)]
mod wasapi;
#[cfg(windows)]
use wasapi as system;
#[cfg(windows)]
pub use wasapi::{available, WasapiPcm as Pcm};

/// The device the system plays on unless told otherwise.
pub const DEFAULT: &str = "default";

/// A playback device to choose from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// What the settings keep and [`Pcm::open`] takes.
    pub name: String,
    /// What people read.
    pub label: String,
}

/// The playback devices, the system's default first.
pub fn devices() -> Vec<Device> {
    let mut list = vec![Device { name: DEFAULT.into(), label: "Default device".into() }];
    list.extend(system::devices().into_iter().filter(|d| d.name != DEFAULT));
    list
}
