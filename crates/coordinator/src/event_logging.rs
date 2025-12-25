use serde::Serialize;
use std::path::Path;
use shared::log;
use shared::colony_events::ColonyEvent;
use shared::be_api::ColonyLifeRules;
use crate::coordinator_context::CoordinatorContext;

const BASE_BUCKET_DIR: &str = "output/s3/distributed-colony";

#[derive(Serialize)]
pub struct EventJson {
    #[serde(rename = "colony_instance_id")]
    pub colony_instance_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick: Option<u64>,
    #[serde(rename = "event_type")]
    pub event_type: String,
    #[serde(rename = "event_description")]
    pub event_description: String,
    #[serde(rename = "event_data", skip_serializing_if = "Option::is_none")]
    pub event_data: Option<ColonyEvent>,
    pub rules: ColonyLifeRules,
}

#[derive(Serialize)]
pub struct ColonyCreatedEventJson {
    #[serde(rename = "colony_instance_id")]
    pub colony_instance_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick: Option<u64>,
    #[serde(rename = "event_type")]
    pub event_type: String,
    pub rules: ColonyLifeRules,
}

/// Get timestamp string in format YYYY_MM_DD__HH_MM_SS
fn get_event_timestamp() -> String {
    let now = chrono::Local::now();
    now.format("%Y_%m_%d__%H_%M_%S").to_string()
}

/// Write event JSON to disk
pub fn write_event_json(
    event: &ColonyEvent,
    tick: u64,
    event_type: &str,
    event_description: &str,
    rules: ColonyLifeRules,
) -> Result<(), String> {
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    
    let instance_id = match stored_info.colony_instance_id.as_deref() {
        Some(id) => id,
        None => {
            // Skip logging if instance ID is not set
            return Ok(());
        }
    };
    
    let timestamp = get_event_timestamp();
    
    let event_json = EventJson {
        colony_instance_id: instance_id.to_string(),
        tick: Some(tick),
        event_type: event_type.to_string(),
        event_description: event_description.to_string(),
        event_data: Some(event.clone()),
        rules,
    };
    
    save_event_to_disk(&event_json, instance_id, &timestamp)
}

/// Write colony creation event JSON to disk
pub fn write_colony_created_event_json(rules: ColonyLifeRules) -> Result<(), String> {
    let context = CoordinatorContext::get_instance();
    let stored_info = context.get_coord_stored_info();
    
    let instance_id = match stored_info.colony_instance_id.as_deref() {
        Some(id) => id,
        None => {
            // Skip logging if instance ID is not set
            return Ok(());
        }
    };
    
    let timestamp = get_event_timestamp();
    
    let event_json = ColonyCreatedEventJson {
        colony_instance_id: instance_id.to_string(),
        event_type: "ColonyCreated".to_string(),
        tick: Some(1),
        rules,
    };
    
    save_colony_created_event_to_disk(&event_json, instance_id, &timestamp)
}

fn save_event_to_disk(event_json: &EventJson, instance_id: &str, timestamp: &str) -> Result<(), String> {
    // Build directory path: output/s3/distributed-colony/{id}/events
    let dir_path = Path::new(BASE_BUCKET_DIR).join(instance_id).join("events");
    if let Err(e) = std::fs::create_dir_all(&dir_path) {
        return Err(format!("Failed to create directory {}: {}", dir_path.display(), e));
    }
    
    // Construct full file path: event_{timestamp}.json
    let filename = format!("event_{}.json", timestamp);
    let file_path = dir_path.join(&filename);
    
    // Serialize to JSON
    let json = serde_json::to_string_pretty(event_json)
        .map_err(|e| format!("Failed to serialize event to JSON: {}", e))?;
    
    // Write to file
    std::fs::write(&file_path, json)
        .map_err(|e| format!("Failed to write event file to {}: {}", file_path.display(), e))?;
    
    log!("Successfully saved event to: {}/{}/events/{}", BASE_BUCKET_DIR, instance_id, filename);
    
    Ok(())
}

fn save_colony_created_event_to_disk(event_json: &ColonyCreatedEventJson, instance_id: &str, timestamp: &str) -> Result<(), String> {
    // Build directory path: output/s3/distributed-colony/{id}/events
    let dir_path = Path::new(BASE_BUCKET_DIR).join(instance_id).join("events");
    if let Err(e) = std::fs::create_dir_all(&dir_path) {
        return Err(format!("Failed to create directory {}: {}", dir_path.display(), e));
    }
    
    // Construct full file path: event_{timestamp}.json
    let filename = format!("event_{}.json", timestamp);
    let file_path = dir_path.join(&filename);
    
    // Serialize to JSON
    let json = serde_json::to_string_pretty(event_json)
        .map_err(|e| format!("Failed to serialize colony created event to JSON: {}", e))?;
    
    // Write to file
    std::fs::write(&file_path, json)
        .map_err(|e| format!("Failed to write colony created event file to {}: {}", file_path.display(), e))?;
    
    log!("Successfully saved colony creation event to: {}/{}/events/{}", BASE_BUCKET_DIR, instance_id, filename);
    
    Ok(())
}

