//! Decoders for various audio formats and file types.
//!
//! These are normally accessed via
//! [sounds::open_file][crate::sounds::open_file()].
#[cfg(feature = "rmp3-mp3")]
mod mp3;
#[cfg(feature = "qoa")]
mod qoa;
#[cfg(feature = "symphonia")]
mod symphonia;
#[cfg(feature = "symphonia")]
mod opus;
#[cfg(feature = "hound-wav")]
mod wav;

#[cfg(feature = "rmp3-mp3")]
pub use mp3::Mp3Decoder;
#[cfg(feature = "qoa")]
pub use qoa::QoaDecoder;
#[cfg(feature = "qoa")]
pub use qoaudio::DecodeError as QoaDecodeError;

use once_cell::sync::Lazy;

/// Default Symphonia [`CodecRegistry`], including the (audiopus-backed) Opus codec.
pub static CODEC_REGISTRY: Lazy<CodecRegistry> = Lazy::new(|| {
    let mut registry = CodecRegistry::new();
    register_enabled_codecs(&mut registry);
    registry.register_all::<OpusDecoder>();
    registry
});

use ::symphonia::default::register_enabled_codecs;
#[cfg(feature = "symphonia")]
pub use symphonia::SymphoniaDecoder;
use symphonia_core::codecs::CodecRegistry;
#[cfg(feature = "hound-wav")]
pub use wav::WavDecoder;

#[cfg(feature = "symphonia")]
use self::opus::OpusDecoder;

