# Dependency Audit

Audited: 2026-06-24
Rust: 1.96.0 (ac68faa20 2026-05-25)
Target: x86_64-pc-windows-msvc
Host: Windows (win32)

## Selection Criteria

- Latest stable (no alpha, beta, rc)
- Compatible with Rust 1.96.0
- Compiles on x86_64-pc-windows-msvc
- No unnecessary external runtime deps
- Compatible with each other

## Crate Versions

| Crate | Version | Status | Purpose | Notes |
|-------|---------|--------|---------|-------|
| `slint` | 1.17.0 | PASS | Declarative UI | Latest stable. Features: renderer-femtovg, renderer-software. GPL-3.0/royalty-free license. |
| `slint-build` | 1.17.0 | PASS | Build-time .slint compiler | Must match slint exactly. |
| `cpal` | 0.18.1 | PASS | Cross-platform audio I/O | Latest stable. Uses WASAPI on Windows via `default_host()`. |
| `symphonia` | 0.6.0 | PASS | Pure-Rust audio decode/demux | Features: wav, pcm, aiff, isomp4 (expand per tested format). |
| `symphonia-core` | 0.6.0 | PASS | Symphonia core types | Re-exported by symphonia as `symphonia::core`. |
| `ringbuf` | 0.5.0 | PASS | Lock-free SPSC queue | Feeds CPAL callback from decoder thread. No allocations in hot path. |
| `rubato` | 3.0.0 | PASS | Async FFT resampler | Quality resampling. Feature: `fft_resampler` (default). |
| `serde` | 1.0 | PASS | Serialization framework | Used via serde_json for config. |
| `serde_json` | 1.0.150 | PASS | JSON serialization | For atomic settings. |
| `directories` | 5.0 | PASS | Platform dirs | Resolves AppData/Roaming. |
| `log` | 0.4 | PASS | Logging facade | |
| `simplelog` | 0.12 | PASS | Stderr logger | |

## Version Verification

```powershell
cargo search slint --limit 1     # slint = "1.17.0"
cargo search cpal --limit 1      # cpal = "0.18.1"
cargo search symphonia --limit 1 # symphonia = "0.6.0"
cargo search ringbuf --limit 1   # ringbuf = "0.5.0"
cargo search rubato --limit 1    # rubato = "3.0.0"
cargo search windows --limit 1   # windows = "0.62.2"
```

All versions confirmed as latest stable at audit time.

## Lockfile

Versions are pinned via `Cargo.lock`. Regenerate with:
```
cargo update
```

## Key Design Decisions

1. **symphonia features**: Only enable formats after testing with real fixtures.
   Current: wav, pcm, aiff, isomp4. Expand per FORMAT_SUPPORT.md.

2. **Opus**: Symphonia 0.6.0 has no native Opus decoder.
   Requires `symphonia-adapter-libopus` (v0.3.0) with bundled libopus.
   Pending: evaluate license (MIT/Apache-2.0) and build impact.

3. **Rubato path**: rubato 3.0.0 uses `Fft` + `FixedSync::Output` for
   synchronous FFT resampling via audioadapter buffers. Bypasses when
   source_rate == target_rate (no alloc).

4. **CPAL mutex**: Consumer/producer wrapped in `Arc<Mutex<>>` to satisfy
   Rust move semantics across format branches. Mutex is uncontested;
   held only during push_slice/pop_slice (~ns). To revisit: use lock-free
   shared references if CPAL API allows.

5. **No Windows crate in v0.1**: SMTC, IPC, registry, and power management
   use the `windows` crate (0.62.2). Not included in vertical slice — added
   per feature phase.

## Runtime Dependencies

Zero external DLLs required. All decoders (Symphonia), resampler (Rubato),
and audio I/O (CPAL/WASAPI) are pure Rust or link statically.
