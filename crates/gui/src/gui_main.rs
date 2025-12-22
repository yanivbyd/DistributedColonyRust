#![allow(deprecated)]
use eframe::{egui, App};
use egui_extras::RetainedImage;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use shared::be_api::{ShardLayer, ColonyLifeRules};
use shared::cluster_topology::ClusterTopology;
use shared::cluster_registry::create_cluster_registry;
use shared::ssm;
use shared::coordinator_api::ColonyEventDescription;
use shared::log;

use crate::call_be::get_colony_stats;
mod call_be;
mod latency_tracker;

const REFRESH_INTERVAL_MS_LOCALHOST: u64 = 100;
const REFRESH_INTERVAL_MS_AWS: u64 = 5000;
const MIN_CREATURE_SIZE_LEGEND_MAX: i32 = 30;
const FOOD_VALUE_LEGEND_MAX: i32 = 255;

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Creatures,
    ExtraFood,
    Food,
    Sizes,
    CanKill,
    CanMove,
    CostPerTurn,
    Health,
    Age,
    Info,
    Stats,
    Cluster,
}

#[derive(Clone)]
pub struct ShardConfig {
    pub total_width: i32,
    pub total_height: i32,
    pub cols: usize,
    pub rows: usize,
}

impl ShardConfig {
    fn from_topology(topology: &ClusterTopology) -> Self {
        let width_in_shards = topology.calculate_width_in_shards();
        let height_in_shards = topology.calculate_height_in_shards();
        let shard_width = topology.get_shard_width_from_mapping();
        let shard_height = topology.get_shard_height_from_mapping();
        
        Self {
            total_width: width_in_shards * shard_width,
            total_height: height_in_shards * shard_height,
            cols: width_in_shards as usize,
            rows: height_in_shards as usize,
        }
    }
}

impl ShardConfig {
    fn shard_width(&self) -> i32 {
        // Calculate from total width and cols
        if self.cols > 0 {
            self.total_width / self.cols as i32
        } else {
            0
        }
    }
    
    fn shard_height(&self) -> i32 {
        // Calculate from total height and rows
        if self.rows > 0 {
            self.total_height / self.rows as i32
        } else {
            0
        }
    }
    
    fn total_shards(&self) -> usize {
        self.cols * self.rows
    }
    
    fn get_shard(&self, index: usize) -> shared::be_api::Shard {
        let row = index / self.cols;
        let col = index % self.cols;
        
        let shard_width = self.shard_width();
        let shard_height = self.shard_height();
        
        let x = col as i32 * shard_width;
        let y = row as i32 * shard_height;
        
        shared::be_api::Shard { 
            x, 
            y, 
            width: shard_width, 
            height: shard_height 
        }
    }
}

