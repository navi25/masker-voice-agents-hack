from __future__ import annotations


class StubGemmaClient:
    name = "gemma_stub"

    def generate(self, masked_prompt: str) -> dict[str, str]:
        return {
            "model": self.name,
            "text": f"[stubbed-local-gemma] Received masked prompt: {masked_prompt}",
        }
