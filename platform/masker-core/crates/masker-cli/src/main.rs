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
use std::io::{self, BufRead, IsTerminal};
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;
#[cfg(feature = "cactus")]
use std::process::{Command as ProcessCommand, Stdio};
#[cfg(feature = "cactus")]
use std::sync::Arc;
#[cfg(feature = "cactus")]
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
    contracts::{DetectionResult, PolicyName, Route},
    AudioChunk, AudioChunkResult, MaskMode, Router, StreamingPipeline, Tracer, VoiceLoop,
};

use masker::crypto::{Kek, PersistedState, TokenVault};

#[derive(Copy, Clone, Debug, ValueEnum)]
enum Backend {
    Stub,
    Gemini,
    Cactus,
    Auto,
}

#[derive(Copy, Clone, Debug)]
#[allow(dead_code)]
enum TermColor {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
}

fn term_supports_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    std::io::stdout().is_terminal()
}

fn term_code(color: TermColor) -> &'static str {
    match color {
        TermColor::Red => "31",
        TermColor::Green => "32",
        TermColor::Yellow => "33",
        TermColor::Blue => "34",
        TermColor::Magenta => "35",
        TermColor::Cyan => "36",
        TermColor::Gray => "90",
    }
}

fn term_paint(text: impl AsRef<str>, codes: &str) -> String {
    let text = text.as_ref();
    if !term_supports_color() {
        return text.to_string();
    }
    format!("\x1b[{codes}m{text}\x1b[0m")
}

#[allow(dead_code)]
fn term_fg(text: impl AsRef<str>, color: TermColor) -> String {
    term_paint(text, term_code(color))
}

fn term_fg_bold(text: impl AsRef<str>, color: TermColor) -> String {
    term_paint(text, &format!("1;{}", term_code(color)))
}

#[allow(dead_code)]
fn term_dim(text: impl AsRef<str>) -> String {
    term_paint(text, "2")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut out = s.chars().take(max.saturating_sub(3)).collect::<String>();
    out.push_str("...");
    out
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum LiveSttEngine {
    /// Dedicated STT model via `CactusModel::transcribe_pcm` (Whisper/Parakeet).
    Cactus,
    /// Prompted ASR using Gemma audio input (requires `AudioChunk.source_path`).
    Gemma4,
}

#[derive(Subcommand, Debug)]
enum CoremlCommand {
    /// Display Core ML metadata for a `.mlmodel` or `.mlpackage`.
    Metadata {
        /// Path to a `.mlmodel` or `.mlpackage`.
        #[arg(long)]
        model: PathBuf,
    },

    /// Compile a `.mlmodel` or `.mlpackage` to a `.mlmodelc` directory.
    Compile {
        /// Path to a `.mlmodel` or `.mlpackage`.
        #[arg(long)]
        model: PathBuf,

        /// Output directory to write the compiled `.mlmodelc` bundle into.
        #[arg(long)]
        out_dir: PathBuf,
    },

    /// Check for the `.mlpackage` files that Gemma 4 E2B typically needs.
    CheckGemmaE2b {
        /// Path to `.../gemma-4-e2b-it` directory.
        #[arg(long)]
        gemma_dir: PathBuf,
    },
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

        /// Select the STT engine: dedicated STT (default) or Gemma-audio prompted ASR.
        #[arg(long, value_enum, default_value_t = LiveSttEngine::Cactus)]
        stt_engine: LiveSttEngine,

        /// Override the Gemma STT model path for this run.
        /// Falls back to CACTUS_GEMMA_STT_MODEL_PATH, then CACTUS_DETECTION_MODEL_PATH,
        /// then CACTUS_MODEL_PATH.
        #[arg(long)]
        gemma_stt_model_path: Option<String>,

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

    /// Recover a vault token (tok_...) back to its original plaintext.
    ///
    /// Requires:
    /// - `MASKER_KEK` to be set (base64 32 bytes)
    /// - a persisted state file created during `masker live` / `masker stream`
    Detokenize {
        /// The token to recover (e.g. tok_...)
        #[arg(long)]
        token: String,

        /// Use case for choosing the correct DEK (e.g. healthcare)
        #[arg(long)]
        use_case: String,

        /// Path to the persisted state JSON (defaults to ~/.masker/state.json)
        #[arg(long)]
        state_file: Option<PathBuf>,
    },

    /// Core ML helpers (macOS-only).
    Coreml {
        #[command(subcommand)]
        cmd: CoremlCommand,
    },
}

fn default_state_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".masker").join("state.json")
}