struct BEImageApp {
    creatures: Arc<Mutex<Vec<Option<RetainedImage>>>>,
    creatures_color_data: Arc<Mutex<Vec<Option<Vec<shared::be_api::Color>>>>>,
    extra_food: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    sizes: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    can_kill: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    can_move: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    cost_per_turn: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    food: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    health: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    age: Arc<Mutex<Vec<Option<Vec<i32>>>>>,
    colony_info: Arc<Mutex<Option<(Option<shared::be_api::ColonyLifeRules>, Option<u64>)>>>,
    colony_events: Arc<Mutex<Option<Vec<ColonyEventDescription>>>>,
    ctx: Option<egui::Context>,
    thread_started: bool,
    current_tab: Tab,
    shared_current_tab: Arc<Mutex<Tab>>,
    shard_config: Arc<Mutex<ShardConfig>>,
    cluster_topology: Arc<ClusterTopology>,
    last_update_time: Arc<Mutex<Instant>>,
    combined_texture: Option<egui::TextureHandle>,
    cached_stats: Option<(u64, Vec<shared::coordinator_api::ColonyMetricStats>)>,
    deployment_mode: String,
    coordinator_http_port: Option<u16>,
    backend_http_ports: std::collections::HashMap<shared::cluster_topology::HostInfo, u16>,
    latency_tracker: Arc<latency_tracker::LatencyTracker>,
    colony_instance_id: Option<String>,
    tab_change_signal: Arc<(Mutex<bool>, Condvar)>,
    responsiveness_state: Arc<Mutex<GuiResponsivenessState>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuiResponsivenessState {
    Healthy,
    Slow,
    Unresponsive,
}

impl BEImageApp {
    fn new(cluster_topology: Arc<ClusterTopology>, deployment_mode: String, coordinator_http_port: Option<u16>, backend_http_ports: std::collections::HashMap<shared::cluster_topology::HostInfo, u16>, colony_instance_id: Option<String>) -> Self {
        let shard_config = Arc::new(Mutex::new(ShardConfig::from_topology(&cluster_topology)));
        let total_shards = {
            let config_guard = shard_config.lock().unwrap();
            config_guard.total_shards()
        };

        let latency_tracker = Arc::new(latency_tracker::LatencyTracker::new(100));
        let creatures = Arc::new(Mutex::new(call_be::get_all_shard_retained_images(&shard_config.lock().unwrap(), cluster_topology.as_ref(), &latency_tracker, &backend_http_ports)));
        let creatures_color_data = Arc::new(Mutex::new(call_be::get_all_shard_color_data(&shard_config.lock().unwrap(), cluster_topology.as_ref(), &latency_tracker, &backend_http_ports)));
        let extra_food = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let sizes = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let can_kill = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let can_move = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let cost_per_turn = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let food = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let health = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let age = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let colony_info = Arc::new(Mutex::new(None));
        let colony_events = Arc::new(Mutex::new(None));
        let current_tab = Tab::Creatures;
        let tab_change_signal = Arc::new((Mutex::new(false), Condvar::new()));
        let responsiveness_state = Arc::new(Mutex::new(GuiResponsivenessState::Healthy));
        Self {
            creatures,
            creatures_color_data,
            extra_food,
            sizes,
            can_kill,
            can_move,
            cost_per_turn,
            food,
            health,
            age,
            colony_info,
            colony_events,
            ctx: None,
            thread_started: false,
            current_tab,
            shared_current_tab: Arc::new(Mutex::new(current_tab)),
            shard_config,
            cluster_topology,
            last_update_time: Arc::new(Mutex::new(Instant::now())),
            combined_texture: None,
            cached_stats: None,
            deployment_mode,
            coordinator_http_port,
            backend_http_ports,
            latency_tracker,
            colony_instance_id,
            tab_change_signal,
            responsiveness_state,
        }
    }
}

impl App for BEImageApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // On the first frame, store ctx and spawn the background thread
        if !self.thread_started {
            self.ctx = Some(ctx.clone());
            let creatures = self.creatures.clone();
            let creatures_color_data = self.creatures_color_data.clone();
            let extra_food = self.extra_food.clone();
            let sizes = self.sizes.clone();
            let can_kill = self.can_kill.clone();
            let can_move = self.can_move.clone();
            let cost_per_turn = self.cost_per_turn.clone();
            let food = self.food.clone();
            let health = self.health.clone();
            let age = self.age.clone();
            let ctx_clone = ctx.clone();
            let shared_current_tab = self.shared_current_tab.clone();
            let shard_config = self.shard_config.clone();
            let cluster_topology = Arc::clone(&self.cluster_topology);
            let last_update_time = self.last_update_time.clone();
            let deployment_mode = self.deployment_mode.clone();
            let latency_tracker = Arc::clone(&self.latency_tracker);
            let tab_change_signal = Arc::clone(&self.tab_change_signal);
            let deployment_mode_clone = deployment_mode.clone();
            let backend_http_ports = self.backend_http_ports.clone();
            thread::spawn(move || {
                let refresh_interval_ms = if deployment_mode == "aws" {
                    REFRESH_INTERVAL_MS_AWS
                } else {
                    REFRESH_INTERVAL_MS_LOCALHOST
                };
                loop {
                    // Start polling cycle timing
                    let cycle_start = Instant::now();
                    let time_before_update = *last_update_time.lock().unwrap();
                    let mut had_success = false;
                    
                    // Look at the selected tab and get only the info required for the current Tab
                    let tab = *shared_current_tab.lock().unwrap();
                    let config = shard_config.lock().unwrap().clone();
                    
                    match tab {
                        Tab::Creatures => {
                            let images = call_be::get_all_shard_retained_images(&config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            let color_data = call_be::get_all_shard_color_data(&config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !images.iter().all(|img| img.is_none()) {
                                let mut locked = creatures.lock().unwrap();
                                *locked = images;
                                *last_update_time.lock().unwrap() = Instant::now();
                                had_success = true;
                            }
                            if !color_data.iter().all(|data| data.is_none()) {
                                let mut locked = creatures_color_data.lock().unwrap();
                                *locked = color_data;
                            }
                    }
                    Tab::ExtraFood => {
                            let extra_food_data = call_be::get_all_shard_layer_data(ShardLayer::ExtraFood, &config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !extra_food_data.iter().all(|data| data.is_none()) {
                                let mut locked = extra_food.lock().unwrap();
                                *locked = extra_food_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                                had_success = true;
                            }
                    }
                    Tab::Sizes => {
                            let sizes_data = call_be::get_all_shard_layer_data(ShardLayer::CreatureSize, &config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !sizes_data.iter().all(|data| data.is_none()) {
                                let mut locked = sizes.lock().unwrap();
                                *locked = sizes_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                                had_success = true;
                            }
                    }
                    Tab::Age => {
                        let age_data = call_be::get_all_shard_layer_data(ShardLayer::Age, &config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                        if !age_data.iter().all(|data| data.is_none()) {
                            let mut locked = age.lock().unwrap();
                            *locked = age_data;
                            *last_update_time.lock().unwrap() = Instant::now();
                            had_success = true;
                        }
                    }
                        Tab::CanKill => {
                            let can_kill_data = call_be::get_all_shard_layer_data(ShardLayer::CanKill, &config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !can_kill_data.iter().all(|data| data.is_none()) {
                                let mut locked = can_kill.lock().unwrap();
                                *locked = can_kill_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                                had_success = true;
                            }
                        }
                        Tab::CanMove => {
                            let can_move_data = call_be::get_all_shard_layer_data(ShardLayer::CanMove, &config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !can_move_data.iter().all(|data| data.is_none()) {
                                let mut locked = can_move.lock().unwrap();
                                *locked = can_move_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                                had_success = true;
                            }
                        }
                        Tab::CostPerTurn => {
                            let cost_per_turn_data = call_be::get_all_shard_layer_data(ShardLayer::CostPerTurn, &config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !cost_per_turn_data.iter().all(|data| data.is_none()) {
                                let mut locked = cost_per_turn.lock().unwrap();
                                *locked = cost_per_turn_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                                had_success = true;
                            }
                        }
                        Tab::Food => {
                            let food_data = call_be::get_all_shard_layer_data(ShardLayer::Food, &config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !food_data.iter().all(|data| data.is_none()) {
                                let mut locked = food.lock().unwrap();
                                *locked = food_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                                had_success = true;
                            }
                        }
                        Tab::Health => {
                            let health_data = call_be::get_all_shard_layer_data(ShardLayer::Health, &config, cluster_topology.as_ref(), &latency_tracker, &backend_http_ports);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !health_data.iter().all(|data| data.is_none()) {
                                let mut locked = health.lock().unwrap();
                                *locked = health_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                                had_success = true;
                            }
                        }
                        Tab::Info => {
                            // No automatic polling for Info tab - data is loaded once when tab is first accessed
                    }
                    Tab::Cluster => {
                            // No automatic polling for Cluster tab - data is static and retrieved at startup
                    }
                    Tab::Stats => {
                            // No background polling for stats
                        }
                    }
                    
                    // End polling cycle timing and log
                    let cycle_end = Instant::now();
                    let cycle_duration_ms = cycle_end.duration_since(cycle_start).as_millis() as f64;
                    let current_update_time = *last_update_time.lock().unwrap();
                    let time_since_last_update = if had_success {
                        0.0  // Just updated
                    } else {
                        current_update_time.duration_since(time_before_update).as_millis() as f64
                    };
                    
                    let successes = if had_success { 1 } else { 0 };
                    let errors = if had_success { 0 } else { 1 };
                    
                    log!("GUI poll cycle: duration_ms={:.2}, successes={}, errors={}, time_since_last_update_ms={:.2}, mode={}", 
                         cycle_duration_ms, successes, errors, time_since_last_update, deployment_mode_clone);
                    
                    ctx_clone.request_repaint();
                    
                    // Wait for either timeout or tab change signal
                    let (lock, cvar) = &*tab_change_signal;
                    let mut signaled = lock.lock().unwrap();
                    let timeout = Duration::from_millis(refresh_interval_ms);
                    let result = cvar.wait_timeout(signaled, timeout).unwrap();
                    signaled = result.0;
                    
                    if *signaled {
                        // Tab changed, reset flag and continue immediately (skip sleep)
                        *signaled = false;
                    }
                    // If timeout reached, continue normally (equivalent to sleep)
                }
            });
            self.thread_started = true;
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            
            // Tab control
            ui.horizontal(|ui| {
                let old_tab = self.current_tab;
                ui.selectable_value(&mut self.current_tab, Tab::Creatures, "Creatures");
                ui.selectable_value(&mut self.current_tab, Tab::ExtraFood, "Extra Food");
                ui.selectable_value(&mut self.current_tab, Tab::Food, "Food");
                ui.selectable_value(&mut self.current_tab, Tab::Sizes, "Sizes");
                ui.selectable_value(&mut self.current_tab, Tab::CanKill, "Can Kill");
                ui.selectable_value(&mut self.current_tab, Tab::CanMove, "Can Move");
                ui.selectable_value(&mut self.current_tab, Tab::CostPerTurn, "Cost Per Turn");
                ui.selectable_value(&mut self.current_tab, Tab::Health, "Health");
                ui.selectable_value(&mut self.current_tab, Tab::Age, "Age");
                ui.selectable_value(&mut self.current_tab, Tab::Info, "Info");
                ui.selectable_value(&mut self.current_tab, Tab::Stats, "Stats");
                ui.selectable_value(&mut self.current_tab, Tab::Cluster, "Cluster");
                
                // Update shared tab if changed
                if self.current_tab != old_tab {
                    if let Ok(mut shared_tab) = self.shared_current_tab.lock() {
                        *shared_tab = self.current_tab;
                    }
                    // Signal background thread to wake up immediately
                    let (lock, cvar) = &*self.tab_change_signal;
                    *lock.lock().unwrap() = true;
                    cvar.notify_one();
                }
                
                // Show status indicator only when there are issues and not on Info, Stats, or Cluster tabs
                if self.current_tab != Tab::Info && self.current_tab != Tab::Stats && self.current_tab != Tab::Cluster {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let last_update = *self.last_update_time.lock().unwrap();
                        let time_since_update = last_update.elapsed();
                        let time_since_update_ms = time_since_update.as_millis() as f64;
                        
                        // Determine current responsiveness state
                        let current_state = if time_since_update.as_secs() > 5 {
                            GuiResponsivenessState::Unresponsive
                        } else if time_since_update.as_millis() > 1000 {
                            GuiResponsivenessState::Slow
                        } else {
                            GuiResponsivenessState::Healthy
                        };
                        
                        // Check for state transition and log
                        let mut state_guard = self.responsiveness_state.lock().unwrap();
                        let prev_state = *state_guard;
                        if prev_state != current_state {
                            log!("GUI responsiveness state changed: prev={:?}, current={:?}, time_since_update_ms={:.2}", 
                                 prev_state, current_state, time_since_update_ms);
                            *state_guard = current_state;
                        }
                        drop(state_guard);
                        
                        // Display UI indicator
                        if time_since_update.as_secs() > 5 {
                            ui.colored_label(egui::Color32::RED, "⚠️ Backend Unresponsive");
                        } else if time_since_update.as_millis() > 1000 {
                            ui.colored_label(egui::Color32::YELLOW, "🔄 Slow Response");
                        }
                        // Don't show anything when all is well (time_since_update <= 1000ms)
                    });
                }
            });
            ui.separator();
            
            match self.current_tab {
                Tab::Creatures => self.show_creatures_tab(ui),
                Tab::ExtraFood => self.show_extra_food_tab(ui),
                Tab::Food => self.show_food_tab(ui),
                Tab::Sizes => self.show_sizes_tab(ui),
                Tab::CanKill => self.show_can_kill_tab(ui),
                Tab::CanMove => self.show_can_move_tab(ui),
                Tab::CostPerTurn => self.show_cost_per_turn_tab(ui),
                Tab::Health => self.show_health_tab(ui),
                Tab::Age => self.show_age_tab(ui),
                Tab::Info => self.show_info_tab(ui),
                Tab::Stats => self.show_stats_tab(ui),
                Tab::Cluster => self.show_cluster_tab(ui),
            }
        });
    }
}

impl BEImageApp {
    fn lerp(a: u8, b: u8, t: f32) -> u8 {
        ((1.0 - t) * (a as f32) + t * (b as f32)).round() as u8
    }

    fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
        (
            Self::lerp(a.0, b.0, t),
            Self::lerp(a.1, b.1, t),
            Self::lerp(a.2, b.2, t),
        )
    }

    fn terrain_color(normalized: f32) -> egui::Color32 {
        // From top to bottom in the image
        let palette = [
            (0, 102, 0),      // Dark Green
            (0, 204, 0),      // Green
            (153, 255, 102),  // Light Green
            (255, 255, 128),  // Yellow
            (222, 184, 135),  // Tan
            (204, 51, 0),     // Red
            (143, 10, 10),     // Dark Red
        ];

        let clamped = normalized.clamp(0.0, 1.0);
        let scaled = clamped * (palette.len() - 1) as f32;
        let idx = scaled.floor() as usize;
        let t = scaled.fract();

        let (r, g, b) = if idx >= palette.len() - 1 {
            palette[palette.len() - 1]
        } else {
            Self::lerp_rgb(palette[idx], palette[idx + 1], t)
        };

        egui::Color32::from_rgb(r, g, b)
    }
    
    fn show_creatures_tab(&mut self, ui: &mut egui::Ui) {
        let colors: Vec<Option<Vec<shared::be_api::Color>>> = {
            let locked = self.creatures_color_data.lock().unwrap();
            locked.clone()
        };
        self.show_combined_image(ui, &colors, |shard_data| {
            shard_data.clone()
        });
    }

    fn show_combined_image<T, F>(&mut self, ui: &mut egui::Ui, data: &[Option<T>], converter: F)
    where
        F: Fn(&Option<T>) -> Option<Vec<shared::be_api::Color>>,
    {
        // Create a combined image using the shard configuration
        let config = self.shard_config.lock().unwrap();
        let total_width = config.total_width as usize;
        let total_height = config.total_height as usize;
        let mut combined_img = egui::ColorImage::new([total_width, total_height], egui::Color32::BLACK);
        
        // Process each shard
        for (idx, shard_data) in data.iter().enumerate() {
            let row = idx / config.cols;
            let col = idx % config.cols;
            
            if let Some(colors) = converter(shard_data) {
                // Calculate shard position and size
                let shard_x = col * config.shard_width() as usize;
                let shard_y = row * config.shard_height() as usize;
                let shard_width = if col == config.cols - 1 {
                    total_width - (config.cols - 1) * config.shard_width() as usize
                } else {
                    config.shard_width() as usize
                };
                let _shard_height = if row == config.rows - 1 {
                    total_height - (config.rows - 1) * config.shard_height() as usize
                } else {
                    config.shard_height() as usize
                };
                
                // Copy shard data to combined image
                for (i, color) in colors.iter().enumerate() {
                    let local_x = i % shard_width;
                    let local_y = i / shard_width;
                    let global_x = shard_x + local_x;
                    let global_y = shard_y + local_y;
                    
                    if global_x < total_width && global_y < total_height {
                        let pixel_idx = global_y * total_width + global_x;
                        combined_img.pixels[pixel_idx] = egui::Color32::from_rgb(color.red, color.green, color.blue);
                    }
                }
            }
        }
        
        // Upload/update a persistent texture and display it
        let texture_options = egui::TextureOptions::LINEAR;
        if let Some(tex) = &mut self.combined_texture {
            tex.set(combined_img, texture_options);
            ui.add(
                egui::Image::new(&*tex)
                    .fit_to_exact_size(egui::vec2(800.0, 600.0))
            );
        } else {
            let tex = ui.ctx().load_texture("combined", combined_img, texture_options);
            let handle = tex;
            self.combined_texture = Some(handle);
            let tex_ref = self.combined_texture.as_ref().unwrap();
            ui.add(
                egui::Image::new(tex_ref)
                    .fit_to_exact_size(egui::vec2(800.0, 600.0))
            );
        }
    }

    fn show_layer_tab(&mut self, ui: &mut egui::Ui, data: &Arc<Mutex<Vec<Option<Vec<i32>>>>>) {
        self.show_layer_tab_with_legend(ui, data, None)
    }

    fn show_layer_tab_with_legend(&mut self, ui: &mut egui::Ui, data: &Arc<Mutex<Vec<Option<Vec<i32>>>>>, legend_max_value: Option<i32>) {
        let locked_vec: Vec<Option<Vec<i32>>> = {
            let locked = data.lock().unwrap();
            locked.clone()
        };
        
        // Find global maximum across all shards for consistent normalization
        let global_max = locked_vec.iter()
            .filter_map(|shard_data| shard_data.as_ref())
            .flat_map(|data| data.iter())
            .max()
            .copied()
            .unwrap_or(0);

        // Use provided legend values or calculate from data
        let legend_min = 0;
        let legend_max = legend_max_value.unwrap_or(global_max);
        let global_max = legend_max.max(global_max);

        self.show_combined_image(ui, &locked_vec, |shard_data| {
            if let Some(data) = shard_data {
                if global_max > 0 {
                    // Convert i32 data to colors using global normalization
                    let mut colors = Vec::new();
                    for &val in data {
                        if val == 0 {
                            colors.push(shared::be_api::Color { red: 255, green: 255, blue: 255, });
                        } else {
                            let normalized = val as f32 / global_max as f32;
                            let color = Self::terrain_color(normalized);
                            colors.push(shared::be_api::Color { red: color.r(), green: color.g(), blue: color.b(), });
                        }
                    }
                    Some(colors)
                } else {
                    // All values are 0, use white
                    let mut colors = Vec::new();
                    for _ in 0..data.len() {
                        colors.push(shared::be_api::Color { red: 255, green: 255, blue: 255, });
                    }
                    Some(colors)
                }
            } else {
                None
            }
        });
        
        // Add legend below the image
        if global_max > 0 {
            ui.add_space(20.0);
            
            let legend_width = 800.0;
            let legend_height = 5.0;
            let legend_rect = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(legend_width, legend_height)
            );
            
            let painter = ui.painter();
            
            // Draw color gradient
            for i in 0..legend_width as usize {
                let normalized = i as f32 / legend_width;
                let color = Self::terrain_color(normalized);
                let x = legend_rect.min.x + i as f32;
                painter.line_segment(
                    [egui::pos2(x, legend_rect.min.y), egui::pos2(x, legend_rect.max.y)],
                    egui::Stroke::new(1.0, color)
                );
            }
            
            // Add labels
            ui.add_space(legend_height + 5.0);
            ui.horizontal(|ui| {
                ui.label(format!("{}", legend_min));
                ui.add_space(legend_width / 2.0 - 30.0);
                ui.label(format!("{}", (legend_min + legend_max) / 2));
                ui.add_space(legend_width / 2.0 - 30.0);
                ui.label(format!("{}", legend_max));
            });
        }
    }

    fn show_extra_food_tab(&mut self, ui: &mut egui::Ui) {
        let extra_food = self.extra_food.clone();
        self.show_layer_tab(ui, &extra_food);
    }

    fn show_sizes_tab(&mut self, ui: &mut egui::Ui) {
        let sizes = self.sizes.clone();
        self.show_layer_tab_with_legend(ui, &sizes, Some(MIN_CREATURE_SIZE_LEGEND_MAX));
    }

    fn show_can_kill_tab(&mut self, ui: &mut egui::Ui) {
        let can_kill = self.can_kill.clone();
        self.show_layer_tab_boolean(ui, &can_kill);
    }

    fn show_can_move_tab(&mut self, ui: &mut egui::Ui) {
        let can_move = self.can_move.clone();
        self.show_layer_tab_boolean(ui, &can_move);
    }

    fn show_cost_per_turn_tab(&mut self, ui: &mut egui::Ui) {
        let cost_per_turn = self.cost_per_turn.clone();
        self.show_layer_tab(ui, &cost_per_turn);
    }

    fn show_food_tab(&mut self, ui: &mut egui::Ui) {
        let food = self.food.clone();
        self.show_layer_tab_with_legend(ui, &food, Some(FOOD_VALUE_LEGEND_MAX));
    }

    fn show_health_tab(&mut self, ui: &mut egui::Ui) {
        let health = self.health.clone();
        self.show_layer_tab_with_legend(ui, &health, Some(10)); 
    }

    fn show_age_tab(&mut self, ui: &mut egui::Ui) {
        let age = self.age.clone();
        self.show_layer_tab(ui, &age);
    }

    fn show_layer_tab_boolean(&mut self, ui: &mut egui::Ui, data: &Arc<Mutex<Vec<Option<Vec<i32>>>>>) {
        let locked_vec: Vec<Option<Vec<i32>>> = {
            let locked = data.lock().unwrap();
            locked.clone()
        };
        self.show_combined_image(ui, &locked_vec, |shard_data| {
            if let Some(data) = shard_data {
                // Convert i32 data to colors using boolean mapping
                let mut colors = Vec::new();
                for &val in data {
                    match val {
                        1 => {
                            let color = Self::terrain_color(0.0);
                            colors.push(shared::be_api::Color { red: color.r(), green: color.g(), blue: color.b() });
                        }
                        2 => {
                            let color = Self::terrain_color(1.0);
                            colors.push(shared::be_api::Color { red: color.r(), green: color.g(), blue: color.b() });
                        }
                        _ => {
                            colors.push(shared::be_api::Color { red: 255, green: 255, blue: 255 });
                        }
                    }
                }
                Some(colors)
            } else {
                None
            }
        });
    }

    fn format_number_with_commas(num: u64) -> String {
        let num_str = num.to_string();
        let mut result = String::new();
        let chars: Vec<char> = num_str.chars().collect();
        
        for (i, &ch) in chars.iter().enumerate() {
            if i > 0 && (chars.len() - i) % 3 == 0 {
                result.push(',');
            }
            result.push(ch);
        }
        
        result
    }

    fn show_info_tab(&self, ui: &mut egui::Ui) {
        
        
        // Always refresh data when Info tab is accessed
        if let Some(info) = call_be::get_colony_info(self.cluster_topology.as_ref(), &self.backend_http_ports) {
            let mut locked = self.colony_info.lock().unwrap();
            *locked = Some(info);
        }
        
        if let Some(events) = call_be::get_colony_events(30, self.coordinator_http_port, &self.deployment_mode) {
            let mut locked = self.colony_events.lock().unwrap();
            *locked = Some(events);
        }
        
        // Get cached colony info
        let colony_info_guard = self.colony_info.lock().unwrap();
        if let Some((colony_life_rules, current_tick)) = colony_info_guard.as_ref() {
            ui.group(|ui| {
                
                // Display current tick
                if let Some(tick) = current_tick {
                    ui.horizontal(|ui| {
                        ui.label("Current Tick:");
                        ui.label(format!("{}", Self::format_number_with_commas(*tick)));
                    });
                } else {
                    ui.label("Current Tick: Not available");
                }
            });
            
            ui.add_space(10.0);
            
            // Display ColonyLifeRules in a table format
            if let Some(life_info) = colony_life_rules {
                ui.group(|ui| {
                    
                    // Initial rules for comparison
                    const INITIAL_RULES: ColonyLifeRules = ColonyLifeRules {
                        health_cost_per_size_unit: 2,
                        eat_capacity_per_size_unit: 5,
                        health_cost_if_can_kill: 10,
                        health_cost_if_can_move: 5,
                        mutation_chance: 100,
                        random_death_chance: 100,
                    };
                    
                    egui::Grid::new("colony_life_rules_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Health Cost Per Size Unit:");
                            let current = life_info.health_cost_per_size_unit;
                            let initial = INITIAL_RULES.health_cost_per_size_unit;
                            if current != initial {
                                ui.label(format!("{} (initial={})", current, initial));
                            } else {
                                ui.label(format!("{}", current));
                            }
                            ui.end_row();
                            
                            ui.label("Eat Capacity Per Size Unit:");
                            let current = life_info.eat_capacity_per_size_unit;
                            let initial = INITIAL_RULES.eat_capacity_per_size_unit;
                            if current != initial {
                                ui.label(format!("{} (initial={})", current, initial));
                            } else {
                                ui.label(format!("{}", current));
                            }
                            ui.end_row();
                            
                            ui.label("Health Cost If Can Kill:");
                            let current = life_info.health_cost_if_can_kill;
                            let initial = INITIAL_RULES.health_cost_if_can_kill;
                            if current != initial {
                                ui.label(format!("{} (initial={})", current, initial));
                            } else {
                                ui.label(format!("{}", current));
                            }
                            ui.end_row();
                            
                            ui.label("Health Cost If Can Move:");
                            let current = life_info.health_cost_if_can_move;
                            let initial = INITIAL_RULES.health_cost_if_can_move;
                            if current != initial {
                                ui.label(format!("{} (initial={})", current, initial));
                            } else {
                                ui.label(format!("{}", current));
                            }
                            ui.end_row();
                            
                            ui.label("Mutation Chance:");
                            let current = life_info.mutation_chance;
                            let initial = INITIAL_RULES.mutation_chance;
                            if current != initial {
                                ui.label(format!("{} (initial={})", current, initial));
                            } else {
                                ui.label(format!("{}", current));
                            }
                            ui.end_row();
                            
                            ui.label("Random Death Chance:");
                            let current = life_info.random_death_chance;
                            let initial = INITIAL_RULES.random_death_chance;
                            if current != initial {
                                ui.label(format!("{} (initial={})", current, initial));
                            } else {
                                ui.label(format!("{}", current));
                            }
                            ui.end_row();
                        });
                });
            } else {
                ui.label("Colony Life Configuration: Not available");
            }
            
            ui.add_space(20.0);
            
            // Display colony events
            ui.group(|ui| {
                
                let events_guard = self.colony_events.lock().unwrap();
                if let Some(events) = events_guard.as_ref() {
                    if events.is_empty() {
                        ui.label("No events recorded yet.");
                    } else {
                        egui::Grid::new("colony_events_grid")
                            .num_columns(3)
                            .spacing([20.0, 4.0])
                            .show(ui, |ui| {
                                // Header row
                                ui.label("Tick");
                                ui.label("Event Type");
                                ui.label("Description");
                                ui.end_row();
                                
                                ui.separator();
                                ui.separator();
                                ui.separator();
                                ui.end_row();
                                
                                // Event rows
                                for event in events.iter() {
                                    ui.label(format!("{}", Self::format_number_with_commas(event.tick)));
                                    ui.label(&event.event_type);
                                    ui.label(&event.description);
                                    ui.end_row();
                                }
                            });
                    }
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "Loading colony events...");
                }
            });
        } else {
            ui.colored_label(egui::Color32::YELLOW, "Loading colony information...");
        }
    }

