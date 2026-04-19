from __future__ import annotations

import asyncio
import json
import queue
from typing import Any

from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.responses import JSONResponse, StreamingResponse
from pydantic import BaseModel

from .models import SessionConfig
from .session import DemoSessionManager, default_session_id
from .settings import SETTINGS


class StartSessionRequest(BaseModel):
    session_id: str | None = None
    audio_mode: str = "mic"
    audio_path: str | None = None
    stt_model: str | None = None
    language: str | None = None
    no_model: bool = False
    policy_mode: str | None = None
    partial_interval_ms: int | None = None
    sample_rate: int | None = None
    device: str | int | None = None
    simulate_realtime: bool = True


app = FastAPI(title="Masker Demo Backend", version="0.1.0")
manager = DemoSessionManager(safe_log_dir=SETTINGS.log_dir)


@app.get("/health")
def health() -> dict[str, Any]:
    return {
        "ok": True,
        "running": manager.is_running,
        "session_id": manager.active_session_id,
    }


@app.post("/api/session/start")
def start_session(request: StartSessionRequest):
    config = SessionConfig(
        session_id=request.session_id or default_session_id(),
        audio_mode=request.audio_mode,
        audio_path=request.audio_path,
        stt_model=request.stt_model or SETTINGS.stt_model,
        language=request.language if request.language is not None else SETTINGS.language,
        no_model=request.no_model,
        policy_mode=request.policy_mode or SETTINGS.policy_mode,
        partial_interval_ms=request.partial_interval_ms or SETTINGS.partial_interval_ms,
        sample_rate=request.sample_rate or SETTINGS.sample_rate,
        device=request.device if request.device is not None else SETTINGS.default_device,
        simulate_realtime=request.simulate_realtime,
    )
    try:
        result = manager.start(config)
    except RuntimeError as exc:
        return JSONResponse({"ok": False, "error": str(exc)}, status_code=409)
    return {"ok": True, **result, "config": config.__dict__}


@app.post("/api/session/stop")
def stop_session():
    return {"ok": True, **manager.stop()}


@app.post("/api/session/reset")
def reset_session():
    return {"ok": True, **manager.reset()}


@app.websocket("/ws/demo")
async def demo_ws(websocket: WebSocket) -> None:
    await websocket.accept()
    subscription = manager.subscribe()
    try:
        for event in subscription.replay:
            await websocket.send_json(event)
        while True:
            try:
                event = await asyncio.to_thread(subscription.queue.get, True, 1.0)
            except queue.Empty:
                await asyncio.sleep(0.1)
                continue
            await websocket.send_json(event)
    except WebSocketDisconnect:
        return
    except Exception:
        return
    finally:
        manager.event_bus.unsubscribe(subscription)


@app.get("/api/events/stream")
async def events_stream():
    subscription = manager.subscribe()

    async def generator():
        try:
            for event in subscription.replay:
                yield f"data: {json.dumps(event)}\n\n"
            while True:
                try:
                    event = await asyncio.to_thread(subscription.queue.get, True, 1.0)
                except queue.Empty:
                    yield ": keep-alive\n\n"
                    await asyncio.sleep(0.1)
                    continue
                yield f"data: {json.dumps(event)}\n\n"
        finally:
            manager.event_bus.unsubscribe(subscription)

    return StreamingResponse(generator(), media_type="text/event-stream")
