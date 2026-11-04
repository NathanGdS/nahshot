//! Low-level keyboard hook that takes Print Screen away from Windows.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_SNAPSHOT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

/// What the hook calls on a bare Print Screen press. The hook proc is a plain
/// function, so it reaches its owner through this slot.
static ON_PRESS: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);
/// Set while a swallowed Print Screen press is still held.
static SWALLOWING: AtomicBool = AtomicBool::new(false);

/// An installed `WH_KEYBOARD_LL` hook; dropping it uninstalls the hook.
///
/// At most one exists at a time. It must be created on, and dropped from, a thread
/// that pumps messages, since Windows calls the hook on that thread.
pub(crate) struct PrintScreenHook(HHOOK);

impl PrintScreenHook {
    /// Installs the hook; `on_press` runs (on this thread) for each bare Print Screen press.
    pub(crate) fn install(on_press: Arc<dyn Fn() + Send + Sync>) -> io::Result<Self> {
        *ON_PRESS.lock().unwrap_or_else(|p| p.into_inner()) = Some(on_press);
        SWALLOWING.store(false, Ordering::Relaxed);
        // SAFETY: `hook_proc` matches HOOKPROC and lives for the whole program; the
        // module handle of the running executable is always valid.
        let hook = unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(hook_proc),
                GetModuleHandleW(std::ptr::null()),
                0,
            )
        };
        if hook.is_null() {
            let err = io::Error::last_os_error();
            ON_PRESS.lock().unwrap_or_else(|p| p.into_inner()).take();
            return Err(err);
        }
        Ok(PrintScreenHook(hook))
    }
}

impl Drop for PrintScreenHook {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a successful SetWindowsHookExW and is unhooked once.
        unsafe { UnhookWindowsHookEx(self.0) };
        ON_PRESS.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

/// Kept minimal: Windows silently removes low-level hooks that answer too slowly.
unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, `lparam` points to a valid KBDLLHOOKSTRUCT.
        let vk = unsafe { (*(lparam as *const KBDLLHOOKSTRUCT)).vkCode };
        let down = matches!(wparam as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        let mut swallowing = SWALLOWING.load(Ordering::Relaxed);
        let v = verdict(
            vk,
            down,
            vk == u32::from(VK_SNAPSHOT) && modifiers_held(),
            &mut swallowing,
        );
        SWALLOWING.store(swallowing, Ordering::Relaxed);
        match v {
            Verdict::Pass => {}
            Verdict::Swallow => return 1,
            Verdict::Capture => {
                if let Some(on_press) = ON_PRESS.lock().unwrap_or_else(|p| p.into_inner()).as_ref()
                {
                    on_press();
                }
                return 1;
            }
        }
    }
    // SAFETY: forwarding the unchanged arguments, as the hook contract requires.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

/// Whether Ctrl, Alt, Shift or Win is physically down right now.
fn modifiers_held() -> bool {
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .into_iter()
        // SAFETY: GetAsyncKeyState has no preconditions. The high bit means "down".
        .any(|vk| unsafe { GetAsyncKeyState(i32::from(vk)) } < 0)
}

/// What the hook does with one keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// Let Windows and other apps see it.
    Pass,
    /// Hide it from Windows.
    Swallow,
    /// Hide it from Windows and open the overlay.
    Capture,
}

/// Decides the fate of a keystroke. `swallowing` remembers that a Print Screen
/// press was taken, so its auto-repeats and release are taken too, while a press
/// with Ctrl/Alt/Shift/Win held (e.g. Alt+Print Screen) is left to Windows whole.
fn verdict(vk: u32, down: bool, modifiers_held: bool, swallowing: &mut bool) -> Verdict {
    if vk != u32::from(VK_SNAPSHOT) {
        Verdict::Pass
    } else if !down {
        if std::mem::take(swallowing) {
            Verdict::Swallow
        } else {
            Verdict::Pass
        }
    } else if *swallowing {
        Verdict::Swallow
    } else if modifiers_held {
        Verdict::Pass
    } else {
        *swallowing = true;
        Verdict::Capture
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_A;

    const SNAP: u32 = VK_SNAPSHOT as u32;

    #[test]
    fn bare_print_screen_press_and_release_are_swallowed() {
        let mut s = false;
        assert_eq!(verdict(SNAP, true, false, &mut s), Verdict::Capture);
        assert_eq!(
            verdict(SNAP, true, false, &mut s),
            Verdict::Swallow,
            "auto-repeat"
        );
        assert_eq!(verdict(SNAP, false, false, &mut s), Verdict::Swallow);
        assert!(!s);
    }

    #[test]
    fn print_screen_with_modifiers_is_left_to_windows() {
        let mut s = false;
        assert_eq!(verdict(SNAP, true, true, &mut s), Verdict::Pass);
        assert_eq!(verdict(SNAP, false, true, &mut s), Verdict::Pass);
        // Modifier released before the key: the release still belongs to Windows.
        assert_eq!(verdict(SNAP, true, true, &mut s), Verdict::Pass);
        assert_eq!(verdict(SNAP, false, false, &mut s), Verdict::Pass);
    }

    #[test]
    fn other_keys_pass_through() {
        let mut s = false;
        assert_eq!(verdict(VK_A as u32, true, false, &mut s), Verdict::Pass);
        assert_eq!(verdict(VK_A as u32, false, false, &mut s), Verdict::Pass);
        assert!(!s);
    }
}
