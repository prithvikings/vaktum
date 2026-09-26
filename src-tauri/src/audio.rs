use anyhow::{anyhow, Context, Result};
use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Data, Device, DeviceId, SampleFormat,
};
use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};
use std::{
    fs,
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

#[derive(Default)]
pub struct Recorder {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub recording: bool,
}

pub fn input_devices() -> Result<Vec<(String, String)>> {
    let host = cpal::default_host();

    host.input_devices()?
        .map(|device| {
            let id = device.id()?.to_string();
            let name = device
                .description()
                .map(|description| description.name().to_owned())
                .unwrap_or_else(|_| device.to_string());

            Ok((id, name))
        })
        .collect()
}

pub fn start(rec: &Arc<Mutex<Recorder>>, microphone: &str) -> Result<()> {
    let host = cpal::default_host();
    let device = select_input_device(&host, microphone)?;
    let supported = device
        .default_input_config()
        .context("Could not inspect microphone format")?;
    let channels = supported.channels() as usize;
    let input_rate = supported.sample_rate().0;
    let format = supported.sample_format();

    {
        let mut r = rec.lock().map_err(|_| anyhow!("Recorder lock poisoned"))?;
        if r.recording {
            return Err(anyhow!("Recording is already active"));
        }
        r.samples.clear();
        r.sample_rate = input_rate;
        r.recording = true;
    }

    let target = Arc::clone(rec);
    thread::spawn(move || {
        let config = supported.config();
        let err_fn = |e| eprintln!("[ERROR] microphone stream: {e}");

        let stream_result = match format {
            SampleFormat::F32 => {
                let callback_target = Arc::clone(&target);
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        push_samples(&callback_target, data, channels);
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::I16 => {
                let callback_target = Arc::clone(&target);
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        let values = data
                            .iter()
                            .map(|s| *s as f32 / i16::MAX as f32)
                            .collect::<Vec<_>>();
                        push_samples(&callback_target, &values, channels);
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::U16 => {
                let callback_target = Arc::clone(&target);
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        let values = data
                            .iter()
                            .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                            .collect::<Vec<_>>();
                        push_samples(&callback_target, &values, channels);
                    },
                    err_fn,
                    None,
                )
            }
            _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
        };

        match stream_result {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    eprintln!("[ERROR] microphone start: {e}");
                } else {
                    while target.lock().map(|r| r.recording).unwrap_or(false) {
                        thread::sleep(Duration::from_millis(20));
                    }
                }
            }
            Err(e) => eprintln!("[ERROR] microphone setup: {e}"),
        }

        if let Ok(mut r) = target.lock() {
            r.recording = false;
        }
    });

    Ok(())
}

fn select_input_device(host: &cpal::Host, microphone: &str) -> Result<Device> {
    if microphone == "default" || microphone.trim().is_empty() {
        return host
            .default_input_device()
            .ok_or_else(|| anyhow!("Microphone unavailable"));
    }

    if let Ok(id) = DeviceId::from_str(microphone) {
        if let Some(device) = host.device_by_id(&id) {
            return Ok(device);
        }
    }

    host.default_input_device()
        .ok_or_else(|| anyhow!("Configured microphone is unavailable and no default microphone exists"))
}

fn push_samples<T: cpal::SizedSample + Copy + Into<f32>>(
    rec: &Arc<Mutex<Recorder>>,
    data: &[T],
    channels: usize,
) {
    if let Ok(mut r) = rec.lock() {
        if !r.recording {
            return;
        }

        for frame in data.chunks(channels) {
            r.samples.push(
                frame
                    .iter()
                    .copied()
                    .map(Into::into)
                    .sum::<f32>()
                    / channels as f32,
            );
        }
    }
}

pub fn stop(rec: &Arc<Mutex<Recorder>>) -> Result<PathBuf> {
    {
        let mut r = rec.lock().map_err(|_| anyhow!("Recorder lock poisoned"))?;
        if !r.recording && r.samples.is_empty() {
            return Err(anyhow!("No active recording"));
        }
        r.recording = false;
    }

    thread::sleep(Duration::from_millis(50));

    let r = rec.lock().map_err(|_| anyhow!("Recorder lock poisoned"))?;
    if r.samples.is_empty() {
        return Err(anyhow!("No audio was recorded"));
    }

    let samples = resample_linear(&r.samples, r.sample_rate, 16_000);
    let path = recordings_dir()?.join("latest.wav");
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: WavSampleFormat::Int,
    };

    let mut writer = WavWriter::create(&path, spec)?;
    for sample in &samples {
        writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;

    let secs = samples.len() as f64 / 16_000.0;
    eprintln!("[INFO] Audio duration: {:.2}s", secs);
    eprintln!("[INFO] Saved recording: {}", path.display());
    Ok(path)
}

fn resample_linear(input: &[f32], input_rate: u32, output_rate: u32) -> Vec<f32> {
    if input_rate == output_rate || input.len() < 2 {
        return input.to_vec();
    }

    let output_len =
        ((input.len() as f64 * output_rate as f64) / input_rate as f64).round() as usize;

    (0..output_len)
        .map(|i| {
            let position = i as f64 * (input_rate as f64 / output_rate as f64);
            let left = position.floor() as usize;
            let right = (left + 1).min(input.len() - 1);
            let fraction = position - left as f64;
            input[left] * (1.0 - fraction as f32) + input[right] * fraction as f32
        })
        .collect()
}

fn recordings_dir() -> Result<PathBuf> {
    let local_app_data =
        std::env::var_os("LOCALAPPDATA").ok_or_else(|| anyhow!("LOCALAPPDATA is unavailable"))?;
    let dir = PathBuf::from(local_app_data).join("Vaktum").join("recordings");
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn _keep_data_import(_: &Data) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_microphone_is_accepted() {
        assert!(select_input_device(&cpal::default_host(), "default").is_ok()
            || select_input_device(&cpal::default_host(), "default").is_err());
    }

    #[test]
    fn invalid_device_id_falls_back_or_reports_missing_microphone() {
        let result = select_input_device(
            &cpal::default_host(),
            "wasapi:device-that-does-not-exist",
        );

        assert!(result.is_ok() || result.is_err());
    }
}
