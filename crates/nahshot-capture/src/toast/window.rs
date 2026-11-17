//! The toast's native window: a click-through layered popup on a short-lived thread.
//!
//! Each toast owns a thread that creates the window, animates it frame by frame with
//! `UpdateLayeredWindow` (per-pixel alpha, so rounded corners and a real shadow) and
//! destroys it when the animation ends or a newer toast takes over. Nothing outlives
//! the toast.

use crate::geom::PxRect;
use crate::toast::model::{frame_at, layout, palette, Palette, ToastKind};
use crate::toast::paint::{paint, Fonts};
use log::{info, warn};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT, SIZE};
use windows_sys::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW,
    MonitorFromPoint, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    MDT_EFFECTIVE_DPI,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GWL_EXSTYLE;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetCursorPos, PeekMessageW,
    RegisterClassW, SetWindowDisplayAffinity, SetWindowLongPtrW, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, MSG, PM_REMOVE, SW_SHOWNOACTIVATE, ULW_ALPHA, WDA_EXCLUDEFROMCAPTURE,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};

/// Time between animation frames (about 60 per second).
const FRAME: Duration = Duration::from_millis(16);

/// Which toast is the live one. Claiming a new generation tells the older toast's
/// thread to close its window on its next frame.
#[derive(Default)]
pub(crate) struct Current(AtomicU64);

impl Current {
    /// Makes a new toast the live one and returns its generation.
    pub fn claim(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Ends the live toast, if any, without starting a new one.
    pub fn dismiss(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.0.load(Ordering::SeqCst) == generation
    }
}

static CURRENT: Current = Current(AtomicU64::new(0));

/// Closes the toast on screen, if any, within a frame. Called when a new capture
/// starts, so the toast does not sit over the overlay.
pub fn dismiss() {
    CURRENT.dismiss();
}

/// Shows a toast for `kind` without blocking, replacing any toast on screen.
/// Failures only reach the log: a missing toast must never break a capture.
pub fn show(kind: ToastKind, dark: bool) {
    info!("toast: {kind:?} (dark: {dark})");
    let generation = CURRENT.claim();
    let spawned = std::thread::Builder::new()
        .name("nahshot-toast".into())
        .spawn(move || run(kind, palette(dark), generation));
    if let Err(e) = spawned {
        warn!("no thread for the toast: {e}");
    }
}

/// The toast thread: build the window, animate it, tear it down.
fn run(kind: ToastKind, pal: Palette, generation: u64) {
    // SAFETY: only changes this thread's DPI mode, so coordinates below are physical.
    unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    let fonts = match Fonts::load() {
        Ok(f) => f,
        Err(e) => return warn!("toast fonts: {e}"),
    };
    let Some((work, scale)) = cursor_monitor() else {
        return warn!("toast: no monitor under the cursor");
    };
    let rect = layout(work, scale);
    let Some(mut window) = Window::create(rect) else {
        return;
    };
    let start = Instant::now();
    let mut shown = false;
    while CURRENT.is_current(generation) {
        let Some(frame) = frame_at(start.elapsed()) else {
            break;
        };
        pump_messages();
        let pixmap = paint(kind, &pal, scale, &frame, &fonts);
        let x = rect.x + (frame.offset_x * scale).round() as i32;
        if !window.present(pixmap.data(), x, rect.y) {
            break;
        }
        if !shown {
            // SAFETY: `window.hwnd` is a live window owned by this thread.
            unsafe { ShowWindow(window.hwnd, SW_SHOWNOACTIVATE) };
            shown = true;
        }
        std::thread::sleep(FRAME);
    }
}

/// Work area and DPI scale of the monitor under the cursor.
fn cursor_monitor() -> Option<(PxRect, f32)> {
    let mut pt = POINT { x: 0, y: 0 };
    // SAFETY: plain out-parameters on the stack.
    unsafe {
        if GetCursorPos(&mut pt) == 0 {
            return None;
        }
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }
        let (mut dpi, mut dpi_y) = (96u32, 96u32);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi, &mut dpi_y) < 0 {
            dpi = 96;
        }
        Some((to_rect(info.rcWork), dpi as f32 / 96.0))
    }
}

fn to_rect(r: RECT) -> PxRect {
    PxRect {
        x: r.left,
        y: r.top,
        w: (r.right - r.left).max(0) as u32,
        h: (r.bottom - r.top).max(0) as u32,
    }
}

