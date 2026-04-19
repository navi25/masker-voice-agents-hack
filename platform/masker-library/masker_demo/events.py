from __future__ import annotations

import queue
import threading
import time
from collections import deque
from dataclasses import dataclass
from typing import Any


def now_ms() -> int:
    return int(time.time() * 1000)


def build_event(event_type: str, session_id: str, **payload: Any) -> dict[str, Any]:
    return {
        "type": event_type,
        "session_id": session_id,
        "timestamp_ms": now_ms(),
        **payload,
    }


@dataclass
class EventSubscription:
    queue: "queue.Queue[dict[str, Any]]"
    replay: list[dict[str, Any]]


class EventBus:
    def __init__(self, max_recent: int = 256) -> None:
        self._lock = threading.Lock()
        self._recent: deque[dict[str, Any]] = deque(maxlen=max_recent)
        self._subscribers: list["queue.Queue[dict[str, Any]]"] = []

    def subscribe(self) -> EventSubscription:
        subscriber: "queue.Queue[dict[str, Any]]" = queue.Queue(maxsize=256)
        with self._lock:
            self._subscribers.append(subscriber)
            replay = list(self._recent)
        return EventSubscription(queue=subscriber, replay=replay)

    def unsubscribe(self, subscription: EventSubscription) -> None:
        with self._lock:
            if subscription.queue in self._subscribers:
                self._subscribers.remove(subscription.queue)

    def emit(self, event: dict[str, Any]) -> None:
        with self._lock:
            self._recent.append(event)
            subscribers = list(self._subscribers)

        for subscriber in subscribers:
            try:
                subscriber.put_nowait(event)
            except queue.Full:
                try:
                    subscriber.get_nowait()
                except queue.Empty:
                    pass
                try:
                    subscriber.put_nowait(event)
                except queue.Full:
                    continue
