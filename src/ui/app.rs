use eframe::egui;
use crate::trie::{Entry, HOTNode, HOT};
use std::collections::{HashMap, HashSet};


pub struct HotApp {
    trie: HOT<String, String>,
    new_key: String,
    last_op_message: String,
    // For visualization
    highlighted_nodes: HashSet<usize>,
    zoom: f32,
    pan: egui::Vec2,
    batch_counter: usize, // To ensure unique batch keys
}

impl Default for HotApp {
    fn default() -> Self {
        Self {
            trie: HOT::new(),
            new_key: String::new(),
            last_op_message: String::from("Ready"),
            highlighted_nodes: HashSet::new(),
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            batch_counter: 0,
        }
    }
}

impl HotApp {
    fn capture_heights(&self) -> HashMap<usize, u16> {
        let mut map = HashMap::new();
        if let Some(root) = &self.trie.root {
            self.collect_heights_recursive(root, &mut map);
        }
        map
    }

    fn collect_heights_recursive(
        &self,
        node: &HOTNode<String, String>,
        map: &mut HashMap<usize, u16>,
    ) {
        let id = node as *const _ as usize;
        map.insert(id, node.height);
        for entry in &node.entries {
            if let Entry::Child(_, child) = entry {
                self.collect_heights_recursive(child, map);
            }
        }
    }

    fn update_highlights(&mut self, old_heights: HashMap<usize, u16>) {
        let new_heights = self.capture_heights();
        self.highlighted_nodes.clear();

        // This is tricky because pointers change when nodes are re-boxed.
        // However, we can highlight any node that has a different height than its "logical" equivalent.
        // For simplicity in this live viewer, we'll highlight nodes that are present in new_heights
        // but whose height value is different from what was recorded at similar "representative key" positions?
        // Actually, let's just highlight nodes that were modified.
        
        for (id, &height) in &new_heights {
            if let Some(&old_h) = old_heights.get(id) {
                if old_h != height {
                    self.highlighted_nodes.insert(*id);
                }
            } else {
                // It's a new node. Usually new nodes in HOT are the ones that changed height or were split.
                self.highlighted_nodes.insert(*id);
            }
        }
    }

    fn get_subtree_width(node: &HOTNode<String, String>) -> f32 {
        if node.entries.is_empty() {
            return 100.0;
        }

        let mut total_width = 0.0;
        for entry in &node.entries {
            match entry {
                Entry::Child(_, child) => {
                    total_width += Self::get_subtree_width(child);
                }
                Entry::Leaf(_, _) => {
                    total_width += 80.0; // Fixed width for leaf rendering
                }
            }
        }

        let spacing = (node.entries.len().saturating_sub(1)) as f32 * 20.0;
        f32::max(total_width + spacing, 100.0)
    }

