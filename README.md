# Macro Recorder

High-accuracy Windows macro recorder built with Tauri v2, Rust, React, TypeScript, and Bun.

## Tests

Run the Rust engine checks with:

```powershell
cargo test --manifest-path .\src-tauri\Cargo.toml
```

The timing tests use `NtSetTimerResolution`, high-priority spin waits, and real OS scheduling behavior. Run them on a real Windows machine; VM timing jitter can invalidate the measured thresholds.