    fn show_stats_tab(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Update Stats").clicked() {
                let metrics = vec![
                    shared::be_api::StatMetric::CreatureSize,
                    shared::be_api::StatMetric::CreateCanKill,
                    shared::be_api::StatMetric::CreateCanMove,
                    shared::be_api::StatMetric::Food,
                    shared::be_api::StatMetric::Health,
                            shared::be_api::StatMetric::Age,
                ];
                if let Some(results) = get_colony_stats(metrics, self.coordinator_http_port, &self.deployment_mode) {
                    // Cache results in a local field for drawing
                    self.cached_stats = Some(results);
                }
            }
        });
        ui.separator();

        if let Some((tick_count, results)) = &self.cached_stats {
            ui.add_space(5.0);
            let available_width = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(available_width, 0.0),
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    ui.label(format!("Tick: {}", Self::format_number_with_commas(*tick_count)));
                }
            );
            egui::Grid::new("stats_grid")
                .num_columns(3)
                .spacing([20.0, 12.0])
                .show(ui, |ui| {
                    for (idx, item) in results.iter().enumerate() {
                        let (metric, buckets, avg) = (&item.metric, &item.buckets, item.avg);
                        ui.vertical(|ui| {
                            let title = match metric {
                                shared::be_api::StatMetric::Health => "Health",
                                shared::be_api::StatMetric::CreatureSize => "Creature Size",
                                shared::be_api::StatMetric::Food => "Food",
                                shared::be_api::StatMetric::CreateCanKill => "Can Kill",
                                shared::be_api::StatMetric::CreateCanMove => "Can Move",
                                shared::be_api::StatMetric::Age => "Age",
                            };
                            if !buckets.is_empty() {
                                let mut min_v = i32::MAX;
                                let mut max_v = i32::MIN;
                                for b in buckets.iter() {
                                    if b.value < min_v { min_v = b.value; }
                                    if b.value > max_v { max_v = b.value; }
                                }
                                ui.heading(format!("{} ({:.2})", title, avg));
                                ui.label(format!(
                                    "avg={:.2}, range=[{}...{}]",
                                    avg,
                                    Self::format_number_with_commas(min_v as u64),
                                    Self::format_number_with_commas(max_v as u64)
                                ));

                                let chart_size = egui::vec2(200.0, 100.0);
                                let (rect, _resp) = ui.allocate_exact_size(chart_size, egui::Sense::hover());
                                let painter = ui.painter();
                                let border_color = egui::Color32::from_gray(140);
                                painter.rect_stroke(rect, 0.0, egui::Stroke::new(1.0, border_color));

                                let max_occs = buckets.iter().map(|b| b.occs).max().unwrap_or(1) as f32;
                                let value_span = (max_v - min_v).max(1) as f32;
                                let bar_w = 2.0f32;
                                for b in buckets.iter() {
                                    let x_norm = (b.value - min_v) as f32 / value_span;
                                    let x = rect.min.x + x_norm * (rect.width().max(1.0));
                                    let h = (b.occs as f32 / max_occs) * rect.height();
                                    let bar_rect = egui::Rect::from_min_max(
                                        egui::pos2(x - bar_w * 0.5, rect.max.y - h),
                                        egui::pos2(x + bar_w * 0.5, rect.max.y),
                                    );
                                    painter.rect_filled(bar_rect, 0.0, egui::Color32::from_rgb(100, 150, 220));
                                }
                            } else {
                                ui.heading(title);
                                ui.label("No data");
                            }
                        });
                        if (idx + 1) % 3 == 0 { ui.end_row(); }
                    }
                });
        } else {
            ui.label("Click 'Update Stats' to load metrics.");
        }
    }
    
    fn show_cluster_tab(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("Cluster Topology");
            ui.separator();
            
            // Display instance ID in separate info section
            match &self.colony_instance_id {
                Some(id) => {
                    ui.horizontal(|ui| {
                        ui.label("Colony Instance:");
                        ui.label(egui::RichText::new(id).strong().color(egui::Color32::from_rgb(100, 200, 100)));
                    });
                    ui.separator();
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.label("Colony Instance:");
                        ui.colored_label(egui::Color32::YELLOW, "Not set");
                    });
                    ui.separator();
                }
            }
            
            // Deployment mode header
            ui.heading(format!("Deployment Mode: {}", self.deployment_mode));
            ui.add_space(20.0);
            
            // Node list
            ui.group(|ui| {
                egui::Grid::new("cluster_nodes_grid")
                    .num_columns(7)
                    .spacing([20.0, 4.0])
                    .show(ui, |ui| {
                        // Header row
                        ui.label(egui::RichText::new("Role").strong());
                        ui.label(egui::RichText::new("Hostname").strong());
                        ui.label(egui::RichText::new("RPC Port").strong());
                        ui.label(egui::RichText::new("HTTP Port").strong());
                        ui.label(egui::RichText::new("Shards").strong());
                        ui.label(egui::RichText::new("Lat").strong());
                        ui.label(egui::RichText::new("Err %").strong());
                        ui.end_row();

                        ui.separator();
                        ui.separator();
                        ui.separator();
                        ui.separator();
                        ui.separator();
                        ui.separator();
                        ui.separator();
                        ui.end_row();
                        
                        // Coordinator node
                        let coordinator_host = self.cluster_topology.get_coordinator_host();
                        let coordinator_http = self.coordinator_http_port
                            .map(|p| p.to_string())
                            .unwrap_or_else(|| "N/A".to_string());
                        
                        // Get coordinator latency stats
                        let coord_stats = self.latency_tracker.get_node_stats(coordinator_host);
                        let coord_lat_str = coord_stats.avg_latency_ms
                            .map(|lat| format!("{}ms", lat.round() as i32))
                            .unwrap_or_else(|| "N/A".to_string());
                        let coord_err_str = if coord_stats.total_error_rate > 0.0 {
                            format!("{:.1}%", coord_stats.total_error_rate)
                        } else if coord_stats.avg_latency_ms.is_some() {
                            "0%".to_string()
                        } else {
                            "N/A".to_string()
                        };

                        ui.label("Coordinator");
                        ui.label(&coordinator_host.hostname);
                        ui.label(coordinator_host.port.to_string());
                        ui.label(coordinator_http);
                        ui.label("—"); // Coordinator doesn't have shards
                        ui.label(coord_lat_str);
                        ui.label(coord_err_str);
                        ui.end_row();
                        
                        // Backend nodes
                        let backend_hosts = self.cluster_topology.get_all_backend_hosts();
                        let shard_to_host = &self.cluster_topology.shard_to_host;
                        
                        // Calculate shard counts per backend
                        let mut backend_shard_counts: std::collections::HashMap<shared::cluster_topology::HostInfo, usize> = 
                            std::collections::HashMap::new();
                        for (_, host) in shard_to_host.iter() {
                            *backend_shard_counts.entry(host.clone()).or_insert(0) += 1;
                        }
                        
                        // Sort backends for consistent display
                        let mut sorted_backends = backend_hosts.clone();
                        sorted_backends.sort_by(|a, b| {
                            a.hostname.cmp(&b.hostname)
                                .then_with(|| a.port.cmp(&b.port))
                        });
                        
                        for backend in sorted_backends {
                            let shard_count = backend_shard_counts.get(&backend).copied().unwrap_or(0);
                            let backend_http = self.backend_http_ports
                                .get(&backend)
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "N/A".to_string());

                            // Get latency stats for this backend
                            let node_stats = self.latency_tracker.get_node_stats(&backend);

                            let lat_str = node_stats.avg_latency_ms
                                .map(|lat| format!("{}ms", lat.round() as i32))
                                .unwrap_or_else(|| "N/A".to_string());

                            let err_rate_str = if node_stats.total_error_rate > 0.0 {
                                format!("{:.1}%", node_stats.total_error_rate)
                            } else if node_stats.avg_latency_ms.is_some() {
                                "0%".to_string()
                            } else {
                                "N/A".to_string()
                            };

                            ui.label("Backend");
                            ui.label(&backend.hostname);
                            ui.label(backend.port.to_string());
                            ui.label(backend_http);
                            ui.label(shard_count.to_string());
                            ui.label(lat_str);
                            ui.label(err_rate_str);
                            ui.end_row();
                        }
                    });
            });
        });
    }
}

