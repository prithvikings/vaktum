use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{SampleFormat, WavSpec, WavWriter};
use std::{fs, path::PathBuf, sync::{Arc, Mutex}, thread, time::Duration};

#[derive(Default)]
pub struct Recorder { pub samples: Vec<f32>, pub sample_rate: u32, pub recording: bool, pub stream_alive: bool }

pub fn start(rec: &Arc<Mutex<Recorder>>) -> Result<()> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or_else(|| anyhow!("Microphone unavailable"))?;
    let supported = device.default_input_config().context("Could not inspect microphone format")?;
    let channels = supported.channels() as usize;
    let input_rate = supported.sample_rate().0;
    {
        let mut r = rec.lock().map_err(|_| anyhow!("Recorder lock poisoned"))?;
        r.samples.clear(); r.sample_rate = input_rate; r.recording = true; r.stream_alive = true;
    }
    let target = Arc::clone(rec);
    thread::spawn(move || {
        let input = supported.config();
        let err_fn = |e| eprintln!("[ERROR] microphone stream: {e}");
        let stream = device.build_input_stream(&input, move |data: &[f32], _| {
            if let Ok(mut r) = target.lock() {
                if !r.recording { return; }
                for frame in data.chunks(channels) { r.samples.push(frame.iter().copied().sum::<f32>() / channels as f32); }
            }
        }, err_fn, None);
        match stream {
            Ok(stream) => { if let Err(e) = stream.play() { eprintln!("[ERROR] microphone start: {e}"); }
                while target.lock().map(|r| r.recording).unwrap_or(false) { thread::sleep(Duration::from_millis(20)); }
            }
            Err(e) => eprintln!("[ERROR] microphone setup: {e}"),
        }
        if let Ok(mut r) = target.lock() { r.stream_alive = false; }
    });
    Ok(())
}

pub fn stop(rec: &Arc<Mutex<Recorder>>) -> Result<PathBuf> {
    let path = recordings_dir()?.join("latest.wav");
    { let mut r = rec.lock().map_err(|_| anyhow!("Recorder lock poisoned"))?; r.recording = false; }
    thread::sleep(Duration::from_millis(30));
    let r = rec.lock().map_err(|_| anyhow!("Recorder lock poisoned"))?;
    if r.samples.is_empty() { return Err(anyhow!("No audio was recorded")); }
    let spec = WavSpec { channels: 1, sample_rate: r.sample_rate, bits_per_sample: 16, sample_format: SampleFormat::Int };
    let mut writer = WavWriter::create(&path, spec)?;
    for s in &r.samples { writer.write_sample((s.clamp(-1.0,1.0) * i16::MAX as f32) as i16)?; }
    writer.finalize()?;
    let secs = r.samples.len() as f64 / r.sample_rate as f64;
    eprintln!("[INFO] Audio duration: {:.2}s", secs);
    eprintln!("[INFO] Saved recording: {}", path.display());
    Ok(path)
}

fn recordings_dir() -> Result<PathBuf> { let dir = dirs_path()?.join("recordings"); fs::create_dir_all(&dir)?; Ok(dir) }
fn dirs_path() -> Result<PathBuf> { std::env::var_os("LOCALAPPDATA").map(PathBuf::from).ok_or_else(|| anyhow!("LOCALAPPDATA is unavailable")) }
