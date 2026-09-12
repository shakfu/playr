use cpal::traits::{DeviceTrait, HostTrait};
fn main() {
    let host = cpal::default_host();
    println!("host: {:?}", host.id());
    let dev = host.default_output_device().expect("no device");
    println!(
        "device: {:?}",
        dev.description().ok().map(|d| d.name().to_string())
    );
    if let Ok(d) = dev.default_output_config() {
        println!(
            "default: {} Hz, {} ch, {:?}",
            d.sample_rate(),
            d.channels(),
            d.sample_format()
        );
    }
    println!("supported ranges:");
    for c in dev.supported_output_configs().unwrap() {
        println!(
            "  {}-{} Hz  {} ch  {:?}",
            c.min_sample_rate(),
            c.max_sample_rate(),
            c.channels(),
            c.sample_format()
        );
    }
    println!("\nnegotiation for common source rates:");
    for (rate, ch) in [
        (44100u32, 2u16),
        (48000, 2),
        (96000, 2),
        (192000, 2),
        (22050, 1),
    ] {
        let spec = playr::audio::decode::Spec { rate, channels: ch };
        match playr::audio::output::negotiate(&dev, spec) {
            Ok(p) => println!(
                "  src {rate:>6} Hz {ch}ch -> dev {:>6} Hz {}ch {:?}  resample={}",
                p.rate,
                p.channels,
                p.format,
                p.needs_resample(spec)
            ),
            Err(e) => println!("  src {rate} Hz: {e}"),
        }
    }
}
