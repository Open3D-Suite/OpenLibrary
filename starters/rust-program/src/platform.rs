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
