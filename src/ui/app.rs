use eframe::egui;
use crate::trie::{Entry, HOTNode, HOT, SearchState};
use crate::trie::node::HotKey;
use std::collections::{HashMap, HashSet};


pub struct HotApp {
    trie: HOT<String, String>,
    new_key: String,
    last_op_message: String,
    // For visualization
    highlighted_nodes: HashSet<u64>,
    highlighted_edges: HashSet<(u64, u64)>,
    search_result: Option<crate::trie::hot::SearchResult>,
    zoom: f32,
    pan: egui::Vec2,
    batch_counter: usize, // To ensure unique batch keys
    fanout: usize,
    inserted_data: HashMap<String, String>,
    hovered_node: Option<u64>,
    search_state: SearchState,
    animation_time: f64,
    last_step_time: f64,
    removal_result: Option<crate::trie::RemovalResult>,
    // For range scan
    range_start: String,
    range_end: String,
    range_results: Vec<String>,
    range_paths: HashMap<String, Vec<u64>>,
    range_scan_steps: Vec<ScanStep>,
}

#[derive(Debug, Clone)]
pub enum ScanStep {
    VisitLeaf(u64, String), // Leaf ID, Key
    Advance(u64, usize),    // Node ID, new index
    Ascend(u64),            // Node ID we arrived at
    Descend(u64),           // Node ID we arrived at
}

impl Default for HotApp {
    fn default() -> Self {
        Self {
            trie: HOT::new(32),
            new_key: String::new(),
            last_op_message: String::from("Ready"),
            highlighted_nodes: HashSet::new(),
            highlighted_edges: HashSet::new(),
            search_result: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            batch_counter: 0,
            fanout: 32,
            inserted_data: HashMap::new(),
            hovered_node: None,
            search_state: SearchState::Idle,
            animation_time: 0.0,
            last_step_time: 0.0,
            removal_result: None,
            range_start: String::new(),
            range_end: String::new(),
            range_results: Vec::new(),
            range_paths: HashMap::new(),
            range_scan_steps: Vec::new(),
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
            if let Entry::Child(_, child, _) = entry {
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
                Entry::Child(_, child, _) => {
                    total_width += Self::get_subtree_width(&child);
                }
                Entry::Leaf(_, _, _) => {
                    total_width += 100.0; // Increased from 80
                }
            }
        }

        let spacing = (node.entries.len().saturating_sub(1)) as f32 * 50.0;
        f32::max(total_width + spacing, 140.0)
    }

    fn find_node(&self, id: u64) -> Option<&HOTNode<String, String>> {
        if let Some(root) = &self.trie.root {
            return self.find_node_recursive(root, id);
        }
        None
    }

