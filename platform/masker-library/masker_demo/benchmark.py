from __future__ import annotations

import argparse
import json
import time
import wave
from dataclasses import dataclass
from pathlib import Path

from .redactor import SessionRedactor
from .stt import FasterWhisperTranscriber


DEFAULT_MODELS = ["tiny.en", "base.en", "small.en"]


@dataclass(frozen=True)
class FixtureSpec:
    name: str
    wav_path: Path
    expected_entity_types: tuple[str, ...]


def load_fixture_specs(base_dir: Path) -> list[FixtureSpec]:
    demos_dir = base_dir / "demos"
    return [
        FixtureSpec(
            name="healthcare",
            wav_path=demos_dir / "healthcare.wav",
            expected_entity_types=("PERSON", "DOB", "SSN"),
        ),
        FixtureSpec(
            name="finance",
            wav_path=demos_dir / "finance.wav",
            expected_entity_types=("PERSON", "CARD", "PHONE"),
        ),
        FixtureSpec(
            name="phone_address",
            wav_path=demos_dir / "phone_address.wav",
            expected_entity_types=("PERSON", "PHONE", "ADDRESS", "EMAIL"),
        ),
    ]


def pcm_from_wav(path: Path) -> tuple[bytes, int, float]:
    with wave.open(str(path), "rb") as wav_file:
        if wav_file.getnchannels() != 1 or wav_file.getsampwidth() != 2:
            raise RuntimeError(f"{path} must be mono 16-bit PCM WAV")
        sample_rate = wav_file.getframerate()
        pcm = wav_file.readframes(wav_file.getnframes())
        duration_s = wav_file.getnframes() / float(sample_rate)
    return pcm, sample_rate, duration_s


def benchmark_model(model_name: str, fixtures: list[FixtureSpec]) -> dict:
    transcriber = FasterWhisperTranscriber(model_name_or_path=model_name)
    redactor = SessionRedactor()

    fixture_rows: list[dict] = []
    cold_load_ms = None

    for index, fixture in enumerate(fixtures):
        redactor.reset()
        pcm, sample_rate, duration_s = pcm_from_wav(fixture.wav_path)

        start = time.perf_counter()
        stt_result = transcriber.transcribe_pcm16(pcm, sample_rate=sample_rate, language="en")
        transcribe_ms = (time.perf_counter() - start) * 1000
        if index == 0:
            cold_load_ms = transcribe_ms

        redact_start = time.perf_counter()
        redaction = redactor.redact(stt_result.text)
        redaction_ms = (time.perf_counter() - redact_start) * 1000

        detected_types = sorted({entity.entity_type for entity in redaction.entities})
        expected_types = list(fixture.expected_entity_types)
        matched_types = sorted(set(detected_types) & set(expected_types))
        missing_types = sorted(set(expected_types) - set(detected_types))
        extra_types = sorted(set(detected_types) - set(expected_types))

        fixture_rows.append(
            {
                "fixture": fixture.name,
                "audio_seconds": round(duration_s, 3),
                "transcribe_ms": round(transcribe_ms, 1),
                "redaction_ms": round(redaction_ms, 3),
                "pipeline_ms": round(transcribe_ms + redaction_ms, 1),
                "rtf": round((transcribe_ms / 1000) / duration_s, 3) if duration_s else None,
                "detected_entity_types": detected_types,
                "expected_entity_types": expected_types,
                "matched_entity_types": matched_types,
                "missing_entity_types": missing_types,
                "extra_entity_types": extra_types,
                "entity_recall": round(len(matched_types) / len(expected_types), 3) if expected_types else 1.0,
                "transcript": stt_result.text,
                "redacted_text": redaction.redacted_text,
            }
        )

    warm_rows = fixture_rows[1:] if len(fixture_rows) > 1 else fixture_rows
    avg_pipeline_ms = sum(row["pipeline_ms"] for row in fixture_rows) / len(fixture_rows)
    avg_warm_pipeline_ms = sum(row["pipeline_ms"] for row in warm_rows) / len(warm_rows)
    avg_recall = sum(row["entity_recall"] for row in fixture_rows) / len(fixture_rows)
    avg_rtf = sum(row["rtf"] for row in fixture_rows if row["rtf"] is not None) / len(fixture_rows)

    return {
        "model": model_name,
        "cold_first_fixture_ms": round(cold_load_ms or 0.0, 1),
        "avg_pipeline_ms": round(avg_pipeline_ms, 1),
        "avg_warm_pipeline_ms": round(avg_warm_pipeline_ms, 1),
        "avg_entity_recall": round(avg_recall, 3),
        "avg_rtf": round(avg_rtf, 3),
        "fixtures": fixture_rows,
    }


def render_human(results: list[dict]) -> str:
    lines: list[str] = []
    lines.append("============================================================")
    lines.append("        MASKER STT + REDACTION BENCHMARK")
    lines.append("============================================================")
    for result in results:
        lines.append(
            f"{result['model']}: cold={result['cold_first_fixture_ms']} ms, "
            f"avg={result['avg_pipeline_ms']} ms, warm_avg={result['avg_warm_pipeline_ms']} ms, "
            f"avg_recall={result['avg_entity_recall']:.3f}, avg_rtf={result['avg_rtf']:.3f}"
        )
        for row in result["fixtures"]:
            lines.append(
                f"  - {row['fixture']}: pipeline={row['pipeline_ms']} ms, "
                f"recall={row['entity_recall']:.3f}, missing={row['missing_entity_types'] or ['-']}, "
                f"extra={row['extra_entity_types'] or ['-']}"
            )
    return "\n".join(lines)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Benchmark STT model latency and redaction coverage")
    parser.add_argument("--models", nargs="+", default=DEFAULT_MODELS)
    parser.add_argument("--json", action="store_true")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    base_dir = Path(__file__).resolve().parent.parent
    fixtures = load_fixture_specs(base_dir)
    results = [benchmark_model(model_name, fixtures) for model_name in args.models]
    if args.json:
        print(json.dumps({"results": results}, indent=2))
    else:
        print(render_human(results))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
