from __future__ import annotations

import json
from pathlib import Path

from .models import SafeLogEntry, to_dict


class SafeLogger:
    def __init__(self, log_dir: str | Path) -> None:
        self.log_dir = Path(log_dir)
        self.log_dir.mkdir(parents=True, exist_ok=True)

    def write(self, entry: SafeLogEntry) -> Path:
        log_path = self.log_dir / f"{entry.session_id}.safe.jsonl"
        with log_path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(to_dict(entry), ensure_ascii=True))
            handle.write("\n")
        return log_path
