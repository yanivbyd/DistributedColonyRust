use shared::log;
use shared::colony_model::Shard;
use shared::colony_event_shared::log_event;
use crate::colony_event_generator::{randomize_event_by_frequency, get_next_event_tick_by_frequency, EventFrequency};
use shared::utils::new_random_generator;
use crate::backend_client;
use crate::tick_monitor::TickMonitor;
use std::sync::Mutex;
use std::collections::HashMap;

const EVENT_FREQUENCIES: [EventFrequency; 3] = [
    EventFrequency::Normal,
    EventFrequency::Rare,
    EventFrequency::Extinction,
];

fn log_tick(tick_count: u64, tick_monitor: &Mutex<TickMonitor>) {
    log!("[{}] Tick pace: {:.2} ticks/sec", 
        tick_count, 
        tick_monitor.lock().unwrap().calculate_pace(tick_count));
}

fn handle_colony_events(tick_count: u64, next_event_ticks: &mut HashMap<EventFrequency, u64>, colony_width: i32, colony_height: i32) {
    for frequency in EVENT_FREQUENCIES.iter() {
        let mut event_rng = new_random_generator();
        
        if let Some(&next_tick) = next_event_ticks.get(frequency) {
            if tick_count >= next_tick {
                let event = randomize_event_by_frequency(*frequency, colony_width, colony_height, &mut event_rng);
                log_event(&event, tick_count);
                backend_client::broadcast_event_to_backends(event);
                next_event_ticks.insert(*frequency, tick_count + get_next_event_tick_by_frequency(*frequency, &mut event_rng));
            }
        } else {
            next_event_ticks.insert(*frequency, tick_count + get_next_event_tick_by_frequency(*frequency, &mut event_rng));
        }
    }
}

pub fn start_coordinator_ticker() {
    std::thread::spawn(move || {
        let tick_monitor = Mutex::new(TickMonitor::new());
        let mut next_event_ticks: HashMap<EventFrequency, u64> = HashMap::new();
        let mut colony_dimensions: Option<(i32, i32)> = None;
        
        loop {
            let shard = Shard { x: 0, y: 0, width: 250, height: 250 };
            
            if let Some(tick_count) = backend_client::call_backend_for_tick_count(shard) {                
                log_tick(tick_count, &tick_monitor);
                
                // Get colony dimensions once and cache them
                if colony_dimensions.is_none() {
                    colony_dimensions = backend_client::call_backend_get_colony_info();
                }
                
                if let Some((width, height)) = colony_dimensions {
                    handle_colony_events(tick_count, &mut next_event_ticks, width, height);
                }
            }
            
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

