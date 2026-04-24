use eframe::egui;
use crate::trie::{Entry, HOTNode, HOT};
use std::collections::{HashMap, HashSet};


pub struct HotApp {
    trie: HOT<String, String>,
    new_key: String,
    last_op_message: String,
    // For visualization
    highlighted_nodes: HashSet<u64>,
    zoom: f32,
    pan: egui::Vec2,
    batch_counter: usize, // To ensure unique batch keys
    fanout: usize,
    inserted_data: HashMap<String, String>,
}

impl Default for HotApp {
    fn default() -> Self {
        Self {
            trie: HOT::new(32),
            new_key: String::new(),
            last_op_message: String::from("Ready"),
            highlighted_nodes: HashSet::new(),
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            batch_counter: 0,
            fanout: 32,
            inserted_data: HashMap::new(),
        }
    }
}

impl HotApp {
    fn capture_heights(&self) -> HashMap<u64, u16> {
        let mut map = HashMap::new();
        if let Some(root) = &self.trie.root {
            self.collect_heights_recursive(root, &mut map);
        }
        map
    }

    fn collect_heights_recursive(
        &self,
        node: &HOTNode<String, String>,
        map: &mut HashMap<u64, u16>,
    ) {
        let id = node.id;
        map.insert(id, node.height);
        for entry in &node.entries {
            if let Entry::Child(_, child) = entry {
                self.collect_heights_recursive(&child, map);
            }
        }
    }

    fn update_highlights(&mut self, old_heights: HashMap<u64, u16>) {
        let new_heights = self.capture_heights();
        self.highlighted_nodes.clear();
        
        for (id, &height) in &new_heights {
            if let Some(&old_h) = old_heights.get(id) {
                if old_h != height {
                    self.highlighted_nodes.insert(*id);
                }
            } else {
                self.highlighted_nodes.insert(*id);
            }
        }
    }

    fn rebuild_trie(&mut self) {
        let mut new_trie = HOT::new(self.fanout);
        // We need to insert in a stable order or just re-insert everything.
        // For HOT, order matters for the final structure if multiple ways to split exist,
        // but here it's mostly about the fanout.
        let mut keys: Vec<_> = self.inserted_data.keys().cloned().collect();
        keys.sort();
        for k in keys {
            new_trie.insert(k.clone(), self.inserted_data.get(&k).unwrap().clone());
        }
        self.trie = new_trie;
    }

    fn get_subtree_width(node: &HOTNode<String, String>) -> f32 {
        if node.entries.is_empty() {
            return 120.0;
        }

        let mut total_width = 0.0;
        for entry in &node.entries {
            match entry {
                Entry::Child(_, child) => {
                    total_width += Self::get_subtree_width(&child);
                }
                Entry::Leaf(_, _) => {
                    total_width += 100.0; // Increased from 80
                }
            }
        }

        let spacing = (node.entries.len().saturating_sub(1)) as f32 * 50.0;
        f32::max(total_width + spacing, 140.0)
    }

    fn draw_node_recursive(
        highlighted_nodes: &mut HashSet<u64>,
        last_op_message: &mut String,
        ui: &mut egui::Ui,
        node: &HOTNode<String, String>,
        pos: egui::Pos2,
        zoom: f32,
        is_root: bool,
    ) {
        let id = node.id;
        let is_highlighted = highlighted_nodes.contains(&id);

        // 1. Scale base node dimensions
        let node_width = 150.0 * zoom;
        let node_height = 70.0 * zoom;
        let v_spacing = 220.0 * zoom; // Increased from 180
        let padding = 50.0 * zoom; // Increased from 35

        let rect = egui::Rect::from_center_size(pos, egui::vec2(node_width, node_height));

        // INTERACTION: Click to highlight
        let response = ui.interact(rect, ui.id().with(id), egui::Sense::click());
        if response.clicked() {
            highlighted_nodes.clear();
            highlighted_nodes.insert(id);
            *last_op_message = format!("Node selected: ID {}", id);
        }

        // Modern Premium Colors
        let fill_color = if is_highlighted {
            egui::Color32::from_rgb(255, 165, 0) // Orange-Gold for highlight
        } else {
            egui::Color32::from_rgb(45, 45, 60) // Dark sleek slate
        };
        
        let stroke_color = if is_root {
            egui::Color32::from_rgb(0, 255, 255) // Cyan for root
        } else if is_highlighted {
            egui::Color32::from_rgb(255, 255, 255)
        } else {
            egui::Color32::from_rgb(100, 100, 120)
        };

        // 2. Scale strokes and corner radii
        ui.painter().rect_filled(rect, 8.0 * zoom, fill_color);
        ui.painter().rect_stroke(
            rect,
            8.0 * zoom,
            egui::Stroke::new(if is_root || is_highlighted { 4.0 * zoom } else { 2.0 * zoom }, stroke_color),
        );

        // 3. Scale text
        let label = format!("Height: {}", node.height);
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(18.0 * zoom),
            egui::Color32::WHITE,
        );

