//! Slice points for samplers that take one file: a `cue ` chunk in the WAV,
//! which the Dirtywave M8 and the 1010music blackbox are reported to read,
//! and an Octatrack `.ot` file beside it.
//!
//! `docs/dev/hardware_samplers.md` has both layouts, where each came from,
//! and what is still unchecked. The `.ot` layout and its constants follow
//! ot_utils and OctaChainer; no device was tried.

use std::fs;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

/// Most slices an `.ot` file holds.
pub const OT_SLICES: usize = 64;

/// The tempo an `.ot` file is written with, as ot_utils writes it: the file's
/// length in bars is counted at this tempo too, so the two agree.
const OT_TEMPO: u32 = 124;

/// A `cue ` chunk with a point at each of `points`, frames from the start of
/// the audio, whole with its header. Both of a point's position fields hold
/// the frame, as common writers store it.
pub fn cue_chunk(points: &[u32]) -> Vec<u8> {
    let count = points.len() as u32;
    let mut chunk = Vec::with_capacity(12 + 24 * points.len());
    chunk.extend(b"cue ");
    chunk.extend((4 + 24 * count).to_le_bytes());
    chunk.extend(count.to_le_bytes());
    for (id, frame) in (1u32..).zip(points) {
        chunk.extend(id.to_le_bytes());
        chunk.extend(frame.to_le_bytes());
        chunk.extend(b"data");
        // One data chunk, uncompressed: no chunk or block to start from.
        chunk.extend([0; 8]);
        chunk.extend(frame.to_le_bytes());
    }
    chunk
}

/// Adds a `cue ` chunk for `points` to the WAV file at `path`, after its
/// audio, and corrects the file's length in its header.
pub fn append_cue(path: &Path, points: &[u32]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().read(true).write(true).open(path)?;
    // A chunk starts on an even byte; 24-bit mono audio can end on an odd one.
    if file.seek(SeekFrom::End(0))? % 2 == 1 {
        file.write_all(&[0])?;
    }
    file.write_all(&cue_chunk(points))?;
    let riff = file.stream_position()? - 8;
    let riff = u32::try_from(riff).map_err(|_| std::io::Error::other("over 4 GB"))?;
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&riff.to_le_bytes())
}

/// An Octatrack `.ot` file for audio of `frames` frames at `rate`, cut at
/// `slices`, each a start and an end in frames, at most [`OT_SLICES`] of
/// them. Its numbers are big-endian.
pub fn ot_file(rate: u32, frames: u32, slices: &[(u32, u32)]) -> Vec<u8> {
    assert!(slices.len() <= OT_SLICES, "an .ot file holds 64 slices");
    let mut ot = Vec::with_capacity(832);
    ot.extend(b"FORM\0\0\0\0DPS1SMPA\0\0\0\0\0\x02\0");
    let u32s = |ot: &mut Vec<u8>, values: &[u32]| {
        for v in values {
            ot.extend(v.to_be_bytes());
        }
    };
    // The length in beats at the tempo, rounded, in units of a 25th.
    let beats = (OT_TEMPO as f32 * frames as f32 / (rate * 60) as f32 + 0.5) as u32;
    // Tempo, trim length, loop length, stretch off, loop off.
    u32s(&mut ot, &[OT_TEMPO * 24, beats * 25, beats * 25, 0, 0]);
    // Gain, then quantize.
    ot.extend(48u16.to_be_bytes());
    ot.push(255);
    // Trim start, trim end, loop point.
    u32s(&mut ot, &[0, frames, 0]);
    for i in 0..OT_SLICES {
        match slices.get(i) {
            // Start, end, and a loop point at the slice's length.
            Some(&(start, end)) => u32s(&mut ot, &[start, end, end - start]),
            None => u32s(&mut ot, &[0, 0, 0]),
        }
    }
    u32s(&mut ot, &[slices.len() as u32]);
    let sum = ot[16..]
        .iter()
        .fold(0u16, |sum, b| sum.wrapping_add(*b as u16));
    ot.extend(sum.to_be_bytes());
    ot
}