    fn draw_node_recursive(
        highlighted_nodes: &mut HashSet<usize>,
        last_op_message: &mut String,
        ui: &mut egui::Ui,
        node: &HOTNode<String, String>,
        pos: egui::Pos2,
        zoom: f32,
    ) {
        let id = node as *const _ as usize;
        let is_highlighted = highlighted_nodes.contains(&id);

        // 1. Scale base node dimensions - INCREASED AS REQUESTED
        let node_width = 140.0 * zoom;
        let node_height = 60.0 * zoom;
        let v_spacing = 180.0 * zoom; // SIGNIFICANTLY INCREASED SPACING
        let padding = 35.0 * zoom;

        let rect = egui::Rect::from_center_size(pos, egui::vec2(node_width, node_height));

        // INTERACTION: Click to highlight
        let response = ui.interact(rect, ui.id().with(id), egui::Sense::click());
        if response.clicked() {
            highlighted_nodes.clear();
            highlighted_nodes.insert(id);
            *last_op_message = format!("Node selected: 0x{:x}", id);
        }

        // Modern Premium Colors
        let fill_color = if is_highlighted {
            egui::Color32::from_rgb(255, 165, 0) // Orange-Gold for highlight
        } else {
            egui::Color32::from_rgb(45, 45, 60) // Dark sleek slate
        };
        
        let stroke_color = if is_highlighted {
            egui::Color32::from_rgb(255, 255, 255)
        } else {
            egui::Color32::from_rgb(100, 100, 120)
        };

        // 2. Scale strokes and corner radii
        ui.painter().rect_filled(rect, 8.0 * zoom, fill_color);
        ui.painter().rect_stroke(
            rect,
            8.0 * zoom,
            egui::Stroke::new(if is_highlighted { 3.0 * zoom } else { 2.0 * zoom }, stroke_color),
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
                Entry::Child(_, child) => Self::get_subtree_width(child) * zoom,
                Entry::Leaf(_, _) => 80.0 * zoom,
            };

            let child_center_x = current_x + entry_width / 2.0;
            let child_pos = egui::pos2(child_center_x, pos.y + v_spacing);

            // 5. Scale line segments - ENHANCED VISIBILITY
            ui.painter().line_segment(
                [
                    pos + egui::vec2(0.0, node_height / 2.0),
                    child_pos - egui::vec2(0.0, 30.0 * zoom),
                ],
                egui::Stroke::new(2.0 * zoom, egui::Color32::from_rgb(120, 120, 160)),
            );

            match entry {
                Entry::Leaf(k, _) => {
                    let leaf_id = k as *const _ as usize;
                    let is_leaf_highlighted = highlighted_nodes.contains(&leaf_id);
                    
                    let leaf_rect = egui::Rect::from_center_size(child_pos, egui::vec2(20.0 * zoom, 20.0 * zoom));
                    let leaf_response = ui.interact(leaf_rect, ui.id().with(leaf_id), egui::Sense::click());
                    
                    if leaf_response.clicked() {
                        highlighted_nodes.clear();
                        highlighted_nodes.insert(leaf_id);
                        *last_op_message = format!("Leaf selected: '{}' (0x{:x})", k, leaf_id);
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
                    Self::draw_node_recursive(highlighted_nodes, last_op_message, ui, child, child_pos, zoom);
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

                group_frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("ENTRY MANAGEMENT").small().strong().color(egui::Color32::from_rgb(120, 120, 255)));
                    ui.add_space(10.0);

                    ui.label("Target Key:");
                    let _text_edit = ui.add_sized(
                        [ui.available_width(), 32.0],
                        egui::TextEdit::singleline(&mut self.new_key)
                            .hint_text("Enter key...")
                    );

                    ui.add_space(15.0);

                    let button_size = egui::vec2(ui.available_width() / 2.1, 42.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized(button_size, egui::Button::new(egui::RichText::new("➕ Insert").size(16.0)).rounding(6.0)).clicked() {
                            let old_heights = self.capture_heights();
                            self.trie.insert(self.new_key.clone(), format!("val_{}", self.new_key));
                            self.update_highlights(old_heights);
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

                    ui.add_space(10.0);
                    
                    if !self.highlighted_nodes.is_empty() {
                        if ui.add_sized(
                            [ui.available_width(), 36.0],
                            egui::Button::new(egui::RichText::new("🗑 Delete Highlighted").size(14.0))
                                .fill(egui::Color32::from_rgb(150, 50, 50))
                                .rounding(6.0)
                        ).clicked() {
                            let to_delete: Vec<usize> = self.highlighted_nodes.iter().cloned().collect();
                            let mut deleted_any = false;
                            for id in to_delete {
                                if self.trie.remove_by_id(id) {
                                    deleted_any = true;
                                }
                            }
                            if deleted_any {
                                self.highlighted_nodes.clear();
                                self.last_op_message = "Subtree(s) deleted.".to_string();
                            } else {
                                self.last_op_message = "Could not delete node.".to_string();
                            }
                        }
                    }
                });

                ui.add_space(15.0);

                group_frame.show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("BATCH OPERATIONS").small().strong().color(egui::Color32::from_rgb(255, 120, 120)));
                    ui.add_space(10.0);

                    if ui.add_sized(
                        [ui.available_width(), 48.0],
                        egui::Button::new(egui::RichText::new("🚀 Fill Node (31 Keys)").size(16.0)).rounding(6.0)
                    ).clicked() {
                        let old_heights = self.capture_heights();
                        for _ in 0..31 {
                            let key = format!("batch_{}", self.batch_counter);
                            self.trie.insert(key.clone(), format!("val_{}", key));
                            self.batch_counter += 1;
                        }
                        self.update_highlights(old_heights);
                        self.last_op_message = "Batch insertion complete.".to_string();
                    }
                });

                ui.add_space(20.0);
                ui.separator();
                ui.add_space(20.0);

                ui.label(egui::RichText::new("VIEWPORT").small().strong());
                ui.add_space(10.0);
                
                ui.horizontal(|ui| {
                    ui.label(format!("Zoom: {:.2}x", self.zoom));
                    if ui.button("⟲ Reset").clicked() {
                        self.zoom = 1.0;
                        self.pan = egui::Vec2::ZERO;
                    }
                });

                
                ui.add_space(10.0);
                ui.label(egui::RichText::new("🖱 Scroll to Zoom").small().color(egui::Color32::GRAY));
                ui.label(egui::RichText::new("🖱 Drag to Pan").small().color(egui::Color32::GRAY));

                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(20.0);
                    ui.label(egui::RichText::new(format!("Status: {}", self.last_op_message)).italics());
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

            // Zoom logic (Scroll Wheel Only - No Ctrl needed)
            let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0 {
                let zoom_delta = (scroll_delta / 400.0).exp(); // Reduced sensitivity for smoother wheel zoom
                self.zoom *= zoom_delta;
                self.zoom = self.zoom.clamp(0.05, 10.0); // Expanded range
            }

            // 2. Render the Tree - Ensure clipping to the CentralPanel
            if let Some(root) = &self.trie.root {
                // Calculate centered starting position with Pan and Zoom
                let center_x = canvas_rect.center().x + self.pan.x;
                let start_y = canvas_rect.top() + 80.0 + self.pan.y;
                let start_pos = egui::pos2(center_x, start_y);

                // Draw tree
                Self::draw_node_recursive(
                    &mut self.highlighted_nodes,
                    &mut self.last_op_message,
                    ui,
                    root,
                    start_pos,
                    self.zoom,
                );
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Trie is empty. Insert a key to begin.").size(18.0).color(egui::Color32::GRAY));
                });
            }
        });
    }
}