fn retrieve_http_ports(
    mode: &str,
    topology: &ClusterTopology,
) -> Result<(Option<u16>, std::collections::HashMap<shared::cluster_topology::HostInfo, u16>), String> {
    // Initialize cluster registry
    let _registry = create_cluster_registry(mode);
    
    // Create tokio runtime for async operations
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Failed to create tokio runtime: {}", e))?;
    
    // Discover coordinator and get HTTP port
    let coordinator_addr = rt.block_on(ssm::discover_coordinator())
        .ok_or_else(|| "Failed to discover coordinator".to_string())?;
    let coordinator_http_port = Some(coordinator_addr.http_port);
    
    // Discover backends and match with topology
    let backend_addresses = rt.block_on(ssm::discover_backends());
    let mut backend_http_ports = std::collections::HashMap::new();
    
    // Match backend addresses with topology hosts
    // Compare by IP/hostname and internal port
    for backend_addr in backend_addresses {
        // Try to match with coordinator host first (in case coordinator is in backend list)
        let coordinator_host = topology.get_coordinator_host();
        if (backend_addr.private_ip == coordinator_host.hostname || 
            backend_addr.private_ip == "127.0.0.1" && coordinator_host.hostname == "127.0.0.1" ||
            backend_addr.private_ip == "localhost" && coordinator_host.hostname == "localhost") &&
           backend_addr.internal_port == coordinator_host.port {
            // This is the coordinator, skip it
            continue;
        }
        
        // Match with backend hosts
        for backend_host in topology.get_all_backend_hosts() {
            if (backend_addr.private_ip == backend_host.hostname ||
                backend_addr.private_ip == "127.0.0.1" && backend_host.hostname == "127.0.0.1" ||
                backend_addr.private_ip == "localhost" && backend_host.hostname == "localhost") &&
               backend_addr.internal_port == backend_host.port {
                backend_http_ports.insert(backend_host.clone(), backend_addr.http_port);
                break;
            }
        }
    }
    
    Ok((coordinator_http_port, backend_http_ports))
}

