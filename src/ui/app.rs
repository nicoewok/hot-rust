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

    fn get_subtree_width(&self, node: &HOTNode<String, String>) -> f32 {
        if node.entries.is_empty() {
            return 100.0;
        }

        let mut total_width = 0.0;
        for entry in &node.entries {
            match entry {
                Entry::Child(_, child) => {
                    total_width += self.get_subtree_width(child);
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
        &self,
        painter: &egui::Painter,
        node: &HOTNode<String, String>,
        pos: egui::Pos2,
        zoom: f32,
    ) {
        let id = node as *const _ as usize;
        let is_highlighted = self.highlighted_nodes.contains(&id);

        // 1. Scale base node dimensions
        let node_width = 110.0 * zoom;
        let node_height = 45.0 * zoom;
        let v_spacing = 100.0 * zoom;
        let padding = 25.0 * zoom;

        let rect = egui::Rect::from_center_size(pos, egui::vec2(node_width, node_height));

        // Modern Premium Colors
        let fill_color = if is_highlighted {
            egui::Color32::from_rgb(255, 215, 0) // Gold for highlight
        } else {
            egui::Color32::from_rgb(45, 45, 60) // Dark sleek slate
        };
        
        let stroke_color = if is_highlighted {
            egui::Color32::from_rgb(255, 255, 255)
        } else {
            egui::Color32::from_rgb(100, 100, 120)
        };

        // 2. Scale strokes and corner radii
        painter.rect_filled(rect, 8.0 * zoom, fill_color);
        painter.rect_stroke(
            rect,
            8.0 * zoom,
            egui::Stroke::new(1.5 * zoom, stroke_color),
        );

        // 3. Scale text
        let label = format!("H: {}", node.height);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(15.0 * zoom),
            egui::Color32::WHITE,
        );

        // 4. Scale subtree widths
        let total_width = self.get_subtree_width(node) * zoom;
        let mut current_x = pos.x - total_width / 2.0;

        for entry in &node.entries {
            let entry_width = match entry {
                Entry::Child(_, child) => self.get_subtree_width(child) * zoom,
                Entry::Leaf(_, _) => 80.0 * zoom,
            };

            let child_center_x = current_x + entry_width / 2.0;
            let child_pos = egui::pos2(child_center_x, pos.y + v_spacing);

            // 5. Scale line segments
            painter.line_segment(
                [
                    pos + egui::vec2(0.0, node_height / 2.0),
                    child_pos - egui::vec2(0.0, 25.0 * zoom),
                ],
                egui::Stroke::new(1.5 * zoom, egui::Color32::from_rgb(180, 180, 200)),
            );

            match entry {
                Entry::Leaf(k, _) => {
                    painter.circle_filled(
                        child_pos,
                        6.0 * zoom,
                        egui::Color32::from_rgb(0, 200, 150), // Emerald green
                    );
                    painter.text(
                        child_pos + egui::vec2(0.0, 15.0 * zoom),
                        egui::Align2::CENTER_TOP,
                        format!("'{}'", k),
                        egui::FontId::proportional(13.0 * zoom),
                        egui::Color32::from_rgb(220, 220, 240),
                    );
                }
                Entry::Child(rep, child) => {
                    painter.text(
                        (pos + child_pos.to_vec2()) / 2.0 + egui::vec2(5.0 * zoom, -5.0 * zoom),
                        egui::Align2::LEFT_CENTER,
                        format!("rep: {}", rep),
                        egui::FontId::proportional(11.0 * zoom),
                        egui::Color32::from_rgb(150, 150, 170),
                    );
                    self.draw_node_recursive(painter, child, child_pos, zoom);
                }
            }

            current_x += entry_width + padding;
        }
    }

}

impl eframe::App for HotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Side Panel (Modernized Taskbar) ---
        egui::SidePanel::left("control_panel")
            .resizable(false)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.heading(egui::RichText::new("HOT Engine").size(24.0).strong());
                    ui.label(egui::RichText::new("Interactive Visualizer").italics().color(egui::Color32::GRAY));
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
                            if let Some(val) = self.trie.lookup(&self.new_key) {
                                self.last_op_message = format!("Found: {}", val);
                            } else {
                                self.last_op_message = format!("Not found: {}", self.new_key);
                            }
                        }
                    });
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


        // --- Central Panel (Visualizer) ---
        egui::CentralPanel::default().show(ctx, |ui| {
            // 1. Handle Input for Zoom and Pan
            let rect = ui.max_rect();
            let response = ui.interact(rect, ui.id(), egui::Sense::click_and_drag());

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
                // Get a painter clipped to this UI's rect
                let painter = ui.painter().with_clip_rect(rect);
                
                // Calculate centered starting position with Pan and Zoom
                let center_x = rect.center().x + self.pan.x;
                let start_y = rect.top() + 80.0 + self.pan.y;
                let start_pos = egui::pos2(center_x, start_y);

                // Draw tree
                self.draw_node_recursive(&painter, root, start_pos, self.zoom);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Trie is empty. Insert a key to begin.").size(18.0).color(egui::Color32::GRAY));
                });
            }
        });
    }
}