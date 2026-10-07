//! Noctorium Stats: a listener's statistics from their Noctorium account, in a window.
//!
//! A window program on Windows, so that opening it from the Start menu does not flash a console up first.
//! Debug builds keep the console, which is where anything worth reading while working on it goes.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;

const USAGE: &str = "\
Noctorium Stats

Your Noctorium listening statistics: sign in with your Noctorium account and it shows what you have
listened to, over the last week, month, quarter, year or all of it.

  --version   the version
  --help      this

  NOCTORIUM_SERVICE_URL   the account service to ask, rather than the live one, as the player reads it
  NOCTORIUM_STATS_HOME    keep settings, the cache and the sign-in in this folder, and leave the system's
                          keyring alone
  NOCTORIUM_NO_UPDATE_CHECK  do not ask GitHub whether a newer release is out
";

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let said = |flags: &[&str]| arguments.iter().any(|a| flags.contains(&a.as_str()));

    // Printed to whatever standard output is: on Windows that is nothing from a double click, and a pipe or
    // a file when run as `"Noctorium Stats.exe" --version | more`, which is what a script checking it does.
    if said(&["--version", "-V"]) {
        println!("Noctorium Stats {}", noctorium_stats::version());
        return ExitCode::SUCCESS;
    }
    if said(&["--help", "-h"]) {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    match noctorium_stats::gui::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            complain(&format!(
                "Noctorium Stats could not open its window.\n\n{why}"
            ));
            ExitCode::FAILURE
        }
    }
}

/// Says why there is no window, somewhere it will be seen. A window program on Windows has nowhere to print
/// to, and the usual reason -- a machine whose graphics driver offers no OpenGL 2, a virtual machine above
/// all -- is one somebody can do something about once they have been told.
#[cfg(windows)]
fn complain(text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide("Noctorium Stats"));
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
fn complain(text: &str) {
    eprintln!("{text}");
}