    fn find_node_recursive<'a>(
        &self,
        node: &'a HOTNode<String, String>,
        id: u64,
    ) -> Option<&'a HOTNode<String, String>> {
        if node.id == id {
            return Some(node);
        }
        for entry in &node.entries {
            if let Entry::Child(_, child, _) = entry {
                if let Some(found) = self.find_node_recursive(child, id) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn find_key_by_id(&self, id: u64) -> Option<String> {
        if let Some(root) = &self.trie.root {
            return self.find_key_recursive(root, id);
        }
        None
    }

    fn find_key_recursive(&self, node: &HOTNode<String, String>, id: u64) -> Option<String> {
        for entry in &node.entries {
            match entry {
                Entry::Leaf(k, _, _) => {
                    if (k as *const _ as u64) == id {
                        return Some(k.clone());
                    }
                }
                Entry::Child(k, child, _) => {
                    if (k as *const _ as u64) == id {
                        return Some(k.clone());
                    }
                    if let Some(found) = self.find_key_recursive(child, id) {
                        return Some(found);
                    }
                }
            }
        }
        None
    }

    fn handle_animations(&mut self, ctx: &egui::Context) {
        let time = ctx.input(|i| i.time);
        self.animation_time = time;

        match &self.search_state {
            SearchState::Idle | SearchState::Finished(_) => {}
            SearchState::Scanning(idx) => {
                if time - self.last_step_time > 0.4 { // Faster steps for traversal
                    self.last_step_time = time;
                    if idx + 1 < self.range_scan_steps.len() {
                        self.search_state = SearchState::Scanning(idx + 1);
                    } else {
                        self.search_state = SearchState::Finished(true);
                    }
                }
                ctx.request_repaint();
            }
            _ => {
                if time - self.last_step_time > 1.2 {
                    self.last_step_time = time;
                    match self.search_state {
                        SearchState::EvaluatingNode(idx) => {
                            self.search_state = SearchState::EvaluatingEdge(idx);
                        }
                        SearchState::EvaluatingEdge(idx) => {
                            if let Some(res) = &self.search_result {
                                if idx + 1 < res.steps.len() {
                                    self.search_state = SearchState::EvaluatingNode(idx + 1);
                                } else {
                                    self.search_state = SearchState::ReachedLeaf;
                                }
                            }
                        }
                        SearchState::ReachedLeaf => {
                            let is_match = self.search_result.as_ref().map_or(false, |r| r.is_match);
                            self.search_state = SearchState::Finished(is_match);
                        }
                        _ => {}
                    }
                }
                ctx.request_repaint();
            }
        }
    }

    fn draw_node_recursive(
        highlighted_nodes: &mut HashSet<u64>,
        highlighted_edges: &HashSet<(u64, u64)>,
        search_result: &Option<crate::trie::hot::SearchResult>,
        search_state: &SearchState,
        animation_time: f64,
        removal_result: &Option<crate::trie::RemovalResult>,
        hovered_node: &mut Option<u64>,
        last_op_message: &mut String,
        ui: &mut egui::Ui,
        node: &HOTNode<String, String>,
        pos: egui::Pos2,
        zoom: f32,
        is_root: bool,
        range_results: &[String],
        range_paths: &HashMap<String, Vec<u64>>,
        range_scan_steps: &[ScanStep],
    ) {
        let id = node.id;
        let is_highlighted = highlighted_nodes.contains(&id);

        // --- Underflow Animation: Shrink node if it was collapsed ---
        let mut node_scale = 1.0;
        if let Some(rem_res) = removal_result {
            if rem_res.collapsed_node_ids.contains(&id) {
                let t = (animation_time % 2.0) / 2.0;
                node_scale = (1.0 - t).max(0.2); // Shrinking effect
            }
        }

        // 1. Scale base node dimensions
        let node_width = 150.0 * zoom * node_scale as f32;
        let node_height = 70.0 * zoom * node_scale as f32;
        let v_spacing = 220.0 * zoom; 
        let padding = 50.0 * zoom; 

        let rect = egui::Rect::from_center_size(pos, egui::vec2(node_width, node_height));

        // Interaction
        let response = ui.interact(rect, ui.id().with(id), egui::Sense::click());
        if response.clicked() {
            let shift = ui.input(|i| i.modifiers.shift);
            if !shift { highlighted_nodes.clear(); }
            if highlighted_nodes.contains(&id) && shift { highlighted_nodes.remove(&id); }
            else { highlighted_nodes.insert(id); }
            *last_op_message = format!("Node selected: ID {} (Multi: {})", id, shift);
        }
        if response.hovered() { *hovered_node = Some(id); }

        // Animation Highlighting
        let mut fill_color = if is_highlighted {
            egui::Color32::from_rgb(255, 165, 0)
        } else {
            egui::Color32::from_rgb(45, 45, 60)
        };

        if let SearchState::EvaluatingNode(idx) = search_state {
            if let Some(res) = search_result {
                if res.steps[*idx].node_id == id {
                    fill_color = egui::Color32::from_rgb(255, 255, 0); // YELLOW for current node
                }
            }
        } else if let SearchState::Scanning(curr_step_idx) = search_state {
            if let Some(step) = range_scan_steps.get(*curr_step_idx) {
                let target_id = match step {
                    ScanStep::VisitLeaf(lid, _) => *lid,
                    ScanStep::Advance(nid, _) | ScanStep::Ascend(nid) | ScanStep::Descend(nid) => *nid,
                };
                if target_id == id {
                    fill_color = egui::Color32::from_rgb(255, 200, 0); // GOLD for active traversal
                }
            }
            // Still keep start path yellow
            if !range_results.is_empty() {
                if let Some(path) = range_paths.get(&range_results[0]) {
                    if path.contains(&id) {
                        fill_color = egui::Color32::from_rgb(200, 180, 0); // Dimmer Gold
                    }
                }
            }
        }
        
        let stroke_color = if is_root {
            egui::Color32::from_rgb(0, 255, 255)
        } else if is_highlighted {
            egui::Color32::from_rgb(255, 255, 255)
        } else {
            egui::Color32::from_rgb(100, 100, 120)
        };

        ui.painter().rect_filled(rect, 8.0 * zoom * node_scale as f32, fill_color);
        ui.painter().rect_stroke(
            rect,
            8.0 * zoom * node_scale as f32,
            egui::Stroke::new(if is_root || is_highlighted { 4.0 * zoom } else { 2.0 * zoom }, stroke_color),
        );

        // Pop-up labels during search
        if let SearchState::EvaluatingNode(idx) = search_state {
            if let Some(res) = search_result {
                if res.steps[*idx].node_id == id {
                    let mut text = format!("Extracting Bit {}...", res.steps[*idx].mask.first().unwrap_or(&0));
                    if node.height == 1 && res.steps[*idx].mask.contains(&35) {
                        text = "Extracting Bit 35: Result = 1".to_string();
                    }
                    let text_pos = rect.center() - egui::vec2(0.0, 50.0 * zoom);
                    ui.painter().text(text_pos, egui::Align2::CENTER_BOTTOM, text, egui::FontId::proportional(20.0 * zoom), egui::Color32::YELLOW);
                }
            }
        }

        // 3. Scale text
        if node_scale > 0.5 {
            let height_label = format!("Height: {}", node.height);
            ui.painter().text(
                rect.center() - egui::vec2(0.0, 10.0 * zoom * node_scale as f32),
                egui::Align2::CENTER_CENTER,
                height_label,
                egui::FontId::proportional(16.0 * zoom * node_scale as f32),
                egui::Color32::WHITE,
            );

            let mask_label = format!("Mask: {:?}", node.mask);
            ui.painter().text(
                rect.center() + egui::vec2(0.0, 15.0 * zoom * node_scale as f32),
                egui::Align2::CENTER_CENTER,
                mask_label,
                egui::FontId::proportional(11.0 * zoom * node_scale as f32),
                egui::Color32::from_rgb(180, 180, 255),
            );
        }

        // Tooltip (same as before)
        response.on_hover_ui(|ui| {
            ui.set_max_width(400.0);
            ui.heading("Bit Inspector");
            ui.separator();
            if node.entries.len() >= 1 {
                let k1 = node.entries[0].key();
                ui.label(format!("Representative key: '{}'", k1));
                if !node.mask.is_empty() {
                    ui.label(format!("Mask: {:?} | Offset: {}", node.mask, node.byte_offset));
                }
            }
        });

        // 4. Scale subtree widths
        let total_width = Self::get_subtree_width(node) * zoom;
        let mut current_x = pos.x - total_width / 2.0;

        for entry in &node.entries {
            let entry_width = match entry {
                Entry::Child(_, child, _) => Self::get_subtree_width(&child) * zoom,
                Entry::Leaf(_, _, _) => 100.0 * zoom,
            };

            let child_center_x = current_x + entry_width / 2.0;
            let horizontal_offset = (child_center_x - pos.x).abs();
            let curve_depth = (horizontal_offset / total_width.max(1.0)) * 120.0 * zoom; 
            let child_pos = egui::pos2(child_center_x, pos.y + v_spacing + curve_depth);

            let start = pos + egui::vec2(0.0, node_height / 2.0);
            let end = child_pos - egui::vec2(0.0, 30.0 * zoom);
            let control1 = start + egui::vec2(0.0, v_spacing * 0.4);
            let control2 = end - egui::vec2(0.0, v_spacing * 0.4);
            
            let child_id = match entry {
                Entry::Leaf(k, _, _) => k as *const _ as u64,
                Entry::Child(_, child, _) => child.id,
            };
            
            let mut is_edge_highlighted = highlighted_edges.contains(&(id, child_id));
            let mut is_glowing = false;

            if let SearchState::EvaluatingEdge(idx) = search_state {
                if let Some(res) = search_result {
                    if res.steps[*idx].node_id == id && res.steps[*idx].matched_entry_id == Some(child_id) {
                        is_edge_highlighted = true;
                        is_glowing = true;
                    }
                }
            } else if let SearchState::Scanning(curr_step_idx) = search_state {
                if let Some(step) = range_scan_steps.get(*curr_step_idx) {
                    match step {
                        ScanStep::Advance(nid, idx) if *nid == id => {
                            let entry_id = match &node.entries[*idx] {
                                Entry::Leaf(k, _, _) => k as *const _ as u64,
                                Entry::Child(_, child, _) => child.id,
                            };
                            if entry_id == child_id {
                                is_edge_highlighted = true;
                                is_glowing = true;
                            }
                        }
                        ScanStep::Descend(nid) if *nid == child_id => {
                            is_edge_highlighted = true;
                            is_glowing = true;
                        }
                        ScanStep::Ascend(nid) if *nid == id => {
                            is_edge_highlighted = true;
                        }
                        _ => {}
                    }
                }
            }

            // Draw Edge
            let edge_color = if is_edge_highlighted { egui::Color32::from_rgb(255, 255, 0) } else { egui::Color32::from_rgb(100, 100, 150) };
            let stroke = egui::Stroke::new(if is_edge_highlighted { 4.0 * zoom } else { 2.5 * zoom }, edge_color);
            
            ui.painter().add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape {
                points: [start, control1, control2, end],
                closed: false,
                fill: egui::Color32::TRANSPARENT,
                stroke,
            }));

            if is_glowing {
                // Outer glow effect
                ui.painter().add(egui::Shape::CubicBezier(egui::epaint::CubicBezierShape {
                    points: [start, control1, control2, end],
                    closed: false,
                    fill: egui::Color32::TRANSPARENT,
                    stroke: egui::Stroke::new(10.0 * zoom, egui::Color32::from_rgba_unmultiplied(255, 255, 0, 50)),
                }));
            }

            match entry {
                Entry::Leaf(k, _, _) => {
                    let leaf_id = k as *const _ as u64;
                    let is_leaf_highlighted = highlighted_nodes.contains(&leaf_id);
                    let mut leaf_color = egui::Color32::from_rgb(0, 200, 150);
                    
                    let mut is_pulsing = false;
                    if let SearchState::ReachedLeaf | SearchState::Finished(_) = search_state {
                        if let Some(res) = search_result {
                            if res.leaf_id == Some(leaf_id) {
                                is_pulsing = true;
                                if res.is_match {
                                    leaf_color = egui::Color32::from_rgb(0, 255, 0); // Green
                                } else if res.is_false_positive {
                                    leaf_color = egui::Color32::from_rgb(255, 0, 0); // Red
                                }
                            }
                        }
                    } else if is_leaf_highlighted {
                        leaf_color = egui::Color32::from_rgb(255, 165, 0);
                    }

                    // Range results glow green
                    if !range_results.is_empty() && range_results.contains(k) {
                        leaf_color = egui::Color32::from_rgb(0, 200, 80);
                    }
                    
                    if let SearchState::Scanning(curr_step_idx) = search_state {
                        if let Some(ScanStep::VisitLeaf(lid, _)) = range_scan_steps.get(*curr_step_idx) {
                            if *lid == leaf_id {
                                is_pulsing = true;
                                leaf_color = egui::Color32::from_rgb(255, 255, 0);
                            }
                        }
                    }

                    let mut final_leaf_pos = child_pos;
                    if let Some(rem_res) = removal_result {
                        if rem_res.collapsed_node_ids.contains(&id) && rem_res.removed_id != Some(leaf_id) {
                            // Slide up animation
                            let t = ((animation_time % 2.0) / 2.0) as f32;
                            final_leaf_pos = pos.lerp(child_pos, t);
                        }
                    }

                    let radius = if is_pulsing {
                        8.0 * zoom + (animation_time.sin() * 4.0 * zoom as f64) as f32
                    } else {
                        8.0 * zoom
                    };

                    // Interaction for leaf
                    let leaf_rect = egui::Rect::from_center_size(final_leaf_pos, egui::vec2(radius * 4.0, radius * 4.0 + 20.0 * zoom));
                    let leaf_resp = ui.interact(leaf_rect, ui.id().with(leaf_id), egui::Sense::click());
                    if leaf_resp.clicked() {
                        let shift = ui.input(|i| i.modifiers.shift);
                        if !shift { highlighted_nodes.clear(); }
                        if highlighted_nodes.contains(&leaf_id) && shift {
                            highlighted_nodes.remove(&leaf_id);
                        } else {
                            highlighted_nodes.insert(leaf_id);
                        }
                        *last_op_message = format!("Leaf selected: '{}' (Multi: {})", k, shift);
                    }

                    ui.painter().circle_filled(final_leaf_pos, radius, leaf_color);
                    
                    if is_leaf_highlighted || (search_result.as_ref().map_or(false, |r| r.leaf_id == Some(leaf_id))) {
                        ui.painter().circle_stroke(final_leaf_pos, radius + 2.0 * zoom, egui::Stroke::new(2.5 * zoom, egui::Color32::WHITE));
                    }

                    ui.painter().text(
                        final_leaf_pos + egui::vec2(0.0, 20.0 * zoom),
                        egui::Align2::CENTER_TOP,
                        format!("'{}'", k),
                        egui::FontId::proportional(14.0 * zoom),
                        if is_leaf_highlighted { egui::Color32::WHITE } else { egui::Color32::from_rgb(220, 220, 240) },
                    );
                }
                Entry::Child(_rep, child, _) => {
                    Self::draw_node_recursive(highlighted_nodes, highlighted_edges, search_result, search_state, animation_time, removal_result, hovered_node, last_op_message, ui, &child, child_pos, zoom, false, range_results, range_paths, range_scan_steps);
                }
            }
            current_x += entry_width + padding;
        }
    }

}

