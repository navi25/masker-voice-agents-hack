# NEXT STEPS

## 1) Add DeBERTa token classification
- Implement a `LearnedDetector` backend with ONNX Runtime or candle.
- Feed committed token windows plus timing/speaker features.
- Calibrate model scores into `suspected/likely/confirmed` policy thresholds.

## 2) Training dataset schema
- Suggested JSONL schema:
  - `utterance_id`, `chunk_id`, `speaker_id`, `is_final`
  - `tokens`: `{raw, normalized, start_ms, end_ms}`
  - `labels`: BIO tags (`B-SSN`, `I-SSN`, etc.)
  - `entity_meta`: hashed value, type, domain
- Include partial-hypothesis correction examples and chunk boundary cases.

## 3) ASR adapters
- Add adapters for Deepgram, Whisper streaming, and Cactus local ASR.
- Map provider event payloads into `TranscriptEvent`.
- Preserve provider revision IDs for partial->final replacement semantics.

## 4) Domain-specific entities (healthcare + finance)
- Healthcare: MRN, policy number, claim number, Rx number.
- Finance: account number, routing number, card PAN fragments, tax ID.
- Add per-entity deterministic assemblers and policy overrides.
