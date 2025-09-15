#![allow(deprecated)]
use eframe::{egui, App};
use egui_extras::RetainedImage;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use shared::be_api::ShardLayer;
use shared::cluster_topology::ClusterTopology;
mod call_be;
mod connection_pool;

const REFRESH_INTERVAL_MS: u64 = 100;
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
    Info,
}

#[derive(Clone)]
pub struct ShardConfig {
    pub total_width: i32,
    pub total_height: i32,
    pub cols: usize,
    pub rows: usize,
}

impl Default for ShardConfig {
    fn default() -> Self {
        let width_in_shards = ClusterTopology::get_width_in_shards();
        let height_in_shards = ClusterTopology::get_height_in_shards();
        let shard_width = ClusterTopology::get_shard_width();
        let shard_height = ClusterTopology::get_shard_height();
        
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
        ClusterTopology::get_shard_width()
    }
    
    fn shard_height(&self) -> i32 {
        ClusterTopology::get_shard_height()
    }
    
    fn total_shards(&self) -> usize {
        self.cols * self.rows
    }
    
    fn get_shard(&self, index: usize) -> shared::be_api::Shard {
        let row = index / self.cols;
        let col = index % self.cols;
        
        let shard_width = ClusterTopology::get_shard_width();
        let shard_height = ClusterTopology::get_shard_height();
        
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
    colony_info: Arc<Mutex<Option<(Option<shared::be_api::ColonyLifeInfo>, Option<u64>)>>>,
    colony_info_last_update: Arc<Mutex<Instant>>,
    ctx: Option<egui::Context>,
    thread_started: bool,
    current_tab: Tab,
    shared_current_tab: Arc<Mutex<Tab>>,
    shard_config: Arc<Mutex<ShardConfig>>,
    cluster_topology: &'static ClusterTopology,
    last_update_time: Arc<Mutex<Instant>>,
}

impl Default for BEImageApp {
    fn default() -> Self {
        let shard_config = Arc::new(Mutex::new(ShardConfig::default()));
        let total_shards = {
            let config_guard = shard_config.lock().unwrap();
            config_guard.total_shards()
        };
        
        // Initialize cluster topology
        let cluster_topology = call_be::get_cluster_topology();
        let creatures = Arc::new(Mutex::new(call_be::get_all_shard_retained_images(&shard_config.lock().unwrap(), cluster_topology)));
        let creatures_color_data = Arc::new(Mutex::new(call_be::get_all_shard_color_data(&shard_config.lock().unwrap(), cluster_topology)));
        let extra_food = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let sizes = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let can_kill = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let can_move = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let cost_per_turn = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let food = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let health = Arc::new(Mutex::new((0..total_shards).map(|_| None).collect()));
        let colony_info = Arc::new(Mutex::new(None));
        let colony_info_last_update = Arc::new(Mutex::new(Instant::now()));
        let current_tab = Tab::Creatures;
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
            colony_info,
            colony_info_last_update,
            ctx: None,
            thread_started: false,
            current_tab,
            shared_current_tab: Arc::new(Mutex::new(current_tab)),
            shard_config,
            cluster_topology,
            last_update_time: Arc::new(Mutex::new(Instant::now())),
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
            let colony_info = self.colony_info.clone();
            let colony_info_last_update = self.colony_info_last_update.clone();
            let ctx_clone = ctx.clone();
            let shared_current_tab = self.shared_current_tab.clone();
            let shard_config = self.shard_config.clone();
            let cluster_topology = self.cluster_topology;
            let last_update_time = self.last_update_time.clone();
            thread::spawn(move || {
                loop {
                    // Look at the selected tab and get only the info required for the current Tab
                    let tab = *shared_current_tab.lock().unwrap();
                    let config = shard_config.lock().unwrap().clone();
                    
                    match tab {
                        Tab::Creatures => {
                            let images = call_be::get_all_shard_retained_images(&config, cluster_topology);
                            let color_data = call_be::get_all_shard_color_data(&config, cluster_topology);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !images.iter().all(|img| img.is_none()) {
                                let mut locked = creatures.lock().unwrap();
                                *locked = images;
                                *last_update_time.lock().unwrap() = Instant::now();
                            }
                            if !color_data.iter().all(|data| data.is_none()) {
                                let mut locked = creatures_color_data.lock().unwrap();
                                *locked = color_data;
                            }
                        }
                        Tab::ExtraFood => {
                            let extra_food_data = call_be::get_all_shard_layer_data(ShardLayer::ExtraFood, &config, cluster_topology);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !extra_food_data.iter().all(|data| data.is_none()) {
                                let mut locked = extra_food.lock().unwrap();
                                *locked = extra_food_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                            }
                        }
                        Tab::Sizes => {
                            let sizes_data = call_be::get_all_shard_layer_data(ShardLayer::CreatureSize, &config, cluster_topology);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !sizes_data.iter().all(|data| data.is_none()) {
                                let mut locked = sizes.lock().unwrap();
                                *locked = sizes_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                            }
                        }
                        Tab::CanKill => {
                            let can_kill_data = call_be::get_all_shard_layer_data(ShardLayer::CanKill, &config, cluster_topology);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !can_kill_data.iter().all(|data| data.is_none()) {
                                let mut locked = can_kill.lock().unwrap();
                                *locked = can_kill_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                            }
                        }
                        Tab::CanMove => {
                            let can_move_data = call_be::get_all_shard_layer_data(ShardLayer::CanMove, &config, cluster_topology);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !can_move_data.iter().all(|data| data.is_none()) {
                                let mut locked = can_move.lock().unwrap();
                                *locked = can_move_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                            }
                        }
                        Tab::CostPerTurn => {
                            let cost_per_turn_data = call_be::get_all_shard_layer_data(ShardLayer::CostPerTurn, &config, cluster_topology);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !cost_per_turn_data.iter().all(|data| data.is_none()) {
                                let mut locked = cost_per_turn.lock().unwrap();
                                *locked = cost_per_turn_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                            }
                        }
                        Tab::Food => {
                            let food_data = call_be::get_all_shard_layer_data(ShardLayer::Food, &config, cluster_topology);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !food_data.iter().all(|data| data.is_none()) {
                                let mut locked = food.lock().unwrap();
                                *locked = food_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                            }
                        }
                        Tab::Health => {
                            let health_data = call_be::get_all_shard_layer_data(ShardLayer::Health, &config, cluster_topology);
                            // Only update if we got valid data (don't overwrite with None on backend failures)
                            if !health_data.iter().all(|data| data.is_none()) {
                                let mut locked = health.lock().unwrap();
                                *locked = health_data;
                                *last_update_time.lock().unwrap() = Instant::now();
                            }
                        }
                        Tab::Info => {
                            // Update colony info every 1 second when Info tab is active
                            let last_update = *colony_info_last_update.lock().unwrap();
                            if last_update.elapsed().as_secs() >= 1 {
                                if let Some(info) = call_be::get_colony_info(cluster_topology) {
                                    let mut locked = colony_info.lock().unwrap();
                                    *locked = Some(info);
                                    *colony_info_last_update.lock().unwrap() = Instant::now();
                                    *last_update_time.lock().unwrap() = Instant::now();
                                }
                            }
                        }
                    }
                    ctx_clone.request_repaint();
                    thread::sleep(Duration::from_millis(REFRESH_INTERVAL_MS));
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
                ui.selectable_value(&mut self.current_tab, Tab::Info, "Info");
                
                // Update shared tab if changed
                if self.current_tab != old_tab {
                    if let Ok(mut shared_tab) = self.shared_current_tab.lock() {
                        *shared_tab = self.current_tab;
                    }
                }
                
                // Show status indicator only when there are issues
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let last_update = *self.last_update_time.lock().unwrap();
                    let time_since_update = last_update.elapsed();
                    if time_since_update.as_secs() > 5 {
                        ui.colored_label(egui::Color32::RED, "⚠️ Backend Unresponsive");
                    } else if time_since_update.as_millis() > 1000 {
                        ui.colored_label(egui::Color32::YELLOW, "🔄 Slow Response");
                    }
                    // Don't show anything when all is well (time_since_update <= 1000ms)
                });
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
                Tab::Info => self.show_info_tab(ui),
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
    
    fn show_creatures_tab(&self, ui: &mut egui::Ui) {
        let locked = self.creatures_color_data.lock().unwrap();
        
        self.show_combined_image(ui, &locked, |shard_data| {
            shard_data.clone()
        });
    }

    fn show_combined_image<T, F>(&self, ui: &mut egui::Ui, data: &[Option<T>], converter: F)
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
        
        // Display the combined image
        let retained_image = egui_extras::RetainedImage::from_color_image("combined", combined_img);
        retained_image.show_max_size(ui, egui::vec2(800.0, 600.0));
    }

    fn show_layer_tab(&self, ui: &mut egui::Ui, data: &Arc<Mutex<Vec<Option<Vec<i32>>>>>) {
        self.show_layer_tab_with_legend(ui, data, None)
    }

    fn show_layer_tab_with_legend(&self, ui: &mut egui::Ui, data: &Arc<Mutex<Vec<Option<Vec<i32>>>>>, legend_max_value: Option<i32>) {
        let locked = data.lock().unwrap();
        
        // Find global maximum across all shards for consistent normalization
        let global_max = locked.iter()
            .filter_map(|shard_data| shard_data.as_ref())
            .flat_map(|data| data.iter())
            .max()
            .copied()
            .unwrap_or(0);

        // Use provided legend values or calculate from data
        let legend_min = 0;
        let legend_max = legend_max_value.unwrap_or(global_max);
        let global_max = legend_max.max(global_max);

        self.show_combined_image(ui, &locked, |shard_data| {
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

    fn show_extra_food_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab(ui, &self.extra_food);
    }

    fn show_sizes_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab_with_legend(ui, &self.sizes, Some(MIN_CREATURE_SIZE_LEGEND_MAX));
    }

    fn show_can_kill_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab_boolean(ui, &self.can_kill);
    }

    fn show_can_move_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab_boolean(ui, &self.can_move);
    }

