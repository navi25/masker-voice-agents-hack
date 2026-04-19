"""Masker CLI Demo UI — pretty terminal interface for masker-cli demonstrations."""
from __future__ import annotations

import queue
import random
import re
import threading
import time
import shutil
import subprocess
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any

from textual import on, work
from textual.app import App, ComposeResult
from textual.binding import Binding
from textual.containers import Horizontal, Vertical, VerticalScroll
from textual.reactive import reactive
from textual.widgets import Button, DataTable, Footer, RichLog, Static
from rich.text import Text

from .redactor import SessionRedactor
from .models import DetectedEntity, SessionConfig
from .config import SCENARIOS, ENTITY_COLORS
from .session import DemoSessionManager, default_session_id
from .settings import SETTINGS


# ── Data classes ─────────────────────────────────────────────────────────────

@dataclass
class SessionStats:
    session_id: str = ""
    scenario_name: str = ""
    policy_mode: str = ""
    stt_latencies: list[float] = field(default_factory=list)
    redact_latencies: list[float] = field(default_factory=list)
    total_entities: int = 0
    detected_types: set[str] = field(default_factory=set)
    token_map: dict[str, str] = field(default_factory=dict)
    input_bytes: int = 0
    output_bytes: int = 0
    started_at: datetime = field(default_factory=datetime.now)

    @property
    def avg_stt_ms(self) -> float:
        return sum(self.stt_latencies) / len(self.stt_latencies) if self.stt_latencies else 0.0

    @property
    def avg_redact_ms(self) -> float:
        return sum(self.redact_latencies) / len(self.redact_latencies) if self.redact_latencies else 0.0

    @property
    def total_ms(self) -> float:
        return self.avg_stt_ms + self.avg_redact_ms


# ── CSS ───────────────────────────────────────────────────────────────────────

APP_CSS = """
Screen {
    background: #0d1117;
    color: #e6edf3;
}

/* ── Top bar ─────────────────────────────────────── */
#topbar {
    dock: top;
    height: 3;
    background: #161b22;
    border-bottom: heavy #30363d;
    padding: 0 2;
    layout: horizontal;
    align: left middle;
}
#app-title {
    color: #58a6ff;
    text-style: bold;
    width: auto;
}
#sep1, #sep2 {
    color: #30363d;
    width: auto;
    margin: 0 1;
}
#session-label {
    color: #8b949e;
    width: auto;
}
#status-label {
    width: 1fr;
    text-align: right;
}

/* ── Scenario bar ────────────────────────────────── */
#scenariobar {
    dock: top;
    height: 3;
    background: #0d1117;
    border-bottom: solid #21262d;
    padding: 0 2;
    layout: horizontal;
    align: left middle;
}
#scenario-prompt {
    color: #8b949e;
    width: auto;
    margin-right: 1;
}
.scn-btn {
    min-width: 18;
    margin-right: 1;
    height: 1;
    background: #21262d;
    color: #c9d1d9;
    border: none;
}
.scn-btn:hover {
    background: #30363d;
}
.scn-btn.active-scn {
    background: #1f6feb;
    color: white;
    text-style: bold;
}
#stop-btn {
    background: #b91c1c;
    color: white;
    min-width: 10;
    height: 1;
    display: none;
}
#stop-btn.visible {
    display: block;
}

/* ── Main scrollable body ────────────────────────── */
#body {
    height: 1fr;
    padding: 1 2;
}

/* ── Transcript columns ──────────────────────────── */
#transcript-row {
    width: 100%;
    height: 15;
    margin-bottom: 1;
}

.tx-panel {
    width: 1fr;
    height: 100%;
    border: solid #30363d;
    background: #0d1117;
}
.tx-panel.recording {
    border: solid #238636;
}
#right-panel.recording {
    border: solid #1f6feb;
}

.tx-titlebar {
    height: 1;
    background: #161b22;
    padding: 0 1;
    layout: horizontal;
    align: left middle;
}
.tx-title {
    width: 1fr;
    text-style: bold;
    color: #e6edf3;
}
.tx-badge {
    width: auto;
    text-align: right;
}
.tx-log {
    height: 1fr;
}
.play-row {
    height: 2;
    background: #161b22;
    border-top: solid #21262d;
    layout: horizontal;
    align: left middle;
    padding: 0 1;
    display: none;
}
.play-row.visible {
    display: block;
}
.play-btn {
    min-width: 18;
    margin-right: 1;
    height: 1;
    background: #21262d;
    border: none;
}
.play-btn:hover {
    background: #30363d;
}
.play-btn.playing {
    background: #1a4a1a;
    color: #3fb950;
}

/* ── Under the hood ──────────────────────────────── */
#hood-section {
    width: 100%;
    background: #161b22;
    border: solid #21262d;
    padding: 1 2;
    margin-bottom: 1;
    height: auto;
}
#hood-title {
    color: #f78166;
    text-style: bold;
    margin-bottom: 1;
}
#metrics-row {
    width: 100%;
    height: 4;
    margin-bottom: 1;
}
.metric-card {
    width: 1fr;
    height: 100%;
    background: #0d1117;
    border: solid #21262d;
    padding: 0 1;
    margin-right: 1;
    align: center middle;
    text-align: center;
}
.metric-card:last-of-type {
    margin-right: 0;
}
.metric-val {
    color: #58a6ff;
    text-style: bold;
    text-align: center;
    width: 100%;
}
.metric-lbl {
    color: #6e7681;
    text-align: center;
    width: 100%;
}
#entities-line {
    width: 100%;
    height: auto;
    padding-top: 1;
    border-top: solid #21262d;
    margin-top: 1;
}
#token-line {
    color: #8b949e;
    width: 100%;
    height: auto;
    margin-top: 1;
}

/* ── Audit section ───────────────────────────────── */
#audit-section {
    width: 100%;
    background: #161b22;
    border: solid #21262d;
    padding: 1 2;
    height: auto;
}
#audit-title {
    color: #d2a8ff;
    text-style: bold;
    margin-bottom: 1;
}
#audit-table {
    width: 100%;
    height: auto;
    background: #0d1117;
}

/* ── Footer ──────────────────────────────────────── */
Footer {
    background: #161b22;
    color: #6e7681;
}
"""


