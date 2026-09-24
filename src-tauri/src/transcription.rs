use crate::cleanup;
use anyhow::{anyhow, Context, Result};
use hound::WavReader;
use std::path::{Path, PathBuf};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const MODEL_ENV: &str = "VAKTUM_WHISPER_MODEL";
const DEFAULT_MODEL_FILE: &str = "ggml-base.en.bin";

#[derive(Debug)]
pub struct WhisperTranscriber {
    model_path: PathBuf,
}

impl WhisperTranscriber {
    pub fn from_environment() -> Result<Self> {
        let model_path = configured_model_path()?;

        if !model_path.is_file() {
            return Err(anyhow!(
                "Whisper model not found: {}. Set {MODEL_ENV} to the model path or place {DEFAULT_MODEL_FILE} under %LOCALAPPDATA%\\Vaktum\\models",
                model_path.display()
            ));
        }

        Ok(Self { model_path })
    }

    pub fn transcribe<P: AsRef<Path>>(&self, wav_path: P) -> Result<String> {
        let wav_path = wav_path.as_ref();

        validate_wav_path(wav_path)?;

        eprintln!("[INFO] Loading Whisper model: {}", self.model_path.display());
        let context = WhisperContext::new_with_params(
            &self.model_path,
            WhisperContextParameters::default(),
        )
        .map_err(|e| anyhow!("Unable to load Whisper model: {e}"))?;

        let mut reader = WavReader::open(wav_path)
            .with_context(|| format!("Unable to open WAV file: {}", wav_path.display()))?;
        let spec = reader.spec();

        if spec.channels != 1 || spec.sample_rate != 16_000 || spec.bits_per_sample != 16 {
            return Err(anyhow!(
                "Unsupported audio format: expected mono 16-bit PCM WAV at 16 kHz, got {} channel(s), {} Hz, {} bits",
                spec.channels,
                spec.sample_rate,
                spec.bits_per_sample
            ));
        }

        if spec.sample_format != hound::SampleFormat::Int {
            return Err(anyhow!("Unsupported audio format: expected integer PCM WAV"));
        }

        let samples: Vec<i16> = reader
            .samples::<i16>()
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("Failed to read WAV samples")?;

        if samples.is_empty() {
            return Err(anyhow!("Whisper transcription failed: WAV contains no audio samples"));
        }

        let mut audio = vec![0.0_f32; samples.len()];
        whisper_rs::convert_integer_to_float_audio(&samples, &mut audio)
            .map_err(|e| anyhow!("Whisper audio conversion failed: {e}"))?;

        eprintln!("[INFO] Starting transcription: {}", wav_path.display());

        let mut state = context
            .create_state()
            .map_err(|e| anyhow!("Whisper initialization failed: {e}"))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_translate(false);
        params.set_no_context(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        state
            .full(params, &audio)
            .map_err(|e| anyhow!("Whisper transcription failed: {e}"))?;

        let raw_transcript = state
            .as_iter()
            .map(|segment| segment.to_string())
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_owned();

        if raw_transcript.is_empty() {
            return Err(anyhow!("Whisper transcription failed: no transcript was produced"));
        }

        let transcript = cleanup::clean_transcript(&raw_transcript);

        if transcript.is_empty() {
            return Err(anyhow!("Whisper transcription failed: cleanup produced an empty transcript"));
        }

        eprintln!("[INFO] Transcription completed");
        Ok(transcript)
    }
}

pub fn latest_recording_path() -> Result<PathBuf> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| anyhow!("LOCALAPPDATA is unavailable"))?;

    Ok(PathBuf::from(local_app_data)
        .join("Vaktum")
        .join("recordings")
        .join("latest.wav"))
}

pub fn configured_model_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(MODEL_ENV) {
        if path.is_empty() {
            return Err(anyhow!("{MODEL_ENV} is set but empty"));
        }
        return Ok(PathBuf::from(path));
    }

    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or_else(|| anyhow!("LOCALAPPDATA is unavailable; set {MODEL_ENV} to the Whisper model path"))?;

    Ok(PathBuf::from(local_app_data)
        .join("Vaktum")
        .join("models")
        .join(DEFAULT_MODEL_FILE))
}

fn validate_wav_path(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(anyhow!("Invalid WAV path: file does not exist: {}", path.display()));
    }

    if !path.is_file() {
        return Err(anyhow!("Invalid WAV path: not a file: {}", path.display()));
    }

    let is_wav = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"));

    if !is_wav {
        return Err(anyhow!("Invalid WAV path: expected a .wav file: {}", path.display()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_model_path_is_reported() {
        let path = std::env::temp_dir().join("vaktum-missing-whisper-model.bin");
        let _ = std::fs::remove_file(&path);

        std::env::set_var(MODEL_ENV, &path);
        let result = WhisperTranscriber::from_environment();
        std::env::remove_var(MODEL_ENV);

        let error = result.expect_err("missing model should fail");
        assert!(error.to_string().contains("Whisper model not found"));
    }

    #[test]
    fn missing_wav_path_is_reported() {
        let path = std::env::temp_dir().join("vaktum-missing-recording.wav");
        let _ = std::fs::remove_file(&path);

        let error = validate_wav_path(&path).expect_err("missing WAV should fail");
        assert!(error.to_string().contains("Invalid WAV path"));
    }

    #[test]
    fn non_wav_path_is_rejected() {
        let path = std::env::temp_dir().join("vaktum-recording.txt");
        std::fs::write(&path, b"not audio").expect("test fixture should be writable");

        let error = validate_wav_path(&path).expect_err("non-WAV should fail");
        let _ = std::fs::remove_file(&path);

        assert!(error.to_string().contains("expected a .wav file"));
    }
}