    fn show_cost_per_turn_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab(ui, &self.cost_per_turn);
    }

    fn show_food_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab_with_legend(ui, &self.food, Some(FOOD_VALUE_LEGEND_MAX));
    }

    fn show_health_tab(&self, ui: &mut egui::Ui) {
        self.show_layer_tab_with_legend(ui, &self.health, Some(10)); 
    }

    fn show_layer_tab_boolean(&self, ui: &mut egui::Ui, data: &Arc<Mutex<Vec<Option<Vec<i32>>>>>) {
        let locked = data.lock().unwrap();
        
        self.show_combined_image(ui, &locked, |shard_data| {
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
        ui.heading("Colony Information");
        ui.separator();
        
        // Get cached colony info (updated by background thread)
        let colony_info_guard = self.colony_info.lock().unwrap();
        if let Some((colony_life_info, current_tick)) = colony_info_guard.as_ref() {
            ui.group(|ui| {
                ui.heading("Current Status");
                
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
            
            // Display ColonyLifeInfo in a table format
            if let Some(life_info) = colony_life_info {
                ui.group(|ui| {
                    ui.heading("Colony Life Configuration");
                    
                    egui::Grid::new("colony_life_info_grid")
                        .num_columns(2)
                        .spacing([20.0, 4.0])
                        .show(ui, |ui| {
                            ui.label("Health Cost Per Size Unit:");
                            ui.label(format!("{}", life_info.health_cost_per_size_unit));
                            ui.end_row();
                            
                            ui.label("Eat Capacity Per Size Unit:");
                            ui.label(format!("{}", life_info.eat_capacity_per_size_unit));
                            ui.end_row();
                            
                            ui.label("Health Cost If Can Kill:");
                            ui.label(format!("{}", life_info.health_cost_if_can_kill));
                            ui.end_row();
                            
                            ui.label("Health Cost If Can Move:");
                            ui.label(format!("{}", life_info.health_cost_if_can_move));
                            ui.end_row();
                            
                            ui.label("Mutation Chance:");
                            ui.label(format!("{}", life_info.mutation_chance));
                            ui.end_row();
                        });
                });
            } else {
                ui.label("Colony Life Configuration: Not available");
            }
        } else {
            ui.colored_label(egui::Color32::YELLOW, "Loading colony information...");
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Colony Viewer",
        options,
        Box::new(|_cc| Box::new(BEImageApp::default())),
    )
}
