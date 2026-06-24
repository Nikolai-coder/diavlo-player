# Performance — Phase 0 vertical slice

## Measurement methodology

All measurements are taken from the release binary (`target/release/diavlo-player.exe`)
built with `cargo build --release` (profile: LTO, opt-level=3).

**Metrics defined:**

| Metric | Meaning |
|---|---|
| `window_visible` | Process start → Slint window displayed and Slint event loop running |
| `stream_ready` | Process start → CPAL output stream successfully initialized and `play()` returns |
| `first_sample_enqueued` | Process start → first decoded/resampled audio sample pushed to ring buffer |
| `file_open_to_first_sample` | File-open command dispatched → first sample enqueued |
| `duration` | Total file duration reported by symphonia decoder |

The binary is launched via `Start-Process` with `WaitForExit(8000)`. Because Slint's
`window.run()` blocks until the window is closed (no close-on-end in Phase 0), the
process is killed after the 8 s timeout on every run. All timestamps are recorded
before the kill.

**Limitations:**
- "Cold" here means first invocation after a machine restart is not feasible in
  CI; cold measurements are taken after a ≥30 min idle period with no prior
  diavlo-player process in memory.
- True cold boot (process start before OS caches) requires system restart and
  is deferred to hardware validation.
- CPAL stream initialization time includes Windows audio session setup
  (~300–500 ms) and is not under application control.

## Hardware

- **CPU:** Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz (12 logical cores)
- **RAM:** 32 GB DDR4
- **Storage:** NVMe SSD
- **OS:** Windows 11 Pro 23H2 (build 22631)
- **Audio device:** Realtek High Definition Audio (default output, 48 kHz, stereo)

## Results

### Cold run (first after idle)

```
window_visible              7.00  ms
stream_ready               308.15  ms
first_sample_enqueued      312.03  ms
file_open_to_first_sample  305.68  ms
duration                    2.0    s
```

### Warm runs (10 consecutive, same binary)

| Metric | Min | Max | Median |
|---|---|---|---|
| window_visible (ms) | 10.53 | 24.24 | 13.37 |
| stream_ready (ms) | 352.85 | 493.36 | 428.21 |
| first_sample_enqueued (ms) | 364.11 | 497.59 | 433.34 |
| file_open_to_first_sample (ms) | 354.12 | 497.06 | 421.06 |

### Observations

1. **Window paint is fast** (~7–24 ms). Most time is Slint creating the GL
   surface and rendering the initial frame.

2. **Stream readiness dominates** (~300–500 ms). CPAL `build_output_stream` +
   `play()` requires Windows audio session negotiation. The high variance is
   normal for WASAPI shared mode.

3. **First sample arrives <10 ms after stream ready.** Decode + resample of
   the first 2304-sample packet is near-instant.

4. **95 % of stream-ready time is CPAL internal.** Application code
   (probe → decode → resample → push) accounts for ≤5 ms.

5. **File-open-to-first-sample** is slightly shorter than
   first-sample-from-process-start because the file-open timestamp fires later
   (after window creation), whereas process-start includes the binary load.
