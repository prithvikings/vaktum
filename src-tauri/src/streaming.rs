use crate::{audio, cleanup, config::AppConfig, history, insertion, transcription};
use anyhow::{anyhow, Result};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};

const ROLLING_WINDOW_MS: u64 = 5_000;
const POLL_INTERVAL_MS: u64 = 700;
const MIN_AUDIO_MS: u64 = 900;

#[derive(Debug, Clone, serde::Serialize)]
struct StreamingUpdate {
    transcript: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct StreamingCompleted {
    raw_transcript: String,
    final_transcript: String,
    history_error: Option<String>,
}

pub struct StreamingSession {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl StreamingSession {
    pub fn start(
        app: AppHandle,
        recorder: Arc<Mutex<audio::Recorder>>,
        target_window: i64,
        config: AppConfig,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);

        let handle = thread::Builder::new()
            .name("vaktum-streaming-worker".to_owned())
            .spawn(move || {
                let emergency_recorder = Arc::clone(&recorder);
                if let Err(error) = run(
                    app.clone(),
                    recorder,
                    target_window,
                    config,
                    worker_stop,
                ) {
                    let _ = audio::stop(&emergency_recorder);
                    eprintln!("[ERROR] streaming dictation: {error}");
                    let _ = app.emit(
                        "vaktum://streaming-fatal-error",
                        "Streaming dictation failed. Please try again.",
                    );
                }
            })
            .map_err(|error| anyhow!("Unable to start streaming worker: {error}"))?;

        Ok(Self {
            stop,
            handle: Some(handle),
        })
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run(
    app: AppHandle,
    recorder: Arc<Mutex<audio::Recorder>>,
    target_window: i64,
    config: AppConfig,
    stop: Arc<AtomicBool>,
) -> Result<()> {
    let mut transcriber = transcription::WhisperTranscriber::from_config(&config)?;
    let mut previous_hypothesis = String::new();
    let mut committed = String::new();

    while !stop.load(Ordering::Acquire) {
        let (samples, sample_rate) = audio::snapshot_recent(&recorder, ROLLING_WINDOW_MS)?;

        if samples.len() >= ((sample_rate as u64 * MIN_AUDIO_MS) / 1000) as usize {
            let audio = resample_linear(&samples, sample_rate, 16_000);
            if !audio.is_empty() {
                let _ = app.emit("vaktum://streaming-transcribing", ());
                match transcriber.transcribe_samples(&audio) {
                    Ok(result) => {
                        let hypothesis = result.raw_transcript.trim().to_owned();
                        if hypothesis.is_empty() {
                            previous_hypothesis.clear();
                            thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                            continue;
                        }

                        let stable = stable_prefix(&previous_hypothesis, &hypothesis);

                        if !stable.is_empty() {
                            let delta = delta_after_committed(&committed, &stable);
                            if !delta.is_empty() {
                                let insert_delta = join_delta(&committed, &delta);
                                let _ = app.emit("vaktum://streaming-inserting", ());
                                match insertion::insert_text(target_window, &insert_delta) {
                                    Ok(()) => {
                                        committed = join_delta(&committed, &delta);
                                    }
                                    Err(error) => {
                                        eprintln!("[ERROR] streaming insertion: {error}");
                                        let _ = app.emit(
                                            "vaktum://streaming-error",
                                            "Unable to insert streaming text into the target application.",
                                        );
                                    }
                                }
                            }
                        }

                        previous_hypothesis = hypothesis.clone();
                        let _ = app.emit(
                            "vaktum://streaming-updated",
                            StreamingUpdate {
                                transcript: cleanup::clean_transcript(&hypothesis),
                            },
                        );
                        let _ = app.emit("vaktum://streaming-resume", ());
                    }
                    Err(error) => {
                        eprintln!("[WARN] streaming transcription pass skipped: {error}");
                    }
                }
            }
        }

        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }

    let path = audio::stop(&recorder)?;
    let _ = app.emit("vaktum://streaming-transcribing", ());
    let final_result = match transcriber.transcribe(&path) {
        Ok(result) => result,
        Err(error) if is_empty_transcription_error(&error) => transcription::TranscriptionResult {
            raw_transcript: committed.clone(),
            final_transcript: committed.clone(),
        },
        Err(error) => return Err(error),
    };

    let remaining = reconcile_final(&committed, &final_result.final_transcript);
    if !remaining.is_empty() {
        let delta = join_delta(&committed, &remaining);
        let _ = app.emit("vaktum://streaming-inserting", ());
        insertion::insert_text(target_window, &delta)
            .map_err(|error| anyhow!("Final insertion failed: {error}"))?;
    }

    let history_error = if config.history_enabled && !final_result.final_transcript.trim().is_empty() {
        let entry = history::HistoryEntry {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs().to_string())
                .unwrap_or_else(|_| "0".to_owned()),
            raw_transcript: final_result.raw_transcript.clone(),
            final_transcript: final_result.final_transcript.clone(),
        };

        match history::append(entry) {
            Ok(()) => None,
            Err(error) => {
                eprintln!("[ERROR] history save: {error}");
                Some("Unable to save history.".to_owned())
            }
        }
    } else {
        None
    };

