mod trie;
mod ui;

use ui::HotApp;
use eframe::egui;

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
