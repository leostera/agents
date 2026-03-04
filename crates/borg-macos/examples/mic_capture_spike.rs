use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use anyhow::{Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

fn main() -> Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| anyhow!("no default input device"))?;
    let config = device.default_input_config()?;
    println!("input_device:{}", device.description()?.name());
    println!(
        "input_config:{:?} {}ch @{}Hz",
        config.sample_format(),
        config.channels(),
        config.sample_rate()
    );

    let seen = Arc::new(AtomicUsize::new(0));
    let seen_cb = Arc::clone(&seen);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.clone().into(),
            move |data: &[f32], _| {
                seen_cb.fetch_add(data.len(), Ordering::Relaxed);
            },
            |err| eprintln!("mic_stream_error:{err}"),
            None,
        )?,
        other => return Err(anyhow!("unsupported sample format: {:?}", other)),
    };

    stream.play()?;
    std::thread::sleep(Duration::from_secs(2));
    drop(stream);

    println!("samples_seen:{}", seen.load(Ordering::Relaxed));
    Ok(())
}
