#![windows_subsystem = "windows"]

use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Instant;

use slint::ComponentHandle;

use diavlo_player::audio::engine::{AudioCommand, AudioEngine, AudioEvent, PlaybackState};
use diavlo_player::platform::{registry, windows};

slint::include_modules!();

fn main() {
    let _ = simplelog::SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default());

    let args: Vec<String> = std::env::args().collect();
    let file_to_play: Option<String> = if args.len() > 1 {
        let p = &args[1];
        if Path::new(p).exists() {
            log::info!("CLI file: {}", p);
            Some(p.clone())
        } else { None }
    } else { None };

    let window = AppWindow::new().expect("Failed to create Slint window");
    let window_weak = window.as_weak();

    // Apply frameless glass window
    if let Some(hwnd) = unsafe { windows::hwnd_from_slint(window.window()) } {
        unsafe { windows::apply_frameless_glass(hwnd); }
        log::info!("Frameless glass applied");

        window.on_minimize_window(move || unsafe { windows::minimize_window(hwnd) });
        window.on_maximize_window(move || unsafe { windows::maximize_restore_window(hwnd) });
        window.on_close_window(move || unsafe { windows::close_window(hwnd) });
    }

    // Audio engine
    let (event_tx, event_rx) = channel();
    let engine = Arc::new(AudioEngine::new(event_tx));
    let start = Instant::now();

    let w_events = window_weak.clone();
    std::thread::spawn(move || {
        for event in event_rx {
            let w = w_events.clone();
            let _ = w.upgrade_in_event_loop(move |win| match event {
                AudioEvent::StateChanged(s) => {
                    win.set_is_playing(matches!(s, PlaybackState::Playing));
                    let label = match s {
                        PlaybackState::Idle => "Stopped",
                        PlaybackState::Loading => "Loading...",
                        PlaybackState::Playing => "Playing",
                        PlaybackState::Paused => "Paused",
                        PlaybackState::Ended => "Finished",
                        PlaybackState::Error => "Error",
                    };
                    win.set_status_text(label.into());
                }
                AudioEvent::DurationLoaded(dur) => {
                    win.set_total_secs(dur as f32);
                }
                AudioEvent::StreamReady => {
                    log::info!("Stream ready: {:.2?}", start.elapsed());
                }
                AudioEvent::FirstSampleEnqueued => {
                    log::info!("First sample: {:.2?}", start.elapsed());
                }
                AudioEvent::PositionUpdated(pos) => {
                    win.set_position_secs(pos as f32);
                }
                AudioEvent::Error(e) => {
                    win.set_status_text(format!("Error: {}", e).into());
                    win.set_is_playing(false);
                }
                AudioEvent::Finished => {
                    win.set_is_playing(false);
                    win.set_status_text("Finished".into());
                }
            });
        }
    });

    // Play/Pause callback
    let eng_pb = engine.clone();
    window.on_play_pause_clicked(move || {
        if eng_pb.is_playing() { eng_pb.send_command(AudioCommand::Pause); }
        else { eng_pb.send_command(AudioCommand::Resume); }
    });

    // File chooser
    let eng_choose = engine.clone();
    let w_choose = window_weak.clone();
    window.on_choose_file(move || {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Audio", &["wav", "flac", "mp3", "aac", "m4a", "ogg", "opus", "aiff"])
            .pick_file()
        {
            let fp = path.to_string_lossy().to_string();
            if let Some(w) = w_choose.upgrade() {
                w.set_track_title(Path::new(&fp).file_stem().and_then(|n| n.to_str()).unwrap_or("Unknown").into());
                w.set_has_track(true);
                w.set_status_text("Loading...".into());
                eng_choose.send_command(AudioCommand::Play(fp));
            }
        }
    });

    // Mute toggle
    let w_mute = window_weak.clone();
    window.on_mute_toggled(move || {
        if let Some(w) = w_mute.upgrade() { w.set_is_muted(!w.get_is_muted()); }
    });

    // Set as default player
    window.on_set_as_default(|| {
        match unsafe { registry::register_file_associations() } {
            Ok(()) => log::info!("File associations registered"),
            Err(e) => log::error!("Registry error: {e}"),
        }
        unsafe { registry::open_default_apps_settings(); }
    });

    window.on_seeked(|_| {});
    window.on_volume_changed(|_| {});

    // Initial CLI file
    if let Some(ref fp) = file_to_play {
        window.set_track_title(Path::new(fp).file_stem().and_then(|n| n.to_str()).unwrap_or("Unknown").into());
        window.set_has_track(true);
        window.set_status_text("Loading...".into());
        engine.send_command(AudioCommand::Play(fp.clone()));
    }

    log::info!("Window visible: {:.2?}", start.elapsed());
    window.run().unwrap();

    log::info!("Shutdown...");
    engine.send_command(AudioCommand::Stop);
    std::thread::sleep(std::time::Duration::from_millis(100));
}
