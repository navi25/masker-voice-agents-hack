"""Subprocess wrapper for the `masker` CLI binary."""
from __future__ import annotations

import json
import os
import shutil
import subprocess
from pathlib import Path


def _find_binary() -> str:
    # 1. Explicit override via env var
    if env := os.environ.get("MASKER_BIN"):
        return env

    # 2. On PATH
    if found := shutil.which("masker"):
        return found

    # 3. Relative to this file (repo layout: platform/masker-library/masker/_cli.py)
    here = Path(__file__).resolve().parent
    candidates = [
        here / "../../../platform/masker-core/target/release/masker",
        here / "../../masker-core/target/release/masker",
        here / "../../masker-core/target/debug/masker",
    ]
    for path in candidates:
        if path.is_file():
            return str(path.resolve())

    raise RuntimeError(
        "masker binary not found.\n"
        "Build it with:\n"
        "  cd platform/masker-core && cargo build --release\n"
        "Then either add target/release/ to your PATH, or set the MASKER_BIN "
        "environment variable to the full path of the masker binary."
    )


def run_cli(*args: str) -> dict:
    binary = _find_binary()
    proc = subprocess.run(
        [binary, *args],
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise RuntimeError(
            f"masker CLI exited with code {proc.returncode}:\n{proc.stderr.strip()}"
        )
    return json.loads(proc.stdout)
