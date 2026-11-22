// SPDX-License-Identifier: GPL-3.0-or-later
// No console window behind the program; a terminal that starts it with arguments is attached below.
#![windows_subsystem = "windows"]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => openexample::run(),
        Some("--version" | "-V") => {
            openexample::platform::attach_console();
            println!("{}", openexample::version_line());
        }
        Some(other) => {
            openexample::platform::attach_console();
            eprintln!("OpenExample: unknown argument '{other}'. Usage: OpenExample, or OpenExample --version");
            std::process::exit(2);
        }
    }
}
