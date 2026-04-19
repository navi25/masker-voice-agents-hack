//! `masker` CLI — runs the four BACKLOG scenarios end-to-end.
//!
//! Examples:
//!     masker                          # stub backend, all scenarios, pretty
//!     masker --backend stub --json    # JSONL output for piping to jq
//!     masker --scenario healthcare    # only one scenario
//!     masker --backend gemini --policy hipaa_clinical

#[cfg(feature = "cactus")]
use std::env;
#[cfg(feature = "cactus")]
use std::fs;
#[cfg(feature = "cactus")]
use std::io::Write;
use std::io::{self, BufRead};
#[cfg(feature = "cactus")]
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
#[cfg(feature = "cactus")]
use std::process::{Command as ProcessCommand, Stdio};
#[cfg(feature = "cactus")]
use std::sync::Arc;
use std::time::Instant;
#[cfg(feature = "cactus")]
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand, ValueEnum};

#[cfg(feature = "cactus")]
use masker::backends::LocalCactusBackend;
use masker::backends::{GeminiCloudBackend, GemmaBackend, StubBackend};
#[cfg(feature = "cactus")]
use masker::{contracts::EntityType, SttSegment};
use masker::{
    contracts::{DetectionResult, PolicyName, Route, TraceEvent, TraceStage},
    AudioChunk, AudioChunkResult, MaskMode, Router, StreamingPipeline, Tracer, VoiceLoop,
};

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Backend {
    Stub,
    Gemini,
    Cactus,
    Auto,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
#[allow(clippy::enum_variant_names)]
enum CliPolicy {
    HipaaBase,
    HipaaLogging,
    HipaaClinical,
}

impl From<CliPolicy> for PolicyName {
    fn from(p: CliPolicy) -> Self {
        match p {
            CliPolicy::HipaaBase => PolicyName::HipaaBase,
            CliPolicy::HipaaLogging => PolicyName::HipaaLogging,
            CliPolicy::HipaaClinical => PolicyName::HipaaClinical,
        }
    }
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CliMaskMode {
    Placeholder,
    Token,
}

impl From<CliMaskMode> for MaskMode {
    fn from(mode: CliMaskMode) -> Self {
        match mode {
            CliMaskMode::Placeholder => MaskMode::Placeholder,
            CliMaskMode::Token => MaskMode::Token,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum LiveOutputFormat {
    Human,
    Json,
    PrettyJson,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum SttPreset {
    Whisper,
    Parakeet,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum DetectionPreset {
    Gemma4,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run detection + policy + masking and emit JSON for SDK integrations.
    FilterInput {
        #[arg(long)]
        text: String,

        #[arg(long, value_enum, default_value_t = CliPolicy::HipaaBase)]
        policy: CliPolicy,

        #[arg(long, value_enum, default_value_t = CliMaskMode::Placeholder)]
        mask_mode: CliMaskMode,
    },

    /// Re-scan model output and scrub any leaked sensitive values.
    FilterOutput {
        #[arg(long)]
        text: String,

        /// Optional DetectionResult JSON from a prior `filter-input` call.
        #[arg(long)]
        detection_json: Option<String>,
    },

    /// Run a single end-to-end turn and emit a TurnResult JSON payload.
    RunTurn {
        #[arg(long)]
        text: String,

        #[arg(long, value_enum, default_value_t = Backend::Auto)]
        backend: Backend,

        #[arg(long, value_enum, default_value_t = CliPolicy::HipaaBase)]
        policy: CliPolicy,
    },

    /// Stream text lines through the full audio pipeline (STT stub → detect →
    /// policy → mask → TTS stub → encrypted audit log).
    ///
    /// Reads one line of text per chunk from stdin (or --text for a single
    /// chunk). Emits one JSON object per chunk to stdout. Encrypted audit
    /// entries are printed to stderr as JSON lines.
    ///
    /// Example (interactive):
    ///   echo "My SSN is 482-55-1234." | masker stream --session ses_001
    ///
    /// Example (batch):
    ///   masker stream --text "Call John at 555-867-5309." --api-key msk_live_k9Xp_healthcare_prod
    Stream {
        /// Session identifier for audit grouping.
        #[arg(long, default_value = "ses_cli")]
        session: String,

        /// Optional API key to select client policy. Defaults to HIPAA-base.
        #[arg(long)]
        api_key: Option<String>,

        /// Process a single text chunk instead of reading from stdin.
        #[arg(long)]
        text: Option<String>,

        /// Emit full audit entries (encrypted) to stderr as JSON lines.
        #[arg(long, default_value_t = false)]
        audit: bool,
    },

    /// Record live audio, transcribe it with Cactus STT, and run the result
    /// through the Masker streaming pipeline with model-backed detection plus
    /// regex fallback.
    ///
    /// Example:
    ///   masker live --seconds 5
    ///
    /// Example (existing clip, no recording step):
    ///   masker live --audio-file /tmp/sample.wav
    Live {
        /// Session identifier for audit grouping.
        #[arg(long, default_value = "ses_live")]
        session: String,

        /// Optional API key to select client policy. Defaults to HIPAA-base.
        #[arg(long)]
        api_key: Option<String>,

        /// Use an existing audio file instead of recording from the mic.
        #[arg(long)]
        audio_file: Option<PathBuf>,

        /// How long to record from the mic when --audio-file is not provided.
        #[arg(long, default_value_t = 5)]
        seconds: u64,

        /// Record from the microphone until Enter is pressed.
        #[arg(long, default_value_t = false)]
        interactive: bool,

        /// Raw ffmpeg avfoundation input selector for the microphone.
        #[arg(long, default_value = ":0")]
        input: String,

        /// Override the STT model path for this run. Falls back to
        /// CACTUS_STT_MODEL_PATH.
        #[arg(long)]
        stt_model_path: Option<String>,

        /// Use a built-in STT preset instead of a full model path.
        #[arg(long, value_enum, conflicts_with = "stt_model_path")]
        stt: Option<SttPreset>,

        /// Override the detection model path for this run. Falls back to
        /// CACTUS_DETECTION_MODEL_PATH, then CACTUS_MODEL_PATH.
        #[arg(long)]
        detection_model_path: Option<String>,

        /// Use a built-in detection preset instead of a full model path.
        #[arg(long, value_enum, conflicts_with = "detection_model_path")]
        detect: Option<DetectionPreset>,

        /// Keep the captured raw PCM file on disk after processing.
        #[arg(long, default_value_t = false)]
        keep_audio: bool,

        /// Play the captured input audio after processing.
        #[arg(long, default_value_t = false)]
        play_input: bool,

        /// Play local output audio: original audio for safe spans, spoken
        /// redactions only where sensitive content was detected.
        #[arg(long, default_value_t = false)]
        play_output: bool,

        /// How to render the result.
        #[arg(long, value_enum, default_value_t = LiveOutputFormat::Human)]
        output: LiveOutputFormat,
    },
}

#[derive(Parser, Debug)]
#[command(
    name = "masker",
    about = "Masker demo — PII/PHI filter for voice agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(long, value_enum, default_value_t = Backend::Stub)]
    backend: Backend,

    #[arg(long, value_enum, default_value_t = CliPolicy::HipaaBase)]
    policy: CliPolicy,

    /// Emit one JSON object per scenario instead of the human-readable view.
    #[arg(long, default_value_t = false)]
    json: bool,

    /// Substring filter against scenario labels (case-insensitive).
    #[arg(long)]
    scenario: Option<String>,
}

struct Scenario {
    label: &'static str,
    text: &'static str,
    expected_route: Route,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        label: "A — Personal info",
        text: "Text Sarah my address is 4821 Mission Street, my number is 415-555-0123.",
        expected_route: Route::MaskedSend,
    },
    Scenario {
        label: "B — Healthcare",
        text: "I have chest pain and my insurance ID is BCBS-887421, MRN 99812.",
        expected_route: Route::LocalOnly,
    },
    Scenario {
        label: "C — Safe query",
        text: "What's the weather tomorrow?",
        expected_route: Route::SafeToSend,
    },
    Scenario {
        label: "D — Work context",
        text: "Summarize the Apollo escalation for the Redwood account, contact priya@redwood.com.",
        expected_route: Route::MaskedSend,
    },
];

fn stream_result_json(result: &AudioChunkResult) -> serde_json::Value {
    serde_json::json!({
        "seq": result.seq,
        "raw_transcript": result.raw_transcript,
        "route": result.route.as_str(),
        "policy": result.policy.policy.as_str(),
        "entity_count": result.detection.entities.len(),
        "entity_types": result
            .detection
            .entities
            .iter()
            .map(|e| e.kind.as_str())
            .collect::<Vec<_>>(),
        "risk_level": result.detection.risk_level.as_str(),
        "masked_transcript": result.masked_transcript,
        "processing_ms": result.processing_ms,
        "trace": result
            .trace
            .iter()
            .map(|e| serde_json::json!({
                "stage": e.stage.as_str(),
                "message": e.message,
                "elapsed_ms": e.elapsed_ms,
            }))
            .collect::<Vec<_>>(),
    })
}

fn process_stream_chunk(
    pipeline: &StreamingPipeline,
    session: &str,
    api_key: Option<&str>,
    seq: u64,
    text: &str,
) -> Result<AudioChunkResult> {
    let chunk = AudioChunk {
        seq,
        data: text.as_bytes().to_vec(),
        source_path: None,
        sample_rate: 16_000,
        duration_ms: 500,
    };
    pipeline
        .process(session, api_key, &chunk)
        .map_err(|e| anyhow!("stream error (seq={seq}): {e:#}"))
}

#[cfg(feature = "cactus")]
fn default_live_artifact_path(ext: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    env::temp_dir().join(format!("masker-live-{}-{millis}.{ext}", std::process::id()))
}

#[cfg(feature = "cactus")]
fn record_audio_with_ffmpeg(output_path: &Path, seconds: u64, input: &str) -> Result<()> {
    let output = ProcessCommand::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "avfoundation", "-i", input])
        .args([
            "-t",
            &seconds.to_string(),
            "-ac",
            "1",
            "-ar",
            "16000",
            "-f",
            "s16le",
        ])
        .arg(output_path)
        .output()
        .map_err(|e| anyhow!("failed to start ffmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ffmpeg recording failed. If the default mic is not device 0, run `ffmpeg -f avfoundation -list_devices true -i \"\"` to inspect inputs.\n{}",
            stderr.trim()
        ));
    }

    Ok(())
}

#[cfg(feature = "cactus")]
fn record_audio_until_enter_with_ffmpeg(output_path: &Path, input: &str) -> Result<()> {
    let mut child = ProcessCommand::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "avfoundation", "-i", input])
        .args(["-ac", "1", "-ar", "16000", "-f", "s16le"])
        .arg(output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("failed to start ffmpeg: {e}"))?;

    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|e| anyhow!("failed to read Enter key: {e}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
    }

    let output = child
        .wait_with_output()
        .map_err(|e| anyhow!("failed waiting for ffmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ffmpeg recording failed. If the default mic is not device 0, run `ffmpeg -f avfoundation -list_devices true -i \"\"` to inspect inputs.\n{}",
            stderr.trim()
        ));
    }

    Ok(())
}

