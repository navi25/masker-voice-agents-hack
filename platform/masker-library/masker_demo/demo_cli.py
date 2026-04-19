from __future__ import annotations

import argparse
import queue
from pathlib import Path

from .benchmark import DEFAULT_MODELS, main as benchmark_main
from .events import EventSubscription
from .models import SessionConfig
from .redactor import SessionRedactor
from .session import DemoSessionManager, default_session_id
from .settings import SETTINGS


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="python -m masker_demo", description="Masker local-first demo backend")
    subparsers = parser.add_subparsers(dest="command", required=True)

    subparsers.add_parser("ui", help="Launch the interactive terminal demo UI")

    serve = subparsers.add_parser("serve", help="Start the FastAPI service")
    serve.add_argument("--host", default=SETTINGS.host)
    serve.add_argument("--port", type=int, default=SETTINGS.port)
    serve.add_argument("--reload", action="store_true")

    live = subparsers.add_parser("live", help="Run a live microphone session")
    _add_common_runtime_args(live)

    replay = subparsers.add_parser("replay", help="Replay a mono 16-bit 16kHz WAV file")
    replay.add_argument("audio_path")
    replay.add_argument("--no-realtime", action="store_true")
    _add_common_runtime_args(replay)

    test_redaction = subparsers.add_parser("test-redaction", help="Run text fixtures through the redactor")
    test_redaction.add_argument("--fixture", choices=["all", "healthcare", "finance", "phone_address"], default="all")

    benchmark = subparsers.add_parser("benchmark", help="Benchmark STT models on replay fixtures")
    benchmark.add_argument("--models", nargs="+", default=DEFAULT_MODELS)
    benchmark.add_argument("--json", action="store_true")

    return parser


def _add_common_runtime_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--session-id", default=default_session_id())
    parser.add_argument("--stt-model", default=SETTINGS.stt_model)
    parser.add_argument("--language", default=SETTINGS.language)
    parser.add_argument("--no-model", action="store_true")
    parser.add_argument("--policy-mode", default=SETTINGS.policy_mode)
    parser.add_argument("--device", default=SETTINGS.default_device)
    parser.add_argument("--partial-interval-ms", type=int, default=SETTINGS.partial_interval_ms)


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.command == "ui":
        from .demo_ui import run as run_ui
        run_ui()
        return 0

    if args.command == "serve":
        import uvicorn

        uvicorn.run(
            "masker_demo.app:app",
            host=args.host,
            port=args.port,
            reload=args.reload,
        )
        return 0

    if args.command == "test-redaction":
        return run_redaction_test(args.fixture)

    if args.command == "benchmark":
        benchmark_args = ["--models", *args.models]
        if args.json:
            benchmark_args.append("--json")
        return benchmark_main(benchmark_args)

    manager = DemoSessionManager(safe_log_dir=SETTINGS.log_dir)
    subscription = manager.subscribe()

    if args.command == "live":
        config = SessionConfig(
            session_id=args.session_id,
            audio_mode="mic",
            stt_model=args.stt_model,
            language=args.language,
            no_model=args.no_model,
            policy_mode=args.policy_mode,
            partial_interval_ms=args.partial_interval_ms,
            sample_rate=SETTINGS.sample_rate,
            device=args.device,
        )
    else:
        config = SessionConfig(
            session_id=args.session_id,
            audio_mode="replay",
            audio_path=args.audio_path,
            stt_model=args.stt_model,
            language=args.language,
            no_model=args.no_model,
            policy_mode=args.policy_mode,
            partial_interval_ms=args.partial_interval_ms,
            sample_rate=SETTINGS.sample_rate,
            device=args.device,
            simulate_realtime=not args.no_realtime,
        )

    print_banner(config)
    manager.start(config)
    return stream_cli_events(manager, subscription)


def stream_cli_events(manager: DemoSessionManager, subscription: EventSubscription) -> int:
    exit_code = 0
    try:
        for event in subscription.replay:
            _print_event(event)

        while manager.is_running or not subscription.queue.empty():
            try:
                event = subscription.queue.get(timeout=0.5)
            except queue.Empty:
                continue
            _print_event(event)
            if event["type"] == "error":
                exit_code = 1
    except KeyboardInterrupt:
        manager.stop()
        manager.wait(timeout=2.0)
    finally:
        manager.event_bus.unsubscribe(subscription)
    return exit_code


def _print_event(event: dict) -> None:
    event_type = event["type"]
    if event_type == "session.started":
        print(f"\nSession {event['session_id']} started")
        return
    if event_type == "audio.speech_started":
        print(f"\n[AUDIO] speech started ({event['utterance_id']})")
        return
    if event_type == "audio.speech_ended":
        print(f"[AUDIO] speech ended ({event['utterance_id']}, {event['duration_ms']} ms)")
        return
    if event_type == "transcript.partial":
        print("\nRAW PARTIAL")
        print(event["raw_text"])
        print("REDACTED PARTIAL")
        print(event["redacted_text"])
        return
    if event_type == "transcript.final":
        print("\nRAW FINAL")
        print(event["raw_text"])
        print("REDACTED FINAL")
        print(event["redacted_text"])
        return
    if event_type == "entities.detected":
        print("DETECTIONS")
        for entity in event["entities"]:
            print(f"  - {entity['entity_type']}: {entity['raw_value']} -> {entity['token']}")
        return
    if event_type == "model.input.ready":
        print("MASKED MODEL INPUT")
        print(event["masked_prompt"])
        return
    if event_type == "model.output":
        print("MODEL OUTPUT")
        if event.get("skipped"):
            print(f"  skipped: {event.get('reason', 'unknown')}")
        else:
            print(event.get("text", ""))
        return
    if event_type == "log.safe_entry":
        print("SAFE LOG ENTRY")
        safe_log = event["safe_log"]
        print(safe_log["redacted_text"])
        print(f"log file: {event['log_path']}")
        return
    if event_type == "error":
        print(f"ERROR: {event['message']}")
        return
    if event_type == "session.stopped":
        print(f"\nSession stopped ({event['reason']})")
        return


def print_banner(config: SessionConfig) -> None:
    print("============================================================")
    print("                MASKER LIVE REDACTION DEMO")
    print("============================================================")
    print(f"Session      : {config.session_id}")
    print(f"Audio mode   : {config.audio_mode}")
    if config.audio_path:
        print(f"Replay file  : {config.audio_path}")
    print(f"STT model    : {config.stt_model}")
    print(f"No model     : {config.no_model}")
    print("------------------------------------------------------------")


def run_redaction_test(fixture: str) -> int:
    fixture_dir = Path(__file__).resolve().parent.parent / "demos"
    selected = []
    if fixture == "all":
        selected = sorted(fixture_dir.glob("*.txt"))
    else:
        selected = [fixture_dir / f"{fixture}.txt"]

    redactor = SessionRedactor()
    for fixture_path in selected:
        text = fixture_path.read_text(encoding="utf-8").strip()
        result = redactor.redact(text)
        print(f"\n{fixture_path.stem.upper()}")
        print("RAW")
        print(result.raw_text)
        print("REDACTED")
        print(result.redacted_text)
        print("TOKENS")
        for entity in result.entities:
            print(f"  - {entity.entity_type}: {entity.token} -> {entity.raw_value}")
    return 0