fn load_state(path: &PathBuf) -> Result<PersistedState> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow!("failed to read state file {}: {e}", path.display()))?;
    serde_json::from_str(&raw)
        .map_err(|e| anyhow!("failed to parse state file {}: {e}", path.display()))
}

fn save_state(path: &PathBuf, state: &PersistedState) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            anyhow!(
                "failed to create state dir {}: {e}",
                parent.to_string_lossy()
            )
        })?;
    }
    let json = serde_json::to_string_pretty(state)?;
    std::fs::write(path, json)
        .map_err(|e| anyhow!("failed to write state file {}: {e}", path.display()))?;
    Ok(())
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    #[allow(dead_code)]
    sample_count: usize,
    peak_abs: i16,
    rms: f32,
}

#[cfg(feature = "cactus")]
struct LiveDetectionStatus {
    engine: &'static str,
    #[allow(dead_code)]
    model_path: Option<String>,
    #[allow(dead_code)]
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
fn build_live_pipeline(stt: Arc<dyn masker::SttBackend>) -> Result<(StreamingPipeline, LiveDetectionStatus)> {
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

    let kek = masker::Kek::from_env().unwrap_or_else(|_| {
        eprintln!(
            "[WARN] MASKER_KEK not set — using ephemeral key (vault tokens won't survive restart). \
             Set MASKER_KEK=$(openssl rand -base64 32) to persist."
        );
        masker::Kek::generate()
    });
    let pipeline = StreamingPipeline::new(stt, tts, detector, kek, masker::InMemorySink::new());

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
    let bar = term_dim("─".repeat(72));
    println!("{bar}");
    println!("{}", term_fg_bold("MASKER LIVE", TermColor::Cyan));
    println!("{bar}");
    println!("{} {}", term_dim("Session   :"), term_fg_bold(session, TermColor::Cyan));

    let route = match result.route {
        Route::LocalOnly => term_fg_bold(result.route.as_str(), TermColor::Magenta),
        Route::MaskedSend => term_fg_bold(result.route.as_str(), TermColor::Yellow),
        Route::SafeToSend => term_fg_bold(result.route.as_str(), TermColor::Green),
    };
    println!("{} {route}", term_dim("Route     :"));

    let risk = match result.detection.risk_level {
        masker::contracts::RiskLevel::None => term_fg_bold("none", TermColor::Gray),
        masker::contracts::RiskLevel::Low => term_fg_bold("low", TermColor::Green),
        masker::contracts::RiskLevel::Medium => term_fg_bold("medium", TermColor::Yellow),
        masker::contracts::RiskLevel::High => term_fg_bold("high", TermColor::Red),
    };
    println!("{} {risk}", term_dim("Risk      :"));

    let entities = format_entity_list(result);
    let entities = if entities == "none" {
        term_dim("none")
    } else {
        term_fg(entities, TermColor::Red)
    };
    println!("{} {entities}", term_dim("Entities  :"));

    println!(
        "{} {}",
        term_dim("Policy    :"),
        term_fg(result.policy.policy.as_str(), TermColor::Blue)
    );
    println!(
        "{} {}",
        term_dim("Latency   :"),
        term_fg_bold(format!("{:.0} ms", total_ms), TermColor::Green)
    );
    println!(
        "{} {}",
        term_dim("Audio     :"),
        term_dim(format!(
            "{} ms  peak_abs={}  rms={:.1}",
            duration_ms, signal.peak_abs, signal.rms
        ))
    );
    println!(
        "{} {}",
        term_dim("STT       :"),
        term_dim(stt_model_path.unwrap_or("CACTUS_STT_MODEL_PATH not set"))
    );
    println!(
        "{} {}{}",
        term_dim("Detection :"),
        term_dim(detection_status.engine),
        if detection_status.active {
            "".to_string()
        } else {
            term_fg(" (regex fallback only)", TermColor::Yellow)
        }
    );
    if let Some(err) = &detection_status.init_error {
        println!(
            "{} {}",
            term_dim("Init Error:"),
            term_fg(truncate(err, 120), TermColor::Yellow)
        );
    }
    println!();
    println!("{}", term_fg_bold("Raw Transcript", TermColor::Cyan));
    println!(
        "{}",
        prettify_raw_transcript(&result.raw_transcript, &result.detection.entities)
    );
    if result.masked_transcript != result.raw_transcript {
        println!();
        println!("{}", term_fg_bold("Sanitized", TermColor::Cyan));
        println!("{}", prettify_masked_transcript(&result.masked_transcript));
    }
    if let Some(warning) = repeated_segment_warning(&result.raw_transcript) {
        println!();
        println!("{}", term_fg_bold("Warning", TermColor::Yellow));
        println!("{}", term_fg(warning, TermColor::Yellow));
        println!("{}", term_dim("Tip: stop recording right after you finish speaking, or use `--seconds 8` for a bounded capture."));
    }
    println!();
    println!("{}", term_fg_bold("Trace", TermColor::Cyan));
    for event in &result.trace {
        let stage = match event.stage.as_str() {
            "stt" => term_fg(event.stage.as_str(), TermColor::Blue),
            "detect" | "detection" => term_fg(event.stage.as_str(), TermColor::Red),
            "policy" => term_fg(event.stage.as_str(), TermColor::Magenta),
            "mask" | "masking" => term_fg(event.stage.as_str(), TermColor::Cyan),
            _ => term_dim(event.stage.as_str()),
        };
        println!(
            "  {}  {:<10} {}",
            term_dim(format!("{:>6.0} ms", event.elapsed_ms)),
            stage,
            event.message
        );
    }
}

#[cfg(feature = "cactus")]
fn merge_spans(mut spans: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    spans.sort_by_key(|(s, _)| *s);
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        if start >= end {
            continue;
        }
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

#[cfg(feature = "cactus")]
fn paint_spans(text: &str, spans: &[(usize, usize)], codes: &str) -> String {
    if spans.is_empty() || !term_supports_color() {
        return text.to_string();
    }
    let spans = merge_spans(spans.to_vec());
    let mut out = String::with_capacity(text.len() + spans.len() * 8);
    let mut cursor = 0usize;
    for (start, end) in spans {
        let start = start.min(text.len());
        let end = end.min(text.len());
        if let Some(prefix) = text.get(cursor..start) {
            out.push_str(prefix);
        }
        if let Some(span) = text.get(start..end) {
            out.push_str(&term_paint(span, codes));
        } else {
            // If we hit invalid UTF-8 boundaries, fall back to the raw text.
            return text.to_string();
        }
        cursor = end;
    }
    if let Some(tail) = text.get(cursor..) {
        out.push_str(tail);
    }
    out
}

#[cfg(feature = "cactus")]
fn prettify_raw_transcript(raw: &str, entities: &[masker::contracts::Entity]) -> String {
    let spans = entities
        .iter()
        .map(|e| (e.start, e.end))
        .collect::<Vec<_>>();
    // Underline + red for sensitive spans.
    paint_spans(raw, &spans, "4;31")
}

#[cfg(feature = "cactus")]
fn prettify_masked_transcript(masked: &str) -> String {
    if !term_supports_color() {
        return masked.to_string();
    }

    let mut out = String::with_capacity(masked.len() + 32);
    let mut rest = masked;

    loop {
        let Some(start) = rest.find("[MASKED:") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find(']') else {
            out.push_str(after);
            break;
        };
        let token = &after[..end + 1];
        out.push_str(&term_fg_bold(token, TermColor::Cyan));
        rest = &after[end + 1..];
    }

    // Also highlight vault tokens if present.
    let mut final_out = String::with_capacity(out.len() + 16);
    let mut rest = out.as_str();
    loop {
        let Some(start) = rest.find("tok_") else {
            final_out.push_str(rest);
            break;
        };
        final_out.push_str(&rest[..start]);
        let after = &rest[start..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(after.len());
        let token = &after[..end];
        final_out.push_str(&term_fg_bold(token, TermColor::Magenta));
        rest = &after[end..];
    }

    final_out
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
        EntityType::RoutingNumber => "routing number",
        EntityType::AccountNumber => "account number",
        EntityType::Pin => "PIN",
        EntityType::IpAddress => "IP address",
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
        let status = if ok {
            term_fg_bold("OK", TermColor::Green)
        } else {
            term_fg_bold("MISMATCH", TermColor::Red)
        };
        println!("[{status}] {}", term_fg_bold(s.label, TermColor::Cyan));
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

    Ok(if failures == 0 { 0 } else { 1 })
}

fn run_command(command: Command) -> Result<i32> {
    match command {
        Command::FilterInput {
            text,
            policy,
            mask_mode,
        } => {
            let detection = masker::detection::detect(&text);
            let decision = masker::policy::decide(&detection, policy.into());
            let masked = masker::masking::mask(&text, &detection, mask_mode.into());
            let out = serde_json::json!({
                "masked": masked,
                "policy": decision,
                "detection": detection,
            });
            println!("{}", out);
            Ok(0)
        }

        Command::FilterOutput {
            text,
            detection_json,
        } => {
            let detection: DetectionResult = if let Some(raw) = detection_json {
                serde_json::from_str(&raw).map_err(|e| anyhow!("invalid detection_json: {e}"))?
            } else {
                masker::detection::detect(&text)
            };
            let safe = masker::masking::scrub_output(&text, &detection);
            println!("{safe}");
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
            audit: _,
        } => {
            let kek = Kek::from_env().map_err(|e| anyhow!("{e}"))?;
            let pipeline = StreamingPipeline::new(
                std::sync::Arc::new(masker::StubStt),
                std::sync::Arc::new(masker::StubTts),
                std::sync::Arc::new(masker::RegexDetector),
                kek,
                masker::InMemorySink::new(),
            );
            let mut seq = 0u64;

            if let Some(one) = text {
                let result =
                    process_stream_chunk(&pipeline, &session, api_key.as_deref(), seq, &one)?;
                println!("{}", stream_result_json(&result));
            } else {
                let stdin = io::stdin();
                for line in stdin.lock().lines() {
                    let line = line?;
                    if line.trim().is_empty() {
                        continue;
                    }
                    let result = process_stream_chunk(
                        &pipeline,
                        &session,
                        api_key.as_deref(),
                        seq,
                        line.trim_end(),
                    )?;
                    println!("{}", stream_result_json(&result));
                    seq += 1;
                }
            }

            let state_path = default_state_path();
            save_state(&state_path, &pipeline.export_state())?;

            Ok(0)
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
            stt_engine,
            gemma_stt_model_path,
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
                    stt_engine,
                    gemma_stt_model_path,
                    detection_model_path,
                    detect,
                    keep_audio,
                    play_input,
                    play_output,
                    output,
                );
                return Err(anyhow!(
                    "binary built without `cactus` feature — rebuild with `cargo build --features cactus`"
                ));
            }

            #[cfg(feature = "cactus")]
            {
                let t0 = Instant::now();

                let stt_backend: Arc<dyn masker::SttBackend> = match stt_engine {
                    LiveSttEngine::Cactus => {
                        if let Some(path) = stt_model_path {
                            std::env::set_var("CACTUS_STT_MODEL_PATH", path);
                        } else if let Some(preset) = stt {
                            let path = resolve_stt_preset_path(preset)?;
                            std::env::set_var("CACTUS_STT_MODEL_PATH", path);
                        }
                        Arc::new(masker::CactusSttBackend::from_env()?)
                    }
                    LiveSttEngine::Gemma4 => {
                        if let Some(path) = gemma_stt_model_path {
                            std::env::set_var("CACTUS_GEMMA_STT_MODEL_PATH", path);
                        }
                        Arc::new(masker::GemmaAudioSttBackend::from_env()?)
                    }
                };

                if let Some(path) = detection_model_path {
                    std::env::set_var("CACTUS_DETECTION_MODEL_PATH", path);
                } else if detect.is_some() {
                    if let Some(path) = resolve_default_detection_model_path() {
                        std::env::set_var("CACTUS_DETECTION_MODEL_PATH", path);
                    }
                }

                let (pipeline, detection_status) = build_live_pipeline(stt_backend)?;

                let pcm_path = default_live_artifact_path("pcm");
                if let Some(audio) = audio_file {
                    normalize_audio_to_pcm(&audio, &pcm_path)?;
                } else if interactive {
                    record_audio_until_enter_with_ffmpeg(&pcm_path, &input)?;
                } else {
                    record_audio_with_ffmpeg(&pcm_path, seconds, &input)?;
                }
                let pcm_bytes = fs::read(&pcm_path)?;

                // Always provide a normalized WAV artifact for audio-aware Gemma detection and
                // Gemma-powered transcription (if selected).
                let wav_path = default_live_artifact_path("wav");
                pcm_to_wav(&pcm_path, &wav_path)?;

                if play_input {
                    let _ = ProcessCommand::new("afplay").arg(&wav_path).status();
                }

                let duration_ms = pcm_duration_ms(pcm_bytes.len());
                let signal = pcm_signal_stats(&pcm_bytes);

                let chunk = AudioChunk {
                    seq: 0,
                    data: pcm_bytes,
                    source_path: Some(wav_path.display().to_string()),
                    sample_rate: 16_000,
                    duration_ms,
                };

                let result = pipeline.process(&session, api_key.as_deref(), &chunk)?;
                let total_ms = t0.elapsed().as_secs_f64() * 1000.0;

                match output {
                    LiveOutputFormat::Human => {
                        print_live_human_summary(
                            &session,
                        &result,
                        total_ms,
                        signal,
                        duration_ms,
                        &detection_status,
                        match stt_engine {
                            LiveSttEngine::Cactus => std::env::var("CACTUS_STT_MODEL_PATH").ok(),
                            LiveSttEngine::Gemma4 => std::env::var("CACTUS_GEMMA_STT_MODEL_PATH")
                                .ok()
                                .or_else(|| std::env::var("CACTUS_DETECTION_MODEL_PATH").ok())
                                .or_else(|| std::env::var("CACTUS_MODEL_PATH").ok()),
                        }
                        .as_deref(),
                    );
                }
                LiveOutputFormat::Json => {
                    println!("{}", serde_json::to_string(&result)?);
                }
                    LiveOutputFormat::PrettyJson => {
                        println!("{}", serde_json::to_string_pretty(&result)?);
                    }
                }

                if play_output {
                    play_redacted_output_audio(&wav_path, &result, duration_ms)?;
                }

                if !keep_audio {
                    let _ = fs::remove_file(&pcm_path);
                    let _ = fs::remove_file(&wav_path);
                }

                let state_path = default_state_path();
                save_state(&state_path, &pipeline.export_state())?;

                Ok(0)
            }
        }

        Command::Detokenize {
            token,
            use_case,
            state_file,
        } => {
            let state_path = state_file.unwrap_or_else(default_state_path);
            let state = load_state(&state_path)?;

            let kek = Kek::from_env().map_err(|e| anyhow!("{e}"))?;
            let key_store = masker::KeyStore::new(kek);
            key_store.import_wrapped_deks(state.wrapped_deks);

            let vault = TokenVault::new();
            vault.import(state.token_entries);

            let dek = key_store
                .dek_for(&use_case)
                .map_err(|e| anyhow!("key store: {e}"))?;
            let recovered = vault.get(&token, &dek).map_err(|e| anyhow!("vault: {e}"))?;
            println!("{recovered}");
            Ok(0)
        }

        Command::Coreml { cmd } => match cmd {
            CoremlCommand::Metadata { model } => {
                let out = coremlcompiler_metadata(&model)?;
                print!("{out}");
                Ok(0)
            }
            CoremlCommand::Compile { model, out_dir } => {
                let compiled = coremlcompiler_compile(&model, &out_dir)?;
                for path in compiled {
                    println!("{}", path.display());
                }
                Ok(0)
            }
            CoremlCommand::CheckGemmaE2b { gemma_dir } => {
                coreml_check_gemma_e2b(&gemma_dir)?;
                Ok(0)
            }
        },
    }
}

fn coremlcompiler_metadata(model: &Path) -> Result<String> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = model;
        return Err(anyhow!("Core ML utilities are only supported on macOS"));
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("xcrun")
            .arg("coremlcompiler")
            .arg("metadata")
            .arg(model)
            .output()
            .map_err(|e| anyhow!("failed to run `xcrun coremlcompiler metadata`: {e}"))?;