        // 4. Scale subtree widths
        let total_width = Self::get_subtree_width(node) * zoom;
        let mut current_x = pos.x - total_width / 2.0;

        for entry in &node.entries {
            let entry_width = match entry {
                Entry::Child(_, child) => Self::get_subtree_width(&child) * zoom,
                Entry::Leaf(_, _) => 100.0 * zoom,
            };

            let child_center_x = current_x + entry_width / 2.0;
            
            // CURVED LAYOUT: Calculate a vertical offset based on horizontal distance from center
            // This creates a "fanning out" effect
            let horizontal_offset = (child_center_x - pos.x).abs();
            let curve_depth = (horizontal_offset / total_width.max(1.0)) * 120.0 * zoom; 
            let child_pos = egui::pos2(child_center_x, pos.y + v_spacing + curve_depth);

            // 5. BEZIER CONNECTIONS - Premium Look
            let start = pos + egui::vec2(0.0, node_height / 2.0);
            let end = child_pos - egui::vec2(0.0, 30.0 * zoom);
            let control1 = start + egui::vec2(0.0, v_spacing * 0.4);
            let control2 = end - egui::vec2(0.0, v_spacing * 0.4);
            
            ui.painter().add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape {
                points: [start, control1, control2, end],
                closed: false,
                fill: egui::Color32::TRANSPARENT,
                stroke: egui::Stroke::new(2.5 * zoom, egui::Color32::from_rgb(100, 100, 150)),
            }));

            match entry {
                Entry::Leaf(k, _) => {
                    let leaf_id = k as *const _ as u64;
                    let is_leaf_highlighted = highlighted_nodes.contains(&leaf_id);
                    
                    let leaf_rect = egui::Rect::from_center_size(child_pos, egui::vec2(20.0 * zoom, 20.0 * zoom));
                    let leaf_response = ui.interact(leaf_rect, ui.id().with(leaf_id), egui::Sense::click());
                    
                    if leaf_response.clicked() {
                        highlighted_nodes.clear();
                        highlighted_nodes.insert(leaf_id);
                        *last_op_message = format!("Leaf selected: '{}' (ID {})", k, leaf_id);
                    }

                    ui.painter().circle_filled(
                        child_pos,
                        8.0 * zoom,
                        if is_leaf_highlighted { egui::Color32::from_rgb(255, 165, 0) } else { egui::Color32::from_rgb(0, 200, 150) },
                    );
                    
                    if is_leaf_highlighted {
                        ui.painter().circle_stroke(child_pos, 10.0 * zoom, egui::Stroke::new(2.5 * zoom, egui::Color32::WHITE));
                    }

                    ui.painter().text(
                        child_pos + egui::vec2(0.0, 20.0 * zoom),
                        egui::Align2::CENTER_TOP,
                        format!("'{}'", k),
                        egui::FontId::proportional(14.0 * zoom),
                        if is_leaf_highlighted { egui::Color32::WHITE } else { egui::Color32::from_rgb(220, 220, 240) },
                    );
                }
                Entry::Child(rep, child) => {
                    // Division reason label (prominent)
                    let text_pos = (pos + child_pos.to_vec2()) / 2.0 + egui::vec2(8.0 * zoom, -10.0 * zoom);
                    ui.painter().text(
                        text_pos,
                        egui::Align2::LEFT_CENTER,
                        format!("rep: {}", rep),
                        egui::FontId::proportional(13.0 * zoom),
                        egui::Color32::from_rgb(180, 180, 255),
                    );
                    Self::draw_node_recursive(highlighted_nodes, last_op_message, ui, &child, child_pos, zoom, false);
                }
            }

            current_x += entry_width + padding;
        }
    }

}