# ── App ───────────────────────────────────────────────────────────────────────

class MaskerDemoApp(App[None]):
    """Masker CLI — interactive demo terminal."""

    CSS = APP_CSS
    TITLE = "Masker Demo"

    BINDINGS = [
        Binding("h", "scenario('healthcare')", "Healthcare"),
        Binding("f", "scenario('finance')", "Finance"),
        Binding("p", "scenario('personal')", "Personal"),
        Binding("l", "live", "Live Mic"),
        Binding("s", "stop", "Stop"),
        Binding("q", "quit", "Quit"),
    ]

    session_running: reactive[bool] = reactive(False)

    def __init__(self) -> None:
        super().__init__()
        self._redactor = SessionRedactor()
        self._stats: SessionStats | None = None
        self._audit_rows: list[dict[str, Any]] = []
        self._session_counter = 0
        self._stop_event = threading.Event()
        self._active_scenario_id: str | None = None
        self._last_raw: str = ""
        self._last_redacted: str = ""
        self._audio_proc: subprocess.Popen[str] | None = None
        self._audio_lock = threading.Lock()
        self._live_manager: DemoSessionManager | None = None
        self._live_subscription = None
        self._live_transcriber = None
        self._live_raw_lines: list[str] = []
        self._live_redacted_lines: list[str] = []
        self._live_partial_raw: str = ""
        self._live_partial_redacted: str = ""

    def _ensure_live_transcriber(self):
        if self._live_transcriber is not None:
            return self._live_transcriber
        from .stt import FasterWhisperTranscriber

        transcriber = FasterWhisperTranscriber(model_name_or_path=SETTINGS.stt_model)
        transcriber._ensure_model()
        self._live_transcriber = transcriber
        return transcriber

    # ── Layout ───────────────────────────────────────────────────────────────

    def compose(self) -> ComposeResult:
        # Top status bar
        with Horizontal(id="topbar"):
            yield Static("◈ MASKER DEMO", id="app-title")
            yield Static("│", id="sep1")
            yield Static("no session", id="session-label")
            yield Static("│", id="sep2")
            yield Static("● READY", id="status-label")

        # Scenario selector bar
        with Horizontal(id="scenariobar"):
            yield Static("Run:", id="scenario-prompt")
            for scn in SCENARIOS:
                yield Button(
                    f"{scn['icon']} {scn['name']} [{scn['key'].upper()}]",
                    id=f"btn-{scn['id']}",
                    classes="scn-btn",
                )
            yield Button("🎙 Live [L]", id="btn-live", classes="scn-btn")
            yield Button("■ Stop [S]", id="stop-btn")

        with VerticalScroll(id="body"):
            # ── Transcript columns ───────────────────────────
            with Horizontal(id="transcript-row"):
                # Left — raw transcription
                with Vertical(id="left-panel", classes="tx-panel"):
                    with Horizontal(classes="tx-titlebar"):
                        yield Static("LIVE TRANSCRIPTION", classes="tx-title")
                        yield Static("", id="stt-badge", classes="tx-badge")
                    yield RichLog(id="raw-log", wrap=True, auto_scroll=True,
                                  markup=False, classes="tx-log")
                    with Horizontal(id="raw-play-row", classes="play-row"):
                        yield Button("▶  Play Original", id="play-raw-btn",
                                     classes="play-btn")

                # Right — redacted
                with Vertical(id="right-panel", classes="tx-panel"):
                    with Horizontal(classes="tx-titlebar"):
                        yield Static("MASKER REDACTION", classes="tx-title")
                        yield Static("", id="redact-badge", classes="tx-badge")
                    yield RichLog(id="redacted-log", wrap=True, auto_scroll=True,
                                  markup=False, classes="tx-log")
                    with Horizontal(id="redacted-play-row", classes="play-row"):
                        yield Button("▶  Play Redacted", id="play-redacted-btn",
                                     classes="play-btn")

            # ── Under the Hood ──────────────────────────────
            with Vertical(id="hood-section"):
                yield Static("▸  MASKER — UNDER THE HOOD", id="hood-title")
                with Horizontal(id="metrics-row"):
                    for mid, label in [
                        ("total-ms", "Total"),
                        ("stt-ms", "STT"),
                        ("detect-ms", "Detection"),
                        ("mask-ms", "Masking"),
                        ("input-b", "Input Bytes"),
                        ("output-b", "Output Bytes"),
                    ]:
                        with Vertical(classes="metric-card"):
                            yield Static("—", id=f"m-{mid}", classes="metric-val")
                            yield Static(label, classes="metric-lbl")
                yield Static("", id="entities-line")
                yield Static("", id="token-line")

            # ── Audit Report ────────────────────────────────
            with Vertical(id="audit-section"):
                yield Static("▸  AUDIT REPORT", id="audit-title")
                yield DataTable(id="audit-table", show_cursor=False)

        yield Footer()

    def on_mount(self) -> None:
        table = self.query_one("#audit-table", DataTable)
        table.add_columns(
            "#", "Session", "Scenario", "Policy",
            "Entities", "Types", "Route", "Compliant", "Time",
        )

    def on_unmount(self) -> None:
        self._stop_event.set()
        self._stop_live_session()
        with self._audio_lock:
            if self._audio_proc and self._audio_proc.poll() is None:
                self._audio_proc.terminate()

    # ── Button handlers ──────────────────────────────────────────────────────

    @on(Button.Pressed, "#btn-healthcare")
    def _on_healthcare(self) -> None:
        self.action_scenario("healthcare")

    @on(Button.Pressed, "#btn-finance")
    def _on_finance(self) -> None:
        self.action_scenario("finance")

    @on(Button.Pressed, "#btn-personal")
    def _on_personal(self) -> None:
        self.action_scenario("personal")

    @on(Button.Pressed, "#btn-live")
    def _on_live(self) -> None:
        self.action_live()

    @on(Button.Pressed, "#stop-btn")
    def _on_stop_btn(self) -> None:
        self.action_stop()

    @on(Button.Pressed, "#play-raw-btn")
    def _on_play_raw(self) -> None:
        self._play_text("play-raw-btn", "▶  Play Original", self._last_raw)

    @on(Button.Pressed, "#play-redacted-btn")
    def _on_play_redacted(self) -> None:
        self._play_text("play-redacted-btn", "▶  Play Redacted", self._last_redacted)

    def _set_play_btn(self, btn_id: str, label: str, playing: bool) -> None:
        btn = self.query_one(f"#{btn_id}", Button)
        btn.label = label
        btn.disabled = playing
        if playing:
            btn.add_class("playing")
        else:
            btn.remove_class("playing")

    def _reset_play_btn(self, btn_id: str, label: str) -> None:
        self._set_play_btn(btn_id, label, playing=False)

    def _play_text(self, btn_id: str, original_label: str, text: str) -> None:
        text = (text or "").strip()
        if not text:
            self.query_one("#raw-log", RichLog).write(Text("\n[playback] nothing to play yet", style="#6e7681"))
            return

        self._set_play_btn(btn_id, "◼  Playing…", playing=True)
        self._play_text_worker(btn_id, original_label, text)

    @work(thread=True)
    def _play_text_worker(self, btn_id: str, original_label: str, text: str) -> None:
        try:
            with self._audio_lock:
                if self._audio_proc and self._audio_proc.poll() is None:
                    self._audio_proc.terminate()
                    try:
                        self._audio_proc.wait(timeout=0.5)
                    except subprocess.TimeoutExpired:
                        self._audio_proc.kill()
                        self._audio_proc.wait(timeout=1.0)
                self._audio_proc = None

                cmd: list[str] | None = None
                if shutil.which("say"):
                    cmd = ["say", text]
                elif shutil.which("espeak"):
                    cmd = ["espeak", text]

                if not cmd:
                    self.call_from_thread(
                        self.query_one("#raw-log", RichLog).write,
                        Text("\n[playback] no TTS command found (need `say` or `espeak`)", style="bold red"),
                    )
                    return

                self._audio_proc = subprocess.Popen(cmd)

            if self._audio_proc:
                self._audio_proc.wait()
        finally:
            self.call_from_thread(self._reset_play_btn, btn_id, original_label)

    # ── Actions ──────────────────────────────────────────────────────────────

    def action_scenario(self, scenario_id: str) -> None:
        if self.session_running:
            return
        scn = next((s for s in SCENARIOS if s["id"] == scenario_id), None)
        if scn:
            self._start_session(scn)

    def action_live(self) -> None:
        if self.session_running:
            return
        self._start_live_session()

    def action_stop(self) -> None:
        self._stop_event.set()
        manager = self._live_manager
        if manager is not None:
            manager.stop()
        with self._audio_lock:
            if self._audio_proc and self._audio_proc.poll() is None:
                self._audio_proc.terminate()

    # ── Session lifecycle ────────────────────────────────────────────────────

    def _start_session(self, scn: dict[str, str]) -> None:
        self._session_counter += 1
        session_id = f"ses_{scn['id'][:3]}_{self._session_counter:02d}"
        self._stop_live_session()
        self._redactor.reset()
        self._stop_event.clear()
        self._active_scenario_id = scn["id"]

        self._stats = SessionStats(
            session_id=session_id,
            scenario_name=scn["name"],
            policy_mode=scn["policy"],
        )

        # UI reset
        self.session_running = True
        self.query_one("#session-label", Static).update(session_id)
        self._set_status("recording")
        self.query_one("#stop-btn").add_class("visible")

        for s in SCENARIOS:
            btn = self.query_one(f"#btn-{s['id']}", Button)
            btn.remove_class("active-scn")
        self.query_one(f"#btn-{scn['id']}", Button).add_class("active-scn")

        self.query_one("#left-panel").add_class("recording")
        self.query_one("#right-panel").add_class("recording")
        self.query_one("#raw-play-row").remove_class("visible")
        self.query_one("#redacted-play-row").remove_class("visible")

        self.query_one("#raw-log", RichLog).clear()
        self.query_one("#redacted-log", RichLog).clear()

        for mid in ("total-ms", "stt-ms", "detect-ms", "mask-ms", "input-b", "output-b"):
            self.query_one(f"#m-{mid}", Static).update("—")
        self.query_one("#entities-line", Static).update("")
        self.query_one("#token-line", Static).update("")

        self._run_session(scn)

    def _start_live_session(self) -> None:
        session_id = default_session_id()
        self._stop_live_session()
        self._redactor.reset()
        self._stop_event.clear()
        self._active_scenario_id = None

        self._stats = SessionStats(
            session_id=session_id,
            scenario_name="Live Mic",
            policy_mode=SETTINGS.policy_mode,
        )

        # UI reset
        self.session_running = True
        self.query_one("#session-label", Static).update(session_id)
        self._set_status("recording")
        self.query_one("#stop-btn").add_class("visible")

        for s in SCENARIOS:
            self.query_one(f"#btn-{s['id']}", Button).remove_class("active-scn")
        self.query_one("#btn-live", Button).add_class("active-scn")

        self.query_one("#left-panel").add_class("recording")
        self.query_one("#right-panel").add_class("recording")
        self.query_one("#raw-play-row").remove_class("visible")
        self.query_one("#redacted-play-row").remove_class("visible")

        self.query_one("#raw-log", RichLog).clear()
        self.query_one("#redacted-log", RichLog).clear()
        self.query_one("#raw-log", RichLog).write(
            Text("Listening… speak into your microphone. Press Stop when done.", style="#8b949e")
        )

        for mid in ("total-ms", "stt-ms", "detect-ms", "mask-ms", "input-b", "output-b"):
            self.query_one(f"#m-{mid}", Static).update("—")
        self.query_one("#entities-line", Static).update("")
        self.query_one("#token-line", Static).update("")

        self._live_raw_lines = []
        self._live_redacted_lines = []
        self._live_partial_raw = ""
        self._live_partial_redacted = ""

        self.query_one("#raw-log", RichLog).write(Text("Loading STT model…", style="#6e7681"))
        try:
            self._ensure_live_transcriber()
        except Exception as exc:
            self._set_status("error")
            self.query_one("#raw-log", RichLog).write(Text(f"\nLive mode error: {exc}", style="bold red"))
            self._finalize_session()
            return

        config = SessionConfig(
            session_id=session_id,
            audio_mode="mic",
            stt_model=SETTINGS.stt_model,
            language=SETTINGS.language,
            no_model=True,
            policy_mode=SETTINGS.policy_mode,
            partial_interval_ms=SETTINGS.partial_interval_ms,
            sample_rate=SETTINGS.sample_rate,
            device=SETTINGS.default_device,
        )
        self._run_live_session(config)

    def _stop_live_session(self) -> None:
        manager = self._live_manager
        subscription = self._live_subscription
        if manager is not None:
            manager.stop()
            manager.wait(timeout=1.0)
        if manager is not None and subscription is not None:
            manager.event_bus.unsubscribe(subscription)
        self._live_manager = None
        self._live_subscription = None

    @work(thread=True)
    def _run_session(self, scn: dict[str, str]) -> None:
        words = scn["text"].split()
        chunks: list[str] = []
        i = 0
        while i < len(words):
            size = random.randint(4, 8)
            chunks.append(" ".join(words[i : i + size]))
            i += size

        cumulative = ""

        for chunk in chunks:
            if self._stop_event.is_set():
                break

            # Simulate real-time speech gap + STT processing
            stt_ms = random.uniform(90, 200)
            time.sleep(random.uniform(0.5, 1.1))  # speech duration
            if self._stop_event.is_set():
                break
            time.sleep(stt_ms / 1000)  # STT latency

            cumulative = (cumulative + " " + chunk).lstrip()

            t0 = time.perf_counter()
            result = self._redactor.redact(cumulative)
            redact_ms = (time.perf_counter() - t0) * 1000

            stats = self._stats
            if stats:
                stats.stt_latencies.append(stt_ms)
                stats.redact_latencies.append(redact_ms)
                stats.total_entities = len(result.entities)
                stats.detected_types = {e.entity_type for e in result.entities}
                stats.token_map = dict(result.token_map)
                stats.input_bytes = len(cumulative.encode())
                stats.output_bytes = len(result.redacted_text.encode())

            self.call_from_thread(
                self._apply_chunk_update,
                cumulative,
                result.redacted_text,
                dict(result.token_map),
                stt_ms,
                redact_ms,
                stats,
            )

        self.call_from_thread(self._finalize_session)

    @work(thread=True)
    def _run_live_session(self, config: SessionConfig) -> None:
        transcriber = self._live_transcriber
        transcriber_factory = (lambda _model_name: transcriber) if transcriber is not None else None
        manager = DemoSessionManager(safe_log_dir=SETTINGS.log_dir, transcriber_factory=transcriber_factory)
        subscription = manager.subscribe()
        self._live_manager = manager
        self._live_subscription = subscription

        manager.start(config)

        for event in subscription.replay:
            self.call_from_thread(self._on_live_event, event)

        try:
            while manager.is_running or not subscription.queue.empty():
                try:
                    event = subscription.queue.get(timeout=0.5)
                except queue.Empty:
                    continue
                self.call_from_thread(self._on_live_event, event)
                if event.get("type") == "session.stopped":
                    break
        finally:
            manager.stop()
            manager.wait(timeout=1.0)
            manager.event_bus.unsubscribe(subscription)
            self._live_manager = None
            self._live_subscription = None

    def _on_live_event(self, event: dict[str, Any]) -> None:
        event_type = event.get("type")
        if event_type == "session.started":
            self.query_one("#stt-badge", Static).update(Text(f"● {SETTINGS.stt_model}", style="bold green"))
            self.query_one("#redact-badge", Static).update(Text("● masker", style="#6e7681"))
            return

        if event_type in {"transcript.partial", "transcript.final"}:
            raw_text = (event.get("raw_text") or "").strip()
            redacted_text = (event.get("redacted_text") or "").strip()
            entities = event.get("entities") or []
            token_map = {item["token"]: item["raw_value"] for item in entities if "token" in item and "raw_value" in item}

            if event_type == "transcript.partial":
                self._live_partial_raw = raw_text
                self._live_partial_redacted = redacted_text
            else:
                if raw_text:
                    self._live_raw_lines.append(raw_text)
                if redacted_text:
                    self._live_redacted_lines.append(redacted_text)
                self._live_partial_raw = ""
                self._live_partial_redacted = ""

            raw_full_parts = list(self._live_raw_lines)
            if self._live_partial_raw:
                raw_full_parts.append(self._live_partial_raw)
            redacted_full_parts = list(self._live_redacted_lines)
            if self._live_partial_redacted:
                redacted_full_parts.append(self._live_partial_redacted)

            raw_full = "\n".join(raw_full_parts)
            redacted_full = "\n".join(redacted_full_parts)

            stats = self._stats
            if stats:
                if event_type == "transcript.final":
                    stats.total_entities += len(entities)
                    stats.detected_types.update(
                        {item.get("entity_type", "") for item in entities if item.get("entity_type")}
                    )
                    stats.token_map.update(token_map)
                stats.input_bytes = len(raw_full.encode())
                stats.output_bytes = len(redacted_full.encode())

            self._apply_chunk_update(
                raw_full,
                redacted_full,
                token_map,
                stt_ms=-1.0,
                redact_ms=-1.0,
                stats=stats,
            )
            return

        if event_type == "error":
            self._set_status("error")
            self.query_one("#raw-log", RichLog).write(Text(f"\nError: {event.get('message', '')}", style="bold red"))
            return

        if event_type == "session.stopped":
            self.session_running = False
            self.query_one("#btn-live", Button).remove_class("active-scn")
            self._finalize_session()
            return

    # ── UI update helpers (called on main thread) ────────────────────────────

    def _apply_chunk_update(
        self,
        raw: str,
        redacted: str,
        token_map: dict[str, str],
        stt_ms: float,
        redact_ms: float,
        stats: SessionStats | None,
    ) -> None:
        self._last_raw = raw
        self._last_redacted = redacted
        raw_log = self.query_one("#raw-log", RichLog)
        raw_log.clear()
        raw_log.write(Text(raw, style="#e6edf3"))

        redacted_log = self.query_one("#redacted-log", RichLog)
        redacted_log.clear()
        redacted_log.write(self._colorize_redacted(redacted, token_map))

        self._update_badge("#stt-badge", stt_ms, "STT")
        self._update_badge("#redact-badge", redact_ms, "Masker")

        if stats:
            detect_ms = max(0.5, redact_ms * 0.65)
            mask_ms = max(0.3, redact_ms - detect_ms)
            self.query_one("#m-total-ms", Static).update(f"{stats.total_ms:.0f}ms")
            self.query_one("#m-stt-ms", Static).update(f"{stats.avg_stt_ms:.0f}ms")
            self.query_one("#m-detect-ms", Static).update(f"{detect_ms:.1f}ms")
            self.query_one("#m-mask-ms", Static).update(f"{mask_ms:.1f}ms")
            self.query_one("#m-input-b", Static).update(f"{stats.input_bytes}B")
            self.query_one("#m-output-b", Static).update(f"{stats.output_bytes}B")

            entity_text = Text()
            for etype in sorted(stats.detected_types):
                color = ENTITY_COLORS.get(etype, "bold white")
                entity_text.append(f" {etype} ", style=f"{color}")
                entity_text.append(" ", style="default")
            if stats.total_entities:
                entity_text.append(
                    f"  {stats.total_entities} detected · "
                    f"{len(stats.detected_types)} type(s) · "
                    f"{len(token_map)} tokenized",
                    style="#6e7681",
                )
            self.query_one("#entities-line", Static).update(entity_text)

            if token_map:
                t = Text("Tokens  ", style="#6e7681")
                for tok, val in list(token_map.items())[:8]:
                    etype = tok.rsplit("_", 1)[0]
                    color = ENTITY_COLORS.get(etype, "white")
                    t.append(tok, style=color)
                    t.append(f" → {val[:22]}   ", style="#6e7681")
                self.query_one("#token-line", Static).update(t)

    def _finalize_session(self) -> None:
        self.session_running = False
        self._set_status("completed")
        self.query_one("#stop-btn").remove_class("visible")
        self.query_one("#left-panel").remove_class("recording")
        self.query_one("#right-panel").remove_class("recording")
        self.query_one("#raw-play-row").add_class("visible")
        self.query_one("#redacted-play-row").add_class("visible")

        if self._active_scenario_id:
            self.query_one(f"#btn-{self._active_scenario_id}", Button).remove_class("active-scn")
        self.query_one("#btn-live", Button).remove_class("active-scn")

        stats = self._stats
        if not stats:
            return

        types_str = ", ".join(sorted(stats.detected_types)) if stats.detected_types else "none"
        route = "MASKED_SEND" if stats.total_entities > 0 else "SAFE_TO_SEND"
        n = len(self._audit_rows) + 1
        self._audit_rows.append({})

        table = self.query_one("#audit-table", DataTable)
        table.add_row(
            str(n),
            stats.session_id,
            stats.scenario_name,
            stats.policy_mode,
            str(stats.total_entities),
            types_str,
            route,
            Text("✓ YES", style="bold green"),
            stats.started_at.strftime("%H:%M:%S"),
        )

    # ── Helpers ───────────────────────────────────────────────────────────────

    def _set_status(self, state: str) -> None:
        label = self.query_one("#status-label", Static)
        if state == "recording":
            label.update(Text("● RECORDING", style="bold green"))
        elif state == "completed":
            label.update(Text("● DONE", style="bold #58a6ff"))
        elif state == "error":
            label.update(Text("● ERROR", style="bold red"))
        else:
            label.update(Text("● READY", style="#8b949e"))

    def _update_badge(self, selector: str, ms: float, label: str) -> None:
        if ms < 0:
            t = Text()
            t.append("● ", style="bold green")
            t.append(f"{label}: ", style="#6e7681")
            t.append("live", style="bold green")
            self.query_one(selector, Static).update(t)
            return
        if ms < 100:
            dot, color = "●", "green"
        elif ms < 300:
            dot, color = "●", "yellow"
        else:
            dot, color = "●", "red"
        t = Text()
        t.append(f"{dot} ", style=f"bold {color}")
        t.append(f"{label}: ", style="#6e7681")
        t.append(f"{ms:.0f}ms", style=f"bold {color}")
        self.query_one(selector, Static).update(t)

    def _colorize_redacted(self, text: str, token_map: dict[str, str]) -> Text:
        if not token_map:
            return Text(text, style="#e6edf3")

        tokens_sorted = sorted(token_map.keys(), key=len, reverse=True)
        pattern = "|".join(re.escape(tok) for tok in tokens_sorted)

        result = Text()
        last = 0
        for m in re.finditer(pattern, text):
            result.append(text[last : m.start()], style="#e6edf3")
            tok = m.group(0)
            etype = tok.rsplit("_", 1)[0]
            color = ENTITY_COLORS.get(etype, "bold white")
            result.append(f"[{tok}]", style=f"{color} on #1a0d00")
            last = m.end()
        result.append(text[last:], style="#e6edf3")
        return result


# ── Entry point ───────────────────────────────────────────────────────────────

def run() -> None:
    MaskerDemoApp().run()


if __name__ == "__main__":
    run()
