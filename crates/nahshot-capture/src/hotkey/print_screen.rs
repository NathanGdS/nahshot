//! Low-level keyboard hook that takes Print Screen away from Windows.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_SNAPSHOT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG, PM_NOREMOVE,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_QUIT, WM_SYSKEYDOWN,
};

/// What the hook calls on a bare Print Screen press. The hook proc is a plain
/// function, so it reaches its owner through this slot.
static ON_PRESS: Mutex<Option<Arc<dyn Fn() + Send + Sync>>> = Mutex::new(None);
/// Set while a swallowed Print Screen press is still held.
static SWALLOWING: AtomicBool = AtomicBool::new(false);

/// An installed `WH_KEYBOARD_LL` hook; dropping it uninstalls the hook.
///
/// The hook lives on a thread of its own that does nothing but pump messages.
/// Windows calls a low-level hook on the thread that installed it and skips (and
/// eventually drops) a hook whose thread does not answer in time; on a thread that
/// also grabs the screen and owns windows, keystrokes were lost that way.
///
/// At most one exists at a time.
pub(crate) struct PrintScreenHook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl PrintScreenHook {
    /// Installs the hook; `on_press` runs (on the hook's thread) for each bare Print
    /// Screen press, so it must only post work, never block.
    pub(crate) fn install(on_press: Arc<dyn Fn() + Send + Sync>) -> io::Result<Self> {
        *ON_PRESS.lock().unwrap_or_else(|p| p.into_inner()) = Some(on_press);
        SWALLOWING.store(false, Ordering::Relaxed);
        let (tx, rx) = sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("nahshot-print-screen-hook".into())
            .spawn(move || run_hook_thread(&tx))?;
        let installed = rx
            .recv()
            .unwrap_or_else(|_| Err(io::Error::other("hook thread exited before installing")));
        match installed {
            Ok(thread_id) => Ok(PrintScreenHook {
                thread_id,
                thread: Some(thread),
            }),
            Err(e) => {
                let _ = thread.join();
                ON_PRESS.lock().unwrap_or_else(|p| p.into_inner()).take();
                Err(e)
            }
        }
    }
}

impl Drop for PrintScreenHook {
    fn drop(&mut self) {
        // SAFETY: plain message post to a thread id; failure only means it is gone.
        unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        ON_PRESS.lock().unwrap_or_else(|p| p.into_inner()).take();
    }
}

/// Body of the hook thread: install, report, pump messages until `WM_QUIT`, unhook.
fn run_hook_thread(report: &SyncSender<io::Result<u32>>) {
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
        let _ = report.send(Err(io::Error::last_os_error()));
        return;
    }
    // SAFETY: no preconditions.
    let thread_id = unsafe { GetCurrentThreadId() };
    // SAFETY: an all-zero MSG is a valid value to be overwritten by PeekMessageW.
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // Create this thread's message queue before anyone posts WM_QUIT to it.
    // SAFETY: `msg` is a valid, writable MSG.
    unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE) };
    let _ = report.send(Ok(thread_id));
    // SAFETY: `msg` is a valid, writable MSG. GetMessageW returns 0 on WM_QUIT and
    // -1 on error; both end the loop.
    while unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) } > 0 {
        // SAFETY: `msg` was just filled by GetMessageW.
        unsafe { DispatchMessageW(&msg) };
    }
    // SAFETY: `hook` came from a successful SetWindowsHookExW and is unhooked once.
    unsafe { UnhookWindowsHookEx(hook) };
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
    fn hook_runs_on_its_own_thread_and_uninstalls_cleanly() {
        for _ in 0..2 {
            let hook = PrintScreenHook::install(Arc::new(|| {})).expect("install hook");
            // SAFETY: no preconditions.
            assert_ne!(hook.thread_id, unsafe { GetCurrentThreadId() });
            assert!(ON_PRESS.lock().unwrap().is_some());
            drop(hook);
            assert!(ON_PRESS.lock().unwrap().is_none());
        }
    }

    #[test]
    fn other_keys_pass_through() {
        let mut s = false;
        assert_eq!(verdict(VK_A as u32, true, false, &mut s), Verdict::Pass);
        assert_eq!(verdict(VK_A as u32, false, false, &mut s), Verdict::Pass);
        assert!(!s);
    }
}
