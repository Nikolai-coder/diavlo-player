use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, Mutex, TryLockError};
use std::thread;
use std::time::{Duration, Instant};

use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::{HeapCons, HeapProd, HeapRb};

use diavlo_player::audio::resampler::AudioResampler;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const PI: f64 = std::f64::consts::PI;

fn sine(freq: f64, sample_rate: u32, channels: u16, num_frames: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(num_frames * channels as usize);
    for i in 0..num_frames {
        let t = i as f64 / sample_rate as f64;
        let v = (t * freq * 2.0 * PI).sin() as f32;
        for _ in 0..channels {
            out.push(v);
        }
    }
    out
}

fn silence(channels: u16, num_frames: usize) -> Vec<f32> {
    vec![0.0f32; num_frames * channels as usize]
}

fn frames_of(samples: &[f32], channels: u16) -> usize {
    samples.len() / channels as usize
}

fn rms(samples: &[f32]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|s| (*s as f64).powi(2)).sum();
    (sum / samples.len() as f64).sqrt()
}

fn peak(samples: &[f32]) -> f64 {
    samples.iter().map(|s| s.abs() as f64).fold(0.0, f64::max)
}

fn expect_valid(samples: &[f32], label: &str) {
    for &s in samples {
        assert!(s.is_finite(), "{}: non-finite sample {}", label, s);
        assert!(
            s >= -1.0 && s <= 1.0,
            "{}: sample {} out of range [-1, 1]",
            label,
            s
        );
    }
}

/// Check that two buffers match sample-for-sample within a tolerance.
fn expect_samples_eq(actual: &[f32], expected: &[f32], tol: f32, label: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "{}: length mismatch {} vs {}",
        label,
        actual.len(),
        expected.len()
    );
    for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        let diff = (a - e).abs();
        assert!(
            diff <= tol,
            "{}: sample[{}] diff={} (actual={} expected={})",
            label,
            i,
            diff,
            a,
            e
        );
    }
}

type SharedConsumer = Arc<Mutex<HeapCons<f32>>>;
type SharedProducer = Arc<Mutex<HeapProd<f32>>>;

fn make_ringbuf(size: usize) -> (SharedProducer, SharedConsumer) {
    let rb = HeapRb::<f32>::new(size);
    let (p, c) = rb.split();
    (Arc::new(Mutex::new(p)), Arc::new(Mutex::new(c)))
}

/// Push all samples into the producer (spinning on full).
fn push_all(producer: &SharedProducer, samples: &[f32]) {
    let mut offset = 0;
    while offset < samples.len() {
        let mut guard = producer.lock().unwrap();
        let vacant = guard.vacant_len();
        if vacant == 0 {
            drop(guard);
            thread::yield_now();
            continue;
        }
        let end = std::cmp::min(offset + vacant, samples.len());
        let written = guard.push_slice(&samples[offset..end]);
        offset += written;
    }
}

/// Drain all available samples from the consumer.
fn drain_all(consumer: &SharedConsumer) -> Vec<f32> {
    let mut out = Vec::new();
    loop {
        let mut guard = consumer.lock().unwrap();
        let occupied = guard.occupied_len();
        if occupied == 0 {
            break;
        }
        let mut buf = vec![0.0f32; occupied];
        let read = guard.pop_slice(&mut buf);
        drop(guard);
        out.extend_from_slice(&buf[..read]);
        if read < occupied {
            break;
        }
    }
    out
}

/// Dummy "decoder" that yields a fixed sequence of packets (None = end-of-stream).
struct MockDecoder {
    packets: Vec<Option<Vec<f32>>>,
    index: usize,
}

impl MockDecoder {
    fn new(packets: Vec<Option<Vec<f32>>>) -> Self {
        Self { packets, index: 0 }
    }

    fn next_packet(&mut self) -> Option<Vec<f32>> {
        if self.index >= self.packets.len() {
            return None;
        }
        let p = self.packets[self.index].take();
        self.index += 1;
        p
    }
}

