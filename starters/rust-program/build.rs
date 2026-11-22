// SPDX-License-Identifier: GPL-3.0-or-later
//! Gives the Windows executable its icon and its file properties. The file version is the version in
//! Cargo.toml (winresource takes it from there), and the packager refuses an exe whose file version
//! is not the version.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    if std::path::Path::new("assets/icon.ico").exists() {
        resource.set_icon("assets/icon.ico");
    }
    resource.set("ProductName", "OpenExample");
    resource.set("FileDescription", "OpenExample");
    resource.set("LegalCopyright", "Copyright (C) 2026 <Author> and OpenExample contributors. GPL-3.0-or-later.");
    if let Err(error) = resource.compile() {
        println!("cargo:warning=the executable has no icon and no file version: {error}");
    }
}