fn retrieve_topology(mode: &str) -> Result<(Arc<ClusterTopology>, Option<String>), String> {
    // Initialize cluster registry
    let _registry = create_cluster_registry(mode);
    
    // Discover coordinator
    let rt = tokio::runtime::Runtime::new().map_err(|e| format!("Failed to create tokio runtime: {}", e))?;
    let coordinator_addr = rt.block_on(ssm::discover_coordinator())
        .ok_or_else(|| "Failed to discover coordinator".to_string())?;
    
    // Extract HTTP port from NodeAddress
    let http_port = coordinator_addr.http_port;
    let coordinator_ip = coordinator_addr.public_ip;
    
    // Make HTTP GET request to /topology
    let url = format!("http://{}:{}/topology", coordinator_ip, http_port);
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .send()
        .map_err(|e| format!("Failed to connect to coordinator at {}: {}", url, e))?;
    
    // Check status code
    let status = response.status();
    
    // Check for in-progress status (200 OK with {"status": "in-progress"})
    if status.is_success() {
        let json_text = response.text()
            .map_err(|e| format!("Failed to read response text: {}", e))?;
        let json_value: serde_json::Value = serde_json::from_str(&json_text)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;
        
        // Check if response indicates in-progress
        if json_value.get("status")
            .and_then(|v| v.as_str())
            .map(|s| s == "in-progress")
            .unwrap_or(false) {
            eprintln!("Topology initialization in progress. Waiting for topology to be available...");
            
            // Enter retry loop to wait for topology
            let mut retry_count = 0;
            let max_retries = 10;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500 * (retry_count + 1)));
                
                let retry_response = client
                    .get(&url)
                    .send()
                    .map_err(|e| format!("Failed to retry topology request: {}", e))?;
                
                let retry_status = retry_response.status();
                if retry_status.is_success() {
                    // Check if still in-progress
                    let retry_json_text = retry_response.text()
                        .map_err(|e| format!("Failed to read response text: {}", e))?;
                    let retry_json_value: serde_json::Value = serde_json::from_str(&retry_json_text)
                        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
                    
                    // If still in-progress, continue polling
                    if retry_json_value.get("status")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "in-progress")
                        .unwrap_or(false) {
                        retry_count += 1;
                        if retry_count >= max_retries {
                            return Err("Topology still initializing after maximum retries. Please wait and try again.".to_string());
                        }
                        continue;
                    }
                    
                    // Success! Deserialize and return
                    let colony_instance_id = retry_json_value.get("colony_instance_id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    
                    let topology: ClusterTopology = serde_json::from_value(retry_json_value.clone())
                        .map_err(|e| format!("Failed to deserialize topology: {}", e))?;
                    
                    if let Some(ref id) = colony_instance_id {
                        eprintln!("GUI: Extracted colony instance ID from topology (retry): {}", id);
                    } else {
                        eprintln!("GUI: Warning - colony instance ID is None in topology response (retry)");
                    }
                    
                    return Ok((Arc::new(topology), colony_instance_id));
                } else if retry_status.as_u16() == 404 {
                    // Topology not initialized - automatically initiate colony-start
                    eprintln!("Topology not initialized. Automatically initiating colony-start...");
                    
                    // Generate idempotency key
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let idempotency_key = format!("gui-auto-{}", 
                        SystemTime::now().duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs());
                    
                    // Make POST request to /colony-start
                    let colony_start_url = format!("http://{}:{}/colony-start?idempotency_key={}", 
                        coordinator_ip, http_port, idempotency_key);
                    let colony_start_response = client
                        .post(&colony_start_url)
                        .send()
                        .map_err(|e| format!("Failed to initiate colony-start: {}", e))?;
                    
                    let colony_start_status = colony_start_response.status();
                    if !colony_start_status.is_success() && colony_start_status.as_u16() != 202 {
                        let error_text = colony_start_response.text().unwrap_or_else(|_| "Unknown error".to_string());
                        return Err(format!("Failed to initiate colony-start: HTTP {}: {}", colony_start_status, error_text));
                    }
                    
                    eprintln!("Colony-start initiated. Waiting for topology to be available...");
                    retry_count = 0; // Reset retry count
                    continue;
                } else {
                    // Some other error
                    let error_text = retry_response.text().unwrap_or_else(|_| "Unknown error".to_string());
                    return Err(format!("HTTP error {}: {}", retry_status, error_text));
                }
            }
        } else {
            // Not in-progress, handle as normal success response
            let colony_instance_id = json_value.get("colony_instance_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            
            let topology: ClusterTopology = serde_json::from_value(json_value)
                .map_err(|e| format!("Failed to deserialize topology: {}", e))?;
            
            if let Some(ref id) = colony_instance_id {
                eprintln!("GUI: Extracted colony instance ID from topology: {}", id);
            } else {
                eprintln!("GUI: Warning - colony instance ID is None in topology response");
            }
            
            return Ok((Arc::new(topology), colony_instance_id));
        }
    }
    
    if status.as_u16() == 404 {
        // Topology not initialized - automatically initiate colony-start
        eprintln!("Topology not initialized. Automatically initiating colony-start...");
        
        // Generate idempotency key
        use std::time::{SystemTime, UNIX_EPOCH};
        let idempotency_key = format!("gui-auto-{}", 
            SystemTime::now().duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs());
        
        // Make POST request to /colony-start
        let colony_start_url = format!("http://{}:{}/colony-start?idempotency_key={}", 
            coordinator_ip, http_port, idempotency_key);
        let colony_start_response = client
            .post(&colony_start_url)
            .send()
            .map_err(|e| format!("Failed to initiate colony-start: {}", e))?;
        
        let colony_start_status = colony_start_response.status();
        if !colony_start_status.is_success() && colony_start_status.as_u16() != 202 {
            let error_text = colony_start_response.text().unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("Failed to initiate colony-start: HTTP {}: {}", colony_start_status, error_text));
        }
        
        eprintln!("Colony-start initiated. Waiting for topology to be available...");
        
        // Wait and retry with exponential backoff (up to 10 seconds total)
        let mut retry_count = 0;
        let max_retries = 10;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500 * (retry_count + 1)));
            
            let retry_response = client
                .get(&url)
                .send()
                .map_err(|e| format!("Failed to retry topology request: {}", e))?;
            
            let retry_status = retry_response.status();
            if retry_status.is_success() {
                // Check if response indicates in-progress
                let json_text = retry_response.text()
                    .map_err(|e| format!("Failed to read response text: {}", e))?;
                let json_value: serde_json::Value = serde_json::from_str(&json_text)
                    .map_err(|e| format!("Failed to parse JSON: {}", e))?;
                
                // If still in-progress, continue polling
                if json_value.get("status")
                    .and_then(|v| v.as_str())
                    .map(|s| s == "in-progress")
                    .unwrap_or(false) {
                    retry_count += 1;
                    if retry_count >= max_retries {
                        return Err("Topology still initializing after maximum retries. Please wait and try again.".to_string());
                    }
                    continue;
                }
                
                // Success! Deserialize and return
                let colony_instance_id = json_value.get("colony_instance_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                let topology: ClusterTopology = serde_json::from_value(json_value.clone())
                    .map_err(|e| format!("Failed to deserialize topology: {}", e))?;
                
                if let Some(ref id) = colony_instance_id {
                    eprintln!("GUI: Extracted colony instance ID from topology (retry): {}", id);
                } else {
                    eprintln!("GUI: Warning - colony instance ID is None in topology response (retry)");
                }
                
                return Ok((Arc::new(topology), colony_instance_id));
            } else if retry_status.as_u16() != 404 {
                // Some other error
                let error_text = retry_response.text().unwrap_or_else(|_| "Unknown error".to_string());
                return Err(format!("HTTP error {}: {}", retry_status, error_text));
            }
            
            // Still 404, retry if we haven't exceeded max retries
            retry_count += 1;
            if retry_count >= max_retries {
                return Err("Topology still not initialized after colony-start. Please wait and try again.".to_string());
            }
        }
    }
    
    // If we reach here, it's an unexpected error
    let error_text = response.text().unwrap_or_else(|_| "Unknown error".to_string());
    Err(format!("HTTP error {}: {}", status, error_text))
}