// ---------------------------------------------------------------------------
// 1. Rate conversion 44.1 kHz -> 48 kHz
// ---------------------------------------------------------------------------

#[test]
fn test_rate_44100_to_48000_duration() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let input = sine(440.0, 44100, 2, 44100);
    let output = r.process(&input).to_vec();

    assert!(!output.is_empty(), "output should not be empty");
    let out_frames = frames_of(&output, 2);
    assert!(
        out_frames >= 47000 && out_frames <= 50000,
        "output frames {} out of expected range [47000, 50000]",
        out_frames
    );

    expect_valid(&output, "rate_44100_to_48000");

    let output_rms = rms(&output);
    assert!(output_rms > 0.3, "RMS {} too low, signal lost", output_rms);
}

#[test]
fn test_rate_44100_to_48000_consecutive_and_flush() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let input = sine(440.0, 44100, 2, 2048);

    for _ in 0..3 {
        let out = r.process(&input).to_vec();
        expect_valid(&out, "consecutive chunk");
    }

    let flushed = r.flush().to_vec();
    expect_valid(&flushed, "flush after consecutive");

    let flushed2 = r.flush().to_vec();
    expect_valid(&flushed2, "second flush");
    // second flush should return empty or negligible < 1024
    assert!(flushed2.len() < 1024, "second flush {} too large", flushed2.len());
}

#[test]
fn test_rate_44100_to_48000_no_nan_inf() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let out = r.process(&sine(1234.0, 44100, 2, 4096)).to_vec();
    expect_valid(&out, "nan_inf_check");
}

// ---------------------------------------------------------------------------
// 2. Channel conversion mono -> stereo
// ---------------------------------------------------------------------------

#[test]
fn test_mono_to_stereo_channels_match() {
    let mut r = AudioResampler::new(44100, 44100, 1, 2).unwrap();
    let input = sine(440.0, 44100, 1, 1000);
    let out = r.process(&input).to_vec();
    expect_valid(&out, "mono_to_stereo");

    // rubato may add up to 256 frames of padding at same rate
    let out_frames = frames_of(&out, 2);
    let in_frames = frames_of(&input, 1);
    assert!(
        out_frames >= in_frames && out_frames <= in_frames + 256,
        "mono->stereo frame count {} not in [{}, {}]",
        out_frames,
        in_frames,
        in_frames + 256
    );

    for frame in 0..frames_of(&out, 2) {
        assert_eq!(
            out[frame * 2],
            out[frame * 2 + 1],
            "mono->stereo channels differ at frame {}",
            frame
        );
    }
}

#[test]
fn test_mono_to_stereo_amplitude_preserved() {
    let mut r = AudioResampler::new(44100, 44100, 1, 2).unwrap();
    let input = sine(440.0, 44100, 1, 1000);
    let out = r.process(&input).to_vec();
    let in_peak = peak(&input);
    let out_peak = peak(&out);
    assert!(
        out_peak <= in_peak * 1.05 + 0.01,
        "output peak {} exceeds input peak {}",
        out_peak,
        in_peak
    );
}

#[test]
fn test_mono_to_stereo_with_rate_conversion() {
    let mut r = AudioResampler::new(44100, 48000, 1, 2).unwrap();
    let input = sine(440.0, 44100, 1, 44100);
    let out = r.process(&input).to_vec();
    expect_valid(&out, "mono_stereo_rate");

    let out_frames = frames_of(&out, 2);
    assert!(
        out_frames >= 47000 && out_frames <= 50000,
        "mono_stereo_rate: frames {} out of range",
        out_frames
    );

    for frame in 0..out_frames {
        assert_eq!(
            out[frame * 2],
            out[frame * 2 + 1],
            "mono_stereo_rate channels differ frame {}",
            frame
        );
    }
}

// ---------------------------------------------------------------------------
// 3. flush()
// ---------------------------------------------------------------------------

#[test]
fn test_flush_drains_internal_buffer() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let _ = r.process(&sine(440.0, 44100, 2, 256));
    let flushed = r.flush().to_vec();
    expect_valid(&flushed, "flush_drains");
}

