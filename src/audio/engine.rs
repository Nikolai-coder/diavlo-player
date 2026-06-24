use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Observer, Producer, Split};
use ringbuf::HeapRb;

use crate::audio::decoder::AudioDecoder;
use crate::audio::resampler::AudioResampler;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlaybackState {
    Idle,
    Loading,
    Playing,
    Paused,
    Ended,
    Error,
}

pub enum AudioCommand {
    Play(String),
    Pause,
    Resume,
    Stop,
}

pub enum AudioEvent {
    StateChanged(PlaybackState),
    DurationLoaded(f64),
    StreamReady,
    FirstSampleEnqueued,
    PositionUpdated(f64),
    Error(String),
    Finished,
}

struct DecoderHandle {
    stop_flag: Arc<AtomicBool>,
}

pub struct AudioEngine {
    command_tx: Sender<AudioCommand>,
    state: Arc<Mutex<PlaybackState>>,
    position: Arc<AtomicU64>,
    is_playing: Arc<AtomicBool>,
}

impl AudioEngine {
    pub fn new(event_tx: Sender<AudioEvent>) -> Self {
        let (command_tx, command_rx) = channel();
        let state = Arc::new(Mutex::new(PlaybackState::Idle));
        let position = Arc::new(AtomicU64::new(0));
        let is_playing = Arc::new(AtomicBool::new(false));

        let st = state.clone();
        let pos = position.clone();
        let ip = is_playing.clone();

        thread::spawn(move || {
            Self::run_controller(command_rx, event_tx, st, pos, ip);
        });

        Self {
            command_tx,
            state,
            position,
            is_playing,
        }
    }

    pub fn send_command(&self, cmd: AudioCommand) {
        let _ = self.command_tx.send(cmd);
    }

    pub fn is_playing(&self) -> bool {
        self.is_playing.load(Ordering::Relaxed)
    }

    pub fn state(&self) -> PlaybackState {
        *self.state.lock().unwrap()
    }

    pub fn position_secs(&self) -> f64 {
        self.position.load(Ordering::Relaxed) as f64 / 1000.0
    }

    fn set_state(
        state: &Arc<Mutex<PlaybackState>>,
        event_tx: &Sender<AudioEvent>,
        new: PlaybackState,
    ) {
        if let Ok(mut s) = state.lock() {
            *s = new;
        }
        let _ = event_tx.send(AudioEvent::StateChanged(new));
    }

    fn run_controller(
        command_rx: Receiver<AudioCommand>,
        event_tx: Sender<AudioEvent>,
        state: Arc<Mutex<PlaybackState>>,
        position: Arc<AtomicU64>,
        is_playing: Arc<AtomicBool>,
    ) {
        let host = cpal::default_host();
        let device = match host.default_output_device() {
            Some(d) => d,
            None => {
                let _ = event_tx.send(AudioEvent::Error("No audio output device".into()));
                return;
            }
        };

        let config = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => {
                let _ = event_tx.send(AudioEvent::Error(format!("Device config: {}", e)));
                return;
            }
        };

        let output_rate = config.sample_rate();
        let output_channels = config.channels();
        let config_for_stream = cpal::StreamConfig::from(config);

        let rb = HeapRb::<f32>::new(262144);
        let (producer, consumer) = rb.split();

        // Wrap consumer/producer in Arc<Mutex> to satisfy Rust's move semantics
        // Lock is uncontested, held only during pop/push (~ns). TODO: revisit for perf
        let producer = Arc::new(std::sync::Mutex::new(producer));
        let consumer = Arc::new(std::sync::Mutex::new(consumer));

        let is_playing_cb = is_playing.clone();
        let is_playing_cb2 = is_playing.clone();
        let err_event_tx = event_tx.clone();
        let err_event_tx2 = event_tx.clone();
        let consumer_cb = consumer.clone();
        let consumer_cb2 = consumer.clone();

