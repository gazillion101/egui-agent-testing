//! A small notes app, made so that a program can see and drive it.
//!
//! Every control has a name (egui turns it into an accessibility node), which
//! is what the tests in `tests/` and the agent driver in `driver.rs` rely on.

pub mod driver;
#[cfg(target_arch = "wasm32")]
pub mod web;

use eframe::egui;

pub struct Notes {
    pub notes: Vec<String>,
    pub draft: String,
    pub font_size: f32,
    pub show_count: bool,
    pub settings_open: bool,
    pub message: String,
}

impl Default for Notes {
    fn default() -> Self {
        Notes {
            notes: Vec::new(),
            draft: String::new(),
            font_size: 14.0,
            show_count: true,
            settings_open: false,
            message: String::new(),
        }
    }
}

impl Notes {
    /// What the driver's `ui_state` tool reports.
    pub fn state(&self) -> String {
        format!(
            "notes: {}\ndraft: {:?}\nfont size: {}\nmessage: {:?}",
            self.notes.len(),
            self.draft,
            self.font_size,
            self.message
        )
    }

    fn add(&mut self) {
        let text = self.draft.trim().to_string();
        if text.is_empty() {
            self.message = "Write something first.".into();
        } else {
            self.notes.push(text);
            self.draft.clear();
            self.message = "Added.".into();
        }
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.add(egui::Slider::new(&mut self.font_size, 10.0..=32.0).text("Font size"));
        ui.checkbox(&mut self.show_count, "Show the number of notes");
        if ui.button("Close settings").clicked() {
            self.settings_open = false;
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Clear all notes…").clicked() {
                        self.notes.clear();
                        self.message = "Cleared.".into();
                        ui.close();
                    }
                });
                if ui.button("Settings").clicked() {
                    self.settings_open = true;
                }
            });
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.label(if self.message.is_empty() { "Ready" } else { &self.message });
        });
        if self.settings_open {
            // On a narrow window the settings take the whole window; beside
            // the notes they would not fit (the overlap test caught this).
            if ui.ctx().content_rect().width() < 520.0 {
                egui::CentralPanel::default().show(ui, |ui| self.settings_ui(ui));
                return;
            }
            egui::Panel::right("settings").show(ui, |ui| self.settings_ui(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Notes");
            ui.horizontal(|ui| {
                let label = ui.label("New note");
                ui.add(
                    egui::TextEdit::singleline(&mut self.draft)
                        .hint_text("A new note")
                        .desired_width(160.0),
                )
                // Without this, a screen reader (or an agent) meets a field
                // with no name.
                .labelled_by(label.id);
                if ui.button("Add note").clicked() {
                    self.add();
                }
            });
            ui.separator();
            if self.show_count {
                ui.label(format!("{} notes", self.notes.len()));
            }
            let size = self.font_size;
            for note in &self.notes {
                ui.label(egui::RichText::new(note).size(size));
            }
        });
    }
}

impl eframe::App for Notes {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
