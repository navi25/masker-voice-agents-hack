# Masker Demo Backend

This package provides the demo-safe live pipeline for tomorrow's presentation:

`Mic -> VAD -> faster-whisper -> Masker redaction -> events -> masked prompt -> safe logs`

It is intentionally local-first and demo-oriented:

- raw and redacted transcript events are emitted separately
- only the redacted transcript is eligible for downstream model input
- safe logs persist only redacted content
- the model leg can be disabled with `--no-model`

## Quick Start

```bash
cd platform/masker-library
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
cp .env.example .env
```

Start the backend service:

```bash
uvicorn masker_demo.app:app --host 127.0.0.1 --port 8008 --reload
```

Or with the built-in CLI wrapper:

```bash
python -m masker_demo serve --reload
```

## CLI Demo

Interactive terminal UI (includes a live mic mode via `L`):

```bash
python -m masker_demo ui
```

Live mic mode:

```bash
python -m masker_demo live --no-model
```

Replay a generated WAV fixture:

```bash
python -m masker_demo replay demos/healthcare.wav --no-model
```

Run text-only redaction checks:

```bash
python -m masker_demo test-redaction
```

Benchmark multiple STT models on latency and redaction coverage:

```bash
python -m masker_demo benchmark --models tiny.en base.en small.en
```

Generate local WAV fixtures on macOS:

```bash
bash scripts/generate_demo_audio.sh
```

## API

- `GET /health`
- `POST /api/session/start`
- `POST /api/session/stop`
- `POST /api/session/reset`
- `GET /api/events/stream`
- `WS /ws/demo`

Minimal start payload:

```json
{
  "audio_mode": "mic",
  "no_model": true
}
```

Replay payload:

```json
{
  "audio_mode": "replay",
  "audio_path": "demos/healthcare.wav",
  "no_model": true
}
```

## Event Shape

Example transcript event:

```json
{
  "type": "transcript.partial",
  "session_id": "ses_demo",
  "utterance_id": "utt_1",
  "raw_text": "Hi, I'm Ravi Kumar",
  "redacted_text": "Hi, I'm PERSON_1",
  "entities": [
    {
      "entity_type": "PERSON",
      "raw_value": "Ravi Kumar",
      "token": "PERSON_1",
      "start": 8,
      "end": 18,
      "confidence": 0.8
    }
  ],
  "timestamp_ms": 1712345678901,
  "is_final": false
}
```

Emitted event types:

- `session.started`
- `audio.speech_started`
- `audio.speech_ended`
- `transcript.partial`
- `transcript.final`
- `entities.detected`
- `model.input.ready`
- `model.output`
- `log.safe_entry`
- `session.reset`
- `session.stopped`
- `error`

## Notes

- The replay path expects a mono 16-bit 16kHz WAV file for reliability.
- `faster-whisper` handles STT locally; the first run may download the chosen model.
- The current model call is a local stub, but the event contract already isolates masked model input from raw transcript handling.

## Tests

```bash
cd platform/masker-library
python -m unittest discover -s tests -v
```