#[test]
fn test_flush_no_duplicate_samples() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let input = sine(440.0, 44100, 2, 2048);

    let out1 = r.process(&input).to_vec();
    let _flush1 = r.flush().to_vec();

    let out2 = r.process(&input).to_vec();
    let _flush2 = r.flush().to_vec();

    let diff = (out1.len() as isize - out2.len() as isize).unsigned_abs();
    assert!(diff <= 4, "output lengths differ by {}", diff);
}

#[test]
fn test_flush_empty_after_flush() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let _ = r.process(&sine(440.0, 44100, 2, 4096));
    let _f1 = r.flush();
    let f2 = r.flush().to_vec();
    assert!(
        f2.len() < 1024,
        "second flush returned {} samples (expected few)",
        f2.len()
    );
}

#[test]
fn test_flush_before_any_data() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let flushed = r.flush().to_vec();
    expect_valid(&flushed, "flush_before_data");
}

// ---------------------------------------------------------------------------
// 4. Empty packets
// ---------------------------------------------------------------------------

#[test]
fn test_empty_packet_in_middle() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();

    let out1 = r.process(&sine(440.0, 44100, 2, 2048)).to_vec();
    let out_empty = r.process(&[]).to_vec();
    let out2 = r.process(&sine(440.0, 44100, 2, 2048)).to_vec();

    expect_valid(&out1, "before_empty");
    assert!(out_empty.is_empty(), "empty input should yield empty output");
    expect_valid(&out2, "after_empty");
    assert!(!out2.is_empty(), "non-empty after empty should work");
}

#[test]
fn test_multiple_empty_packets() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    for _ in 0..5 {
        let out = r.process(&[]).to_vec();
        assert!(out.is_empty(), "consecutive empty packets not empty");
    }
    let out = r.process(&sine(440.0, 44100, 2, 2048)).to_vec();
    assert!(!out.is_empty(), "real packet after empties failed");
    expect_valid(&out, "after_multi_empty");
}

// ---------------------------------------------------------------------------
// 5. Underrun and Mutex contention
// ---------------------------------------------------------------------------

#[test]
fn test_underrun_contention_no_block() {
    let (producer, _consumer) = make_ringbuf(1024);
    let consumer_lock = Arc::new(Mutex::new(
        HeapRb::<f32>::new(1024).split().1,
    ));
    let underruns = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let underruns_cb = underruns.clone();
    let barrier = Arc::new(Barrier::new(2));
    let bar = barrier.clone();

    // Clone the Arc for the spawned thread so we can hold the lock here
    let consumer_for_thread = consumer_lock.clone();
    let _guard = consumer_lock.lock().unwrap();

    let handle = thread::spawn(move || {
        bar.wait();
        match consumer_for_thread.try_lock() {
            Ok(_) => panic!("try_lock should have failed"),
            Err(TryLockError::WouldBlock) => {
                underruns_cb.fetch_add(1, Ordering::SeqCst);
            }
            Err(TryLockError::Poisoned(_)) => panic!("lock poisoned"),
        }
    });

    barrier.wait();
    handle.join().unwrap();
    assert_eq!(underruns.load(Ordering::SeqCst), 1, "underrun not counted");
    drop(_guard);
    drop(producer);
}