impl eframe::App for HotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Side Panel (Modernized Sidebar) ---
        // This panel is fixed and opaque, completely separate from the graph canvas.
        egui::SidePanel::left("control_panel")
            .resizable(false)
            .default_width(320.0)
            .frame(egui::Frame::none()
                .fill(egui::Color32::from_rgb(32, 33, 36)) // Pure solid background (No transparency)
                .inner_margin(egui::Margin::symmetric(20.0, 15.0))
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 80, 100))) // Strong visual boundary
            )
            .show(ctx, |ui| {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.heading(egui::RichText::new("HOT Engine").size(24.0).strong().color(egui::Color32::WHITE));
                    ui.label(egui::RichText::new("Interactive Visualizer").italics().color(egui::Color32::from_rgb(180, 180, 200)));
                });
                
                ui.add_space(30.0);
                ui.separator();
                ui.add_space(20.0);

                let group_frame = egui::Frame::none()
                    .fill(egui::Color32::from_gray(35))
                    .rounding(10.0)
                    .inner_margin(12.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

                // --- Section 1: Manual Control ---
                group_frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("MANUAL CONTROL").small().strong().color(egui::Color32::from_rgb(120, 120, 255)));
                    ui.add_space(10.0);

                    ui.label("Target Key:");
                    ui.add_sized(
                        [ui.available_width(), 32.0],
                        egui::TextEdit::singleline(&mut self.new_key)
                            .hint_text("Enter key...")
                    );

                    ui.add_space(15.0);

                    let button_size = egui::vec2(ui.available_width() / 2.1, 42.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized(button_size, egui::Button::new(egui::RichText::new("➕ Insert").size(16.0)).rounding(6.0)).clicked() {
                            let old_heights = self.capture_heights();
                            let val = format!("val_{}", self.new_key);
                            self.trie.insert(self.new_key.clone(), val.clone());
                            self.inserted_data.insert(self.new_key.clone(), val);
                            self.update_highlights(old_heights);
                            self.last_op_message = format!("Inserted: {}", self.new_key);
                        }

                        if ui.add_sized(button_size, egui::Button::new(egui::RichText::new("🔍 Search").size(16.0)).rounding(6.0)).clicked() {
                            let (val, path) = self.trie.lookup_with_path(&self.new_key);
                            self.highlighted_nodes.clear();
                            for node_id in path {
                                self.highlighted_nodes.insert(node_id);
                            }
                            
                            if let Some(val) = val {
                                self.last_op_message = format!("Found: {}", val);
                            } else {
                                self.last_op_message = format!("Not found: {}", self.new_key);
                            }
                        }
                    });

                    if !self.highlighted_nodes.is_empty() {
                        ui.add_space(10.0);
                        if ui.add_sized(
                            [ui.available_width(), 36.0],
                            egui::Button::new(egui::RichText::new("🗑 Delete Highlighted").size(14.0))
                                .fill(egui::Color32::from_rgb(150, 50, 50))
                                .rounding(6.0)
                        ).clicked() {
                            let to_delete: Vec<u64> = self.highlighted_nodes.iter().cloned().collect();
                            let mut deleted_any = false;
                            for id in to_delete {
                                if self.trie.remove_by_id(id) {
                                    // Note: We don't easily know which key was deleted if we delete by node ID.
                                    // For simplicity in this demo, let's just keep inserted_data as is or
                                    // ideally we'd find all keys in that subtree and remove them.
                                    // Since we want rebuild to work, let's just clear inserted_data for now
                                    // if we delete subtrees, or just accept it's out of sync.
                                    // Better: Don't allow rebuild if data is out of sync?
                                    // Actually, let's just clear everything on delete for now to be safe.
                                    deleted_any = true;
                                }
                            }
                            if deleted_any {
                                // To keep it simple, if we delete a subtree, we just clear the history
                                // because we don't know which keys were in it easily without traversing.
                                self.inserted_data.clear(); 
                                self.highlighted_nodes.clear();
                                self.last_op_message = "Subtree(s) deleted. History cleared.".to_string();
                            } else {
                                self.last_op_message = "Could not delete node.".to_string();
                            }
                        }
                    }
                });

                ui.add_space(15.0);

                // --- Section 2: Demo Scenarios ---
                group_frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("DEMO SCENARIOS").small().strong().color(egui::Color32::from_rgb(255, 120, 120)));
                    ui.add_space(10.0);

                    if ui.add_sized([ui.available_width(), 40.0], egui::Button::new(format!("Scenario: Dense Cluster ({}-1 keys)", self.fanout))).clicked() {
                        let old_heights = self.capture_heights();
                        for i in 0..self.fanout {
                            let key = format!("apple_{:02}", i);
                            let val = format!("val_{}", key);
                            self.trie.insert(key.clone(), val.clone());
                            self.inserted_data.insert(key, val);
                        }
                        self.update_highlights(old_heights);
                        let h = self.trie.root.as_ref().map(|n| n.height).unwrap_or(0);
                        self.last_op_message = format!("Height: {} | Node at capacity ({} keys). Next insert triggers split.", h, self.fanout - 1);
                    }
                    
                    ui.add_space(8.0);

                    if ui.add_sized([ui.available_width(), 40.0], egui::Button::new("Trigger Overflow")).clicked() {
                        let old_heights = self.capture_heights();
                        let key = format!("apple_{:02}", self.fanout);
                        let val = format!("val_{}", key);
                        self.trie.insert(key.clone(), val.clone());
                        self.inserted_data.insert(key.clone(), val);
                        self.update_highlights(old_heights);
                        let h = self.trie.root.as_ref().map(|n| n.height).unwrap_or(0);
                        self.last_op_message = format!("Height remains {} despite {} keys (Parent Pull Up).", h, self.fanout);
                    }

                    ui.add_space(8.0);

                    if ui.add_sized([ui.available_width(), 40.0], egui::Button::new("Scenario: Sparse Range")).clicked() {
                        let old_heights = self.capture_heights();
                        let sparse_keys = vec!["alpha", "bravo", "charlie", "zebra"];
                        for k in sparse_keys {
                            let key = k.to_string();
                            let val = format!("val_{}", key);
                            self.trie.insert(key.clone(), val.clone());
                            self.inserted_data.insert(key, val);
                        }
                        self.update_highlights(old_heights);
                        self.last_op_message = "Sparse keys demonstrating Adaptive Span.".to_string();
                    }

                    ui.add_space(12.0);
                    ui.separator();
                    ui.add_space(12.0);

                    if ui.add_sized([ui.available_width(), 40.0], egui::Button::new("Clear & Reset").fill(egui::Color32::from_rgb(80, 80, 80))).clicked() {
                        self.trie = HOT::new(self.fanout);
                        self.batch_counter = 0;
                        self.new_key.clear();
                        self.inserted_data.clear();
                        self.last_op_message = "App Reset".to_string();
                        self.zoom = 1.0;
                        self.pan = egui::Vec2::ZERO;
                        self.highlighted_nodes.clear();
                    }
                });

                ui.add_space(15.0);

                // --- Section 3: Engine Config ---
                group_frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("ENGINE CONFIG").small().strong().color(egui::Color32::from_rgb(255, 255, 120)));
                    ui.add_space(10.0);

                    ui.label("Max Fanout:");
                    let old_fanout = self.fanout;
                    if ui.add(egui::Slider::new(&mut self.fanout, 2..=32).text("entries")).changed() {
                        if self.fanout != old_fanout {
                            self.rebuild_trie();
                            self.last_op_message = format!("Fanout updated to {}. Trie recalculated.", self.fanout);
                        }
                    }
                    
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new("Adjusting fanout rebuilds the trie structure.").small().color(egui::Color32::GRAY));
                });

                ui.add_space(15.0);

                // --- Section 4: View Controls ---
                group_frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("VIEW CONTROLS").small().strong().color(egui::Color32::from_rgb(120, 255, 120)));
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        ui.label(format!("Zoom: {:.2}x", self.zoom));
                        if ui.button("⟲ Reset View").clicked() {
                            self.zoom = 1.0;
                            self.pan = egui::Vec2::ZERO;
                        }
                    });
                    
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new("🖱 Scroll to Zoom").small().color(egui::Color32::GRAY));
                    ui.label(egui::RichText::new("🖱 Drag to Pan").small().color(egui::Color32::GRAY));
                });

                // --- Section 4: Status Area (Bottom) ---
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(20.0);
                    let status_color = if self.last_op_message.contains("Not found") || self.last_op_message.contains("Could not") {
                        egui::Color32::from_rgb(255, 100, 100)
                    } else if self.last_op_message == "Ready" || self.last_op_message == "Trie Reset" {
                        egui::Color32::from_rgb(150, 150, 150)
                    } else {
                        egui::Color32::from_rgb(100, 255, 200)
                    };
                    
                    ui.label(egui::RichText::new(&self.last_op_message).size(16.0).color(status_color).strong());
                    ui.label(egui::RichText::new("SYSTEM STATUS").small().strong().color(egui::Color32::GRAY));
                    ui.separator();
                });
            });


        // --- Central Panel (Graph Canvas) ---
        // This area is reserved strictly for the trie visualization.
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(15, 15, 20))) // Opaque canvas background
            .show(ctx, |ui| {
                // CLIP: Ensure no graph elements bleed outside this central area
                let canvas_rect = ui.max_rect();
                ui.set_clip_rect(canvas_rect);

            // 1. Handle Input for Zoom and Pan (Only within the canvas area)
            let response = ui.interact(canvas_rect, ui.id(), egui::Sense::click_and_drag());

            // Pan logic
            if response.dragged() {
                self.pan += response.drag_delta();
            }

            // Zoom logic (Scroll Wheel)
            let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_delta = (scroll_delta / 200.0).exp();
                let old_zoom = self.zoom;
                self.zoom *= zoom_delta;
                self.zoom = self.zoom.clamp(0.05, 10.0);
                
                let actual_zoom_ratio = self.zoom / old_zoom;
                
                if let Some(mouse_pos) = ctx.input(|i| i.pointer.hover_pos()) {
                    let anchor = egui::pos2(canvas_rect.center().x, canvas_rect.top() + 120.0);
                    // Adjust pan to zoom towards mouse position
                    self.pan -= (mouse_pos - anchor - self.pan) * (actual_zoom_ratio - 1.0);
                }
            }

            // 2. Render the Tree - Ensure clipping to the CentralPanel
            if let Some(root) = &self.trie.root {
                // Visual Indicator for Tree Height
                ui.vertical_centered(|ui| {
                    ui.add_space(15.0);
                    ui.label(
                        egui::RichText::new(format!("Current Tree Height: {}", root.height))
                            .size(36.0)
                            .strong()
                            .color(egui::Color32::from_rgb(180, 180, 255))
                    );
                });

                // Calculate centered starting position with Pan and Zoom
                let center_x = canvas_rect.center().x + self.pan.x;
                let start_y = canvas_rect.top() + 120.0 + self.pan.y;
                let start_pos = egui::pos2(center_x, start_y);

                // Draw tree
                Self::draw_node_recursive(
                    &mut self.highlighted_nodes,
                    &mut self.last_op_message,
                    ui,
                    root,
                    start_pos,
                    self.zoom,
                    true, // This is the root
                );
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Trie is empty. Insert a key to begin.").size(18.0).color(egui::Color32::GRAY));
                });
            }
        });
    }
}