#[cfg(feature = "cactus")]
fn list_avfoundation_audio_inputs() -> Result<Vec<(usize, String)>> {
    let output = ProcessCommand::new("ffmpeg")
        .args(["-f", "avfoundation", "-list_devices", "true", "-i", ""])
        .output()
        .map_err(|e| anyhow!("failed to list ffmpeg devices: {e}"))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut devices = Vec::new();
    let mut in_audio_section = false;

    for line in stderr.lines() {
        if line.contains("AVFoundation audio devices") {
            in_audio_section = true;
            continue;
        }
        if line.contains("AVFoundation video devices") {
            in_audio_section = false;
            continue;
        }
        if !in_audio_section {
            continue;
        }

        let Some(open) = line.find('[') else {
            continue;
        };
        let Some(close) = line[open + 1..].find(']') else {
            continue;
        };
        let idx = &line[open + 1..open + 1 + close];
        let Ok(index) = idx.parse::<usize>() else {
            continue;
        };
        let name = line[open + close + 2..].trim().to_string();
        if !name.is_empty() {
            devices.push((index, name));
        }
    }

    Ok(devices)
}

#[cfg(feature = "cactus")]
fn normalize_audio_to_pcm(input_file: &Path, output_path: &Path) -> Result<()> {
    let output = ProcessCommand::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input_file)
        .args(["-ac", "1", "-ar", "16000", "-f", "s16le"])
        .arg(output_path)
        .output()
        .map_err(|e| anyhow!("failed to start ffmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ffmpeg audio normalization failed: {}",
            stderr.trim()
        ));
    }

    Ok(())
}

#[cfg(feature = "cactus")]
fn normalize_audio_to_wav(input_file: &Path, output_path: &Path) -> Result<()> {
    let output = ProcessCommand::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(input_file)
        .args(["-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
        .arg(output_path)
        .output()
        .map_err(|e| anyhow!("failed to start ffmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ffmpeg wav normalization failed: {}",
            stderr.trim()
        ));
    }

    Ok(())
}

#[cfg(feature = "cactus")]
fn pcm_to_wav(input_pcm: &Path, output_wav: &Path) -> Result<()> {
    let output = ProcessCommand::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "s16le", "-ar", "16000", "-ac", "1", "-i"])
        .arg(input_pcm)
        .args(["-c:a", "pcm_s16le"])
        .arg(output_wav)
        .output()
        .map_err(|e| anyhow!("failed to start ffmpeg: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ffmpeg pcm->wav conversion failed: {}",
            stderr.trim()
        ));
    }

    Ok(())
}