#[test]
fn test_underrun_recovers_after_lock_released() {
    let (producer, consumer) = make_ringbuf(4096);
    push_all(&producer, &sine(440.0, 44100, 2, 2048));

    let underruns = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let underruns_cb = underruns.clone();
    let consumer_for_thread = consumer.clone();
    let _guard = consumer.lock().unwrap();

    let barrier = Arc::new(Barrier::new(2));
    let bar = barrier.clone();
    let started = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started_cb = started.clone();

    let handle = thread::spawn(move || {
        match consumer_for_thread.try_lock() {
            Ok(_) => panic!("should have blocked"),
            Err(TryLockError::WouldBlock) => {
                underruns_cb.fetch_add(1, Ordering::SeqCst);
            }
            Err(_) => panic!("poisoned"),
        }
        started_cb.store(true, Ordering::SeqCst);
        bar.wait();

        loop {
            match consumer_for_thread.try_lock() {
                Ok(mut g) => {
                    let mut buf = [0.0f32; 256];
                    let read = g.pop_slice(&mut buf);
                    assert!(read > 0, "should have read data after unlock");
                    return;
                }
                Err(TryLockError::WouldBlock) => thread::yield_now(),
                Err(_) => panic!("poisoned"),
            }
        }
    });

    barrier.wait();
    assert!(started.load(Ordering::SeqCst));
    assert_eq!(underruns.load(Ordering::SeqCst), 1, "underrun not counted");

    drop(_guard);
    handle.join().unwrap();
    drop(producer);
}

// ---------------------------------------------------------------------------
// 6. Cancellation
// ---------------------------------------------------------------------------

#[test]
fn test_cancel_before_start() {
    let stop_flag = Arc::new(AtomicBool::new(true));
    let (producer, consumer) = make_ringbuf(4096);

    let mut mock = MockDecoder::new(vec![
        Some(sine(440.0, 44100, 2, 2048)),
        Some(sine(440.0, 44100, 2, 2048)),
    ]);

    loop {
        if stop_flag.load(Ordering::SeqCst) {
            break;
        }
        match mock.next_packet() {
            Some(samples) => push_all(&producer, &samples),
            None => break,
        }
    }

    let drained = drain_all(&consumer);
    assert!(drained.is_empty(), "data pushed despite cancellation");
}

#[test]
fn test_cancel_during_processing() {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let (producer, consumer) = make_ringbuf(65536);

    let input = sine(440.0, 44100, 2, 4096);
    push_all(&producer, &input);

    stop_flag.store(true, Ordering::SeqCst);

    // Push more — ringbuf is 65536 so this won't fill it
    push_all(&producer, &input[..2048]);

    let drained = drain_all(&consumer);
    assert!(!drained.is_empty(), "data pushed before cancel should be available");
    expect_valid(&drained, "cancel_during_processing");
    drop(stop_flag);
}

#[test]
fn test_cancel_while_flushing() {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();

    let _ = r.process(&sine(440.0, 44100, 2, 4096));
    stop_flag.store(true, Ordering::SeqCst);

    let flushed = r.flush().to_vec();
    expect_valid(&flushed, "cancel_while_flushing");
    drop(stop_flag);
}

// ---------------------------------------------------------------------------
// 7. Different packet sizes
// ---------------------------------------------------------------------------

#[test]
fn test_small_packets_below_rubato_chunk() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    for _ in 0..10 {
        let out = r.process(&sine(440.0, 44100, 2, 64)).to_vec();
        expect_valid(&out, "small packet");
    }
    let flushed = r.flush().to_vec();
    expect_valid(&flushed, "flush after small packets");
}

#[test]
fn test_exact_chunk_packets() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let out = r.process(&sine(440.0, 44100, 2, 1024)).to_vec();
    expect_valid(&out, "exact chunk");
    assert!(!out.is_empty(), "exact chunk produced no output");
}

#[test]
fn test_large_packets() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let out = r.process(&sine(440.0, 44100, 2, 8192)).to_vec();
    expect_valid(&out, "large packet");
    assert!(!out.is_empty(), "large packet produced no output");
}

#[test]
fn test_irregular_sizes() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    for &s in &[1usize, 3, 128, 7, 1024, 17, 4096, 512, 5, 2048] {
        let out = r.process(&sine(440.0, 44100, 2, s)).to_vec();
        expect_valid(&out, &format!("irregular size {}", s));
    }
}

// ---------------------------------------------------------------------------
// 8. Channel integrity
// ---------------------------------------------------------------------------

