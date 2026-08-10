use std::{
    env,
    error::Error,
    io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use prollyglot_asr::{SpeechAudio, SpeechEngine, SpeechEvent, SpeechStreamConfig};
use prollyglot_asr_sherpa::SherpaOnlineEngine;
use prollyglot_audio_pipeline::StreamingResampler;
use prollyglot_model_manager::{
    ModelManager, ModelManifest, comparison_english_manifest, initial_english_manifest,
};
use sherpa_onnx::Wave;

const TARGET_SAMPLE_RATE: u32 = 16_000;
const STREAM_CHUNK_SAMPLES: usize = 1_600;

struct BenchmarkResult {
    model: String,
    download_mib: f64,
    prepare_time: Duration,
    load_time: Duration,
    inference_time: Duration,
    real_time_factor: f64,
    first_partial_audio_millis: Option<u64>,
    first_partial_compute_time: Option<Duration>,
    partial_updates: usize,
    word_error_rate: Option<f64>,
    transcript: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Model comparison failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments = env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() != 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: compare_models <model-cache-directory> <mono-wav> \"<reference text or ->\"",
        )
        .into());
    }

    let model_root = PathBuf::from(&arguments[0]);
    let wave_path = PathBuf::from(&arguments[1]);
    let reference = arguments[2].to_string_lossy().into_owned();
    let wave_path_text = wave_path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "WAV path must be valid Unicode",
        )
    })?;
    let wave = Wave::read(wave_path_text).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("could not read mono WAV {}", wave_path.display()),
        )
    })?;
    let input_rate = u32::try_from(wave.sample_rate())
        .ok()
        .filter(|rate| *rate > 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid WAV sample rate"))?;
    let samples = resample(wave.samples(), input_rate)?;
    if samples.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "WAV contains no audio").into());
    }
    let audio_duration = Duration::from_secs_f64(samples.len() as f64 / TARGET_SAMPLE_RATE as f64);
    let reference = (reference != "-").then_some(reference);

    println!("Audio: {}", wave_path.display());
    println!("Duration: {:.3} seconds", audio_duration.as_secs_f64());
    println!("Model cache: {}", model_root.display());
    println!();
    println!(
        "| Model | Download MiB | Prepare s | Load s | Inference s | RTF | First partial audio ms | First partial compute ms | Partial updates | WER | Transcript |"
    );
    println!("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |");

    for manifest in [initial_english_manifest()?, comparison_english_manifest()?] {
        eprintln!("Preparing {}…", manifest.display_name);
        let result = benchmark_model(
            &model_root,
            &manifest,
            &samples,
            audio_duration,
            reference.as_deref(),
        )?;
        print_result(&result);
    }
    Ok(())
}

fn resample(samples: &[f32], input_rate: u32) -> Result<Vec<f32>, Box<dyn Error>> {
    if input_rate == TARGET_SAMPLE_RATE {
        return Ok(samples.to_vec());
    }
    let mut resampler = StreamingResampler::new(input_rate, TARGET_SAMPLE_RATE)?;
    Ok(resampler.process(samples, true))
}