#[cfg(feature = "cactus")]
fn pcm_duration_ms(bytes_len: usize) -> u32 {
    ((bytes_len as f64 / 32_000.0) * 1000.0).round() as u32
}

#[cfg(feature = "cactus")]
#[derive(Debug, Clone, Copy)]
struct PcmSignalStats {
    sample_count: usize,
    peak_abs: i16,
    rms: f32,
}

#[cfg(feature = "cactus")]
struct LiveDetectionStatus {
    engine: &'static str,
    model_path: Option<String>,
    fallback: &'static str,
    active: bool,
    init_error: Option<String>,
}

#[cfg(feature = "cactus")]
fn pcm_signal_stats(bytes: &[u8]) -> PcmSignalStats {
    let mut sample_count = 0usize;
    let mut peak_abs = 0i16;
    let mut sum_squares = 0f64;

    for chunk in bytes.chunks_exact(2) {
        let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
        let abs = sample.saturating_abs();
        peak_abs = peak_abs.max(abs);
        sum_squares += f64::from(sample) * f64::from(sample);
        sample_count += 1;
    }

    let rms = if sample_count == 0 {
        0.0
    } else {
        (sum_squares / sample_count as f64).sqrt() as f32
    };

    PcmSignalStats {
        sample_count,
        peak_abs,
        rms,
    }
}

#[cfg(feature = "cactus")]
fn build_live_pipeline() -> Result<(StreamingPipeline, LiveDetectionStatus)> {
    let stt = Arc::new(masker::CactusSttBackend::from_env()?);
    let tts = Arc::new(masker::StubTts);
    let requested_model_path = std::env::var("CACTUS_DETECTION_MODEL_PATH")
        .ok()
        .or_else(|| std::env::var("CACTUS_MODEL_PATH").ok());

    let (detector, detection_status): (Arc<dyn masker::Detector>, LiveDetectionStatus) =
        match masker::CactusFallbackDetector::from_env() {
            Ok(detector) => (
                Arc::new(detector),
                LiveDetectionStatus {
                    engine: "cactus-audio-first+regex-fallback",
                    model_path: requested_model_path,
                    fallback: "regex",
                    active: true,
                    init_error: None,
                },
            ),
            Err(err) => (
                Arc::new(masker::RegexDetector),
                LiveDetectionStatus {
                    engine: "regex-fallback-only",
                    model_path: requested_model_path,
                    fallback: "regex",
                    active: false,
                    init_error: Some(err.to_string()),
                },
            ),
        };

    let pipeline = StreamingPipeline::new(
        stt,
        tts,
        detector,
        masker::Kek::generate(),
        masker::InMemorySink::new(),
    );

    Ok((pipeline, detection_status))
}

#[cfg(feature = "cactus")]
fn detect_brew_prefix(formula: &str) -> Option<PathBuf> {
    let output = ProcessCommand::new("brew")
        .args(["--prefix", formula])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let prefix = String::from_utf8(output.stdout).ok()?;
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(PathBuf::from(trimmed))
}

#[cfg(feature = "cactus")]
fn stt_preset_dir_name(preset: SttPreset) -> &'static str {
    match preset {
        SttPreset::Whisper => "whisper-small",
        SttPreset::Parakeet => "parakeet-tdt-0.6b-v3",
    }
}

#[cfg(feature = "cactus")]
fn stt_preset_model_name(preset: SttPreset) -> &'static str {
    match preset {
        SttPreset::Whisper => "openai/whisper-small",
        SttPreset::Parakeet => "nvidia/parakeet-tdt-0.6b-v3",
    }
}

#[cfg(feature = "cactus")]
fn detection_preset_dir_name(preset: DetectionPreset) -> &'static str {
    match preset {
        DetectionPreset::Gemma4 => "gemma-4-e2b-it",
    }
}

#[cfg(feature = "cactus")]
fn detection_preset_model_name(preset: DetectionPreset) -> &'static str {
    match preset {
        DetectionPreset::Gemma4 => "google/gemma-4-E2B-it",
    }
}

#[cfg(feature = "cactus")]
fn resolve_stt_preset_path(preset: SttPreset) -> Result<String> {
    let dir_name = stt_preset_dir_name(preset);
    let mut roots = Vec::new();

    if let Ok(weights_dir) = std::env::var("CACTUS_WEIGHTS_DIR") {
        roots.push(PathBuf::from(weights_dir));
    }
    if let Some(prefix) = detect_brew_prefix("cactus") {
        roots.push(prefix.join("libexec").join("weights"));
    }
    roots.push(PathBuf::from("/opt/homebrew/opt/cactus/libexec/weights"));
    roots.push(PathBuf::from("/usr/local/opt/cactus/libexec/weights"));

    for root in roots {
        let candidate = root.join(dir_name);
        if candidate.join("config.txt").is_file() {
            return Ok(candidate.display().to_string());
        }
    }

    Err(anyhow!(
        "could not locate STT preset `{}`. Run `cactus download {}` or pass `--stt-model-path /full/path/to/model`.",
        match preset {
            SttPreset::Whisper => "whisper",
            SttPreset::Parakeet => "parakeet",
        },
        stt_preset_model_name(preset)
    ))
}

#[cfg(feature = "cactus")]
fn resolve_detection_preset_path(preset: DetectionPreset) -> Result<String> {
    let dir_name = detection_preset_dir_name(preset);
    let mut roots = Vec::new();

    if let Ok(weights_dir) = std::env::var("CACTUS_WEIGHTS_DIR") {
        roots.push(PathBuf::from(weights_dir));
    }
    if let Some(prefix) = detect_brew_prefix("cactus") {
        roots.push(prefix.join("libexec").join("weights"));
    }
    roots.push(PathBuf::from("/opt/homebrew/opt/cactus/libexec/weights"));
    roots.push(PathBuf::from("/usr/local/opt/cactus/libexec/weights"));

    for root in roots {
        let candidate = root.join(dir_name);
        if candidate.join("config.txt").is_file() {
            return Ok(candidate.display().to_string());
        }
    }

    Err(anyhow!(
        "could not locate detection preset `gemma4`. Run `cactus download {}` or pass `--detection-model-path /full/path/to/model`.",
        detection_preset_model_name(preset)
    ))
}

#[cfg(feature = "cactus")]
fn resolve_default_detection_model_path() -> Option<String> {
    resolve_detection_preset_path(DetectionPreset::Gemma4).ok()
}

