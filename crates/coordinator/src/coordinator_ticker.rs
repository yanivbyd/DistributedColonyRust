use shared::log;
use shared::colony_model::Shard;
use crate::backend_client;
use crate::tick_monitor::TickMonitor;
use std::sync::Mutex;

pub fn start_coordinator_ticker() {
    std::thread::spawn(move || {
        let tick_monitor = Mutex::new(TickMonitor::new());
        
        loop {
            let shard = Shard { x: 0, y: 0, width: 250, height: 250 };
            
            if let Some(tick_count) = backend_client::call_backend_for_tick_count(shard) {                
                log!("Tick: {}, pace: {:.2} ticks/sec", 
                    tick_count, 
                    tick_monitor.lock().unwrap().calculate_pace(tick_count));
            }
            
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}
