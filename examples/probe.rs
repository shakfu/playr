fn main() {
    for p in std::env::args().skip(1) {
        let path = std::path::Path::new(&p);
        match playr::audio::decode::AudioStream::open(path) {
            Ok(mut s) => {
                let spec = s.spec();
                let mut frames = 0usize;
                let mut peak = 0f32;
                let mut err = None;
                loop {
                    match s.next_chunk() {
                        Ok(Some(c)) => {
                            frames += c.len() / spec.channels.max(1) as usize;
                            for v in c {
                                peak = peak.max(v.abs());
                            }
                        }
                        Ok(None) => break,
                        Err(e) => {
                            err = Some(e.to_string());
                            break;
                        }
                    }
                }
                let secs = frames as f64 / spec.rate.max(1) as f64;
                println!(
                    "{:<12} OK  {}Hz {}ch  {:>8} frames ({:.2}s) peak={:.3} {}",
                    path.file_name().unwrap().to_string_lossy(),
                    spec.rate,
                    spec.channels,
                    frames,
                    secs,
                    peak,
                    err.map(|e| format!("ERR: {e}")).unwrap_or_default()
                );
            }
            Err(e) => println!(
                "{:<12} FAIL {e}",
                path.file_name().unwrap().to_string_lossy()
            ),
        }
    }
}
