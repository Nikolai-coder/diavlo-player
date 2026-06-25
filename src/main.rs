#![windows_subsystem = "windows"]

use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use diavlo_player::audio::engine::{AudioCommand, AudioEngine, AudioEvent, PlaybackState};
use diavlo_player::platform::{registry, single_instance, windows};

slint::include_modules!();

fn format_time(secs: f64) -> String {
    if secs <= 0.0 {
        return "0:00".to_string();
    }
    let total = secs as u64;
    let m = total / 60;
    let s = total % 60;
    if s < 10 {
        format!("{m}:0{s}")
    } else {
        format!("{m}:{s}")
    }
}

fn main() {
    let _ = simplelog::SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default());

    // ── Parse CLI args ──────────────────────────────────
    let args: Vec<String> = std::env::args().collect();
    let file_arg: Option<String> = if args.len() > 1 {
        let p = &args[1];
        if Path::new(p).exists() {
            Some(p.clone())
        } else {
            None
        }
    } else {
        None
    };

    // ── Single-instance check ───────────────────────────
    let mutex = match single_instance::try_lock_single_instance() {
        Some(h) => h,  // We are the primary instance
        None => {
            // Another instance exists — send file path and exit
            if let Some(ref fp) = file_arg {
                if single_instance::send_path_to_primary(fp) {
                    log::info!("Sent file path to existing instance, exiting.");
                    return;
                }
            }
            // Failed to send — continue starting (fallback)
            log::warn!("Could not send to primary, starting new instance.");
            return;
        }
    };

    // ── Create window ───────────────────────────────────
    let window = AppWindow::new().expect("Failed to create Slint window");
    let window_weak = window.as_weak();

    // Apply frameless glass window
    let hwnd_opt = unsafe { windows::hwnd_from_slint(window.window()) };
    if let Some(hwnd) = hwnd_opt {
        unsafe { windows::apply_frameless_glass(hwnd); }
        log::info!("Frameless glass applied");

        window.on_minimize_window(move || unsafe { windows::minimize_window(hwnd) });
        window.on_maximize_window(move || unsafe { windows::maximize_restore_window(hwnd) });
        let hwnd_close = hwnd;
        window.on_close_window(move || unsafe { windows::close_window(hwnd_close) });
    }

    // ── Audio engine ────────────────────────────────────
    let (event_tx, event_rx) = channel();
    let engine = Arc::new(AudioEngine::new(event_tx));
    let start = Instant::now();

    // ── Event listener thread ───────────────────────────
    let w_events = window_weak.clone();
    std::thread::spawn(move || {
        for event in event_rx {
            let w = w_events.clone();
            let _ = w.upgrade_in_event_loop(move |win| match event {
                AudioEvent::StateChanged(s) => {
                    win.set_is_playing(matches!(s, PlaybackState::Playing));
                    let label = match s {
                        PlaybackState::Idle => "",
                        PlaybackState::Loading => "",
                        PlaybackState::Playing => "",
                        PlaybackState::Paused => "",
                        PlaybackState::Ended => "",
                        PlaybackState::Error => "Error",
                    };
                    win.set_status_text(label.into());
                }
                AudioEvent::DurationLoaded(dur) => {
                    win.set_total_secs(dur as f32);
                    win.set_duration_text(format_time(dur).into());
                }
                AudioEvent::MetadataLoaded { title, artist, album } => {
                    win.set_track_title(title.into());
                    win.set_artist_name(artist.into());
                    win.set_album_name(album.into());
                    win.set_has_track(true);
                }
                AudioEvent::StreamReady => {
                    log::info!("Stream ready: {:.2?}", start.elapsed());
                }
                AudioEvent::FirstSampleEnqueued => {
                    log::info!("First sample: {:.2?}", start.elapsed());
                }
                AudioEvent::PositionUpdated(pos) => {
                    win.set_position_secs(pos as f32);
                    win.set_position_text(format_time(pos).into());
                }
                AudioEvent::Error(e) => {
                    log::error!("Audio error: {}", e);
                }
                AudioEvent::Finished => {
                    win.set_is_playing(false);
                }
            });
        }
    });

    // ── Pipe server for single-instance IPC ─────────────
    let w_pipe = window_weak.clone();
    let engine_pipe = engine.clone();
    let pipe_handle = single_instance::start_pipe_server();
    if let Some(pipe) = pipe_handle {
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(200));
                if let Some(path) = single_instance::poll_pipe(pipe) {
                    log::info!("Received file from secondary instance: {}", path);
                    let engine_ref = engine_pipe.clone();
                    let w = w_pipe.clone();
                    let _ = w.upgrade_in_event_loop(move |win| {
                        win.set_has_track(true);
                        win.set_status_text("".into());
                        engine_ref.send_command(AudioCommand::Play(path));
                    });
                    // Bring to front
                    if let Some(hwnd) = unsafe { windows::hwnd_from_slint(w.upgrade().unwrap().window()) } {
                        single_instance::bring_to_front(hwnd);
                    }
                }
            }
        });
    }

    // ── Callbacks ───────────────────────────────────────

    // Play/Pause
    let eng_pb = engine.clone();
    window.on_play_pause_clicked(move || {
        if eng_pb.is_playing() {
            eng_pb.send_command(AudioCommand::Pause);
        } else {
            eng_pb.send_command(AudioCommand::Resume);
        }
    });

    // File chooser
    let eng_choose = engine.clone();
    let w_choose = window_weak.clone();
    window.on_choose_file(move || {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Audio",
                &["wav", "flac", "mp3", "aac", "m4a", "ogg", "opus", "aiff"],
            )
            .pick_file()
        {
            let fp = path.to_string_lossy().to_string();
            if let Some(w) = w_choose.upgrade() {
                w.set_has_track(true);
                w.set_status_text("".into());
                eng_choose.send_command(AudioCommand::Play(fp));
            }
        }
    });

    // Mute toggle
    let w_mute = window_weak.clone();
    window.on_mute_toggled(move || {
        if let Some(w) = w_mute.upgrade() {
            w.set_is_muted(!w.get_is_muted());
        }
    });

    // Settings gear → Set as default
    window.on_set_as_default(|| {
        match unsafe { registry::register_file_associations() } {
            Ok(()) => log::info!("File associations registered"),
            Err(e) => log::error!("Registry error: {e}"),
        }
        unsafe { registry::open_default_apps_settings(); }
    });

    window.on_seeked(|_| {});
    window.on_volume_changed(|_| {});

    // ── Auto-play if file argument provided ─────────────
    if let Some(ref fp) = file_arg {
        log::info!("Auto-playing: {}", fp);
        window.set_has_track(true);
        engine.send_command(AudioCommand::Play(fp.clone()));
    }

    log::info!("Window visible: {:.2?}", start.elapsed());
    window.run().unwrap();

    // ── Cleanup ─────────────────────────────────────────
    log::info!("Shutdown...");
    engine.send_command(AudioCommand::Stop);
    std::thread::sleep(Duration::from_millis(100));
    if mutex != 0 {
        unsafe {
            #[link(name = "kernel32")]
            extern "system" {
                fn CloseHandle(hObject: isize) -> i32;
            }
            CloseHandle(mutex);
        }
    }
}
