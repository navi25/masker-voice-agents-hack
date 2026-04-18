"""Gemma backend wrappers. CURSOR OWNS THIS FILE.

Three backends, all behind the same `GemmaBackend` protocol:

  - LocalCactusBackend   → spawns `cactus run <model>` and pipes prompts in
  - GeminiCloudBackend   → calls Gemini API via google-genai (cloud fallback)
  - StubBackend          → deterministic echo, used in CI / when no model present

`auto_attach()` monkey-patches `google.genai.Client.models.generate_content`
so any team using the Gemini SDK gets Masker filtering for free.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from dataclasses import dataclass
from typing import Callable, Protocol


class GemmaBackend(Protocol):
    name: str

    def generate(self, prompt: str, *, max_tokens: int = 256) -> str: ...


@dataclass
class StubBackend:
    """Deterministic backend used when no model / no API key is available.
    Ensures the integration loop is testable in CI and on plane wifi.
    """

    name: str = "stub"

    def generate(self, prompt: str, *, max_tokens: int = 256) -> str:
        head = prompt.strip().splitlines()[-1] if prompt.strip() else ""
        return f"[stub-gemma reply to] {head[:200]}"


@dataclass
class LocalCactusBackend:
    """Calls the on-device Gemma 4 model via the `cactus` CLI.

    Note: `cactus run` opens an interactive REPL. For non-interactive single-
    shot generation we pipe a prompt on stdin and grab stdout until EOF.
    The hackathon scope only needs short single-turn responses; we keep this
    simple and shell out per turn. A persistent FFI binding is a Phase-2 win.
    """

    model: str = "google/functiongemma-270m-it"
    cactus_bin: str = "cactus"
    timeout_s: float = 30.0
    name: str = "cactus-local"

    def generate(self, prompt: str, *, max_tokens: int = 256) -> str:
        if not shutil.which(self.cactus_bin):
            raise RuntimeError(
                f"`{self.cactus_bin}` not on PATH. Activate the cactus venv: "
                f"`source cactus/venv/bin/activate`."
            )
        try:
            proc = subprocess.run(
                [self.cactus_bin, "run", self.model],
                input=prompt + "\n/quit\n",
                text=True,
                capture_output=True,
                timeout=self.timeout_s,
            )
        except subprocess.TimeoutExpired:
            return "[masker] Local model timed out."
        if proc.returncode != 0:
            return f"[masker] cactus run failed: {proc.stderr[:200]}"
        return _strip_repl_chrome(proc.stdout)


@dataclass
class GeminiCloudBackend:
    """Cloud fallback. Used when policy says `safe-to-send` and the local
    model's confidence is low, or when the caller explicitly wants Gemini.
    """

    model: str = "gemini-2.0-flash"
    api_key_env: str = "GEMINI_API_KEY"
    name: str = "gemini-cloud"

    def generate(self, prompt: str, *, max_tokens: int = 256) -> str:
        api_key = os.environ.get(self.api_key_env)
        if not api_key:
            raise RuntimeError(
                f"{self.api_key_env} not set. Export your Gemini API key or use a different backend."
            )
        try:
            from google import genai  # type: ignore[import-not-found]
        except ImportError as exc:  # pragma: no cover
            raise RuntimeError("`pip install google-genai` to use the cloud backend.") from exc

        client = genai.Client(api_key=api_key)
        response = client.models.generate_content(model=self.model, contents=prompt)
        return getattr(response, "text", str(response))


def _strip_repl_chrome(stdout: str) -> str:
    """Best-effort cleanup of the cactus chat REPL banner / prompt markers."""
    lines = [ln for ln in stdout.splitlines() if ln.strip()]
    drop_prefixes = ("cactus", ">", "loading", "Loading", "[", "═", "─")
    cleaned = [ln for ln in lines if not ln.strip().startswith(drop_prefixes)]
    return "\n".join(cleaned).strip() or stdout.strip()


def default_backend() -> GemmaBackend:
    """Pick the best available backend based on environment.

    Priority: cactus CLI on PATH > GEMINI_API_KEY set > stub.
    """
    if shutil.which("cactus"):
        return LocalCactusBackend()
    if os.environ.get("GEMINI_API_KEY"):
        return GeminiCloudBackend()
    return StubBackend()


def auto_attach(
    *,
    backend: GemmaBackend | None = None,
    on_filter: Callable[[str, str], None] | None = None,
) -> None:
    """Monkey-patch the google-genai client so any caller transparently
    routes through Masker's input/output filters.

    Usage in another team's code:

        from masker import auto_attach
        auto_attach()
        # ...their existing google-genai calls now get filtered for free.
    """
    from . import filter_input, filter_output  # local import to avoid cycle

    try:
        from google.genai import models as _gm  # type: ignore[import-not-found]
    except ImportError:  # pragma: no cover
        raise RuntimeError("google-genai not installed; cannot auto_attach().")

    original = _gm.Models.generate_content

    def patched(self, *, model, contents, **kwargs):  # type: ignore[no-untyped-def]
        prompt = contents if isinstance(contents, str) else str(contents)
        safe_prompt, _md = filter_input(prompt)
        if on_filter:
            on_filter(prompt, safe_prompt)
        result = original(self, model=model, contents=safe_prompt, **kwargs)
        result_text = getattr(result, "text", None)
        if result_text:
            safe_text = filter_output(result_text)
            try:
                result.text = safe_text  # type: ignore[attr-defined]
            except Exception:
                pass
        return result

    _gm.Models.generate_content = patched  # type: ignore[assignment]
