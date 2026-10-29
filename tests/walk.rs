//! Tests that walk the whole interface and check a property of it, instead of
//! clicking one button. They read egui's accessibility tree, as a screen
//! reader or an agent would.

use egui::accesskit::Role;
use egui_agent_testing::Notes;
use egui_kittest::{Harness, kittest::{NodeT, Queryable}};

fn start(width: f32) -> Harness<'static, Notes> {
    let mut h = Harness::builder()
        .with_size(egui::vec2(width, 500.0))
        .build_eframe(|_cc| Notes::default());
    h.run_steps(3);
    h
}

/// The controls on screen: a description and where each one is.
fn controls<S>(h: &Harness<'_, S>) -> Vec<(String, egui::Rect)> {
    h.query_all_by(|n| {
        matches!(
            n.role(),
            Role::Button | Role::Slider | Role::ComboBox | Role::CheckBox | Role::TextInput | Role::SpinButton
        )
    })
    .map(|n| {
        let node = n.accesskit_node();
        (format!("{:?} {:?}", node.role(), node.label().unwrap_or_default()), n.rect())
    })
    .collect()
}

/// Pairs of controls that overlap (one inside the other is fine: a value box
/// in a slider).
fn overlaps(rects: &[(String, egui::Rect)]) -> Vec<String> {
    let mut bad = Vec::new();
    for (i, (a, ra)) in rects.iter().enumerate() {
        for (b, rb) in &rects[i + 1..] {
            let both = ra.intersect(*rb);
            let nested = ra.contains_rect(*rb) || rb.contains_rect(*ra);
            if !nested && both.width() > 2.0 && both.height() > 2.0 {
                bad.push(format!("{a} over {b}: {both:?}"));
            }
        }
    }
    bad
}

#[test]
fn every_control_has_a_name() {
    let mut h = start(700.0);
    h.state_mut().settings_open = true;
    h.run_steps(3);
    let all = controls(&h);
    // A walk that finds nothing proves nothing.
    assert!(all.len() >= 5, "only {} controls found: {all:?}", all.len());
    let unnamed: Vec<_> = all.iter().filter(|(d, _)| d.ends_with("\"\"")).collect();
    assert!(unnamed.is_empty(), "controls without a name: {unnamed:?}");
}

#[test]
fn no_two_controls_overlap_at_any_width() {
    let mut checked = 0;
    for width in [300.0, 420.0, 600.0, 900.0] {
        let mut h = start(width);
        h.state_mut().settings_open = true;
        for n in ["first", "second", "a much longer third note that wraps"] {
            h.state_mut().notes.push(n.into());
        }
        h.run_steps(3);
        let all = controls(&h);
        checked += all.len();
        let bad = overlaps(&all);
        assert!(bad.is_empty(), "at width {width}: {bad:#?}");
    }
    assert!(checked >= 20, "only {checked} controls checked in all");
}

/// The rule from the article: a walker must fail on broken code. This layout
/// puts two buttons on top of each other, and the walker has to say so.
#[test]
fn the_walker_catches_an_overlap() {
    let h = Harness::new_ui(|ui| {
        ui.put(egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(80.0, 30.0)), egui::Button::new("One"));
        ui.put(egui::Rect::from_min_size(egui::pos2(40.0, 20.0), egui::vec2(80.0, 30.0)), egui::Button::new("Two"));
    });
    let bad = overlaps(&controls(&h));
    assert_eq!(bad.len(), 1, "{bad:?}");
}

#[test]
fn adding_a_note_by_label() {
    let mut h = start(700.0);
    h.get_by_label("New note").click();
    h.get_by_label("New note").type_text("buy milk");
    h.get_by_label("Add note").click();
    h.run_steps(3);
    assert_eq!(h.state().notes, vec!["buy milk".to_string()]);
    assert!(h.query_by_label_contains("1 notes").is_some());
}

#[test]
fn an_empty_note_says_why_it_was_not_added() {
    let mut h = start(700.0);
    h.get_by_label("Add note").click();
    h.run_steps(3);
    assert!(h.state().notes.is_empty());
    assert!(h.query_by_label("Write something first.").is_some());
}
