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
        let node_width = 100.0 * zoom;
        let node_height = 40.0 * zoom;
        let v_spacing = 100.0 * zoom;
        let padding = 20.0 * zoom;

        let rect = egui::Rect::from_center_size(pos, egui::vec2(node_width, node_height));

        let fill_color = if is_highlighted {
            egui::Color32::from_rgb(255, 230, 100)
        } else {
            egui::Color32::from_rgb(240, 240, 255)
        };

        // 2. Scale strokes and corner radii
        painter.rect_filled(rect, 4.0 * zoom, fill_color);
        painter.rect_stroke(
            rect,
            4.0 * zoom,
            egui::Stroke::new(1.0 * zoom, egui::Color32::DARK_GRAY),
        );

        // 3. Scale text
        let label = format!("H: {}", node.height);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(14.0 * zoom),
            egui::Color32::BLACK,
        );

        // 4. Scale subtree widths (assuming get_subtree_width returns base unzoomed widths)
        let total_width = self.get_subtree_width(node) * zoom;
        let mut current_x = pos.x - total_width / 2.0;

        for entry in &node.entries {
            let entry_width = match entry {
                Entry::Child(_, child) => self.get_subtree_width(child) * zoom,
                Entry::Leaf(_, _) => 80.0 * zoom,
            };

            let child_center_x = current_x + entry_width / 2.0;
            let child_pos = egui::pos2(child_center_x, pos.y + v_spacing);

            // 5. Scale line offsets and stroke
            painter.line_segment(
                [
                    pos + egui::vec2(0.0, node_height / 2.0),
                    child_pos - egui::vec2(0.0, 20.0 * zoom),
                ],
                egui::Stroke::new(1.0 * zoom, egui::Color32::GRAY),
            );

            match entry {
                Entry::Leaf(k, _) => {
                    // Scale leaf circle and text
                    painter.circle_filled(
                        child_pos,
                        5.0 * zoom,
                        egui::Color32::from_rgb(100, 200, 100),
                    );
                    painter.text(
                        child_pos + egui::vec2(0.0, 10.0 * zoom),
                        egui::Align2::CENTER_TOP,
                        format!("'{}'", k),
                        egui::FontId::proportional(12.0 * zoom),
                        egui::Color32::BLACK,
                    );
                }
                Entry::Child(rep, child) => {
                    // Scale representative key text on the edge
                    painter.text(
                        (pos + child_pos.to_vec2()) / 2.0,
                        egui::Align2::LEFT_CENTER,
                        format!("rep: {}", rep),
                        egui::FontId::proportional(10.0 * zoom),
                        egui::Color32::DARK_GRAY,
                    );
                    // Recurse with the same zoom level
                    self.draw_node_recursive(painter, child, child_pos, zoom);
                }
            }

            current_x += entry_width + padding;
        }
    }
}

impl eframe::App for HotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Side Panel (Controls) ---
        egui::SidePanel::left("control_panel").show(ctx, |ui| {
            ui.heading("HOT Control");
            ui.add_space(10.0);

            ui.label("Key:");
            ui.text_edit_singleline(&mut self.new_key);

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                if ui.button("Insert").clicked() {
                    self.trie.insert(self.new_key.clone(), format!("val_{}", self.new_key));
                }

                if ui.button("Search").clicked() {
                    if let Some(val) = self.trie.lookup(&self.new_key) {
                        self.last_op_message = format!("Found: {}", val);
                    } else {
                        self.last_op_message = format!("Not found: {}", self.new_key);
                    }
                }
            });

            ui.add_space(10.0);
            
            // --- NEW: Batch Insert Button ---
            // HOT nodes have a max fanout k=32. 
            // 31 inserts fill the node to k-1[cite: 162].
            if ui.button("🚀 Fill Node (31 Unique Keys)").clicked() {
                for _ in 0..31 {
                    let key = format!("batch_{}", self.batch_counter);
                    self.trie.insert(key.clone(), format!("val_{}", key));
                    self.batch_counter += 1;
                }
                self.last_op_message = "Inserted 31 keys. Node is now at capacity!".to_string();
            }

            ui.add_space(10.0);
            ui.separator();
            
            // --- NEW: Zoom Controls ---
            ui.label("View Controls:");
            ui.horizontal(|ui| {
                ui.label(format!("Zoom: {:.1}x", self.zoom));
                if ui.button("Reset").clicked() {
                    self.zoom = 1.0;
                    self.pan = egui::Vec2::ZERO;
                }
            });
            ui.small("Use Ctrl + Mouse Wheel to Zoom");
            ui.small("Click and Drag to Pan");

            ui.add_space(20.0);
            ui.separator();
            ui.label(format!("Status: {}", self.last_op_message));
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

            // Zoom logic (Ctrl + Scroll)
            let scroll_delta = ctx.input(|i| i.smooth_scroll_delta.y);
            if ctx.input(|i| i.modifiers.ctrl) && scroll_delta != 0.0 {
                let zoom_delta = (scroll_delta / 200.0).exp();
                self.zoom *= zoom_delta;
                self.zoom = self.zoom.clamp(0.1, 5.0); // Keep it sane
            }

            // 2. Render the Tree
            if let Some(root) = &self.trie.root {
                let painter = ui.painter();
                
                // Calculate centered starting position with Pan and Zoom
                let center_x = rect.center().x + self.pan.x;
                let start_y = rect.top() + 60.0 + self.pan.y;
                let start_pos = egui::pos2(center_x, start_y);

                // Pass the current zoom to your drawing function
                self.draw_node_recursive(painter, root, start_pos, self.zoom);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Trie is empty. Insert a key to begin.");
                });
            }
        });
    }
}