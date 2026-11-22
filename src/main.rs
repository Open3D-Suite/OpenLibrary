// SPDX-License-Identifier: GPL-3.0-or-later
// No console window behind the program; a terminal that starts it with arguments is attached below.
#![windows_subsystem = "windows"]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => openlibrary::run(),
        Some("--version" | "-V") => {
            openlibrary::platform::attach_console();
            println!("{}", openlibrary::version_line());
        }
        Some(other) => {
            openlibrary::platform::attach_console();
            eprintln!("OpenLibrary: unknown argument '{other}'. Usage: OpenLibrary, or OpenLibrary --version");
            std::process::exit(2);
        }
    }
}