#[cfg(feature = "cactus")]
fn format_entity_list(result: &AudioChunkResult) -> String {
    if result.detection.entities.is_empty() {
        return "none".to_string();
    }

    result
        .detection
        .entities
        .iter()
        .map(|entity| entity.kind.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(feature = "cactus")]
fn repeated_segment_warning(text: &str) -> Option<String> {
    let segments: Vec<String> = text
        .split(['.', '!', '?'])
        .map(|segment| segment.trim())
        .filter(|segment| segment.len() >= 12)
        .map(|segment| segment.to_ascii_lowercase())
        .collect();

    let mut best_segment = None;
    let mut best_run = 1usize;
    let mut current_run = 1usize;

    for pair in segments.windows(2) {
        if pair[0] == pair[1] {
            current_run += 1;
            if current_run > best_run {
                best_run = current_run;
                best_segment = Some(pair[1].clone());
            }
        } else {
            current_run = 1;
        }
    }

    if best_run >= 4 {
        return Some(format!(
            "repeated transcript segment {}x detected: \"{}\". This usually means Whisper kept decoding trailing silence or room noise after you finished speaking.",
            best_run,
            truncate(best_segment.as_deref().unwrap_or(""), 72)
        ));
    }

    None
}

#[cfg(feature = "cactus")]
fn print_live_human_summary(
    session: &str,
    result: &AudioChunkResult,
    total_ms: f64,
    signal: PcmSignalStats,
    duration_ms: u32,
    detection_status: &LiveDetectionStatus,
    stt_model_path: Option<&str>,
) {
    let bar = "============================================================";
    println!("{bar}");
    println!("MASKER LIVE RESULT");
    println!("{bar}");
    println!("Session    : {session}");
    println!("Route      : {}", result.route.as_str());
    println!("Risk       : {}", result.detection.risk_level.as_str());
    println!("Entities   : {}", format_entity_list(result));
    println!("Policy     : {}", result.policy.policy.as_str());
    println!("Latency    : {:.0} ms", total_ms);
    println!(
        "Audio      : {} ms, peak_abs={}, rms={:.1}",
        duration_ms, signal.peak_abs, signal.rms
    );
    println!(
        "STT        : {}",
        stt_model_path.unwrap_or("CACTUS_STT_MODEL_PATH not set")
    );
    println!(
        "Detection  : {}{}",
        detection_status.engine,
        if detection_status.active {
            ""
        } else {
            " (regex fallback only)"
        }
    );
    if let Some(err) = &detection_status.init_error {
        println!("Init Error : {}", truncate(err, 120));
    }
    println!();
    println!("Transcript");
    println!("{}", result.raw_transcript);
    if result.masked_transcript != result.raw_transcript {
        println!();
        println!("Masked");
        println!("{}", result.masked_transcript);
    }
    if let Some(warning) = repeated_segment_warning(&result.raw_transcript) {
        println!();
        println!("Warning");
        println!("{warning}");
        println!(
            "Try stopping recording immediately after you finish speaking, or use `--seconds 8` for a bounded capture."
        );
    }
    println!();
    println!("Trace");
    for event in &result.trace {
        println!(
            "  {:>6.0} ms  {:<10} {}",
            event.elapsed_ms,
            event.stage.as_str(),
            event.message
        );
    }
}

#[cfg(feature = "cactus")]
fn placeholder_to_spoken_redaction(kind: &str) -> String {
    format!("redacted {}", kind.replace('_', " "))
}

#[cfg(feature = "cactus")]
fn speech_safe_output_text(result: &AudioChunkResult) -> String {
    let mut text = result.masked_transcript.clone();

    for token in result.masked.token_map.keys() {
        text = text.replace(token, "redacted sensitive information");
    }

    let mut spoken = String::new();
    let mut rest = text.as_str();

    loop {
        let Some(start) = rest.find("[MASKED:") else {
            spoken.push_str(rest);
            break;
        };

        spoken.push_str(&rest[..start]);
        let after = &rest[start + "[MASKED:".len()..];
        let Some(end) = after.find(']') else {
            spoken.push_str(&rest[start..]);
            break;
        };

        spoken.push_str(&placeholder_to_spoken_redaction(&after[..end]));
        rest = &after[end + 1..];
    }

    spoken
}

#[cfg(feature = "cactus")]
#[derive(Debug, Clone, PartialEq)]
struct AlignedSttSegment {
    start_s: f64,
    end_s: f64,
    transcript_start: usize,
    transcript_end: usize,
}

#[cfg(feature = "cactus")]
#[derive(Debug, Clone, PartialEq)]
struct RedactionAudioSpan {
    start_s: f64,
    end_s: f64,
    labels: Vec<&'static str>,
}

#[cfg(feature = "cactus")]
fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
}

#[cfg(feature = "cactus")]
fn align_stt_segments(transcript: &str, segments: &[SttSegment]) -> Vec<AlignedSttSegment> {
    let mut cursor = 0usize;
    let mut aligned = Vec::new();

    for segment in segments {
        let needle = segment.text.trim();
        if needle.is_empty() {
            continue;
        }

        let start = find_ascii_case_insensitive(&transcript[cursor..], needle)
            .map(|offset| cursor + offset)
            .or_else(|| find_ascii_case_insensitive(transcript, needle));

        let Some(start) = start else {
            continue;
        };

        let end = (start + needle.len()).min(transcript.len());
        aligned.push(AlignedSttSegment {
            start_s: segment.start_s,
            end_s: segment.end_s.max(segment.start_s),
            transcript_start: start,
            transcript_end: end,
        });
        cursor = end;
    }

    aligned
}

#[cfg(feature = "cactus")]
fn entity_redaction_label(kind: EntityType) -> &'static str {
    match kind {
        EntityType::Ssn => "ssn",
        EntityType::Phone => "phone number",
        EntityType::Email => "email address",
        EntityType::Name => "name",
        EntityType::Address => "address",
        EntityType::InsuranceId => "insurance ID",
        EntityType::Mrn => "medical record number",
        EntityType::Dob => "date of birth",
        EntityType::HealthContext => "health information",
        EntityType::Other => "sensitive information",
    }
}

#[cfg(feature = "cactus")]
fn push_unique_label(labels: &mut Vec<&'static str>, label: &'static str) {
    if !labels.contains(&label) {
        labels.push(label);
    }
}

