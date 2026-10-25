//! Opus decoding, plugged into Symphonia's codec registry.
//!
//! Symphonia 0.6 demuxes Opus in both OGG and Matroska but ships no decoder for
//! it, so a file reaches [`AudioDecoder`] fully parsed and then fails. This
//! module supplies the missing decoder, which makes both `.opus` and
//! Opus-in-WebM playable without touching the rest of the player.
//!
//! Decoding itself is delegated to libopus through the `opus` crate. libopus is
//! the reference implementation, so it is the conservative choice for a codec on
//! the playback path. The cost is that building playr now needs a C compiler and
//! cmake, which vendor and build libopus.

use symphonia::core::audio::GenericAudioBufferRef;
use symphonia::core::audio::{AsGenericAudioBufferRef, AudioBuffer, AudioMut, AudioSpec};
use symphonia::core::codecs::audio::well_known::CODEC_ID_OPUS;
use symphonia::core::codecs::audio::{
    AudioCodecParameters, AudioDecoder, AudioDecoderOptions, FinalizeResult,
};
use symphonia::core::codecs::registry::{RegisterableAudioDecoder, SupportedAudioCodec};
use symphonia::core::codecs::CodecInfo;
use symphonia::core::errors::{decode_error, unsupported_error, Result};
use symphonia::core::packet::PacketRef;

/// Opus decodes at a fixed 48kHz. Other rates are only a decoder-side
/// convenience resample, so the native rate is always used here.
const OPUS_RATE: u32 = 48_000;

/// Longest Opus frame is 120ms, which is 5760 samples per channel at 48kHz.
const MAX_FRAME: usize = 5760;

/// An OpusHead identification header, when `extra_data` holds one.
///
/// It is `"OpusHead"`, version, channel count, pre-skip, input sample rate,
/// then output gain, so the fields below are at fixed offsets.
fn opus_head(extra_data: Option<&[u8]>) -> Option<&[u8]> {
    extra_data.filter(|d| d.len() >= 18 && d.starts_with(b"OpusHead"))
}

/// The pre-skip field, a little-endian u16 at offset 10.
///
/// Containers do not report this as a packet trim, so discarding it is the
/// decoder's job: left in, every Opus track begins with a few milliseconds of
/// encoder warm-up.
fn pre_skip(extra_data: Option<&[u8]>) -> usize {
    match opus_head(extra_data) {
        Some(d) => u16::from_le_bytes([d[10], d[11]]) as usize,
        None => 0,
    }
}

/// The factor for the output gain field, a signed little-endian Q7.8 dB value
/// at offset 16.
///
/// RFC 7845 section 5.1 requires a player to apply it. Encoders write 0 unless
/// a tool such as `rsgain` has set the file's level there, and `R128_*_GAIN`
/// tags count from it, so ignoring it left both wrong by the same amount.
fn output_gain(extra_data: Option<&[u8]>) -> f32 {
    match opus_head(extra_data) {
        Some(d) => {
            let q78 = i16::from_le_bytes([d[16], d[17]]);
            10f32.powf(q78 as f32 / 256.0 / 20.0)
        }
        None => 1.0,
    }
}

/// The multistream layout of a channel mapping family 1 header: stream
/// count, coupled stream count, and each output channel's stream channel,
/// from offset 19. `None` for family 0, mono or stereo in one stream.
fn multistream(extra_data: Option<&[u8]>) -> Option<(u8, u8, Vec<u8>)> {
    let d = opus_head(extra_data)?;
    let channels = d[9] as usize;
    (d[18] == 1 && d.len() >= 21 + channels).then(|| (d[19], d[20], d[21..21 + channels].to_vec()))
}

/// For each plane Symphonia orders by position, as WAVE does, the channel of
/// libopus's output in Vorbis order that fills it; RFC 7845 section 5.1.1.2
/// gives that order. FFmpeg's `ff_vorbis_channel_layout_offsets`.
const VORBIS_TO_WAVE: [&[usize]; 8] = [
    &[0],
    &[0, 1],
    &[0, 2, 1],
    &[0, 1, 2, 3],
    &[0, 2, 1, 3, 4],
    &[0, 2, 1, 5, 3, 4],
    &[0, 2, 1, 6, 5, 3, 4],
    &[0, 2, 1, 7, 5, 6, 3, 4],
];

/// libopus decoder state, marked `Sync`.
///
/// `opus::Decoder` declares `Send` but not `Sync`, while Symphonia's
/// `AudioDecoder` requires both. `Sync` is sound here: every method that changes
/// decoder state takes `&mut self`, and the one `&self` method
/// (`get_nb_samples`) hands the decoder to libopus as a `const` pointer and only
/// parses the packet. A shared `&Decoder` therefore cannot reach mutable libopus
/// state from any thread.
enum SyncDecoder {
    One(opus::Decoder),
    /// Surround: several streams, each mono or a coupled pair.
    Multi(opus::MSDecoder),
}

// SAFETY: see the type comment above; `MSDecoder` is the same shape.
unsafe impl Sync for SyncDecoder {}

impl SyncDecoder {
    fn decode_float(&mut self, packet: &[u8], out: &mut [f32]) -> opus::Result<usize> {
        match self {
            SyncDecoder::One(d) => d.decode_float(packet, out, false),
            SyncDecoder::Multi(d) => d.decode_float(packet, out, false),
        }
    }

    fn reset_state(&mut self) -> opus::Result<()> {
        match self {
            SyncDecoder::One(d) => d.reset_state(),
            SyncDecoder::Multi(d) => d.reset_state(),
        }
    }
}

