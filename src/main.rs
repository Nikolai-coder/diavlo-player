use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use slint::ComponentHandle;

use diavlo_player::audio::engine::{AudioCommand, AudioEngine, AudioEvent, PlaybackState};

slint::include_modules!();

fn main() {
    let start = Instant::now();

    let _ = simplelog::SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default());

    let args: Vec<String> = std::env::args().collect();
    let file_to_play = if args.len() > 1 {
        let p = &args[1];
        if Path::new(p).exists() {
            log::info!("CLI file: {}", p);
            Some(p.clone())
        } else {
            log::warn!("File not found: {}", p);
            None
        }
    } else {
        None
    };

    let window = AppWindow::new().expect("Failed to create Slint window");
    let window_weak = window.as_weak();

    let (event_tx, event_rx) = channel();

    let engine = Arc::new(AudioEngine::new(event_tx));
    let st = start;
    let first_audio = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let file_open_time = Arc::new(std::sync::Mutex::new(None::<Instant>));

    let fa = first_audio.clone();
    let fot = file_open_time.clone();

    thread::spawn(move || {
        for event in event_rx {
            let w = window_weak.clone();
            let f = fa.clone();
            let fot_local = fot.clone();
            let _ = w.upgrade_in_event_loop(move |win| match event {
                AudioEvent::StateChanged(s) => {
                    let label = match s {
                        PlaybackState::Idle => "Stopped",
                        PlaybackState::Loading => "Loading...",
                        PlaybackState::Playing => "Playing",
                        PlaybackState::Paused => "Paused",
                        PlaybackState::Ended => "Finished",
                        PlaybackState::Error => "Error",
                    };
                    win.set_status_text(label.into());
                    win.set_is_playing(s == PlaybackState::Playing);
                }
                AudioEvent::DurationLoaded(dur) => {
                    log::info!("METRIC: duration={:.1}", dur);
                }
                AudioEvent::StreamReady => {
                    let elapsed = st.elapsed();
                    log::info!("METRIC: stream_ready={:.2?}", elapsed);
                }
                AudioEvent::FirstSampleEnqueued => {
                    if !f.load(Ordering::Relaxed) {
                        f.store(true, Ordering::Relaxed);
                        let from_start = st.elapsed();
                        log::info!("METRIC: first_sample_enqueued={:.2?}", from_start);
                        if let Some(fot) = fot_local.lock().unwrap().as_ref() {
                            let from_open = fot.elapsed();
                            log::info!("METRIC: file_open_to_first_sample={:.2?}", from_open);
                        }
                    }
                }
                AudioEvent::PositionUpdated(pos) => {
                    let m = (pos / 60.0) as u32;
                    let s = (pos % 60.0) as u32;
                    win.set_status_text(format!("{:02}:{:02}", m, s).into());
                }
                AudioEvent::Error(e) => {
                    log::info!("METRIC: error={}", e);
                    win.set_status_text(format!("Error: {}", e).into());
                    win.set_is_playing(false);
                }
                AudioEvent::Finished => {
                    win.set_status_text("Finished".into());
                    win.set_is_playing(false);
                }
            });
        }
    });

    let eng = engine.clone();
    window.on_play_pause_clicked(move || {
        if eng.is_playing() {
            eng.send_command(AudioCommand::Pause);
        } else {
            eng.send_command(AudioCommand::Resume);
        }
    });

    if let Some(ref fp) = file_to_play {
        let fname = Path::new(fp)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("File");
        window.set_track_title(fname.into());
        window.set_status_text("Loading...".into());
        *file_open_time.lock().unwrap() = Some(Instant::now());
        engine.send_command(AudioCommand::Play(fp.clone()));
    } else {
        window.set_track_title("Pass WAV file as argument".into());
        window.set_status_text("Stopped".into());
    }

    log::info!("METRIC: window_visible={:.2?}", start.elapsed());

    window.run().unwrap();

    log::info!("Clean shutdown...");
    engine.send_command(AudioCommand::Stop);
    std::thread::sleep(std::time::Duration::from_millis(100));
    log::info!("Done.");
}
