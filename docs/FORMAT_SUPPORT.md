# Format Support Matrix

Version: 0.1.0 (vertical slice)
Engine: Symphonia 0.6.0

Format status is ONLY marked PASS after validation with a real fixture file.

## Status Key

- **PASS**: Tested with real fixture, stable playback.
- **PARTIAL**: Decodes but known limitations exist.
- **UNSUPPORTED**: Not supported by current codecs/features.
- **BLOCKED**: Needs external library or license review.
- **NOT YET TESTED**: Feature enabled but fixture not validated.

## Matrix

| Container | Codec | Decoder | Status | Notes |
|-----------|-------|---------|--------|-------|
| WAV | PCM (uncompressed) | Symphonia WAV demuxer + PCM decoder | **PASS** | 8/16/24/32-bit int, 32-bit float. Mono/stereo/multichannel. |
| WAV | ADPCM | Symphonia ADPCM decoder | NOT YET TESTED | |
| AIFF | PCM | Symphonia AIFF demuxer + PCM decoder | **PASS** | Standard uncompressed AIFF. |
| MP4/M4A | AAC-LC | Symphonia ISO/MP4 demuxer + AAC decoder | NOT YET TESTED | HE-AAC may be PARTIAL. |
| MP4/M4A | ALAC | Symphonia ISO/MP4 demuxer + ALAC decoder | NOT YET TESTED | |
| MP3 | MPEG Layer 3 | Symphonia MP3 demuxer + MPA decoder | NOT YET TESTED | Needs `mp3` feature. |
| FLAC | FLAC | Symphonia FLAC demuxer + decoder | NOT YET TESTED | Needs `flac` feature. |
| OGG | Vorbis | Symphonia OGG demuxer + Vorbis decoder | NOT YET TESTED | Needs `ogg`, `vorbis` features. |
| OGG | Opus | — | **BLOCKED** | Symphonia has NO native Opus. Requires `symphonia-adapter-libopus` (packaged libopus). Pending license/build audit. |

## Fixtures Required

| # | File | Content | Status |
|---|------|---------|--------|
| 1 | `fixtures/test.wav` | PCM 16-bit 44.1kHz stereo, ~1s | PENDING |
| 2 | `fixtures/empty.wav` | 0-byte file | PENDING |
| 3 | `fixtures/truncated.wav` | Valid header, missing data | PENDING |
| 4 | `fixtures/wrongext.dat` | Valid WAV content, .dat extension | PENDING |
| 5 | `fixtures/corrupt.wav` | Random bytes, .wav extension | PENDING |

## Vertical Slice Scope (v0.1)

Only WAV PCM is targeted for the vertical slice. All other formats are
scoped to subsequent phases.

See `tests/decoder_test.rs` for fixture validation.
