use std::path::Path;

fn fixture_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

fn test_decode(label: &str, filename: &str) {
    let path = fixture_dir().join(filename);
    if !path.exists() {
        println!("  {}: BLOCKED (MISSING)", label);
        return;
    }
    match diavlo_player::audio::decoder::AudioDecoder::new(&path) {
        Ok(mut decoder) => {
            let sr = decoder.sample_rate();
            let ch = decoder.channels();
            let mut total = 0usize;
            let mut packets = 0u32;
            loop {
                match decoder.next_packet() {
                    Ok(Some(s)) => {
                        total += s.len();
                        packets += 1;
                    }
                    Ok(None) => break,
                    Err(_) => {
                        println!(
                            "  {}: PARTIAL (decode error after {} packets, {} samples)",
                            label, packets, total
                        );
                        return;
                    }
                }
            }
            if total > 0 {
                println!(
                    "  {}: PASS (rate={} ch={} packets={} samples={})",
                    label, sr, ch, packets, total
                );
            } else {
                println!("  {}: PARTIAL (no samples)", label);
            }
        }
        Err(e) => {
            let s = e.to_string();
            let r = if s.contains("Unsupported") {
                "UNSUPPORTED"
            } else {
                "BLOCKED"
            };
            println!("  {}: {} ({})", label, r, s);
        }
    }
}

#[test]
fn format_matrix() {
    println!();
    println!("=== FORMAT MATRIX ===");
    println!();
    println!("| Extension | Container | Codec | Decoder | Fixture | Result | Notes |");
    println!("|---|---|---|---|---|---|---|");

    test_decode("WAV PCM16 stereo 44100Hz", "test.wav");
    test_decode("MP3 CBR 128k 44100Hz stereo", "test.mp3");
    test_decode("FLAC level 5 44100Hz stereo", "test.flac");
    test_decode("OGG Vorbis q3 44100Hz stereo", "test.ogg");
    test_decode("Opus 64k 48000Hz stereo (ogg container)", "test.opus");
    test_decode("M4A AAC-LC 128k 44100Hz stereo", "test.m4a");
    test_decode("M4A ALAC 44100Hz stereo", "test.alac.m4a");
    test_decode("AIFF PCM16 stereo 44100Hz", "test.aiff");

    println!();
}
