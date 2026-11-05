//! Manual harness for the capture overlay.
//!
//! Press Enter or Ctrl+Shift+S to capture; each result with an image is saved as
//! `out-N.png`. With `--intercept`, bare Print Screen opens the overlay too instead
//! of reaching Windows.
//! Run with `cargo run -p nahshot-capture --example overlay --release [-- --intercept]`.

use nahshot_capture::hotkey::HotkeyConfig;
use nahshot_capture::overlay::{spawn_overlay, CaptureResult};
use std::io::BufRead;
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let count = AtomicUsize::new(0);
    let overlay = spawn_overlay(move |r: CaptureResult| {
        let Some(image) = r.image else {
            println!("{:?} with image: None", r.action);
            return;
        };
        let path = format!("out-{}.png", count.fetch_add(1, Ordering::Relaxed));
        match image.save(&path) {
            Ok(()) => println!(
                "{:?} -> {path} ({}x{}, captured at {})",
                r.action,
                image.width(),
                image.height(),
                r.captured_at
            ),
            Err(e) => eprintln!("{:?}: saving {path} failed: {e}", r.action),
        }
    });
    let intercept_print_screen = std::env::args().any(|a| a == "--intercept");
    if let Err(e) = overlay.set_hotkeys(HotkeyConfig {
        intercept_print_screen,
        alt: Some("Ctrl+Shift+S".into()),
    }) {
        eprintln!("hotkeys: {e}");
    }
    println!(
        "Press Enter or Ctrl+Shift+S{} to capture; close stdin (Ctrl+Z, Enter) to quit.",
        if intercept_print_screen {
            " or Print Screen"
        } else {
            ""
        }
    );
    for line in std::io::stdin().lock().lines() {
        if line.is_err() {
            break;
        }
        overlay.trigger();
    }
}
