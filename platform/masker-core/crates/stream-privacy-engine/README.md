# stream-privacy-engine (Rust MVP)

Deterministic, stateful streaming privacy detection engine for ASR transcript chunks.

## Architecture
- `events.rs`: transcript event contract (partial vs final).
- `models.rs`: token, span candidate, detection, redaction models.
- `normalizer.rs`: spoken-form normalization (digits, `double`, `oh`).
- `triggers.rs`: deterministic trigger phrase matcher.
- `assembler.rs`: finite-state assembler (`IDLE`, `EXPECTING_*` style) for SSN, phone, DOB, member ID.
- `engine.rs`: orchestration, rolling context, provisional vs committed behavior.
- `policy.rs`: progressive confidence policy mapping.
- `audit.rs`: JSON-safe audit records (no raw sensitive values).
- `learned_detector.rs`: pluggable stub interface for future ML model.

## Run
```bash
cd platform/masker-core
cargo test -p stream-privacy-engine
cargo run -p stream-privacy-engine --example simulate_stream
```
