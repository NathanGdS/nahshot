//! The Windows "Print Screen opens Snipping Tool" toggle, stored under HKCU.

use std::io;
use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, WIN32_ERROR};
use windows_sys::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD,
    RRF_RT_REG_DWORD,
};

/// Key under HKCU that holds the Snipping Tool toggle.
pub const KEYBOARD_KEY: &str = r"Control Panel\Keyboard";
/// DWORD value: `1` lets Print Screen open Snipping Tool, `0` gives the key back to apps.
pub const SNIP_VALUE: &str = "PrintScreenKeyForSnippingEnabled";

/// Reads [`SNIP_VALUE`] under `HKCU\<key_path>`; `None` if the key or value is absent.
pub fn read_snipping(key_path: &str) -> io::Result<Option<u32>> {
    let (key, value) = (wide(key_path), wide(SNIP_VALUE));
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: both strings are NUL-terminated and outlive the call; `data`/`size`
    // describe a writable 4-byte buffer, which RRF_RT_REG_DWORD never exceeds.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            std::ptr::null_mut(),
            (&raw mut data).cast(),
            &mut size,
        )
    };
    match status {
        ERROR_SUCCESS => Ok(Some(data)),
        ERROR_FILE_NOT_FOUND => Ok(None),
        e => Err(os_error(e)),
    }
}

/// Writes [`SNIP_VALUE`] under `HKCU\<key_path>`, creating the key; `None` deletes the value.
pub fn write_snipping(key_path: &str, v: Option<u32>) -> io::Result<()> {
    let (key, value) = (wide(key_path), wide(SNIP_VALUE));
    // SAFETY: both strings are NUL-terminated and outlive the calls; `data` is a
    // readable 4-byte DWORD.
    let status = unsafe {
        match v {
            Some(data) => RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_DWORD,
                (&raw const data).cast(),
                size_of::<u32>() as u32,
            ),
            None => RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()),
        }
    };
    match status {
        ERROR_SUCCESS => Ok(()),
        ERROR_FILE_NOT_FOUND if v.is_none() => Ok(()),
        e => Err(os_error(e)),
    }
}

fn os_error(code: WIN32_ERROR) -> io::Error {
    io::Error::from_raw_os_error(code as i32)
}

/// NUL-terminated UTF-16 for Win32 `PCWSTR` arguments.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_ROOT: &str = r"Software\nahshot-test";
    const TEST_KEY: &str = r"Software\nahshot-test\Keyboard";

    /// Deletes `HKCU\Software\nahshot-test` even when an assertion fails.
    struct Cleanup;
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let root = wide(TEST_ROOT);
            // SAFETY: `root` is a NUL-terminated UTF-16 string that outlives the call.
            unsafe {
                windows_sys::Win32::System::Registry::RegDeleteTreeW(
                    windows_sys::Win32::System::Registry::HKEY_CURRENT_USER,
                    root.as_ptr(),
                )
            };
        }
    }

    #[test]
    fn registry_roundtrip() {
        let _cleanup = Cleanup;
        assert_eq!(read_snipping(TEST_KEY).unwrap(), None);
        write_snipping(TEST_KEY, Some(0)).unwrap();
        assert_eq!(read_snipping(TEST_KEY).unwrap(), Some(0));
        write_snipping(TEST_KEY, Some(1)).unwrap();
        assert_eq!(read_snipping(TEST_KEY).unwrap(), Some(1));
        write_snipping(TEST_KEY, None).unwrap();
        assert_eq!(read_snipping(TEST_KEY).unwrap(), None);
        // Deleting an absent value is not an error.
        write_snipping(TEST_KEY, None).unwrap();
    }
}
