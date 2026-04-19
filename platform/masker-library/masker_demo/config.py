"""
Demo configuration — single source of truth for all runtime defaults.

Values here are the baseline. Most can be overridden via environment variables
(see .env.example) or programmatically when constructing SessionConfig.
"""

# ── Server ─────────────────────────────────────────────────────────────────────

DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 8008

# ── STT ────────────────────────────────────────────────────────────────────────

# Model used when none is specified.  Options: tiny.en | base.en | small.en
DEFAULT_STT_MODEL = "small.en"

# Models included in the benchmark suite (ascending quality / size order)
BENCHMARK_STT_MODELS = ["tiny.en", "base.en", "small.en"]

DEFAULT_LANGUAGE = "en"
DEFAULT_SAMPLE_RATE = 16000       # Hz
DEFAULT_PARTIAL_INTERVAL_MS = 900 # ms between partial-result flushes
DEFAULT_MIN_PARTIAL_MS = 700      # minimum silence before a partial is emitted

# ── Policy ─────────────────────────────────────────────────────────────────────

# Default compliance policy applied to every new session.
# Choices: hipaa_safe_mode | gdpr_safe_mode
DEFAULT_POLICY = "hipaa_safe_mode"

# All policies the demo understands
AVAILABLE_POLICIES = ["hipaa_safe_mode", "gdpr_safe_mode"]

# ── Logging ────────────────────────────────────────────────────────────────────

DEFAULT_LOG_DIR = ".masker_safe_logs"

# ── Demo scenarios ─────────────────────────────────────────────────────────────

SCENARIOS: list[dict] = [
    {
        "id": "healthcare",
        "name": "Healthcare",
        "key": "h",
        "icon": "⚕",
        "desc": "Patient SSN · DOB · health context",
        "text": (
            "Hi, I'm Ravi Kumar, date of birth March 3rd 1989. "
            "My social security is one two three four five six seven eight nine. "
            "I had a heart condition last year and I need a refill for my medication."
        ),
        "policy": "hipaa_safe_mode",
    },
    {
        "id": "finance",
        "name": "Finance",
        "key": "f",
        "icon": "₹",
        "desc": "Credit card · phone · name",
        "text": (
            "Hello, this is Maya Shah. "
            "My credit card number is 4242 4242 4242 4242 "
            "and my phone number is 415-555-0199. "
            "Please help me dispute a duplicate payment."
        ),
        "policy": "gdpr_safe_mode",
    },
    {
        "id": "personal",
        "name": "Personal",
        "key": "p",
        "icon": "◎",
        "desc": "Phone · address · email",
        "text": (
            "Hi, I'm Jordan Lee. "
            "You can call me at 650-555-0112 "
            "and send the package to 42 Market Street, San Francisco, California. "
            "My email is jordan.lee@example.com."
        ),
        "policy": "hipaa_safe_mode",
    },
]

# ── Entity colours (Rich markup) ───────────────────────────────────────────────

ENTITY_COLORS: dict[str, str] = {
    "PERSON":       "bold yellow",
    "SSN":          "bold red",
    "PHONE":        "bold cyan",
    "EMAIL":        "bold bright_blue",
    "ADDRESS":      "bold magenta",
    "CARD":         "bold bright_red",
    "DOB":          "bold green",
    "GOV_ID":       "bold bright_yellow",
    "MRN":          "bold red3",
    "INSURANCE_ID": "bold orange1",
}
