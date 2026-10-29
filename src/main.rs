use std::sync::Arc;

use egui_agent_testing::{Notes, driver::Driver};

/// The app plus, with `--driver PORT`, the agent driver.
struct App {
    notes: Notes,
    driver: Option<Arc<Driver>>,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut eframe::egui::Ui, _frame: &mut eframe::Frame) {
        self.notes.show(ui);
        if let Some(d) = &self.driver {
            d.set_state(self.notes.state());
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let port: Option<u16> = args
        .iter()
        .position(|a| a == "--driver")
        .and_then(|k| args.get(k + 1))
        .and_then(|p| p.parse().ok());
    eframe::run_native(
        "Notes",
        eframe::NativeOptions::default(),
        Box::new(move |cc| {
            let driver = port.map(|p| {
                let d = Driver::new(&cc.egui_ctx);
                d.serve(p);
                eprintln!("driver on http://127.0.0.1:{p}/mcp");
                d
            });
            Ok(Box::new(App { notes: Notes::default(), driver }))
        }),
    )
}
