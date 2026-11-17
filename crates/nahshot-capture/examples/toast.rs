//! Manual harness for the "print salvo" toast.
//!
//! Shows a light "Copied" toast, then a dark "Saved" one that must replace it, and
//! checks on the way that focus never moves, that screen grabs never contain it, that only one toast window exists at a
//! time and that none is left at the end. Saves the bottom-right corner of every
//! monitor as `toast-live-N-M.png` while each toast is on screen.
//! Run with `cargo run -p nahshot-capture --example toast`.

use image::imageops::crop_imm;
use nahshot_capture::grab::grab_all;
use nahshot_capture::toast::{show, ToastKind};
use std::time::Duration;
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowExW, GetForegroundWindow};

fn toast_windows() -> usize {
    let class: Vec<u16> = "nahshot-toast".encode_utf16().chain(Some(0)).collect();
    let mut n = 0;
    let mut after = std::ptr::null_mut();
    loop {
        // SAFETY: walks top-level windows by class name; `class` is NUL-terminated.
        after = unsafe {
            FindWindowExW(
                std::ptr::null_mut(),
                after,
                class.as_ptr(),
                std::ptr::null(),
            )
        };
        if after.is_null() {
            return n;
        }
        n += 1;
    }
}

/// Saves each monitor's bottom-right corner and returns how many of its pixels have
/// the dark card's exact color (#2a2b30): a toast that leaks into captures shows up.
fn snap(step: usize) -> usize {
    let mut card_pixels = 0;
    for (i, shot) in grab_all().expect("grab").iter().enumerate() {
        let (w, h) = shot.image.dimensions();
        let (cw, ch) = (w.min(700), h.min(320));
        let corner = crop_imm(&shot.image, w - cw, h - ch, cw, ch).to_image();
        card_pixels += corner
            .pixels()
            .filter(|p| p.0[..3] == [0x2a, 0x2b, 0x30])
            .count();
        let path = format!("toast-live-{step}-{i}.png");
        corner.save(&path).expect("save");
        println!("saved {path}");
    }
    card_pixels
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // SAFETY: plain query.
    let focus = || unsafe { GetForegroundWindow() } as usize;
    let before = focus();
    let mut ok = true;

    show(ToastKind::Copied, false);
    std::thread::sleep(Duration::from_millis(900));
    snap(1);
    let n = toast_windows();
    println!(
        "1st toast: {n} window(s), focus kept: {}",
        focus() == before
    );
    ok &= n == 1 && focus() == before;

    show(ToastKind::Saved, true);
    std::thread::sleep(Duration::from_millis(900));
    let leaked = snap(2);
    println!("dark card pixels in the screen grab: {leaked}");
    ok &= leaked < 50;
    let n = toast_windows();
    println!(
        "2nd toast: {n} window(s), focus kept: {}",
        focus() == before
    );
    ok &= n == 1 && focus() == before;

    std::thread::sleep(Duration::from_secs(3));
    let n = toast_windows();
    println!("after the animation: {n} window(s)");
    ok &= n == 0;

    println!(
        "{}",
        if ok {
            "ALL CHECKS PASSED"
        } else {
            "CHECKS FAILED"
        }
    );
    std::process::exit(if ok { 0 } else { 1 });
}