#[cfg(feature = "cactus")]
fn describe_redaction_labels(labels: &[&'static str]) -> String {
    match labels {
        [] => "redacted sensitive information".to_string(),
        [one] => format!("redacted {one}"),
        [first, second] => format!("redacted {first} and {second}"),
        _ => {
            let mut text = String::from("redacted ");
            for (index, label) in labels.iter().enumerate() {
                if index > 0 {
                    if index == labels.len() - 1 {
                        text.push_str(", and ");
                    } else {
                        text.push_str(", ");
                    }
                }
                text.push_str(label);
            }
            text
        }
    }
}

#[cfg(feature = "cactus")]
fn build_redaction_audio_spans(result: &AudioChunkResult) -> Result<Vec<RedactionAudioSpan>> {
    if result.detection.entities.is_empty() {
        return Ok(Vec::new());
    }
    if result.stt_segments.is_empty() {
        return Err(anyhow!("STT backend did not provide timing segments"));
    }

    let aligned_segments = align_stt_segments(&result.raw_transcript, &result.stt_segments);
    if aligned_segments.is_empty() {
        return Err(anyhow!("unable to align STT segments to transcript"));
    }

    let mut entities = result.detection.entities.clone();
    entities.sort_by_key(|entity| (entity.start, entity.end));

    let mut spans = Vec::new();
    for entity in entities {
        let mut matching = aligned_segments
            .iter()
            .filter(|segment| {
                segment.transcript_start < entity.end && segment.transcript_end > entity.start
            })
            .peekable();

        let Some(first) = matching.peek().cloned() else {
            return Err(anyhow!(
                "unable to map detected {} span to audio timing",
                entity.kind.as_str()
            ));
        };

        let mut end_s = first.end_s;
        for segment in matching {
            end_s = end_s.max(segment.end_s);
        }

        spans.push(RedactionAudioSpan {
            start_s: first.start_s,
            end_s,
            labels: vec![entity_redaction_label(entity.kind)],
        });
    }

    spans.sort_by(|left, right| left.start_s.total_cmp(&right.start_s));

    let mut merged: Vec<RedactionAudioSpan> = Vec::new();
    for span in spans {
        if let Some(last) = merged.last_mut() {
            if span.start_s <= last.end_s + 0.05 {
                last.end_s = last.end_s.max(span.end_s);
                for label in span.labels {
                    push_unique_label(&mut last.labels, label);
                }
                continue;
            }
        }
        merged.push(span);
    }

    Ok(merged)
}

#[cfg(feature = "cactus")]
fn extract_audio_slice(
    input_path: &Path,
    start_s: f64,
    end_s: f64,
    output_path: &Path,
) -> Result<()> {
    let duration_s = (end_s - start_s).max(0.0);
    if duration_s <= 0.0 {
        return Ok(());
    }

    let output = ProcessCommand::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-ss", &format!("{start_s:.3}")])
        .arg("-i")
        .arg(input_path)
        .args(["-t", &format!("{duration_s:.3}")])
        .args(["-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le"])
        .arg(output_path)
        .output()
        .map_err(|e| anyhow!("failed to start `ffmpeg` for audio slicing: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("`ffmpeg` audio slicing failed: {}", stderr.trim()));
    }

    Ok(())
}

#[cfg(feature = "cactus")]
fn play_audio_slice(path: &Path, start_s: f64, end_s: f64) -> Result<()> {
    if end_s - start_s < 0.05 {
        return Ok(());
    }

    let slice_path = default_live_artifact_path("wav");
    extract_audio_slice(path, start_s, end_s, &slice_path)?;
    let result = play_audio_file(&slice_path);
    let _ = fs::remove_file(&slice_path);
    result
}

#[cfg(feature = "cactus")]
fn play_redacted_output_audio(
    wav_path: &Path,
    result: &AudioChunkResult,
    duration_ms: u32,
) -> Result<()> {
    if result.detection.entities.is_empty() {
        return play_audio_file(wav_path);
    }

    let spans = match build_redaction_audio_spans(result) {
        Ok(spans) => spans,
        Err(err) => {
            eprintln!(
                "masker: play-output fell back to full spoken redaction because span alignment failed: {err}"
            );
            return speak_text_locally(&speech_safe_output_text(result));
        }
    };

    let total_s = f64::from(duration_ms) / 1000.0;
    let mut cursor_s = 0.0;

    for span in spans {
        let start_s = span.start_s.clamp(0.0, total_s);
        let end_s = span.end_s.clamp(start_s, total_s);

        if start_s > cursor_s + 0.05 {
            play_audio_slice(wav_path, cursor_s, start_s)?;
        }

        speak_text_locally(&describe_redaction_labels(&span.labels))?;
        cursor_s = cursor_s.max(end_s);
    }

    if cursor_s < total_s - 0.05 {
        play_audio_slice(wav_path, cursor_s, total_s)?;
    }

    Ok(())
}

#[cfg(feature = "cactus")]
fn play_audio_file(path: &Path) -> Result<()> {
    let output = ProcessCommand::new("afplay")
        .arg(path)
        .output()
        .map_err(|e| anyhow!("failed to start `afplay`: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("`afplay` failed: {}", stderr.trim()));
    }

    Ok(())
}

#[cfg(feature = "cactus")]
fn speak_text_locally(text: &str) -> Result<()> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let text_path = default_live_artifact_path("txt");
    fs::write(&text_path, trimmed)
        .map_err(|e| anyhow!("failed to write speech text {}: {e}", text_path.display()))?;

    let output = ProcessCommand::new("say")
        .args(["-f"])
        .arg(&text_path)
        .output()
        .map_err(|e| anyhow!("failed to start `say`: {e}"))?;

    let _ = fs::remove_file(&text_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("`say` failed: {}", stderr.trim()));
    }

    Ok(())
}

fn build_backend(b: Backend) -> Result<Box<dyn GemmaBackend>> {
    Ok(match b {
        Backend::Stub => Box::new(StubBackend),
        Backend::Gemini => Box::new(
            GeminiCloudBackend::from_env()
                .map_err(|e| anyhow!("gemini backend unavailable: {e}"))?,
        ),
        Backend::Cactus => {
            #[cfg(feature = "cactus")]
            {
                Box::new(
                    LocalCactusBackend::from_env()
                        .map_err(|e| anyhow!("cactus backend unavailable: {e}"))?,
                )
            }
            #[cfg(not(feature = "cactus"))]
            {
                return Err(anyhow!(
                    "binary built without `cactus` feature — rebuild with `cargo build --features cactus`"
                ));
            }
        }
        Backend::Auto => masker::default_backend(),
    })
}

fn run(cli: Cli) -> Result<i32> {
    if let Some(command) = cli.command {
        return run_command(command);
    }

    let backend = build_backend(cli.backend)?;
    let loop_ = VoiceLoop::new(Router::new(backend)).with_policy(cli.policy.into());

    let needle = cli.scenario.as_ref().map(|s| s.to_lowercase());
    let scenarios: Vec<&Scenario> = SCENARIOS
        .iter()
        .filter(|s| {
            needle
                .as_ref()
                .map(|n| s.label.to_lowercase().contains(n))
                .unwrap_or(true)
        })
        .collect();

    if scenarios.is_empty() {
        eprintln!("no scenario matched {:?}", cli.scenario);
        return Ok(2);
    }

    let mut failures = 0;

    for s in scenarios {
        let tracer = Tracer::new();
        let result = loop_.run_text_turn(s.text, &tracer);

        if cli.json {
            let envelope = serde_json::json!({
                "scenario": s.label,
                "expected": s.expected_route.as_str(),
                "result": result,
            });
            println!("{}", envelope);
            continue;
        }

        let ok = result.policy.route == s.expected_route;
        if !ok {
            failures += 1;
        }
        let bar = "─".repeat(78);
        println!("\n{bar}");
        println!("[{}] {}", if ok { "OK" } else { "MISMATCH" }, s.label);
        println!("  user      : {}", s.text);
        println!(
            "  detected  : {:?} (risk={})",
            result
                .detection
                .entities
                .iter()
                .map(|e| e.kind.as_str())
                .collect::<Vec<_>>(),
            result.detection.risk_level.as_str()
        );
        println!(
            "  policy    : {}  (expected={})",
            result.policy.route.as_str(),
            s.expected_route.as_str()
        );
        println!("  rationale : {}", result.policy.rationale);
        println!("  masked    : {}", truncate(&result.masked_input.text, 240));
        println!("  → model   : {}", truncate(&result.model_output, 160));
        println!("  ← safe    : {}", truncate(&result.safe_output, 160));
        println!("  total     : {:.1} ms", result.total_ms);
    }

    Ok(if failures > 0 { 1 } else { 0 })
}

fn run_command(command: Command) -> Result<i32> {
    match command {
        Command::FilterInput {
            text,
            policy,
            mask_mode,
        } => {
            let detection_started = Instant::now();
            let detection = masker::detection::detect(&text);
            let detection_ms = detection_started.elapsed().as_secs_f64() * 1000.0;

            let policy_started = Instant::now();
            let decision = masker::policy::decide(&detection, policy.into());
            let policy_ms = policy_started.elapsed().as_secs_f64() * 1000.0;

            let masking_started = Instant::now();
            let masked = masker::masking::mask(&text, &detection, mask_mode.into());
            let masking_ms = masking_started.elapsed().as_secs_f64() * 1000.0;

            let entity_types: Vec<&str> = detection
                .entities
                .iter()
                .map(|entity| entity.kind.as_str())
                .collect();
            let masked_count = masked.token_map.len();
            let mut trace = vec![
                TraceEvent {
                    stage: TraceStage::Detection,
                    message: "Scanning input for PII/PHI".to_string(),
                    elapsed_ms: detection_ms,
                    payload: masker::payload! {
                        "risk" => detection.risk_level.as_str(),
                        "entity_types" => entity_types,
                    },
                },
                TraceEvent {
                    stage: TraceStage::Detection,
                    message: format!(
                        "risk={}, entities={}",
                        detection.risk_level.as_str(),
                        detection.entities.len()
                    ),
                    elapsed_ms: 0.0,
                    payload: masker::payload! {
                        "risk" => detection.risk_level.as_str(),
                        "entity_types" => detection
                            .entities
                            .iter()
                            .map(|entity| entity.kind.as_str())
                            .collect::<Vec<_>>(),
                    },
                },
                TraceEvent {
                    stage: TraceStage::Policy,
                    message: format!("Applying {}", decision.policy.as_str()),
                    elapsed_ms: policy_ms,
                    payload: masker::payload! {
                        "policy" => decision.policy.as_str(),
                    },
                },
                TraceEvent {
                    stage: TraceStage::Policy,
                    message: format!("route={}", decision.route.as_str()),
                    elapsed_ms: 0.0,
                    payload: masker::payload! {
                        "route" => decision.route.as_str(),
                        "policy" => decision.policy.as_str(),
                        "rationale" => decision.rationale.as_str(),
                    },
                },
                TraceEvent {
                    stage: TraceStage::Masking,
                    message: "Masking sensitive spans".to_string(),
                    elapsed_ms: masking_ms,
                    payload: masker::payload! {},
                },
            ];
            if masked_count > 0 {
                trace.push(TraceEvent {
                    stage: TraceStage::Masking,
                    message: format!("masked {} span(s)", masked_count),
                    elapsed_ms: 0.0,
                    payload: masker::payload! {
                        "masked_count" => masked_count,
                    },
                });
            }

            let payload = serde_json::json!({
                "masked_input": masked,
                "policy": decision,
                "detection": detection,
                "trace": trace,
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(0)
        }
        Command::FilterOutput {
            text,
            detection_json,
        } => {
            let detection = match detection_json {
                Some(raw) => serde_json::from_str::<DetectionResult>(&raw)
                    .map_err(|e| anyhow!("invalid detection JSON: {e}"))?,
                None => masker::detection::detect(&text),
            };

            let started = Instant::now();
            let safe_text = masker::filter_output(&text, &detection);
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;

            let mut trace = vec![TraceEvent {
                stage: TraceStage::OutputFilter,
                message: "Re-scanning model output for leakage".to_string(),
                elapsed_ms,
                payload: masker::payload! {},
            }];
            if safe_text != text {
                trace.push(TraceEvent {
                    stage: TraceStage::OutputFilter,
                    message: "scrubbed leaked entity from output".to_string(),
                    elapsed_ms: 0.0,
                    payload: masker::payload! {},
                });
            }

            let payload = serde_json::json!({
                "safe_text": safe_text,
                "trace": trace,
            });
            println!("{}", serde_json::to_string(&payload)?);
            Ok(0)
        }
        Command::RunTurn {
            text,
            backend,
            policy,
        } => {
            let backend = build_backend(backend)?;
            let loop_ = VoiceLoop::new(Router::new(backend)).with_policy(policy.into());
            let tracer = Tracer::new();
            let result = loop_.run_text_turn(&text, &tracer);
            println!("{}", serde_json::to_string(&result)?);
            Ok(0)
        }

        Command::Stream {
            session,
            api_key,
            text,
            audit,
        } => {
            let pipeline = StreamingPipeline::new_with_defaults();
            let api_key_ref = api_key.as_deref();

            // Collect lines: either the single --text arg or stdin lines.
            let lines: Vec<String> = if let Some(t) = text {
                vec![t]
            } else {
                let stdin = io::stdin();
                stdin.lock().lines().collect::<Result<_, _>>()?
            };

            let mut failures = 0;
            for (seq, line) in lines.iter().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                match process_stream_chunk(&pipeline, &session, api_key_ref, seq as u64, line) {
                    Ok(result) => {
                        println!("{}", stream_result_json(&result));

                        if audit {
                            // Audit entries are emitted to stderr so stdout
                            // stays clean for piping to jq.
                            eprintln!(
                                "audit: seq={} route={} entities={} policy={}",
                                result.seq,
                                result.route.as_str(),
                                result.detection.entities.len(),
                                result.policy.policy.as_str(),
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("{e:#}");
                        failures += 1;
                    }
                }
            }

            Ok(if failures > 0 { 1 } else { 0 })
        }
        Command::Live {
            session,
            api_key,
            audio_file,
            seconds,
            interactive,
            input,
            stt_model_path,
            stt,
            detection_model_path,
            detect,
            keep_audio,
            play_input,
            play_output,
            output,
        } => {
            #[cfg(not(feature = "cactus"))]
            {
                let _ = (
                    session,
                    api_key,
                    audio_file,
                    seconds,
                    interactive,
                    input,
                    stt_model_path,
                    stt,
                    detection_model_path,
                    detect,
                    keep_audio,
                    play_input,
                    play_output,
                    output,
                );
                return Err(anyhow!(
                    "`masker live` requires the `cactus` feature; rebuild with `cargo run --features cactus -p masker-cli -- live ...`"
                ));
            }

            #[cfg(feature = "cactus")]
            {
                if let Some(path) = stt_model_path {
                    std::env::set_var("CACTUS_STT_MODEL_PATH", path);
                } else if let Some(preset) = stt {
                    std::env::set_var("CACTUS_STT_MODEL_PATH", resolve_stt_preset_path(preset)?);
                }
                if let Some(path) = detection_model_path {
                    std::env::set_var("CACTUS_DETECTION_MODEL_PATH", path);
                } else if let Some(preset) = detect {
                    std::env::set_var(
                        "CACTUS_DETECTION_MODEL_PATH",
                        resolve_detection_preset_path(preset)?,
                    );
                } else if std::env::var("CACTUS_DETECTION_MODEL_PATH").is_err()
                    && std::env::var("CACTUS_MODEL_PATH").is_err()
                {
                    if let Some(path) = resolve_default_detection_model_path() {
                        std::env::set_var("CACTUS_DETECTION_MODEL_PATH", path);
                    }
                }

                let recorded_here = audio_file.is_none();
                if interactive && !recorded_here {
                    return Err(anyhow!(
                        "`--interactive` cannot be combined with `--audio-file`"
                    ));
                }
                let pcm_path = default_live_artifact_path("pcm");
                let wav_path = default_live_artifact_path("wav");

                if recorded_here {
                    if interactive {
                        if let Some(model_path) = std::env::var("CACTUS_STT_MODEL_PATH").ok() {
                            eprintln!("Model weights found at {model_path}");
                        }
                        if let Ok(devices) = list_avfoundation_audio_inputs() {
                            if !devices.is_empty() {
                                eprintln!("Available microphones:");
                                for (index, name) in devices {
                                    eprintln!("  [{index}] {name}");
                                }
                                eprintln!();
                            }
                        }
                        eprintln!("============================================================");
                        eprintln!("     🌵 MASKER LIVE TRANSCRIPTION 🌵");
                        eprintln!("============================================================");
                        eprintln!("Listening... Press Enter to stop when you finish speaking");
                        eprintln!("------------------------------------------------------------");
                        record_audio_until_enter_with_ffmpeg(&pcm_path, &input)?;
                    } else {
                        eprintln!(
                            "recording {} second(s) from {} -> {}",
                            seconds,
                            input,
                            pcm_path.display()
                        );
                        record_audio_with_ffmpeg(&pcm_path, seconds, &input)?;
                    }
                    pcm_to_wav(&pcm_path, &wav_path)?;
                } else {
                    let source = audio_file
                        .as_ref()
                        .expect("audio_file must exist when recorded_here is false");
                    if !source.is_file() {
                        return Err(anyhow!("audio file not found: {}", source.display()));
                    }
                    normalize_audio_to_pcm(source, &pcm_path)?;
                    normalize_audio_to_wav(source, &wav_path)?;
                }

                let pcm_bytes = fs::read(&pcm_path).map_err(|e| {
                    anyhow!("failed to read captured audio {}: {e}", pcm_path.display())
                })?;
                let duration_ms = pcm_duration_ms(pcm_bytes.len());
                let signal = pcm_signal_stats(&pcm_bytes);
                let chunk = AudioChunk {
                    seq: 0,
                    data: pcm_bytes,
                    source_path: Some(wav_path.display().to_string()),
                    sample_rate: 16_000,
                    duration_ms,
                };

                let (pipeline, detection_status) = build_live_pipeline()?;
                let started = Instant::now();
                let result = pipeline
                    .process(&session, api_key.as_deref(), &chunk)
                    .map_err(|e| {
                        let base = format!("live audio processing failed: {e:#}");
                        let details = format!(
                            "pcm_path={} duration_ms={} samples={} peak_abs={} rms={:.1}",
                            pcm_path.display(),
                            duration_ms,
                            signal.sample_count,
                            signal.peak_abs,
                            signal.rms
                        );

                        if e.to_string().contains("empty transcript") {
                            anyhow!(
                                "{base}\n{details}\nCactus STT heard no usable speech. This usually means the mic capture was silent, the wrong AVFoundation input is selected, or speech started too late.\nTry:\n  1. Speak immediately and increase the window: `--seconds 8`\n  2. List devices: `ffmpeg -f avfoundation -list_devices true -i \"\"`\n  3. Retry with a different mic, for example `--input \":1\"`\n  4. Inspect the captured audio: `ffmpeg -f s16le -ar 16000 -ac 1 -i {pcm} /tmp/masker-live.wav`\n  5. Re-run STT against that file: `cargo run -q -p masker-cli --features cactus -- live --audio-file /tmp/masker-live.wav`",
                                pcm = pcm_path.display(),
                            )
                        } else {
                            anyhow!("{base}\n{details}")
                        }
                    })?;
                let total_ms = started.elapsed().as_secs_f64() * 1000.0;

                let out = serde_json::json!({
                    "session": session,
                    "audio": {
                        "mode": if recorded_here { "live" } else { "file" },
                        "source_path": audio_file.as_ref().map(|p| p.display().to_string()),
                        "pcm_path": pcm_path.display().to_string(),
                        "wav_path": wav_path.display().to_string(),
                        "retained": keep_audio,
                        "input": if recorded_here { Some(input) } else { None::<String> },
                        "recorded_seconds": if recorded_here && !interactive {
                            Some(seconds)
                        } else {
                            None::<u64>
                        },
                        "duration_ms": duration_ms,
                        "sample_count": signal.sample_count,
                        "peak_abs": signal.peak_abs,
                        "rms": signal.rms,
                    },
                    "stt": {
                        "engine": "cactus_transcribe",
                        "model_path": std::env::var("CACTUS_STT_MODEL_PATH").ok(),
                    },
                    "detection": {
                        "engine": detection_status.engine,
                        "model_path": detection_status.model_path,
                        "fallback": detection_status.fallback,
                        "active": detection_status.active,
                        "init_error": detection_status.init_error,
                    },
                    "elapsed_ms": total_ms,
                    "transcript": result.raw_transcript,
                    "result": stream_result_json(&result),
                });
                if interactive && output != LiveOutputFormat::Human {
                    eprintln!();
                    eprintln!("------------------------------------------------------------");
                    eprintln!("Final transcript:");
                    eprintln!("{}", result.raw_transcript);
                    eprintln!("------------------------------------------------------------");
                }
                match output {
                    LiveOutputFormat::Human => print_live_human_summary(
                        &session,
                        &result,
                        total_ms,
                        signal,
                        duration_ms,
                        &detection_status,
                        std::env::var("CACTUS_STT_MODEL_PATH").ok().as_deref(),
                    ),
                    LiveOutputFormat::Json => println!("{}", serde_json::to_string(&out)?),
                    LiveOutputFormat::PrettyJson => {
                        println!("{}", serde_json::to_string_pretty(&out)?)
                    }
                }

                if play_input {
                    eprintln!("masker: playing captured input audio");
                    play_audio_file(&wav_path)?;
                }

                if play_output {
                    eprintln!("masker: playing local output audio with in-place redactions");
                    play_redacted_output_audio(&wav_path, &result, duration_ms)?;
                }

                if !keep_audio {
                    let _ = fs::remove_file(&pcm_path);
                    let _ = fs::remove_file(&wav_path);
                }

                Ok(0)
            }
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(n).collect();
        out.push('…');
        out
    }
}

#[cfg(all(test, feature = "cactus"))]
mod cli_tests {
    use super::*;
    use masker::contracts::{
        DetectionResult, Entity, EntityType, MaskedText, PolicyDecision, PolicyName, RiskLevel,
        Route,
    };
    use std::collections::BTreeMap;

    #[test]
    fn speech_safe_output_replaces_tokens_and_placeholders() {
        let mut token_map = BTreeMap::new();
        token_map.insert("tok_deadbeef".to_string(), "482-55-1234".to_string());

        let result = AudioChunkResult {
            seq: 0,
            raw_transcript: "My SSN is 482-55-1234.".to_string(),
            stt_segments: Vec::new(),
            masked_transcript: "My SSN is tok_deadbeef and [MASKED:address].".to_string(),
            detection: DetectionResult {
                entities: Vec::new(),
                risk_level: RiskLevel::High,
            },
            policy: PolicyDecision {
                route: Route::LocalOnly,
                policy: PolicyName::HipaaBase,
                rationale: String::new(),
            },
            masked: MaskedText {
                text: "My SSN is tok_deadbeef and [MASKED:address].".to_string(),
                token_map,
            },
            route: Route::LocalOnly,
            audio_out: Vec::new(),
            processing_ms: 0,
            trace: Vec::new(),
        };

        assert_eq!(
            speech_safe_output_text(&result),
            "My SSN is redacted sensitive information and redacted address."
        );
    }

    #[test]
    fn builds_redaction_audio_spans_from_timed_segments() {
        let result = AudioChunkResult {
            seq: 0,
            raw_transcript: "My SSN is 482-55-1234 and email me.".to_string(),
            stt_segments: vec![
                SttSegment {
                    start_s: 0.0,
                    end_s: 0.2,
                    text: "My".to_string(),
                },
                SttSegment {
                    start_s: 0.2,
                    end_s: 0.4,
                    text: "SSN".to_string(),
                },
                SttSegment {
                    start_s: 0.4,
                    end_s: 0.55,
                    text: "is".to_string(),
                },
                SttSegment {
                    start_s: 0.55,
                    end_s: 1.1,
                    text: "482-55-1234".to_string(),
                },
                SttSegment {
                    start_s: 1.1,
                    end_s: 1.4,
                    text: "and email me.".to_string(),
                },
            ],
            masked_transcript: "My SSN is tok_deadbeef and email me.".to_string(),
            detection: DetectionResult {
                entities: vec![Entity {
                    kind: EntityType::Ssn,
                    value: "482-55-1234".to_string(),
                    start: 10,
                    end: 21,
                    confidence: 0.9,
                }],
                risk_level: RiskLevel::High,
            },
            policy: PolicyDecision {
                route: Route::LocalOnly,
                policy: PolicyName::HipaaBase,
                rationale: String::new(),
            },
            masked: MaskedText {
                text: "My SSN is tok_deadbeef and email me.".to_string(),
                token_map: BTreeMap::new(),
            },
            route: Route::LocalOnly,
            audio_out: Vec::new(),
            processing_ms: 0,
            trace: Vec::new(),
        };

        let spans = build_redaction_audio_spans(&result).unwrap();
        assert_eq!(
            spans,
            vec![RedactionAudioSpan {
                start_s: 0.55,
                end_s: 1.1,
                labels: vec!["ssn"],
            }]
        );
    }

    #[test]
    fn keeps_safe_audio_between_distinct_sensitive_spans() {
        let result = AudioChunkResult {
            seq: 0,
            raw_transcript: "My SSN is 482-55-1234 and my address is 1 Main St.".to_string(),
            stt_segments: vec![
                SttSegment {
                    start_s: 0.0,
                    end_s: 0.2,
                    text: "My".to_string(),
                },
                SttSegment {
                    start_s: 0.2,
                    end_s: 0.4,
                    text: "SSN".to_string(),
                },
                SttSegment {
                    start_s: 0.4,
                    end_s: 0.55,
                    text: "is".to_string(),
                },
                SttSegment {
                    start_s: 0.55,
                    end_s: 1.1,
                    text: "482-55-1234".to_string(),
                },
                SttSegment {
                    start_s: 1.1,
                    end_s: 1.2,
                    text: "and".to_string(),
                },
                SttSegment {
                    start_s: 1.2,
                    end_s: 1.4,
                    text: "my address".to_string(),
                },
                SttSegment {
                    start_s: 1.4,
                    end_s: 1.8,
                    text: "is 1 Main St.".to_string(),
                },
            ],
            masked_transcript: "My SSN is tok_deadbeef and my address is [MASKED:address]."
                .to_string(),
            detection: DetectionResult {
                entities: vec![
                    Entity {
                        kind: EntityType::Ssn,
                        value: "482-55-1234".to_string(),
                        start: 10,
                        end: 21,
                        confidence: 0.9,
                    },
                    Entity {
                        kind: EntityType::Address,
                        value: "1 Main St.".to_string(),
                        start: 40,
                        end: 50,
                        confidence: 0.9,
                    },
                ],
                risk_level: RiskLevel::High,
            },
            policy: PolicyDecision {
                route: Route::LocalOnly,
                policy: PolicyName::HipaaBase,
                rationale: String::new(),
            },
            masked: MaskedText {
                text: "My SSN is tok_deadbeef and my address is [MASKED:address].".to_string(),
                token_map: BTreeMap::new(),
            },
            route: Route::LocalOnly,
            audio_out: Vec::new(),
            processing_ms: 0,
            trace: Vec::new(),
        };

        let spans = build_redaction_audio_spans(&result).unwrap();
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].labels, vec!["ssn"]);
        assert_eq!(spans[1].labels, vec!["address"]);
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("masker: {e:#}");
            ExitCode::from(2)
        }
    }
}