fn main() -> eframe::Result<()> {
    eprintln!("GUI MAIN ENTERED");
    // Parse command line arguments for mode
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("localhost");
    
    if mode != "localhost" && mode != "aws" {
        eprintln!("Error: Mode must be 'localhost' or 'aws'");
        eprintln!("Usage: {} [localhost|aws]", args[0]);
        std::process::exit(1);
    }
    
    // Initialize logging
    let log_file = if mode == "aws" {
        "/data/distributed-colony/output/logs/gui.log"
    } else {
        "output/logs/gui.log"
    };
    shared::logging::init_logging(log_file);
    shared::logging::log_startup("GUI");
    shared::logging::set_panic_hook();
    
    // Retrieve topology from coordinator
    let (topology, colony_instance_id) = match retrieve_topology(mode) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error: Failed to retrieve topology: {}", e);
            eprintln!("Please ensure the coordinator is running and the colony is started.");
            std::process::exit(1);
        }
    };
    
    // Retrieve HTTP ports from ClusterRegistry
    let (coordinator_http_port, backend_http_ports) = match retrieve_http_ports(mode, topology.as_ref()) {
        Ok(ports) => ports,
        Err(e) => {
            eprintln!("Warning: Failed to retrieve HTTP ports: {}", e);
            eprintln!("HTTP ports will be shown as N/A in the Cluster tab.");
            (None, std::collections::HashMap::new())
        }
    };
    
    let deployment_mode = mode.to_string();
    let topology_clone = Arc::clone(&topology);
    let coordinator_http_port_clone = coordinator_http_port;
    let backend_http_ports_clone = backend_http_ports;
    let colony_instance_id_clone = colony_instance_id;
    
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Colony Viewer",
        options,
        Box::new(move |cc| {
            // Ensure default fonts are installed
            let fonts = egui::FontDefinitions::default();
            cc.egui_ctx.set_fonts(fonts);
            Ok(Box::new(BEImageApp::new(
                Arc::clone(&topology_clone),
                deployment_mode.clone(),
                coordinator_http_port_clone,
                backend_http_ports_clone.clone(),
                colony_instance_id_clone.clone(),
            )))
        }),
    )
}
