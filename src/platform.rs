// SPDX-License-Identifier: GPL-3.0-or-later
//! Every call to Windows but the one in `open3d_settings.rs`. `unsafe` is allowed by name in each
//! function here, because each calls Windows itself.

/// Connects the output to the terminal the program was started from, if there is one, so that
/// `--version` can be read there.
// `unsafe` is allowed here because this function calls Windows itself (AttachConsole).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn attach_console() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(process: u32) -> i32;
    }
    const PARENT: u32 = u32::MAX;
    // SAFETY: a plain Win32 call without pointers; failing (no parent console) changes nothing.
    unsafe {
        AttachConsole(PARENT);
    }
}

#[cfg(not(windows))]
pub fn attach_console() {}

/// Shows a text in a Windows message box and waits until it is closed.
// `unsafe` is allowed here because this function calls Windows itself (MessageBoxW).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn message_box(title: &str, text: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut std::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    const MB_ICONINFORMATION: u32 = 0x40;
    let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide(title));
    // SAFETY: both strings end in a zero and live until the call returns; there is no owner window.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_ICONINFORMATION);
    }
}

#[cfg(not(windows))]
pub fn message_box(title: &str, text: &str) {
    println!("{title}\n{text}");
}

/// `n` random bytes from Windows' own generator, for the bridge's token.
// `unsafe` is allowed here because this function calls Windows itself (BCryptGenRandom).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn random_bytes(n: usize) -> Option<Vec<u8>> {
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(algorithm: *mut std::ffi::c_void, buffer: *mut u8, size: u32, flags: u32) -> i32;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 2;
    let mut out = vec![0u8; n];
    let size = u32::try_from(n).ok()?;
    // SAFETY: the buffer has `size` bytes and lives until the call returns; no algorithm handle is needed
    // with the system's preferred generator.
    let status = unsafe { BCryptGenRandom(std::ptr::null_mut(), out.as_mut_ptr(), size, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    (status == 0).then_some(out)
}

#[cfg(windows)]
#[repr(C)]
struct DataBlob {
    size: u32,
    data: *mut u8,
}

// `unsafe` is allowed here because these declare Windows' own functions: DPAPI, for `protect` and `unprotect`.
#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "crypt32")]
unsafe extern "system" {
    fn CryptProtectData(
        input: *const DataBlob,
        description: *const u16,
        entropy: *const DataBlob,
        reserved: *mut std::ffi::c_void,
        prompt: *mut std::ffi::c_void,
        flags: u32,
        output: *mut DataBlob,
    ) -> i32;
    fn CryptUnprotectData(
        input: *const DataBlob,
        description: *mut *mut u16,
        entropy: *const DataBlob,
        reserved: *mut std::ffi::c_void,
        prompt: *mut std::ffi::c_void,
        flags: u32,
        output: *mut DataBlob,
    ) -> i32;
}

// `unsafe` is allowed here because these declare Windows' own functions: LocalFree, which frees what DPAPI allocates.
#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}

/// Encrypts data for the Windows user who runs the program (DPAPI): only that user, on any of their computers
/// with a roaming profile, can decrypt it.
// `unsafe` is allowed here because this function calls Windows itself (CryptProtectData).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 1;
    let input = DataBlob { size: u32::try_from(data.len()).ok()?, data: data.as_ptr().cast_mut() };
    let mut output = DataBlob { size: 0, data: std::ptr::null_mut() };
    let null = std::ptr::null_mut();
    // SAFETY: `input` points at `data`, which outlives the call and is only read; Windows allocates `output`,
    // which is copied and then freed with LocalFree.
    unsafe {
        if CryptProtectData(&input, std::ptr::null(), std::ptr::null(), null, null, CRYPTPROTECT_UI_FORBIDDEN, &mut output) == 0 {
            return None;
        }
        let out = std::slice::from_raw_parts(output.data, output.size as usize).to_vec();
        LocalFree(output.data.cast());
        Some(out)
    }
}

/// Decrypts what [`protect`] encrypted; `None` when it is not this user's, or not DPAPI data at all.
// `unsafe` is allowed here because this function calls Windows itself (CryptUnprotectData).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 1;
    let input = DataBlob { size: u32::try_from(data.len()).ok()?, data: data.as_ptr().cast_mut() };
    let mut output = DataBlob { size: 0, data: std::ptr::null_mut() };
    let null = std::ptr::null_mut();
    // SAFETY: as in `protect`: `input` is only read, and Windows' `output` is copied and freed with LocalFree.
    unsafe {
        if CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), null, null, CRYPTPROTECT_UI_FORBIDDEN, &mut output) == 0 {
            return None;
        }
        let out = std::slice::from_raw_parts(output.data, output.size as usize).to_vec();
        LocalFree(output.data.cast());
        Some(out)
    }
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}

// `unsafe` is allowed here because these declare Windows' own functions: the window under the pointer, for `process_under_cursor` and `allow_foreground`.
#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetCursorPos(point: *mut Point) -> i32;
    fn WindowFromPoint(point: Point) -> *mut std::ffi::c_void;
    fn GetAncestor(window: *mut std::ffi::c_void, flags: u32) -> *mut std::ffi::c_void;
    fn GetWindowThreadProcessId(window: *mut std::ffi::c_void, process: *mut u32) -> u32;
    fn AllowSetForegroundWindow(process: u32) -> i32;
}

/// The process that owns the window under the mouse pointer, wherever on the screen it is.
// `unsafe` is allowed here because this function calls Windows itself (GetCursorPos, WindowFromPoint,
// GetAncestor, GetWindowThreadProcessId).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn process_under_cursor() -> Option<u32> {
    const GA_ROOT: u32 = 2;
    let mut point = Point::default();
    // SAFETY: plain Win32 calls; `point` and `pid` live on the stack until they return, and a null window is
    // checked before it is used.
    unsafe {
        if GetCursorPos(&mut point) == 0 {
            return None;
        }
        let window = WindowFromPoint(point);
        if window.is_null() {
            return None;
        }
        let root = GetAncestor(window, GA_ROOT);
        let mut pid = 0u32;
        GetWindowThreadProcessId(if root.is_null() { window } else { root }, &mut pid);
        (pid != 0).then_some(pid)
    }
}

/// Lets a process bring its window to the front (3ds Max, when a model is dropped on it).
// `unsafe` is allowed here because this function calls Windows itself (AllowSetForegroundWindow).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn allow_foreground(pid: u32) {
    // SAFETY: a plain Win32 call without pointers; failing only leaves 3ds Max behind.
    unsafe {
        AllowSetForegroundWindow(pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_bytes_differ() {
        let (a, b) = (random_bytes(32).unwrap(), random_bytes(32).unwrap());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }

    #[test]
    fn protected_data_comes_back_only_whole() {
        let secret = b"the bridge token";
        let sealed = protect(secret).unwrap();
        assert_ne!(sealed.as_slice(), secret);
        assert_eq!(unprotect(&sealed).unwrap(), secret);
        assert_eq!(unprotect(b"not dpapi"), None);
    }
}
