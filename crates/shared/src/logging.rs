use std::fs::{OpenOptions, create_dir_all};
use std::io::Write;
use std::sync::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    static ref LOG_FILE_PATH: Mutex<Option<String>> = Mutex::new(None);
}

pub fn init_logging(log_file: &str) {
    if let Some(parent) = std::path::Path::new(log_file).parent() {
        let _ = create_dir_all(parent);
    }
    let mut path = LOG_FILE_PATH.lock().unwrap();
    *path = Some(log_file.to_string());
}

fn get_log_file() -> Option<String> {
    LOG_FILE_PATH.lock().unwrap().clone()
}

pub fn log_to_file(msg: &str) {
    if let Some(log_file) = get_log_file() {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_file) {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let _ = writeln!(file, "[{}] {}", timestamp, msg);
        }
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {{
        $crate::logging::log_to_file(&format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => {{
        $crate::logging::log_to_file(&format!("[ERROR] {}", format!($($arg)*)));
    }};
}

pub fn log_startup(process_name: &str) {
    log_to_file(&format!("{} Startup", process_name));
} 