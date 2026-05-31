#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

#[cfg(not(test))]
fn main() {
    macro_recorder_lib::run()
}

#[cfg(test)]
fn main() {}
