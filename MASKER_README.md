<img src="assets/banner.png" alt="Masker banner" style="border-radius: 30px; width: 60%;">

# Masker Voice Agents Hack

Masker Voice Agents Hack explores a simple hypothesis: voice AI becomes more trustworthy when sensitive context can be detected, classified, and handled before it leaves the device.

The goal of this project is to prototype an on-device privacy layer for voice interactions on mobile devices and wearables, using local intelligence to decide what should stay local, what should be masked, and what is safe to route onward.

## Project Thesis

If Masker can detect sensitive speech locally before it is forwarded upstream, voice AI becomes safer, faster, and easier to trust.

That creates a compelling extension to the main product:

- Mask PII before it reaches the LLM.
- Decide locally whether a request should stay on-device, be masked, or be safely routed.
- Build a voice-native privacy experience that feels real-time instead of bolted on.

## Scope

This repo is scoped as a prototype, not the full Masker platform.

### In Scope

- A voice-first demo built with Gemma 4 on Cactus.
- On-device speech understanding for short live requests.
- Detection of sensitive content in spoken input.
- Local privacy policy decisions such as `local-only`, `masked-send`, and `safe-to-send`.
- A clear demonstration of how on-device privacy logic complements the main Masker proxy product.

### Out of Scope

- Production-grade compliance guarantees.
- Broad portal, dashboard, or customer management features.
- Large integration surface area.
- General-purpose assistant behavior.

## Why Cactus + Gemma 4

This prototype is built around Gemma 4 on Cactus because the stack supports the exact product question we want to test:

- Can we do meaningful privacy-aware reasoning on-device?
- Can a voice interaction still feel low-latency?
- Can local classification improve trust before the request ever touches the network?

## 💥 One-liner

> **Masker is a real-time privacy layer for local voice agents.**

## Development: masker-core + CLI + Python SDK

### Test `masker-core` (Rust)

```bash
# from repo root
cargo test --manifest-path platform/masker-core/Cargo.toml

# include feature-gated backends (requires optional deps / local setup)
cargo test --manifest-path platform/masker-core/Cargo.toml --all-features
```

### Install `masker-cli` (installs the `masker` binary)

```bash
# installs into ~/.cargo/bin (make sure that's on your PATH)
cargo install --path platform/masker-core/crates/masker-cli

# verify
masker --help
```

### Package the Python SDK (wheel includes the `masker` binary)

The Python package lives at `masker-sdk/masker-python-sdk/` and is built with
`maturin`. Its `pyproject.toml` is configured to compile the Rust CLI at
`platform/masker-core/crates/masker-cli` and bundle it into the wheel.

```bash
python -m pip install --upgrade pip
python -m pip install maturin

# build a local wheel (uses tool.maturin settings in pyproject.toml)
cd masker-sdk/masker-python-sdk
maturin build --release
```

### Publish the Python SDK to PyPI

```bash
python -m pip install maturin

# requires a PyPI token with publish permissions
export MATURIN_PYPI_TOKEN="<pypi-token>"

# publishes the package defined in masker-sdk/masker-python-sdk/pyproject.toml
cd masker-sdk/masker-python-sdk

# if your Rust workspace references local path dependencies (e.g. `cactus-sys`),
# publishing the sdist can fail in a temp directory. This skips the sdist and
# uploads wheels only.
maturin publish --no-sdist --non-interactive
```

### Publish the Rust crates to crates.io

This workspace has two crates:

- `masker` (library)
- `masker-cli` (binary crate that produces the `masker` executable)

```bash
# requires a crates.io token
export CARGO_REGISTRY_TOKEN="<crates-io-token>"

# Note: publishing the `cactus` feature requires `cactus-sys` to be available
# as a versioned dependency (or patched locally).
# publish the library first, then the CLI
cargo publish --manifest-path platform/masker-core/crates/masker/Cargo.toml
cargo publish --manifest-path platform/masker-core/crates/masker-cli/Cargo.toml
```

Notes:

- Crate names on crates.io must be globally unique; if `masker` is already
  taken, rename `package.name` before publishing.
- You must bump versions before re-publishing (workspace version is in
  `platform/masker-core/Cargo.toml`).
- crates.io does not allow `path` dependencies in published crates. If you
  want to keep the optional `cactus` backend, `cactus-sys` must be available
  as a versioned dependency (or you can use a local `[patch.crates-io]` override
  during development).

### Detokenize (recover `tok_...` values)

`masker live` and `masker stream` persist an encrypted token vault snapshot to
`~/.masker/state.json`. To recover a token later:

```bash
export MASKER_KEK="<base64-32-byte-key>"
masker detokenize --token tok_... --use-case healthcare
```