        let stream_result = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config_for_stream,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    if !is_playing_cb.load(Ordering::Relaxed) {
                        data.fill(0.0);
                        return;
                    }
                    let mut guard = consumer_cb.lock().unwrap();
                    let read = guard.pop_slice(data);
                    drop(guard);
                    if read < data.len() {
                        data[read..].fill(0.0);
                    }
                },
                move |err| {
                    let _ = err_event_tx.send(AudioEvent::Error(format!("Stream: {}", err)));
                },
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                config_for_stream,
                move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                    if !is_playing_cb2.load(Ordering::Relaxed) {
                        data.fill(0);
                        return;
                    }
                    let mut tmp = [0.0f32; 4096];
                    let mut offset = 0;
                    while offset < data.len() {
                        let chunk = std::cmp::min(data.len() - offset, tmp.len());
                        let mut guard = consumer_cb2.lock().unwrap();
                        let read = guard.pop_slice(&mut tmp[..chunk]);
                        drop(guard);
                        for i in 0..read {
                            data[offset + i] = (tmp[i].clamp(-1.0, 1.0) * 32767.0) as i16;
                        }
                        if read < chunk {
                            for i in read..chunk {
                                data[offset + i] = 0;
                            }
                            break;
                        }
                        offset += read;
                    }
                },
                move |err| {
                    let _ = err_event_tx2.send(AudioEvent::Error(format!("Stream: {}", err)));
                },
                None,
            ),
            _ => {
                let _ = event_tx.send(AudioEvent::Error(
                    "Unsupported sample format (need F32 or I16)".into(),
                ));
                return;
            }
        };

        let stream = match stream_result {
            Ok(s) => s,
            Err(e) => {
                let _ = event_tx.send(AudioEvent::Error(format!("Cannot build stream: {}", e)));
                return;
            }
        };

        if let Err(e) = stream.play() {
            let _ = event_tx.send(AudioEvent::Error(format!("Cannot play: {}", e)));
            return;
        }
        let _ = event_tx.send(AudioEvent::StreamReady);

        let mut current_decode: Option<DecoderHandle> = None;

        for cmd in command_rx {
            match cmd {
                AudioCommand::Play(filepath) => {
                    if let Some(h) = current_decode.take() {
                        h.stop_flag.store(true, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(30));
                    }

                    is_playing.store(false, Ordering::SeqCst);
                    position.store(0, Ordering::Relaxed);
                    Self::set_state(&state, &event_tx, PlaybackState::Loading);

                    let path = Path::new(&filepath);
                    let mut decoder = match AudioDecoder::new(path) {
                        Ok(d) => d,
                        Err(e) => {
                            Self::set_state(&state, &event_tx, PlaybackState::Error);
                            let _ = event_tx.send(AudioEvent::Error(e.to_string()));
                            continue;
                        }
                    };

                    if let Some(dur) = decoder.total_duration() {
                        let _ = event_tx.send(AudioEvent::DurationLoaded(dur));
                    }

                    let src_rate = decoder.sample_rate();
                    let src_ch = decoder.channels();

                    let mut resampler =
                        match AudioResampler::new(src_rate, output_rate, src_ch, output_channels) {
                            Ok(r) => r,
                            Err(e) => {
                                Self::set_state(&state, &event_tx, PlaybackState::Error);
                                let _ =
                                    event_tx.send(AudioEvent::Error(format!("Resampler: {}", e)));
                                continue;
                            }
                        };

                    let stop_flag = Arc::new(AtomicBool::new(false));
                    let sf = stop_flag.clone();

                    let ip = is_playing.clone();
                    let pos = position.clone();
                    let evt = event_tx.clone();
                    let st = state.clone();
                    let producer_dec = producer.clone();

                    is_playing.store(true, Ordering::SeqCst);
                    Self::set_state(&state, &event_tx, PlaybackState::Playing);

                    thread::spawn(move || {
                        let mut frames_decoded: u64 = 0;
                        let sample_rate = src_rate as u64;
                        let mut first_sample = true;

                        loop {
                            if sf.load(Ordering::SeqCst) {
                                break;
                            }
                            if !ip.load(Ordering::SeqCst) {
                                thread::sleep(Duration::from_millis(10));
                                continue;
                            }

                            match decoder.next_packet() {
                                Ok(Some(samples)) => {
                                    if samples.is_empty() {
                                        continue;
                                    }

                                    let resampled = resampler.process(&samples);
                                    if resampled.is_empty() {
                                        continue;
                                    }

                                    let mut offset = 0;
                                    while offset < resampled.len() {
                                        if sf.load(Ordering::SeqCst) {
                                            break;
                                        }
                                        if !ip.load(Ordering::SeqCst) {
                                            break;
                                        }
                                        let mut prod_guard = producer_dec.lock().unwrap();
                                        let vacant = prod_guard.vacant_len();
                                        if vacant == 0 {
                                            drop(prod_guard);
                                            thread::sleep(Duration::from_millis(5));
                                            continue;
                                        }
                                        let end = std::cmp::min(offset + vacant, resampled.len());
                                        let written =
                                            prod_guard.push_slice(&resampled[offset..end]);
                                        drop(prod_guard);
                                        offset += written;
                                    }

                                    if first_sample {
                                        first_sample = false;
                                        let _ = evt.send(AudioEvent::FirstSampleEnqueued);
                                    }

                                    frames_decoded += samples.len() as u64 / src_ch as u64;
                                    let secs = frames_decoded as f64 / sample_rate as f64;
                                    pos.store((secs * 1000.0) as u64, Ordering::Relaxed);
                                    let _ = evt.send(AudioEvent::PositionUpdated(secs));
                                }
                                Ok(None) => {
                                    ip.store(false, Ordering::SeqCst);
                                    let _ = st.lock().map(|mut s| *s = PlaybackState::Ended);
                                    let _ =
                                        evt.send(AudioEvent::StateChanged(PlaybackState::Ended));
                                    let _ = evt.send(AudioEvent::Finished);
                                    break;
                                }
                                Err(e) => {
                                    ip.store(false, Ordering::SeqCst);
                                    let _ = st.lock().map(|mut s| *s = PlaybackState::Error);
                                    let _ =
                                        evt.send(AudioEvent::StateChanged(PlaybackState::Error));
                                    let _ = evt.send(AudioEvent::Error(e.to_string()));
                                    break;
                                }
                            }
                        }
                    });

                    current_decode = Some(DecoderHandle { stop_flag });
                }
                AudioCommand::Pause => {
                    is_playing.store(false, Ordering::SeqCst);
                    Self::set_state(&state, &event_tx, PlaybackState::Paused);
                }
                AudioCommand::Resume => {
                    let cur = state.lock().ok().map(|s| *s).unwrap_or(PlaybackState::Idle);
                    if cur == PlaybackState::Paused || cur == PlaybackState::Ended {
                        is_playing.store(true, Ordering::SeqCst);
                        Self::set_state(&state, &event_tx, PlaybackState::Playing);
                    }
                }
                AudioCommand::Stop => {
                    if let Some(h) = current_decode.take() {
                        h.stop_flag.store(true, Ordering::SeqCst);
                    }
                    is_playing.store(false, Ordering::SeqCst);
                    position.store(0, Ordering::Relaxed);
                    Self::set_state(&state, &event_tx, PlaybackState::Idle);
                }
            }
        }
    }
}