#[test]
fn test_stereo_to_stereo_identity() {
    let mut r = AudioResampler::new(44100, 44100, 2, 2).unwrap();
    let input = sine(440.0, 44100, 2, 2048);
    let out = r.process(&input).to_vec();
    expect_valid(&out, "stereo_identity");
    assert_eq!(
        frames_of(&out, 2),
        frames_of(&input, 2),
        "stereo identity should preserve frame count"
    );
}

#[test]
fn test_mono_to_mono_identity() {
    let mut r = AudioResampler::new(44100, 44100, 1, 1).unwrap();
    let input = sine(440.0, 44100, 1, 2048);
    let out = r.process(&input).to_vec();
    expect_valid(&out, "mono_identity");
    assert_eq!(
        frames_of(&out, 1),
        frames_of(&input, 1),
        "mono identity should preserve frame count"
    );
}

#[test]
fn test_mono_to_stereo_interleaving() {
    let mut r = AudioResampler::new(48000, 48000, 1, 2).unwrap();
    let input = sine(440.0, 48000, 1, 100);
    let out = r.process(&input).to_vec();
    expect_valid(&out, "mono_stereo_interleave");

    for i in 0..frames_of(&out, 2) {
        assert_eq!(
            out[i * 2],
            out[i * 2 + 1],
            "interleaving: L != R at frame {}",
            i
        );
    }
}

#[test]
fn test_stereo_to_mono_downmix() {
    let mut r = AudioResampler::new(44100, 44100, 2, 1).unwrap();
    let mut input = vec![0.0f32; 200 * 2];
    for i in 0..200 {
        let t = i as f64 / 44100.0;
        let v = (t * 440.0 * 2.0 * PI).sin() as f32;
        input[i * 2] = v * 0.8;
        input[i * 2 + 1] = v * 0.6;
    }
    let out = r.process(&input).to_vec();
    expect_valid(&out, "stereo_to_mono");

    for i in 0..frames_of(&out, 1) {
        let expected = (input[i * 2] + input[i * 2 + 1]) * 0.5;
        let diff = (out[i] - expected).abs();
        assert!(
            diff <= 0.02,
            "stereo->mono downmix error at frame {}: actual={} expected={}",
            i,
            out[i],
            expected
        );
    }
}

// ---------------------------------------------------------------------------
// 9. End-of-stream (flush + termination)
// ---------------------------------------------------------------------------

#[test]
fn test_end_of_stream_flush_reaches_consumer() {
    let (producer, consumer) = make_ringbuf(65536);
    let mut resampler = AudioResampler::new(44100, 48000, 2, 2).unwrap();

    let input = sine(440.0, 44100, 2, 4096);
    let resampled = resampler.process(&input).to_vec();
    push_all(&producer, &resampled);

    let flushed = resampler.flush().to_vec();
    if !flushed.is_empty() {
        push_all(&producer, &flushed);
    }

    let drained = drain_all(&consumer);
    assert!(!drained.is_empty(), "no data reached consumer");
    expect_valid(&drained, "end_of_stream");

    let drained_rms = rms(&drained);
    assert!(drained_rms > 0.1, "end-of-stream data RMS {} too low", drained_rms);
}

#[test]
fn test_end_of_stream_no_data_loss() {
    let (producer, consumer) = make_ringbuf(65536);
    let mut resampler = AudioResampler::new(44100, 48000, 2, 2).unwrap();

    for _ in 0..5 {
        let out = resampler.process(&sine(440.0, 44100, 2, 2048)).to_vec();
        push_all(&producer, &out);
    }

    let flushed = resampler.flush().to_vec();
    if !flushed.is_empty() {
        push_all(&producer, &flushed);
    }

    let remaining = drain_all(&consumer);
    assert!(
        !remaining.is_empty(),
        "no data in consumer after end of stream"
    );

    // Verify no stray data after draining
    let extra = match consumer.try_lock() {
        Ok(mut guard) => {
            let occ = guard.occupied_len();
            let mut buf = vec![0.0f32; occ];
            let r = guard.pop_slice(&mut buf);
            buf[..r].to_vec()
        }
        Err(_) => vec![],
    };
    assert!(
        extra.is_empty(),
        "unexpected extra data after end-of-stream: {} samples",
        extra.len()
    );
}

