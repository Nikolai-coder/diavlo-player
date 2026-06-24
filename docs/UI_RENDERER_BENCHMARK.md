# UI Renderer Benchmark

Purpose: Select between Slint `renderer-femtovg` and `renderer-software`
based on real measurements on the target hardware.

## Test Protocol

1. Build with each renderer feature.
2. Measure cold start (first launch after boot).
3. Measure warm start (second launch).
4. Record: time-to-window, RAM (working set), CPU idle %, binary size.
5. Repeat with GPU acceleration disabled (if applicable).
6. Repeat over Remote Desktop.
7. Repeat with problematic GPU drivers (if available).

## Results

| Metric | femtovg | software | Notes |
|--------|---------|----------|-------|
| Time to window (cold) | TBD | TBD | |
| Time to window (warm) | TBD | TBD | |
| RAM working set | TBD | TBD | |
| CPU idle | TBD | TBD | |
| Binary size | TBD | TBD | |
| No GPU accel | TBD | TBD | |
| Remote Desktop | TBD | TBD | |
| Problematic driver | TBD | TBD | |

## Decision

TBD — run benchmark after vertical slice is stable and before building full UI.

## Build Commands

```powershell
# femtovg (OpenGL)
cargo build --release --features "renderer-femtovg"

# software
cargo build --release --features "renderer-software"

# both (default)
cargo build --release
```
