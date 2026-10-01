// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    {
        use std::env;
        if env::var_os("__NV_DISABLE_EXPLICIT_SYNC").is_none() {
            unsafe {
                env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
            }
        }
    }

    iloader_lib::run()
}
