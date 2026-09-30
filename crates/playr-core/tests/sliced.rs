//! Slice points for samplers that take one file: the `cue ` chunk and the
//! Octatrack's `.ot` file.

use playr_core::sliced::{append_cue, cue_chunk, ot_file, OT_SLICES};

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

/// The chunks of the RIFF file in `bytes` after its 12-byte header: each
/// one's name and data.
fn chunks(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    assert_eq!((&bytes[..4], &bytes[8..12]), (&b"RIFF"[..], &b"WAVE"[..]));
    let riff = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(riff + 8, bytes.len(), "the header's length is the file's");
    let mut found = Vec::new();
    let mut at = 12;
    while at < bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let name = String::from_utf8_lossy(&bytes[at..at + 4]).into_owned();
        found.push((name, bytes[at + 8..at + 8 + size].to_vec()));
        // A chunk of odd length is followed by a pad byte.
        at += 8 + size + size % 2;
    }
    found
}

#[test]
fn an_ot_file_is_byte_for_byte_what_ot_utils_writes() {
    // ot_utils 0.1.5, given mono 44.1 kHz files of 1,000, 2,500 and 40,000
    // frames, wrote these 832 bytes: 94, then zeros, then the last 6.
    let ot = ot_file(
        44_100,
        43_500,
        &[(0, 1_000), (1_000, 3_500), (3_500, 43_500)],
    );
    assert_eq!(ot.len(), 832);
    assert_eq!(
        ot[..94],
        hex("464f524d0000000044505331534d50410000000000020000000ba0000000320000003200000000000000000030ff000000000000a9ec0000000000000000000003e8000003e8000003e800000dac000009c400000dac0000a9ec00009c40")
    );
    assert!(ot[94..826].iter().all(|b| *b == 0));
    assert_eq!(ot[826..], hex("000000030b49"));
}

#[test]
fn an_ot_file_holds_64_slices_and_its_checksum_wraps() {
    // Bytes large enough to carry the sum past 16 bits.
    let slices = [(0xfefe_fefe, 0xffff_ffff); OT_SLICES];
    let ot = ot_file(48_000, 0xffff_ffff, &slices);
    assert_eq!(ot.len(), 832);
    // The last slice's start, end and loop point, then the count.
    assert_eq!(ot[814..826], hex("fefefefeffffffff01010101"));
    assert_eq!(ot[826..830], 64u32.to_be_bytes());
    // Every byte from 16 on, summed in 16 bits.
    let sum = ot[16..830].iter().fold(0u32, |sum, b| sum + *b as u32);
    assert!(sum > u16::MAX as u32, "this case must overflow 16 bits");
    assert_eq!(ot[830..], (sum as u16).to_be_bytes());
}

#[test]
fn a_cue_chunk_holds_a_point_a_frame_in_little_endian() {
    assert_eq!(
        cue_chunk(&[0, 0x0001_e240]),
        [
            &b"cue "[..],
            &hex("34000000"), // 4 + 2 x 24 bytes follow
            &hex("02000000"),
            &hex("01000000"), // the first point's ID
            &hex("00000000"),
            b"data",
            &hex("0000000000000000"),
            &hex("00000000"),
            &hex("02000000"),
            &hex("40e20100"), // frame 123,456
            b"data",
            &hex("0000000000000000"),
            &hex("40e20100"),
        ]
        .concat()
    );
    assert_eq!(cue_chunk(&[]).len(), 12);
}

#[test]
fn a_cue_chunk_is_added_after_the_audio_on_an_even_byte() {
    let dir = tempfile::tempdir().unwrap();
    // Mono 24-bit with an odd number of frames: the audio is an odd length.
    for frames in [3, 4] {
        let path = dir.path().join(format!("{frames}.wav"));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for n in 0..frames {
            writer.write_sample(1_000 * (n + 1)).unwrap();
        }
        writer.finalize().unwrap();

        append_cue(&path, &[0, 2]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len() % 2, 0);
        let chunks = chunks(&bytes);
        let names: Vec<&str> = chunks.iter().map(|c| c.0.as_str()).collect();
        assert_eq!(names.last(), Some(&"cue "), "{names:?}");
        let data = &chunks.iter().find(|c| c.0 == "data").unwrap().1;
        assert_eq!(data.len(), frames as usize * 3);
        assert_eq!(chunks.last().unwrap().1, cue_chunk(&[0, 2])[8..]);
        // The audio still reads as it was written.
        let samples: Vec<i32> = hound::WavReader::open(&path)
            .unwrap()
            .samples::<i32>()
            .map(Result::unwrap)
            .collect();
        assert_eq!(
            samples,
            (0..frames).map(|n| 1_000 * (n + 1)).collect::<Vec<_>>()
        );
    }
}
