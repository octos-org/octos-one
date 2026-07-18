# Provisioning an LLM (bring your own key)

The app needs an LLM to generate cards, but **keys are never in the repo, typed,
or sent over the network.** A user provisions their own provider + key, which is
written into the on-device octos profile config and read by the embedded kernel.

## Where it ends up

`octos-home/.octos/profiles/_main.json`:
```jsonc
{ "config": {
    "llm":      { "primary": { "family_id": "zai", "model_id": "glm-5.2" }, "fallbacks": [] },
    "env_vars": { "ZAI_API_KEY": "sk-…" }          // key env is provider-specific
} }
```
`family_id` selects the provider from octos's registry (`zai`, `deepseek`,
`openai`, `anthropic`, …); each maps to a `<PROVIDER>_API_KEY` env var.

## The QR flow

1. **Encode** the config into a QR on a trusted machine:
   ```bash
   cargo run --manifest-path tools/llm-qr/Cargo.toml -- --family zai --model glm-5.2 --key sk-XXXX
   # prints the JSON payload + a Unicode QR you scan straight off the terminal.
   ```
   The QR payload is a single self-contained JSON object (no URL) — **all the
   info is in the QR**:
   ```json
   {"llm_family":"zai","llm_model":"glm-5.2","llm_key":"sk-XXXX"}
   ```
2. **Scan** it from the app's composer (the QR button) → the app parses the JSON,
   writes `llm_family`/`llm_model` into `config.llm` and `llm_key` into
   `config.env_vars.<PROVIDER>_API_KEY`, and the next turn uses it.

> The QR carries a secret — treat it like a password (don't paste it into chats,
> don't commit the PNG). The key stays on the device once scanned.

## Provisioning without the camera (dev / headless)

The same JSON payload can be applied via the launch intent (no scanning), which is
how the flow is tested:
```bash
adb shell am start -S -n dev.makepad.octos_app/.MakepadApp \
    --es makepad.PROVISION_CONFIG '{"llm_family":"zai","llm_model":"glm-5.2","llm_key":"sk-XXXX"}'
```
Server auth (`base_url|profile|token`) still has its own `makepad.APP_CONFIG`
entry point and is never accepted from an LLM QR — see
[BUILDING-ANDROID.md](BUILDING-ANDROID.md).