        if !output.status.success() {
            return Err(anyhow!(
                "coremlcompiler metadata failed (code={:?}): {}{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn coremlcompiler_compile(model: &Path, out_dir: &Path) -> Result<Vec<PathBuf>> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (model, out_dir);
        return Err(anyhow!("Core ML utilities are only supported on macOS"));
    }

    #[cfg(target_os = "macos")]
    {
        std::fs::create_dir_all(out_dir)
            .map_err(|e| anyhow!("failed to create out_dir {}: {e}", out_dir.display()))?;

        let output = std::process::Command::new("xcrun")
            .arg("coremlcompiler")
            .arg("compile")
            .arg(model)
            .arg(out_dir)
            .output()
            .map_err(|e| anyhow!("failed to run `xcrun coremlcompiler compile`: {e}"))?;

        if !output.status.success() {
            return Err(anyhow!(
                "coremlcompiler compile failed (code={:?}): {}{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            ));
        }

        let mut compiled = Vec::new();
        for entry in std::fs::read_dir(out_dir)
            .map_err(|e| anyhow!("failed to read out_dir {}: {e}", out_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "mlmodelc") {
                compiled.push(path);
            }
        }

        if compiled.is_empty() {
            return Err(anyhow!(
                "coremlcompiler compile succeeded, but no `.mlmodelc` found in {}",
                out_dir.display()
            ));
        }

        Ok(compiled)
    }
}

fn coreml_check_gemma_e2b(gemma_dir: &Path) -> Result<()> {
    let expected = [
        ("audio_encoder", "audio_encoder.mlpackage", "audio_encoder.mlmodelc"),
        ("vision_encoder", "vision_encoder.mlpackage", "vision_encoder.mlmodelc"),
        ("model", "model.mlpackage", "model.mlmodelc"),
    ];

    let mut missing = Vec::new();
    for (label, mlpackage, mlmodelc) in expected {
        let pkg = gemma_dir.join(mlpackage);
        let compiled = gemma_dir.join(mlmodelc);
        if pkg.exists() || compiled.exists() {
            continue;
        }
        missing.push(format!("{label} ({mlpackage} or {mlmodelc})"));
    }

    if missing.is_empty() {
        println!(
            "OK: Core ML artifacts found under {}",
            gemma_dir.display()
        );
        return Ok(());
    }

    Err(anyhow!(
        "missing Core ML artifacts under {}: {}",
        gemma_dir.display(),
        missing.join(", ")
    ))
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