/// Delivers whatever messages reached this thread's window.
fn pump_messages() {
    // SAFETY: standard message loop over this thread's queue.
    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

const CLASS: &str = "nahshot-toast";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Registers the window class once per process; `false` if it cannot be.
fn register_class() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        // Leaked on purpose: the class name must live as long as the class.
        let name: &'static [u16] = Box::leak(wide(CLASS).into_boxed_slice());
        // SAFETY: the struct is fully initialized and `name` is 'static.
        let atom = unsafe {
            let mut class: WNDCLASSW = std::mem::zeroed();
            class.lpfnWndProc = Some(DefWindowProcW);
            class.hInstance = GetModuleHandleW(std::ptr::null());
            class.lpszClassName = name.as_ptr();
            RegisterClassW(&class)
        };
        if atom == 0 {
            warn!("toast: window class registration failed");
        }
        atom != 0
    })
}

/// The layered window plus the memory DC/DIB its frames are drawn into.
struct Window {
    hwnd: HWND,
    screen: HDC,
    mem: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    size: SIZE,
}

impl Window {
    fn create(rect: PxRect) -> Option<Window> {
        if !register_class() {
            return None;
        }
        let class = wide(CLASS);
        let (w, h) = (rect.w as i32, rect.h as i32);
        // SAFETY: every handle is checked before use and released in `Drop`.
        unsafe {
            let ex_style = WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT;
            // Created without WS_EX_LAYERED: Windows refuses display affinity on a
            // window that is already layered, but keeps it once the style is added.
            let hwnd = CreateWindowExW(
                ex_style,
                class.as_ptr(),
                class.as_ptr(),
                WS_POPUP,
                rect.x,
                rect.y,
                w,
                h,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            );
            if hwnd.is_null() {
                warn!("toast: window creation failed");
                return None;
            }
            // Keep the toast out of screen grabs, nahshot's own included: a print taken
            // while it is still showing must not contain it (Windows 10 2004+).
            if SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) == 0 {
                warn!("toast: cannot exclude the window from screen captures");
            }
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, (ex_style | WS_EX_LAYERED) as isize);
            let screen = GetDC(std::ptr::null_mut());
            let mem = CreateCompatibleDC(screen);
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down rows, like the pixmap
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..std::mem::zeroed()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                screen,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            let mut window = Window {
                hwnd,
                screen,
                mem,
                bitmap,
                old: std::ptr::null_mut(),
                bits: bits.cast(),
                size: SIZE { cx: w, cy: h },
            };
            if screen.is_null() || mem.is_null() || bitmap.is_null() || bits.is_null() {
                warn!("toast: drawing surface creation failed");
                return None; // `Drop` releases what was created
            }
            // Not `Window { old, ..window }`: the fields are Copy, so that would copy
            // them and then drop `window`, freeing everything the copy points at.
            window.old = SelectObject(mem, bitmap);
            Some(window)
        }
    }

    /// Shows `rgba` (premultiplied, the window's size) at `(x, y)`; `false` on failure.
    fn present(&mut self, rgba: &[u8], x: i32, y: i32) -> bool {
        let len = (self.size.cx * self.size.cy * 4) as usize;
        if rgba.len() != len {
            warn!("toast: frame size mismatch");
            return false;
        }
        // SAFETY: `bits` points at the DIB's `len` bytes, which nothing else touches.
        let dib = unsafe { std::slice::from_raw_parts_mut(self.bits, len) };
        for (dst, src) in dib
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(rgba.as_chunks::<4>().0)
        {
            dst.copy_from_slice(&[src[2], src[1], src[0], src[3]]); // RGBA → BGRA
        }
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let (dst, src) = (POINT { x, y }, POINT { x: 0, y: 0 });
        // SAFETY: all handles are live; the pointers reference locals.
        let ok = unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                self.screen,
                &dst,
                &self.size,
                self.mem,
                &src,
                0,
                &blend,
                ULW_ALPHA,
            )
        };
        if ok == 0 {
            warn!("toast: UpdateLayeredWindow failed");
        }
        ok != 0
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: each handle was created by `create` and is released exactly once.
        unsafe {
            if !self.old.is_null() {
                SelectObject(self.mem, self.old);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.mem.is_null() {
                DeleteDC(self.mem);
            }
            if !self.screen.is_null() {
                ReleaseDC(std::ptr::null_mut(), self.screen);
            }
            DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacing_closes_previous() {
        let slot = Current::default();
        let first = slot.claim();
        assert!(slot.is_current(first));
        let second = slot.claim();
        assert!(!slot.is_current(first));
        assert!(slot.is_current(second));
    }

    #[test]
    fn dismiss_ends_the_current_toast() {
        let slot = Current::default();
        let toast = slot.claim();
        slot.dismiss();
        assert!(!slot.is_current(toast));
    }
}
