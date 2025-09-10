use shared::log;
use shared::colony_model::Shard;
use shared::colony_event_shared::log_event;
use crate::colony_event_generator::{randomize_event_by_frequency, get_next_event_tick_by_frequency, EventFrequency};
use shared::utils::new_random_generator;
use crate::backend_client;
use crate::tick_monitor::TickMonitor;
use crate::global_topography::{GlobalTopography, GlobalTopographyInfo};
use std::sync::Mutex;
use std::collections::HashMap;

const EVENT_FREQUENCIES: [EventFrequency; 4] = [
    EventFrequency::Normal,
    EventFrequency::Rare,
    EventFrequency::Extinction,
    EventFrequency::Topography,
];

fn log_tick(tick_count: u64, tick_monitor: &Mutex<TickMonitor>) {
    log!("[{}] Tick pace: {:.2} ticks/sec", 
        tick_count, 
        tick_monitor.lock().unwrap().calculate_pace(tick_count));
}

async fn handle_new_topography_event(colony_width: i32, colony_height: i32) {
    log!("Generating new topography for colony {}x{}", colony_width, colony_height);
    
    // Create topography info similar to init_colony.rs
    const SHARD_WIDTH: i32 = 250;
    const SHARD_HEIGHT: i32 = 250;
    
    let topography_info = GlobalTopographyInfo {
        total_width: colony_width as usize,
        total_height: colony_height as usize,
        shard_width: SHARD_WIDTH as usize,
        shard_height: SHARD_HEIGHT as usize,

        base_elevation: 5,
        river_elevation_range: 45, 
        river_influence_distance: 175.0,
        river_count_range: (10, 20),
        river_segments_range: (30, 45),
        river_step_length_range: (20.0, 30.0),
        river_direction_change: 0.6,
        smoothing_iterations: 4,
    };
    
    let topography = GlobalTopography::new(topography_info);
    topography.generate_topography().await;
    
    log!("New topography generation completed");
}

fn handle_colony_events(tick_count: u64, next_event_ticks: &mut HashMap<EventFrequency, u64>, colony_width: i32, colony_height: i32) {
    for frequency in EVENT_FREQUENCIES.iter() {
        let mut event_rng = new_random_generator();
        
        if let Some(&next_tick) = next_event_ticks.get(frequency) {
            if tick_count >= next_tick {
                let event = randomize_event_by_frequency(*frequency, colony_width, colony_height, &mut event_rng);
                log_event(&event, tick_count);
                
                // Special handling for NewTopography event
                if matches!(event, shared::colony_events::ColonyEvent::NewTopography()) {
                    // Run async function in a blocking context
                    let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
                    rt.block_on(handle_new_topography_event(colony_width, colony_height));
                } else {
                    backend_client::broadcast_event_to_backends(event);
                }
                
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

