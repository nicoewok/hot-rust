use eframe::egui;
use hot_rust::{Entry, HOTNode, HOT};
use std::collections::{HashMap, HashSet};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "HOT (Height Optimized Trie) Live Viewer",
        options,
        Box::new(|_cc| Box::<HotApp>::default()),
    )
}

struct HotApp {
    trie: HOT<String, String>,
    new_key: String,
    last_op_message: String,
    // For visualization
    highlighted_nodes: HashSet<usize>,
}

impl Default for HotApp {
    fn default() -> Self {
        Self {
            trie: HOT::new(),
            new_key: String::new(),
            last_op_message: String::from("Ready"),
            highlighted_nodes: HashSet::new(),
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
    ) {
        let id = node as *const _ as usize;
        let is_highlighted = self.highlighted_nodes.contains(&id);

        let node_width = 100.0;
        let node_height = 40.0;
        let rect = egui::Rect::from_center_size(pos, egui::vec2(node_width, node_height));

        let fill_color = if is_highlighted {
            egui::Color32::from_rgb(255, 230, 100)
        } else {
            egui::Color32::from_rgb(240, 240, 255)
        };

        painter.rect_filled(rect, 4.0, fill_color);
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, egui::Color32::DARK_GRAY));

        let label = format!("H: {}", node.height);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(14.0),
            egui::Color32::BLACK,
        );

        // Draw entries and children
        let total_width = self.get_subtree_width(node);
        let mut current_x = pos.x - total_width / 2.0;
        let v_spacing = 100.0;

        for entry in &node.entries {
            let entry_width = match entry {
                Entry::Child(_, child) => self.get_subtree_width(child),
                Entry::Leaf(_, _) => 80.0,
            };

            let child_center_x = current_x + entry_width / 2.0;
            let child_pos = egui::pos2(child_center_x, pos.y + v_spacing);

            // Draw line from node to entry/child
            painter.line_segment(
                [
                    pos + egui::vec2(0.0, node_height / 2.0),
                    child_pos - egui::vec2(0.0, 20.0),
                ],
                egui::Stroke::new(1.0, egui::Color32::GRAY),
            );

            match entry {
                Entry::Leaf(k, _) => {
                    painter.circle_filled(child_pos, 5.0, egui::Color32::from_rgb(100, 200, 100));
                    painter.text(
                        child_pos + egui::vec2(0.0, 10.0),
                        egui::Align2::CENTER_TOP,
                        format!("'{}'", k),
                        egui::FontId::proportional(12.0),
                        egui::Color32::BLACK,
                    );
                }
                Entry::Child(rep, child) => {
                    // Draw representative key on the edge
                    painter.text(
                        (pos + child_pos.to_vec2()) / 2.0,
                        egui::Align2::LEFT_CENTER,
                        format!("rep: {}", rep),
                        egui::FontId::proportional(10.0),
                        egui::Color32::DARK_GRAY,
                    );
                    self.draw_node_recursive(painter, child, child_pos);
                }
            }

            current_x += entry_width + 20.0;
        }
    }
}

impl eframe::App for HotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("control_panel").show(ctx, |ui| {
            ui.heading("HOT Control");
            ui.add_space(10.0);

            ui.label("Key:");
            ui.text_edit_singleline(&mut self.new_key);

            ui.add_space(5.0);
            ui.horizontal(|ui| {
                if ui.button("Insert").clicked() {
                    if !self.new_key.is_empty() {
                        let old_heights = self.capture_heights();
                        let key = self.new_key.clone();
                        self.trie.insert(key.clone(), format!("val_{}", key));
                        self.update_highlights(old_heights);
                        self.last_op_message = format!("Inserted '{}'", key);
                    }
                }

                if ui.button("Search").clicked() {
                    if let Some(val) = self.trie.lookup(&self.new_key) {
                        self.last_op_message = format!("Found: {}", val);
                    } else {
                        self.last_op_message = format!("Not found: {}", self.new_key);
                    }
                }

                if ui.button("Delete").clicked() {
                    self.last_op_message = "Delete not implemented".to_string();
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.label(format!("Status: {}", self.last_op_message));
            
            ui.add_space(20.0);
            ui.label("Explanation:");
            ui.small("HOT nodes expand horizontally until capacity (32).");
            ui.small("Height optimization pushes leaves down or pulls parents up.");
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(root) = &self.trie.root {
                
                // Use a scroll area for large trees
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Re-calculate start pos in scroll content
                        let content_center_x = ui.available_width() / 2.0;
                        let scroll_start_pos = egui::pos2(content_center_x, 40.0);
                        self.draw_node_recursive(ui.painter(), root, scroll_start_pos);
                    });
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Trie is empty. Insert a key to begin.");
                });
            }
        });
    }
}
