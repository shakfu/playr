//! Symphonia wrapper: a file becomes a stream of interleaved f32 frames.
//!
//! This module is the seam for format support. Everything above it sees only
//! `Spec` and `&[f32]`, so adding a decoder Symphonia lacks (Opus, WavPack)
//! means adding a variant here, not changing the player.

use std::fs::File;
use std::path::Path;
use std::time::Duration;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase};

/// Symphonia's built-in decoders plus the ones playr adds.
///
/// Built once: constructing a registry walks every codec, and a track change
/// would otherwise pay for it again.
fn codecs() -> &'static CodecRegistry {
    static REGISTRY: std::sync::OnceLock<CodecRegistry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        #[cfg(feature = "opus")]
        registry.register_audio_decoder::<crate::audio::opus::OpusDecoder>();
        registry
    })
}

/// Sample rate and channel count of a decoded stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    pub rate: u32,
    pub channels: u16,
}

#[derive(Debug)]
pub enum DecodeError {
    Io(std::io::Error),
    /// The container parsed but no decoder is registered for its codec.
    /// Opus and WavPack land here: Symphonia 0.6 demuxes OGG but cannot decode Opus.
    Unsupported(String),
    Other(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Io(e) => write!(f, "io: {e}"),
            // Symphonia's own text ("core (codec): unsupported audio codec")
            // says nothing a listener can act on, so point at `playr formats`.
            DecodeError::Unsupported(_) => {
                write!(f, "no decoder for this format (see: playr formats)")
            }
            DecodeError::Other(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for DecodeError {}

impl From<SymphError> for DecodeError {
    fn from(e: SymphError) -> Self {
        match e {
            SymphError::IoError(e) => DecodeError::Io(e),
            SymphError::Unsupported(s) => DecodeError::Unsupported(s.to_string()),
            other => DecodeError::Other(other.to_string()),
        }
    }
}

/// A decoded audio file, pulled one packet at a time.
pub struct AudioStream {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    spec: Spec,
    duration: Option<Duration>,
    /// Scratch for the interleaved f32 conversion, reused across packets.
    buf: Vec<f32>,
}

impl AudioStream {
    /// Opens `path` and prepares a decoder for its default audio track.
    pub fn open(path: &Path) -> Result<Self, DecodeError> {
        let file = File::open(path).map_err(DecodeError::Io)?;
        let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let reader = symphonia::default::get_probe().probe(
            &hint,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )?;

        let track = reader
            .first_track_known_codec(TrackType::Audio)
            .ok_or_else(|| DecodeError::Unsupported("no decodable audio track".into()))?;

        let track_id = track.id;
        let time_base = track.time_base;
        let duration = track
            .duration
            .zip(time_base)
            .and_then(|(d, tb)| tb.calc_duration(d))
            .map(|t| Duration::from_secs_f64(t.as_secs_f64().max(0.0)));

        let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
            return Err(DecodeError::Unsupported(
                "track has no audio codec parameters".into(),
            ));
        };

        let decoder = codecs().make_audio_decoder(&params, &AudioDecoderOptions::default())?;

        // Rate and channels are not always in the container header; when absent
        // they are filled in from the first decoded buffer.
        let spec = Spec {
            rate: params.sample_rate.unwrap_or(0),
            channels: params
                .channels
                .as_ref()
                .map(|c| c.count() as u16)
                .unwrap_or(0),
        };

        Ok(AudioStream {
            reader,
            decoder,
            track_id,
            time_base,
            spec,
            duration,
            buf: Vec::new(),
        })
    }

    pub fn spec(&self) -> Spec {
        self.spec
    }

    pub fn duration(&self) -> Option<Duration> {
        self.duration
    }

    /// Decodes the next packet into interleaved f32. `Ok(None)` means end of stream.
    ///
    /// Packets that fail to decode are skipped rather than ending playback: a
    /// single corrupt frame in the middle of a file should not stop the track.
    pub fn next_chunk(&mut self) -> Result<Option<&[f32]>, DecodeError> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(None),
                Err(SymphError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Ok(None)
                }
                Err(e) => return Err(e.into()),
            };

            if packet.track_id != self.track_id {
                continue;
            }

            match self.decoder.decode(&packet) {
                Ok(decoded) => {
                    // Fill in rate/channels the container omitted, from the
                    // first decoded buffer.
                    if self.spec.rate == 0 {
                        self.spec.rate = decoded.spec().rate();
                    }
                    if self.spec.channels == 0 {
                        self.spec.channels = decoded.spec().channels().count() as u16;
                    }
                    if decoded.frames() == 0 {
                        continue;
                    }
                    // Encoder delay and padding are already trimmed by the
                    // demuxers that support gapless (FLAC, MP3, Vorbis, WAV,
                    // AIFF, ALAC). Re-applying `packet.trim_*` here double-trims
                    // them. AAC has no gapless support upstream and decodes
                    // ~1900 frames long; that is a known limitation.
                    decoded.copy_to_vec_interleaved(&mut self.buf);
                    return Ok(Some(&self.buf));
                }
                // Recoverable per the Symphonia contract: skip and keep going.
                Err(SymphError::DecodeError(_)) => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Seeks to `pos` from the start of the track.
    pub fn seek(&mut self, pos: Duration) -> Result<(), DecodeError> {
        let time = Time::try_new(pos.as_secs() as i64, pos.subsec_nanos())
            .ok_or_else(|| DecodeError::Other("seek position out of range".into()))?;
        self.reader.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        )?;
        // The decoder holds state from before the seek; discarding it prevents
        // a burst of garbage frames at the new position.
        self.decoder.reset();
        Ok(())
    }

    /// Whether this stream can seek. Streams from non-seekable sources cannot.
    pub fn time_base(&self) -> Option<TimeBase> {
        self.time_base
    }
}