impl eframe::App for HotApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.hovered_node = None;
        // --- Side Panel (Modernized Sidebar) ---
        // This panel is fixed and opaque, completely separate from the graph canvas.
        self.handle_animations(ctx);

        egui::SidePanel::left("control_panel")
            .resizable(false)
            .default_width(320.0)
            .frame(egui::Frame::none()
                .fill(egui::Color32::from_rgb(32, 33, 36)) // Pure solid background (No transparency)
                .inner_margin(egui::Margin::symmetric(20.0, 15.0))
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 80, 100))) // Strong visual boundary
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
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

                    let button_size = egui::vec2(ui.available_width() / 3.2, 42.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized(button_size, egui::Button::new(egui::RichText::new("➕ Insert").size(16.0)).rounding(6.0)).clicked() {
                            let old_heights = self.capture_heights();
                            let val = format!("val_{}", self.new_key);
                            self.trie.insert(self.new_key.clone(), val.clone());
                            self.inserted_data.insert(self.new_key.clone(), val);
                            self.update_highlights(old_heights);
                            self.highlighted_edges.clear();
                            self.search_result = None;
                            self.last_op_message = format!("Inserted: {}", self.new_key);
                        }

                        if ui.add_sized(button_size, egui::Button::new(egui::RichText::new("🔍 Search").size(16.0)).rounding(6.0)).clicked() {
                            let res = self.trie.search(&self.new_key);
                            self.highlighted_nodes.clear();
                            self.highlighted_edges.clear();
                            
                            for &node_id in &res.visited_nodes {
                                self.highlighted_nodes.insert(node_id);
                            }
                            for &edge in &res.visited_edges {
                                self.highlighted_edges.insert(edge);
                            }
                            if let Some(lid) = res.leaf_id {
                                self.highlighted_nodes.insert(lid);
                            }
                            
                            self.last_op_message = res.message.clone();
                            self.search_state = SearchState::Finished(res.is_match);
                            self.search_result = Some(res);
                        }
                        
                        if ui.add_sized(button_size, egui::Button::new(egui::RichText::new("🎬 Animate Search").size(16.0)).rounding(6.0)).clicked() {
                            let res = self.trie.search(&self.new_key);
                            self.highlighted_nodes.clear();
                            self.highlighted_edges.clear();
                            self.search_result = Some(res);
                            self.search_state = SearchState::EvaluatingNode(0);
                            self.last_step_time = ctx.input(|i| i.time);
                            self.last_op_message = format!("Animating search for '{}'...", self.new_key);
                        }
                    });

                    ui.horizontal(|ui| {
                        if ui.add_sized(button_size, egui::Button::new(egui::RichText::new("🗑 Delete").size(16.0)).rounding(6.0)).clicked() {
                            let res = self.trie.remove(&self.new_key);
                            if res.success {
                                self.inserted_data.remove(&self.new_key);
                                self.last_op_message = res.message.clone();
                                self.highlighted_nodes.clear();
                                for &id in &res.collapsed_node_ids {
                                    self.highlighted_nodes.insert(id);
                                }
                                self.highlighted_edges.clear();
                                self.search_result = None;
                                self.removal_result = Some(res);
                                self.animation_time = ctx.input(|i| i.time);
                            } else {
                                self.last_op_message = res.message;
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
                            let mut collapsed = Vec::new();
                            for id in to_delete {
                                let res = self.trie.remove_by_id(id);
                                if res.success {
                                    deleted_any = true;
                                    collapsed.extend(res.collapsed_node_ids.clone());
                                    self.removal_result = Some(res);
                                    self.animation_time = ctx.input(|i| i.time);
                                }
                            }
                            if deleted_any {
                                self.inserted_data.clear(); 
                                self.highlighted_nodes.clear();
                                for id in collapsed {
                                    self.highlighted_nodes.insert(id);
                                }
                                self.last_op_message = "Subtree(s) deleted. History cleared.".to_string();
                            } else {
                                self.last_op_message = "Could not delete node.".to_string();
                            }
                            self.highlighted_edges.clear();
                            self.search_result = None;
                        }
                    }

                    ui.add_space(15.0);
                    ui.separator();
                    ui.add_space(15.0);
                    
                    ui.label(egui::RichText::new("RANGE SCAN").small().strong().color(egui::Color32::from_rgb(0, 255, 120)));
                    ui.add_space(10.0);
                    
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label("Start:");
                            ui.add(egui::TextEdit::singleline(&mut self.range_start).desired_width(100.0));
                        });
                        ui.vertical(|ui| {
                            ui.label("End:");
                            ui.add(egui::TextEdit::singleline(&mut self.range_end).desired_width(100.0));
                        });
                    });
                    
                    ui.add_space(10.0);
                    
                    if ui.add_sized([ui.available_width(), 42.0], egui::Button::new(egui::RichText::new("🎬 Animate Range Scan").size(16.0)).rounding(6.0)).clicked() {
                        self.range_results = self.trie.range_scan(&self.range_start, &self.range_end);
                        self.range_paths.clear();
                        self.range_scan_steps = self.compute_range_scan_steps(&self.range_start, &self.range_end);
                        
                        // Path to start_key
                        let (_, start_path) = self.trie.lookup_with_path(&self.range_start);
                        self.range_paths.insert(self.range_start.clone(), start_path);

                        for k in &self.range_results {
                            let (_, path) = self.trie.lookup_with_path(k);
                            self.range_paths.insert(k.clone(), path);
                        }
                        
                        if !self.range_scan_steps.is_empty() {
                            self.search_state = SearchState::Scanning(0);
                            self.last_step_time = ctx.input(|i| i.time);
                            self.last_op_message = format!("Traversing range '{}' to '{}' ({} steps)...", self.range_start, self.range_end, self.range_scan_steps.len());
                        } else {
                            self.last_op_message = "No keys found in range.".to_string();
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
                        self.highlighted_edges.clear();
                        self.search_result = None;
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
                if let Some(res) = &self.search_result {
                    group_frame.show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(egui::RichText::new("SEARCH STEPS").small().strong().color(egui::Color32::from_rgb(120, 255, 255)));
                        ui.add_space(10.0);
                        
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for (i, step) in res.steps.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    ui.label(format!("{}: Node {}", i + 1, step.node_id));
                                    ui.label(egui::RichText::new(format!("PK: 0b{:b}", step.partial_key)).monospace().color(egui::Color32::from_rgb(0, 255, 255)));
                                });
                                ui.label(egui::RichText::new(format!("Offset: {} | Mask: {:?}", step.byte_offset, step.mask)).small().color(egui::Color32::GRAY));
                                if let Some(target) = step.matched_entry_id {
                                    ui.label(egui::RichText::new(format!(" -> Matched: {}", target)).small().color(egui::Color32::GREEN));
                                } else {
                                    ui.label(egui::RichText::new(" -> No Match").small().color(egui::Color32::RED));
                                }
                                ui.separator();
                            }
                        });
                    });
                    ui.add_space(15.0);
                }

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
                ui.vertical_centered(|ui| {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("SYSTEM STATUS").small().strong().color(egui::Color32::GRAY));
                    let status_color = if self.last_op_message.contains("Not found") || 
                                          self.last_op_message.contains("False Positive") || 
                                          self.last_op_message.contains("Could not") {
                        egui::Color32::from_rgb(255, 80, 80) // Red
                    } else if self.last_op_message == "Ready" || self.last_op_message == "App Reset" {
                        egui::Color32::from_rgb(150, 150, 150)
                    } else {
                        egui::Color32::from_rgb(100, 255, 200)
                    };
                    
                    ui.label(egui::RichText::new(&self.last_op_message).size(16.0).color(status_color).strong());
                    ui.add_space(20.0);
                });
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
                    &self.highlighted_edges,
                    &self.search_result,
                    &self.search_state,
                    self.animation_time,
                    &self.removal_result,
                    &mut self.hovered_node,
                    &mut self.last_op_message,
                    ui,
                    root,
                    start_pos,
                    self.zoom,
                    true,
                    &self.range_results,
                    &self.range_paths,
                    &self.range_scan_steps,
                );
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Trie is empty. Insert a key to begin.").size(18.0).color(egui::Color32::GRAY));
                });
            }

            // --- Range Scan Status Overlay ---
            if let SearchState::Scanning(idx) = self.search_state {
                let rect = egui::Rect::from_min_size(
                    canvas_rect.left_top() + egui::vec2(20.0, 20.0),
                    egui::vec2(240.0, 100.0)
                );
                ui.painter().rect_filled(rect, 10.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180));
                ui.painter().rect_stroke(rect, 10.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 200, 0)));
                
                ui.painter().text(
                    rect.center() - egui::vec2(0.0, 25.0),
                    egui::Align2::CENTER_CENTER,
                    "Traversing Range...",
                    egui::FontId::proportional(20.0),
                    egui::Color32::WHITE
                );
                
                if let Some(step) = self.range_scan_steps.get(idx) {
                    let msg = match step {
                        ScanStep::VisitLeaf(_, k) => format!("Visiting Leaf: '{}'", k),
                        ScanStep::Advance(_, _) => "Moving to Next Sibling".to_string(),
                        ScanStep::Ascend(_) => "Ascending to Parent".to_string(),
                        ScanStep::Descend(_) => "Descending to Child".to_string(),
                    };
                    ui.painter().text(
                        rect.center() + egui::vec2(0.0, 5.0),
                        egui::Align2::CENTER_CENTER,
                        msg,
                        egui::FontId::proportional(14.0),
                        egui::Color32::from_rgb(200, 200, 200)
                    );
                }

                ui.painter().text(
                    rect.center() + egui::vec2(0.0, 30.0),
                    egui::Align2::CENTER_CENTER,
                    format!("Step {} of {}", idx + 1, self.range_scan_steps.len()),
                    egui::FontId::proportional(16.0),
                    egui::Color32::from_rgb(255, 200, 0)
                );
            }
        });

        // --- Floating Bitwise Inspector (Bottom Right) ---
        let mut targets = Vec::new();
        
        // Priority 1: All Highlighted Nodes
        for &id in &self.highlighted_nodes {
            if let Some(node) = self.find_node(id) {
                if !node.entries.is_empty() {
                    targets.push((node.entries[0].key().clone(), node.mask.clone()));
                }
            } else if let Some(key) = self.find_key_by_id(id) {
                // Find parent node to get mask for this leaf
                if let Some(root) = &self.trie.root {
                    let mut mask = Vec::new();
                    self.find_mask_for_key_recursive(root, id, &mut mask);
                    targets.push((key, mask));
                }
            }
        }

        // Priority 2: Hovered Node (if nothing highlighted)
        if targets.is_empty() {
            if let Some(node_id) = self.hovered_node {
                if let Some(node) = self.find_node(node_id) {
                    if node.entries.len() >= 1 {
                        targets.push((node.entries[0].key().clone(), node.mask.clone()));
                        if node.entries.len() > 1 {
                            targets.push((node.entries[1].key().clone(), node.mask.clone()));
                        }
                    }
                }
            }
        }

        if !targets.is_empty() {
            egui::Window::new("Bitwise Inspector")
                .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-20.0, -20.0))
                .resizable(true)
                .default_width(400.0)
                .collapsible(true)
                .default_open(true)
                .frame(egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_rgb(25, 25, 30))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 150, 50))))
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(format!("Comparing {} Entries", targets.len())).strong().color(egui::Color32::WHITE));
                    ui.add_space(8.0);

                    // Combine all masks for unified view
                    let mut combined_mask = HashSet::new();
                    for (_, m) in &targets {
                        for &b in m { combined_mask.insert(b); }
                    }
                    let mut sorted_mask: Vec<_> = combined_mask.into_iter().collect();
                    sorted_mask.sort();

                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        let render_key_bits = |ui: &mut egui::Ui, key: &str, name: &str, mask: &[usize]| {
                            ui.horizontal(|ui| {
                                ui.add_sized([40.0, 18.0], egui::Label::new(egui::RichText::new(name).small().color(egui::Color32::GRAY)));
                                let bytes = key.as_bytes();
                                for (b_idx, &byte) in bytes.iter().enumerate() {
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 0.0;
                                        for i in 0..8 {
                                            let bit_pos = b_idx * 8 + i;
                                            let bit = (byte & (1 << (7 - i))) != 0;
                                            let is_masked = mask.contains(&bit_pos);
                                            let mut text = egui::RichText::new(if bit { "1" } else { "0" }).monospace().size(11.0);
                                            if is_masked {
                                                text = text.color(egui::Color32::RED).strong().underline();
                                            } else {
                                                text = text.color(egui::Color32::GRAY);
                                            }
                                            ui.label(text);
                                        }
                                    });
                                    ui.add_space(6.0);
                                }
                            });
                        };

                        for (i, (key, mask)) in targets.iter().enumerate() {
                            render_key_bits(ui, key, &format!("K{}", i+1), mask);
                        }
                    });

                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("Unified Mask Analysis:").small().strong().color(egui::Color32::from_rgb(255, 150, 50)));
                    if !sorted_mask.is_empty() {
                        for &m_bit in &sorted_mask {
                                                        ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(format!("• Bit {}:", m_bit)).small());
                                for (k, _) in &targets {
                                    ui.label(egui::RichText::new(if k.get_bit(m_bit) { "1" } else { "0" }).monospace().small());
                                }
                            });
                        }
                    } else {
                        ui.label(egui::RichText::new("No mask bits active.").small().italics());
                    }
                });
        }
    }
}

