"""Smoke + integration tests for the masker package.

Run: `python -m unittest discover -s tests -v`
"""

from __future__ import annotations

import unittest

from masker import (
    StubBackend,
    Tracer,
    VoiceLoop,
    filter_input,
    filter_output,
)
from masker.detection import detect
from masker.masking import mask, scrub_output
from masker.policy import decide
from masker.router import Router


class DetectionTests(unittest.TestCase):
    def test_ssn_is_detected_as_high_risk(self):
        det = detect("My SSN is 123-45-6789.")
        self.assertEqual(det.risk_level, "high")
        types = {e.type.value for e in det.entities}
        self.assertIn("ssn", types)

    def test_safe_query_has_no_entities(self):
        det = detect("What's the weather tomorrow?")
        self.assertEqual(det.risk_level, "none")
        self.assertEqual(det.entities, [])

    def test_email_and_phone_are_detected(self):
        det = detect("Email me at jane@example.com or 415-555-0123.")
        types = {e.type.value for e in det.entities}
        self.assertIn("email", types)
        self.assertIn("phone", types)


class PolicyTests(unittest.TestCase):
    def test_safe_query_is_safe_to_send(self):
        det = detect("What time is it in Tokyo?")
        self.assertEqual(decide(det).route, "safe-to-send")

    def test_ssn_is_local_only(self):
        det = detect("SSN 111-22-3333.")
        self.assertEqual(decide(det).route, "local-only")

    def test_email_is_masked_send(self):
        det = detect("Ping priya@redwood.com.")
        self.assertEqual(decide(det).route, "masked-send")


class MaskingTests(unittest.TestCase):
    def test_mask_replaces_spans_with_placeholders(self):
        text = "Email me at jane@example.com."
        det = detect(text)
        m = mask(text, det)
        self.assertNotIn("jane@example.com", m.text)
        self.assertIn("[MASKED:email]", m.text)

    def test_scrub_output_re_masks_leaked_values(self):
        text = "Call 415-555-0123."
        det = detect(text)
        leaked = "Sure, calling 415-555-0123 now."
        scrubbed = scrub_output(leaked, det)
        self.assertNotIn("415-555-0123", scrubbed)


class PublicApiTests(unittest.TestCase):
    def test_filter_input_returns_safe_text_and_metadata(self):
        safe, meta = filter_input("My SSN is 123-45-6789.")
        self.assertNotIn("123-45-6789", safe)
        self.assertEqual(meta["route"], "local-only")
        self.assertEqual(meta["risk_level"], "high")

    def test_filter_output_scrubs_leaked_email(self):
        out = filter_output("Reach me at jane@example.com")
        self.assertNotIn("jane@example.com", out)


class VoiceLoopTests(unittest.TestCase):
    def test_end_to_end_with_stub_backend(self):
        loop = VoiceLoop(router=Router(local_backend=StubBackend()))
        result = loop.run_text_turn("What's the weather tomorrow?", tracer=Tracer())
        self.assertEqual(result.policy.route, "safe-to-send")
        self.assertGreater(len(result.trace), 0)
        stages = {ev.stage for ev in result.trace}
        self.assertIn("detection", stages)
        self.assertIn("policy", stages)
        self.assertIn("llm", stages)

    def test_high_risk_turn_stays_local(self):
        loop = VoiceLoop(router=Router(local_backend=StubBackend()))
        result = loop.run_text_turn(
            "I have chest pain and my MRN is 99812 and SSN 123-45-6789.",
            tracer=Tracer(),
        )
        self.assertEqual(result.policy.route, "local-only")


if __name__ == "__main__":
    unittest.main()
