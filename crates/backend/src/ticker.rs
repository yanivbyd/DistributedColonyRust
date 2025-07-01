use std::thread;
use std::time::Duration;
use crate::colony::ColonySubGrid;

pub fn start_ticker() {
    thread::spawn(|| {
        loop {
            if ColonySubGrid::is_initialized() {
                ColonySubGrid::instance().tick();
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
} 