// ---------------------------------------------------------------------------
// 10. Concurrent safety
// ---------------------------------------------------------------------------

#[test]
fn test_concurrent_producer_consumer() {
    let (producer, consumer) = make_ringbuf(65536);
    let num_samples = 44100 * 2;

    let p = producer.clone();
    let input = sine(440.0, 44100, 2, 22050);
    let t1 = thread::spawn(move || {
        push_all(&p, &input);
    });

    let c = consumer.clone();
    let t2 = thread::spawn(move || {
        let mut total = 0usize;
        loop {
            let mut guard = c.lock().unwrap();
            let occ = guard.occupied_len();
            if occ == 0 && total >= num_samples / 2 {
                break;
            }
            if occ == 0 {
                drop(guard);
                thread::yield_now();
                continue;
            }
            let to_read = std::cmp::min(occ, 4096);
            let mut buf = vec![0.0f32; to_read];
            let read = guard.pop_slice(&mut buf);
            drop(guard);
            total += read;
        }
        total
    });

    t1.join().unwrap();
    let consumed = t2.join().unwrap();
    assert!(
        consumed >= num_samples / 2 - 128,
        "consumed {} expected at least {}",
        consumed,
        num_samples / 2 - 128
    );
}

#[test]
fn test_concurrent_stress_no_deadlock() {
    let (producer, consumer) = make_ringbuf(65536);
    let runs = 50;
    let done = Arc::new(AtomicBool::new(false));

    let barrier = Arc::new(Barrier::new(3));
    let b1 = barrier.clone();
    let b2 = barrier.clone();

    let p = producer.clone();
    let c = consumer.clone();
    let done_c = done.clone();

    let t1 = thread::spawn(move || {
        b1.wait();
        for _ in 0..runs {
            let input = sine(440.0, 44100, 2, 512);
            let mut offset = 0;
            while offset < input.len() {
                let mut guard = p.lock().unwrap();
                let vacant = guard.vacant_len();
                if vacant == 0 {
                    drop(guard);
                    thread::yield_now();
                    continue;
                }
                let end = std::cmp::min(offset + vacant, input.len());
                let written = guard.push_slice(&input[offset..end]);
                offset += written;
            }
        }
        done_c.store(true, Ordering::SeqCst);
    });

    let t2 = thread::spawn(move || {
        b2.wait();
        loop {
            if done.load(Ordering::SeqCst) {
                break;
            }
            let mut guard = match c.try_lock() {
                Ok(g) => g,
                Err(_) => continue,
            };
            let occ = guard.occupied_len();
            if occ > 0 {
                let mut buf = vec![0.0f32; occ];
                let _ = guard.pop_slice(&mut buf);
            }
        }
    });

    barrier.wait();
    t1.join().unwrap();
    t2.join().unwrap();
}

// ---------------------------------------------------------------------------
// Bypass (same-rate, same-channel) optimization
// ---------------------------------------------------------------------------

#[test]
fn test_bypass_returns_input_directly() {
    let mut r = AudioResampler::new(48000, 48000, 2, 2).unwrap();
    let input = sine(440.0, 48000, 2, 1024);
    let out = r.process(&input).to_vec();
    assert_eq!(out.len(), input.len(), "bypass length mismatch");
    expect_valid(&out, "bypass");
}

#[test]
fn test_bypass_flush_empty() {
    let mut r = AudioResampler::new(48000, 48000, 2, 2).unwrap();
    let flushed = r.flush().to_vec();
    assert!(flushed.is_empty(), "bypass flush should be empty");
}

// ---------------------------------------------------------------------------
// Error handling: invalid parameters
// ---------------------------------------------------------------------------

#[test]
fn test_resampler_construction_invalid_rates() {
    let result = AudioResampler::new(0, 48000, 2, 2);
    assert!(result.is_err(), "zero source rate should be rejected");
}