    let _ = app.emit(
        "vaktum://streaming-completed",
        StreamingCompleted {
            raw_transcript: final_result.raw_transcript,
            final_transcript: final_result.final_transcript,
            history_error,
        },
    );

    Ok(())
}

fn is_empty_transcription_error(error: &anyhow::Error) -> bool {
    let message = error.to_string();
    message.contains("no transcript was produced")
        || message.contains("cleanup produced an empty transcript")
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

pub fn stable_prefix(previous: &str, current: &str) -> String {
    if previous.is_empty() || current.is_empty() {
        return String::new();
    }

    let common_len = previous
        .chars()
        .zip(current.chars())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum::<usize>();

    if common_len > 0 {
        let common = &current[..common_len];

        // If the previous hypothesis ended at a word boundary and the new
        // hypothesis continues it with whitespace or punctuation, the whole
        // previous hypothesis is stable. If it is still extending the same
        // word, only the earlier complete words are stable.
        if common_len == previous.len() {
            if current.len() == common_len
                || current[common_len..]
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_whitespace() || ".,!?;:)]}".contains(character))
            {
                return previous.trim_end().to_owned();
            }
        }

        if common.ends_with(char::is_whitespace) {
            return common.trim_end().to_owned();
        }

        if let Some((prefix, _)) = common.rsplit_once(char::is_whitespace) {
            if !prefix.trim().is_empty() {
                return prefix.trim_end().to_owned();
            }
        }
    }

    // Rolling windows eventually stop sharing the same beginning. In that
    // case, use the longest suffix/prefix word overlap.
    let previous_words = previous.split_whitespace().collect::<Vec<_>>();
    let current_words = current.split_whitespace().collect::<Vec<_>>();

    for overlap in (1..=previous_words.len().min(current_words.len())).rev() {
        let previous_start = previous_words.len() - overlap;
        if previous_words[previous_start..]
            .iter()
            .zip(current_words.iter())
            .all(|(left, right)| comparable_word(left) == comparable_word(right))
        {
            return current_words[..overlap].join(" ");
        }
    }

    String::new()
}


pub fn delta_after_committed(committed: &str, stable: &str) -> String {
    if let Some(delta) = stable.strip_prefix(committed) {
        return delta.trim_start().to_owned();
    }

    let committed_words = committed.split_whitespace().collect::<Vec<_>>();
    let stable_words = stable.split_whitespace().collect::<Vec<_>>();

    for overlap in (1..=committed_words.len().min(stable_words.len())).rev() {
        let committed_start = committed_words.len() - overlap;
        if committed_words[committed_start..]
            .iter()
            .zip(stable_words.iter())
            .all(|(left, right)| comparable_word(left) == comparable_word(right))
        {
            return stable_words[overlap..].join(" ");
        }
    }

    String::new()
}


fn comparable_word(word: &str) -> String {
    word.trim_matches(|character: char| ".,!?;:()[]{}\"'".contains(character))
        .to_ascii_lowercase()
}

pub fn reconcile_final(committed: &str, final_transcript: &str) -> String {
    if final_transcript.starts_with(committed) {
        return final_transcript
            .strip_prefix(committed)
            .unwrap_or_default()
            .trim_start()
            .to_owned();
    }

    if committed.starts_with(final_transcript) {
        return String::new();
    }

    let committed_words = committed.split_whitespace().collect::<Vec<_>>();
    let final_words = final_transcript.split_whitespace().collect::<Vec<_>>();

    for overlap in (1..=committed_words.len().min(final_words.len())).rev() {
        let committed_start = committed_words.len() - overlap;
        if committed_words[committed_start..]
            .iter()
            .zip(final_words.iter())
            .all(|(left, right)| comparable_word(left) == comparable_word(right))
        {
            return final_words[overlap..].join(" ");
        }
    }

    final_transcript.trim().to_owned()
}

fn join_delta(committed: &str, delta: &str) -> String {
    if committed.trim().is_empty() || delta.is_empty() {
        return delta.to_owned();
    }

    let starts_with_punctuation = delta
        .chars()
        .next()
        .is_some_and(|character| ",.!?:;)]}".contains(character));

    if starts_with_punctuation {
        delta.to_owned()
    } else {
        format!(" {delta}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_prefix_waits_for_a_word_boundary() {
        assert_eq!(stable_prefix("hello wor", "hello world"), "hello");
    }

    #[test]
    fn repeated_hypothesis_commits_complete_words_only() {
        assert_eq!(stable_prefix("hello world", "hello world"), "hello");
    }

    #[test]
    fn stable_prefix_handles_a_new_sentence() {
        assert_eq!(
            stable_prefix("hello world", "hello world today"),
            "hello world"
        );
        assert_eq!(
            stable_prefix("hello world", "new sentence starts"),
            ""
        );
    }

    #[test]
    fn punctuation_only_revision_keeps_words_stable() {
        assert_eq!(
            stable_prefix("hello world", "hello world."),
            "hello world"
        );
    }

    #[test]
    fn delta_does_not_duplicate_committed_text() {
        assert_eq!(
            delta_after_committed("hello", "hello world"),
            "world"
        );
    }

    #[test]
    fn repeated_partial_result_produces_the_same_delta() {
        let stable = stable_prefix("hello world", "hello world");
        assert_eq!(delta_after_committed("hello", &stable), "");
    }

    #[test]
    fn empty_partial_results_are_ignored() {
        assert_eq!(stable_prefix("hello", ""), "");
        assert_eq!(delta_after_committed("hello", ""), "");
    }

    #[test]
    fn transcript_revision_does_not_create_a_false_delta() {
        assert_eq!(
            stable_prefix("hello world", "hello there"),
            "hello"
        );
        assert_eq!(delta_after_committed("hello", "hello"), "");
    }

    #[test]
    fn shifted_rolling_windows_produce_stable_overlap() {
        assert_eq!(
            stable_prefix("hello how are you", "how are you doing"),
            "how are you"
        );
        assert_eq!(
            delta_after_committed("hello how are", "how are you"),
            "you"
        );
    }

    #[test]
    fn punctuation_changes_do_not_break_rolling_overlap() {
        assert_eq!(
            stable_prefix("hello there world", "there world, today"),
            "there world,"
        );
        assert_eq!(
            delta_after_committed("hello there", "there world, today"),
            "world, today"
        );
    }

    #[test]
    fn final_transcript_reconciles_without_duplication() {
        assert_eq!(
            reconcile_final("hello world", "hello world today"),
            "today"
        );
        assert_eq!(
            reconcile_final("hello world today", "hello world"),
            ""
        );
    }

    #[test]
    fn punctuation_delta_does_not_get_an_extra_space() {
        assert_eq!(join_delta("hello", ","), ",");
        assert_eq!(join_delta("hello", "world"), " world");
    }

    #[test]
    fn repeated_words_are_preserved() {
        let final_text = cleanup::clean_transcript("hello hello world");
        assert_eq!(final_text, "hello hello world");
    }
}