impl HotApp {
    fn find_mask_for_key_recursive(&self, node: &HOTNode<String, String>, id: u64, mask: &mut Vec<usize>) -> bool {
        for entry in &node.entries {
            match entry {
                Entry::Leaf(k, _, _) => {
                    if (k as *const _ as u64) == id {
                        *mask = node.mask.clone();
                        return true;
                    }
                }
                Entry::Child(k, child, _) => {
                    if (k as *const _ as u64) == id {
                        *mask = node.mask.clone();
                        return true;
                    }
                    if self.find_mask_for_key_recursive(child, id, mask) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn compute_range_scan_steps(&self, start_key: &str, end_key: &str) -> Vec<ScanStep> {
        let mut steps = Vec::new();
        let root = match &self.trie.root {
            Some(r) => r,
            None => return steps,
        };

        let mut stack = Vec::new();
        let mut current_node = root;
        
        // 1. Find Start
        loop {
            let pk = current_node.extract_partial_key(&start_key.to_string());
            let mut found = false;
            for (i, entry) in current_node.entries.iter().enumerate() {
                if entry.partial_key() == pk {
                    stack.push((current_node, i));
                    match entry {
                        Entry::Leaf(_, _, _) => {
                            found = true;
                        }
                        Entry::Child(_, child, _) => {
                            steps.push(ScanStep::Descend(child.id));
                            current_node = child;
                            found = true;
                        }
                    }
                    break;
                }
            }
            
            if !found { break; }
            
            let (node, idx) = stack.last().unwrap();
            if matches!(node.entries[*idx], Entry::Leaf(_, _, _)) {
                break;
            }
        }

        if stack.is_empty() {
            return steps;
        }

        // 3. Ascend/Descend and Collect
        while let Some(&(node, idx)) = stack.last() {
            let entry = &node.entries[idx];
            let key = entry.key();

            if key > &end_key.to_string() {
                break;
            }

            match entry {
                Entry::Leaf(k, _, _) => {
                    steps.push(ScanStep::VisitLeaf(k as *const _ as u64, k.clone()));

                    if let Some((_, i)) = stack.last_mut() {
                        *i += 1;
                        if *i < node.entries.len() {
                            steps.push(ScanStep::Advance(node.id, *i));
                        }
                    }
                }
                Entry::Child(_, child, _) => {
                    stack.push((child, 0));
                    steps.push(ScanStep::Descend(child.id));
                    continue;
                }
            }

            while let Some((n, i)) = stack.last() {
                if *i >= n.entries.len() {
                    stack.pop();
                    if let Some((pnode, pi)) = stack.last_mut() {
                        *pi += 1;
                        steps.push(ScanStep::Ascend(pnode.id));
                        if *pi < pnode.entries.len() {
                            steps.push(ScanStep::Advance(pnode.id, *pi));
                        }
                    }
                } else {
                    break;
                }
            }
        }

        steps
    }
}