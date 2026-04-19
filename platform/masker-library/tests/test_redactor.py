from __future__ import annotations

import unittest

from masker_demo.redactor import SessionRedactor


class RedactorTests(unittest.TestCase):
    def test_stable_placeholders_across_session(self) -> None:
        redactor = SessionRedactor()

        first = redactor.redact("Hi, I'm Ravi Kumar and my SSN is 123-45-6789.")
        second = redactor.redact("Ravi Kumar called back. The SSN is still 123-45-6789.")

        self.assertIn("PERSON_1", first.redacted_text)
        self.assertIn("SSN_1", first.redacted_text)
        self.assertIn("PERSON_1", second.redacted_text)
        self.assertIn("SSN_1", second.redacted_text)
        self.assertNotIn("PERSON_2", second.redacted_text)
        self.assertNotIn("SSN_2", second.redacted_text)

    def test_preserves_sentence_structure(self) -> None:
        redactor = SessionRedactor()
        text = (
            "Hi, I'm Ravi Kumar, date of birth March 3rd 1989. "
            "My social security is 123-45-6789. "
            "I had a heart condition last year and I need a refill for my medication."
        )

        result = redactor.redact(text)

        self.assertEqual(
            result.redacted_text,
            "Hi, I'm PERSON_1, date of birth DOB_1. My social security is SSN_1. "
            "I had a heart condition last year and I need a refill for my medication.",
        )
        self.assertIn("heart condition last year", result.redacted_text)
        self.assertNotIn("Ravi Kumar", result.redacted_text)
        self.assertNotIn("123-45-6789", result.redacted_text)

    def test_phone_address_and_email_detection(self) -> None:
        redactor = SessionRedactor()
        text = "You can reach me at 415-555-0109, email ravi@example.com, I live at 42 Market Street."

        result = redactor.redact(text)

        self.assertIn("PHONE_1", result.redacted_text)
        self.assertIn("EMAIL_1", result.redacted_text)
        self.assertIn("ADDRESS_1", result.redacted_text)


if __name__ == "__main__":
    unittest.main()