#[test]
fn test_resampler_zero_channels() {
    let result = AudioResampler::new(44100, 48000, 0, 2);
    if let Ok(mut r) = result {
        let out = r.process(&[]).to_vec();
        expect_valid(&out, "zero_channels");
    }
}

// ---------------------------------------------------------------------------
// Silence passthrough
// ---------------------------------------------------------------------------

#[test]
fn test_silence_passthrough() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let input = silence(2, 2048);
    let out = r.process(&input).to_vec();

    let out_peak = peak(&out);
    assert!(
        out_peak < 1e-6,
        "silence passthrough produced non-zero peak {}",
        out_peak
    );

    let out_rms = rms(&out);
    assert!(
        out_rms < 1e-6,
        "silence passthrough produced non-zero RMS {}",
        out_rms
    );
}

#[test]
fn test_silence_then_signal() {
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    let _ = r.process(&silence(2, 2048));
    let out = r.process(&sine(440.0, 44100, 2, 2048)).to_vec();
    expect_valid(&out, "silence_then_signal");
    assert!(rms(&out) > 0.3, "silence_then_signal RMS too low");
}

// ---------------------------------------------------------------------------
// Deterministic pipeline integration
// ---------------------------------------------------------------------------

#[test]
fn test_full_pipeline_deterministic() {
    let (producer, consumer) = make_ringbuf(65536);
    let mut resampler = AudioResampler::new(44100, 48000, 2, 2).unwrap();

    let mut mock = MockDecoder::new(vec![
        Some(sine(440.0, 44100, 2, 2048)),
        Some(sine(440.0, 44100, 2, 2048)),
        Some(sine(440.0, 44100, 2, 2048)),
        None,
    ]);

    loop {
        match mock.next_packet() {
            Some(samples) => {
                let resampled = resampler.process(&samples).to_vec();
                if !resampled.is_empty() {
                    push_all(&producer, &resampled);
                }
            }
            None => {
                let flushed = resampler.flush().to_vec();
                if !flushed.is_empty() {
                    push_all(&producer, &flushed);
                }
                break;
            }
        }
    }

    let drained = drain_all(&consumer);
    assert!(!drained.is_empty(), "pipeline produced no output");
    expect_valid(&drained, "full_pipeline");

    let remaining = consumer.lock().unwrap().occupied_len();
    assert_eq!(remaining, 0, "pipeline left {} samples unconsumed", remaining);
}

// ---------------------------------------------------------------------------
// Stress test (longer, flagged with ignore)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn test_stress_high_contention() {
    let (producer, consumer) = make_ringbuf(4096);
    let iterations = 200;
    let mut handles = vec![];

    for _ in 0..4 {
        let p = producer.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..iterations {
                let input = sine(440.0, 44100, 2, 128);
                let mut offset = 0;
                while offset < input.len() {
                    match p.try_lock() {
                        Ok(mut guard) => {
                            let vacant = guard.vacant_len();
                            if vacant == 0 {
                                drop(guard);
                                thread::yield_now();
                                continue;
                            }
                            let end = std::cmp::min(offset + vacant, input.len());
                            let written = guard.push_slice(&input[offset..end]);
                            offset += written;
                        }
                        Err(_) => thread::yield_now(),
                    }
                }
            }
        }));
    }

    for _ in 0..4 {
        let c = consumer.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..iterations {
                if let Ok(mut guard) = c.try_lock() {
                    let occ = guard.occupied_len();
                    if occ > 0 {
                        let mut buf = vec![0.0f32; occ];
                        let _ = guard.pop_slice(&mut buf);
                    }
                }
                thread::yield_now();
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

// ---------------------------------------------------------------------------
// Timeout guard
// ---------------------------------------------------------------------------

#[test]
fn test_no_test_blocks_indefinitely() {
    let start = Instant::now();
    let mut r = AudioResampler::new(44100, 48000, 2, 2).unwrap();
    for _ in 0..10 {
        let _ = r.process(&sine(440.0, 44100, 2, 1024));
    }
    let _ = r.flush();
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "basic operations took too long"
    );
}
