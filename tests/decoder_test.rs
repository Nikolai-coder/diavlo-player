use std::fs::File;
use std::io::Write;
use std::path::Path;

fn write_wav_pcm16(path: &Path, sample_rate: u32, channels: u16, samples: &[i16]) {
    let bytes_per_sample = 2u16;
    let block_align = channels * bytes_per_sample;
    let byte_rate = sample_rate * block_align as u32;
    let data_size = samples.len() as u32 * bytes_per_sample as u32;
    let file_size = 36 + data_size;

    let mut f = File::create(path).unwrap();

    f.write_all(b"RIFF").unwrap();
    f.write_all(&file_size.to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();

    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&channels.to_le_bytes()).unwrap();
    f.write_all(&sample_rate.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&block_align.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();

    f.write_all(b"data").unwrap();
    f.write_all(&data_size.to_le_bytes()).unwrap();
    for s in samples {
        f.write_all(&s.to_le_bytes()).unwrap();
    }
}

#[test]
fn test_decode_wav_pcm16_mono() {
    let dir = std::env::temp_dir().join("diavlo_test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test_mono.wav");

    let sample_rate = 44100u32;
    let channels = 1u16;
    let num_samples = 44100;
    let mut samples = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        let val = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        samples.push((val * 32767.0) as i16);
    }

    write_wav_pcm16(&path, sample_rate, channels, &samples);

    let mut decoder = diavlo_player::audio::decoder::AudioDecoder::new(&path).unwrap();
    assert_eq!(decoder.sample_rate(), sample_rate);
    assert_eq!(decoder.channels(), channels);

    let mut total_samples = 0usize;
    while let Some(s) = decoder.next_packet().unwrap() {
        total_samples += s.len();
    }
    assert!(total_samples > 0);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn test_decode_wav_pcm16_stereo() {
    let dir = std::env::temp_dir().join("diavlo_test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test_stereo.wav");

    let sample_rate = 48000u32;
    let channels = 2u16;
    let num_frames = 48000;
    let mut samples = Vec::with_capacity(num_frames * 2);
    for i in 0..num_frames {
        let t = i as f32 / sample_rate as f32;
        let l = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        let r = (t * 660.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        samples.push((l * 32767.0) as i16);
        samples.push((r * 32767.0) as i16);
    }

    write_wav_pcm16(&path, sample_rate, channels, &samples);

    let mut decoder = diavlo_player::audio::decoder::AudioDecoder::new(&path).unwrap();
    assert_eq!(decoder.sample_rate(), sample_rate);
    assert_eq!(decoder.channels(), channels);

    let mut total_samples = 0usize;
    while let Some(s) = decoder.next_packet().unwrap() {
        total_samples += s.len();
    }
    assert!(total_samples > 0);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn test_decode_empty_wav_rejected() {
    let dir = std::env::temp_dir().join("diavlo_test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("empty.wav");

    File::create(&path).unwrap();

    let result = diavlo_player::audio::decoder::AudioDecoder::new(&path);
    assert!(result.is_err());

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn test_decode_corrupt_wav_rejected() {
    let dir = std::env::temp_dir().join("diavlo_test");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("corrupt.wav");

    let mut f = File::create(&path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&[0xff; 4]).unwrap(); // random size

    let result = diavlo_player::audio::decoder::AudioDecoder::new(&path);
    assert!(result.is_err());

    std::fs::remove_file(&path).unwrap();
}