pub struct OpusDecoder {
    inner: SyncDecoder,
    params: AudioCodecParameters,
    buf: AudioBuffer<f32>,
    /// Interleaved output from the decoder, before it is de-interleaved.
    scratch: Vec<f32>,
    channels: usize,
    /// Frames of encoder pre-skip still to be discarded at the start of the stream.
    skip_left: usize,
    /// The header's output gain as a factor; 1.0 leaves samples untouched.
    gain: f32,
    /// The decoder's channel for each plane; see [`VORBIS_TO_WAVE`].
    order: &'static [usize],
}

impl OpusDecoder {
    pub fn try_new(params: &AudioCodecParameters, _opts: &AudioDecoderOptions) -> Result<Self> {
        let channels = match params.channels.as_ref() {
            Some(ch) => ch.count(),
            None => return unsupported_error("opus: channel layout is missing"),
        };
        if channels == 0 || channels > VORBIS_TO_WAVE.len() {
            return unsupported_error("opus: 1 to 8 channels are supported");
        }
        let failed = |_| symphonia::core::errors::Error::Unsupported("opus: decoder setup failed");
        let (inner, order) = match multistream(params.extra_data.as_deref()) {
            Some((streams, coupled, mapping)) if mapping.len() == channels => (
                SyncDecoder::Multi(
                    opus::MSDecoder::new(OPUS_RATE, streams, coupled, &mapping).map_err(failed)?,
                ),
                VORBIS_TO_WAVE[channels - 1],
            ),
            _ if channels > 2 => {
                return unsupported_error("opus: surround without a multistream header")
            }
            _ => {
                let layout = match channels {
                    1 => opus::Channels::Mono,
                    _ => opus::Channels::Stereo,
                };
                let one = opus::Decoder::new(OPUS_RATE, layout).map_err(failed)?;
                (SyncDecoder::One(one), VORBIS_TO_WAVE[channels - 1])
            }
        };

        // Reuse the layout the demuxer reported rather than inventing one.
        let spec = AudioSpec::new(OPUS_RATE, params.channels.clone().unwrap());

        let mut params = params.clone();
        params.sample_rate = Some(OPUS_RATE);

        Ok(OpusDecoder {
            inner,
            buf: AudioBuffer::new(spec, MAX_FRAME),
            scratch: vec![0.0; MAX_FRAME * channels],
            channels,
            skip_left: pre_skip(params.extra_data.as_deref()),
            gain: output_gain(params.extra_data.as_deref()),
            order,
            params,
        })
    }

    fn decode_inner(&mut self, packet: &PacketRef<'_>) -> Result<()> {
        let frames = self
            .inner
            .decode_float(packet.data, &mut self.scratch)
            .map_err(|_| symphonia::core::errors::Error::DecodeError("opus: malformed packet"))?;

        if frames > MAX_FRAME {
            return decode_error("opus: frame longer than the format allows");
        }

        self.buf.clear();
        self.buf.render_uninit(Some(frames));

        // libopus returns interleaved samples in Vorbis order; Symphonia
        // buffers are planar, by position.
        let channels = self.channels;
        for (ch, &from) in self.order.iter().enumerate() {
            let plane = match self.buf.plane_mut(ch) {
                Some(p) => p,
                None => return decode_error("opus: audio buffer has too few planes"),
            };
            for (i, sample) in plane.iter_mut().enumerate().take(frames) {
                *sample = self.scratch[i * channels + from] * self.gain;
            }
        }

        // Container-level trims. For OGG this is the end padding; Matroska
        // reports neither. Every other Symphonia decoder trims here too, so
        // doing it anywhere else would trim twice.
        let mut trim_start = packet.trim_start.get() as usize;

        // Stream-start pre-skip, which no container reports as a trim.
        if self.skip_left > 0 {
            let skip = self.skip_left.min(frames);
            self.skip_left -= skip;
            trim_start += skip;
        }

        self.buf.trim(trim_start, packet.trim_end.get() as usize);
        Ok(())
    }
}

impl AudioDecoder for OpusDecoder {
    fn reset(&mut self) {
        // Failure here would mean the decoder handle is gone, which cannot
        // happen while `self` is alive.
        let _ = self.inner.reset_state();
        // Reset follows a seek, so the stream start is behind us and the
        // pre-skip has either been applied already or no longer applies.
        self.skip_left = 0;
    }

    fn codec_info(&self) -> &CodecInfo {
        &Self::supported_codecs()
            .first()
            .expect("opus codec is registered")
            .info
    }

    fn codec_params(&self) -> &AudioCodecParameters {
        &self.params
    }

    fn decode_ref(&mut self, packet: &PacketRef<'_>) -> Result<GenericAudioBufferRef<'_>> {
        match self.decode_inner(packet) {
            Ok(()) => Ok(self.buf.as_generic_audio_buffer_ref()),
            Err(e) => {
                // The trait requires an empty buffer after a failed decode.
                self.buf.clear();
                Err(e)
            }
        }
    }

    fn finalize(&mut self) -> FinalizeResult {
        Default::default()
    }

    fn last_decoded(&self) -> GenericAudioBufferRef<'_> {
        self.buf.as_generic_audio_buffer_ref()
    }
}

impl RegisterableAudioDecoder for OpusDecoder {
    fn try_registry_new(
        params: &AudioCodecParameters,
        opts: &AudioDecoderOptions,
    ) -> Result<Box<dyn AudioDecoder>>
    where
        Self: Sized,
    {
        Ok(Box::new(OpusDecoder::try_new(params, opts)?))
    }

    fn supported_codecs() -> &'static [SupportedAudioCodec] {
        // Written out rather than via `support_audio_codec!`, whose expansion
        // refers to `symphonia_core` by a path only that crate has.
        &[SupportedAudioCodec {
            id: CODEC_ID_OPUS,
            info: CodecInfo {
                short_name: "opus",
                long_name: "Opus",
                profiles: &[],
            },
        }]
    }
}
