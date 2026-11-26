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
    fn GetTopWindow(window: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn GetWindow(window: *mut std::ffi::c_void, command: u32) -> *mut std::ffi::c_void;
    fn IsWindowVisible(window: *mut std::ffi::c_void) -> i32;
    fn IsIconic(window: *mut std::ffi::c_void) -> i32;
}

/// Where the mouse pointer is on the screen, in pixels.
// `unsafe` is allowed here because this function calls Windows itself (GetCursorPos).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut point = Point::default();
    // SAFETY: a plain Win32 call; `point` lives on the stack until it returns.
    let ok = unsafe { GetCursorPos(&mut point) != 0 };
    ok.then_some((point.x, point.y))
}

/// Of these processes, the one whose window is the highest on the screen.
// `unsafe` is allowed here because this function calls Windows itself (GetTopWindow, GetWindow, IsWindowVisible,
// IsIconic, GetWindowThreadProcessId).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn front_pid(pids: &[u32]) -> Option<u32> {
    const GW_HWNDNEXT: u32 = 2;
    // SAFETY: plain Win32 calls; a window that closes meanwhile only makes them fail, the pid is on the stack, and
    // the list is walked a bounded number of times.
    unsafe {
        let mut window = GetTopWindow(std::ptr::null_mut());
        for _ in 0..10_000 {
            if window.is_null() {
                return None;
            }
            if IsWindowVisible(window) != 0 && IsIconic(window) == 0 {
                let mut pid = 0u32;
                GetWindowThreadProcessId(window, &mut pid);
                if pids.contains(&pid) {
                    return Some(pid);
                }
            }
            window = GetWindow(window, GW_HWNDNEXT);
        }
    }
    None
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

// `unsafe` is allowed here because these declare Windows' own functions: the clipboard, for `clipboard_dib`.
#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(window: *mut std::ffi::c_void) -> i32;
    fn CloseClipboard() -> i32;
    fn GetClipboardData(format: u32) -> *mut std::ffi::c_void;
}

// `unsafe` is allowed here because these declare Windows' own functions: the clipboard's memory, for `clipboard_dib`.
#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalLock(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    fn GlobalUnlock(memory: *mut std::ffi::c_void) -> i32;
    fn GlobalSize(memory: *mut std::ffi::c_void) -> usize;
}

/// The image in the clipboard, as a DIB, when it holds one (the browsers' "Copy image" leaves it).
// `unsafe` is allowed here because this function calls Windows itself (the clipboard and its memory).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn clipboard_dib() -> Option<Vec<u8>> {
    const CF_DIB: u32 = 8;
    // SAFETY: the clipboard is open while its memory is read, which is copied up to the size Windows gives for
    // it; a null handle or pointer is checked before it is used, and both are let go before returning.
    unsafe {
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let memory = GetClipboardData(CF_DIB);
        let mut out = None;
        if !memory.is_null() {
            let data = GlobalLock(memory);
            if !data.is_null() {
                out = Some(std::slice::from_raw_parts(data.cast::<u8>(), GlobalSize(memory)).to_vec());
                GlobalUnlock(memory);
            }
        }
        CloseClipboard();
        out
    }
}

// `unsafe` is allowed here because these declare Windows' own functions: the drives' shares, for `mapped_shares`.
#[cfg(windows)]
#[allow(unsafe_code)]
#[link(name = "mpr")]
unsafe extern "system" {
    fn WNetGetConnectionW(local: *const u16, remote: *mut u16, length: *mut u32) -> u32;
}

/// The network shares the drive letters of this computer are.
// `unsafe` is allowed here because this function calls Windows itself (WNetGetConnectionW).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn mapped_shares() -> Vec<String> {
    let share = |letter: u8| {
        let local = [u16::from(letter), u16::from(b':'), 0];
        let mut remote = [0u16; 1024];
        let mut length = 1024u32;
        // SAFETY: `local` ends in a zero, `remote` has the `length` letters it says, and both outlive the call.
        let status = unsafe { WNetGetConnectionW(local.as_ptr(), remote.as_mut_ptr(), &mut length) };
        let end = remote.iter().position(|c| *c == 0).unwrap_or(0);
        (status == 0 && end > 0).then(|| String::from_utf16_lossy(&remote[..end]))
    };
    (b'A'..=b'Z').filter_map(share).collect()
}

/// Shows a folder in Explorer, or a file selected in its folder.
pub fn reveal(path: &std::path::Path) {
    let root = std::env::var_os("SystemRoot").map_or_else(|| std::path::PathBuf::from("C:/Windows"), std::path::PathBuf::from);
    let mut command = std::process::Command::new(root.join("explorer.exe"));
    // Explorer reads its own command line: the path goes in quotes, which no path of Windows has in it
    let text = path.display().to_string().replace('"', "");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.raw_arg(if path.is_dir() { format!("\"{text}\"") } else { format!("/select,\"{text}\"") });
    }
    let _ = command.spawn(); // without Explorer there is nothing to show
}

/// Opens a document with the program Windows has for it. Only for kinds of file that are not run.
pub fn open_document(path: &std::path::Path) {
    let root = std::env::var_os("SystemRoot").map_or_else(|| std::path::PathBuf::from("C:/Windows"), std::path::PathBuf::from);
    let mut command = std::process::Command::new(root.join("explorer.exe"));
    let text = path.display().to_string().replace('"', "");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.raw_arg(format!("\"{text}\""));
    }
    let _ = command.spawn(); // without Explorer there is nothing to show
}

/// Whether a 3ds Max is open on this computer (a bridge installed now starts with its next start).
pub fn max_running() -> bool {
    let mut command = std::process::Command::new(system_program("tasklist.exe"));
    command.args(["/FI", "IMAGENAME eq 3dsmax.exe", "/NH"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // no console window
    }
    command.output().is_ok_and(|out| String::from_utf8_lossy(&out.stdout).to_lowercase().contains("3dsmax.exe"))
}

/// A program of Windows itself, by its whole path, so that no other program of that name runs instead.
pub fn system_program(name: &str) -> std::path::PathBuf {
    let root = std::env::var_os("SystemRoot").map_or_else(|| std::path::PathBuf::from("C:/Windows"), std::path::PathBuf::from);
    root.join("System32").join(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shares_of_the_drives_are_network_paths() {
        assert!(mapped_shares().iter().all(|s| s.starts_with("\\\\")));
    }

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