fn benchmark_model(
    model_root: &Path,
    manifest: &ModelManifest,
    samples: &[f32],
    audio_duration: Duration,
    reference: Option<&str>,
) -> Result<BenchmarkResult, Box<dyn Error>> {
    let manager = ModelManager::new(model_root);
    let prepare_started = Instant::now();
    let location = manager.install(manifest, |_| {})?;
    let prepare_time = prepare_started.elapsed();

    let mut engine = SherpaOnlineEngine::default();
    let load_started = Instant::now();
    engine.load_model(&location)?;
    let load_time = load_started.elapsed();
    let mut stream = engine.start_stream(SpeechStreamConfig::default())?;

    let inference_started = Instant::now();
    let mut first_partial_audio_millis = None;
    let mut first_partial_compute_time = None;
    let mut partial_updates = 0_usize;
    let mut final_text = Vec::new();
    for (chunk_index, chunk) in samples.chunks(STREAM_CHUNK_SAMPLES).enumerate() {
        let start_micros = chunk_index as u64 * STREAM_CHUNK_SAMPLES as u64 * 1_000_000
            / u64::from(TARGET_SAMPLE_RATE);
        let end_micros =
            start_micros + chunk.len() as u64 * 1_000_000 / u64::from(TARGET_SAMPLE_RATE);
        collect_events(
            stream.push_audio(SpeechAudio {
                start_micros,
                end_micros,
                sample_rate: TARGET_SAMPLE_RATE,
                samples: chunk.to_vec(),
            })?,
            end_micros,
            inference_started,
            &mut first_partial_audio_millis,
            &mut first_partial_compute_time,
            &mut partial_updates,
            &mut final_text,
        );
    }
    let duration_micros = u64::try_from(audio_duration.as_micros()).unwrap_or(u64::MAX);
    collect_events(
        stream.finish(duration_micros)?,
        duration_micros,
        inference_started,
        &mut first_partial_audio_millis,
        &mut first_partial_compute_time,
        &mut partial_updates,
        &mut final_text,
    );
    let inference_time = inference_started.elapsed();
    let transcript = final_text.join(" ");

    Ok(BenchmarkResult {
        model: manifest.display_name.clone(),
        download_mib: manifest.download_size_bytes() as f64 / (1024.0 * 1024.0),
        prepare_time,
        load_time,
        inference_time,
        real_time_factor: inference_time.as_secs_f64() / audio_duration.as_secs_f64(),
        first_partial_audio_millis,
        first_partial_compute_time,
        partial_updates,
        word_error_rate: reference.map(|expected| word_error_rate(expected, &transcript)),
        transcript,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_events(
    events: Vec<SpeechEvent>,
    audio_end_micros: u64,
    inference_started: Instant,
    first_partial_audio_millis: &mut Option<u64>,
    first_partial_compute_time: &mut Option<Duration>,
    partial_updates: &mut usize,
    final_text: &mut Vec<String>,
) {
    for event in events {
        match event {
            SpeechEvent::Partial(_) => {
                *partial_updates = partial_updates.saturating_add(1);
                first_partial_audio_millis.get_or_insert(audio_end_micros / 1_000);
                first_partial_compute_time.get_or_insert_with(|| inference_started.elapsed());
            }
            SpeechEvent::Final(result) => final_text.push(result.text),
        }
    }
}

fn print_result(result: &BenchmarkResult) {
    let audio_millis = result
        .first_partial_audio_millis
        .map_or_else(|| "n/a".into(), |value| value.to_string());
    let compute_millis = result.first_partial_compute_time.map_or_else(
        || "n/a".into(),
        |value| format!("{:.1}", value.as_secs_f64() * 1_000.0),
    );
    let wer = result
        .word_error_rate
        .map_or_else(|| "n/a".into(), |value| format!("{:.1}%", value * 100.0));
    let transcript = result.transcript.replace('|', "\\|").replace('\n', " ");
    println!(
        "| {} | {:.1} | {:.3} | {:.3} | {:.3} | {:.3} | {} | {} | {} | {} | {} |",
        result.model,
        result.download_mib,
        result.prepare_time.as_secs_f64(),
        result.load_time.as_secs_f64(),
        result.inference_time.as_secs_f64(),
        result.real_time_factor,
        audio_millis,
        compute_millis,
        result.partial_updates,
        wer,
        transcript
    );
}

fn word_error_rate(reference: &str, hypothesis: &str) -> f64 {
    let reference = normalized_words(reference);
    let hypothesis = normalized_words(hypothesis);
    if reference.is_empty() {
        return f64::from(!hypothesis.is_empty());
    }

    let mut previous = (0..=hypothesis.len()).collect::<Vec<_>>();
    let mut current = vec![0_usize; hypothesis.len() + 1];
    for (reference_index, expected) in reference.iter().enumerate() {
        current[0] = reference_index + 1;
        for (hypothesis_index, actual) in hypothesis.iter().enumerate() {
            let substitution = previous[hypothesis_index] + usize::from(expected != actual);
            let insertion = current[hypothesis_index] + 1;
            let deletion = previous[hypothesis_index + 1] + 1;
            current[hypothesis_index + 1] = substitution.min(insertion).min(deletion);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[hypothesis.len()] as f64 / reference.len() as f64
}

fn normalized_words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| {
            let normalized = word
                .chars()
                .flat_map(char::to_lowercase)
                .filter(|character| character.is_alphanumeric())
                .collect::<String>();
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_error_rate_handles_insertions_deletions_and_case() {
        assert_eq!(word_error_rate("One two three", "one two three"), 0.0);
        assert!((word_error_rate("one two three", "one four") - 2.0 / 3.0).abs() < f64::EPSILON);
    }
}
