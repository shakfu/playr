use playr::audio::{Cmd, Player, State};
use std::time::{Duration, Instant};

fn main() {
    let files: Vec<std::path::PathBuf> = std::env::args().skip(1).map(Into::into).collect();
    let player = match Player::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("no audio: {e}");
            std::process::exit(2);
        }
    };
    player.send(Cmd::SetVolume(0.2));
    player.send(Cmd::Play(files.clone(), 0));

    let start = Instant::now();
    let mut last = String::new();
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let st = player.status();
        let pos = player.position();
        let cur = st
            .current()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .unwrap_or_default();
        let line = format!(
            "{cur} [{:?}] {:.1}s/{:.1}s src={:?} out={}Hz rs={}",
            st.state,
            pos.as_secs_f32(),
            st.duration.map(|d| d.as_secs_f32()).unwrap_or(0.0),
            st.source,
            st.output_rate,
            st.resampling
        );
        if line != last {
            println!("{:.1}s | {line}", start.elapsed().as_secs_f32());
            last = line;
        }
        if let Some(e) = &st.error {
            println!("ERROR: {e}");
        }
        if st.state == State::Stopped && start.elapsed() > Duration::from_millis(500) {
            break;
        }
        if start.elapsed() > Duration::from_secs(40) {
            println!("TIMEOUT");
            break;
        }
    }
    println!(
        "wall clock: {:.2}s for {} files",
        start.elapsed().as_secs_f32(),
        files.len()
    );
}
