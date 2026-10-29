//! A driver for an AI agent: it reads egui's accessibility tree (the
//! counterpart of the DOM), puts real mouse and keyboard events into egui's
//! input, and serves both over MCP on a local port.
//!
//! Start the app with `--driver PORT`; connect Claude Code with
//! `claude mcp add --transport http notes http://127.0.0.1:PORT/mcp`.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};

use eframe::egui::{self, Event, Modifiers, PointerButton, Pos2, pos2};
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use serde_json::json;

#[derive(Clone, Debug)]
struct Node {
    id: u64,
    role: String,
    label: String,
    value: String,
    rect: Option<egui::Rect>,
    children: Vec<u64>,
    /// Widgets whose text names this one (a text field's label).
    labelled_by: Vec<u64>,
}

#[derive(Default)]
struct Inner {
    /// Input to feed, one batch per frame.
    queue: VecDeque<Vec<Event>>,
    nodes: HashMap<u64, Node>,
    root: Option<u64>,
    /// Short refs (e1, e2, ...) for egui's long widget ids.
    short: HashMap<u64, u64>,
    long: HashMap<u64, u64>,
    frame: u64,
    state: String,
}

/// What a tool call comes to.
pub(crate) enum Plan {
    Answer(String),
    Input(Vec<Vec<Event>>, String),
}

pub struct Driver {
    inner: Mutex<Inner>,
    ctx: egui::Context,
}

impl Driver {
    pub fn new(ctx: &egui::Context) -> Arc<Driver> {
        let d = Arc::new(Driver {
            inner: Mutex::new(Inner::default()),
            ctx: ctx.clone(),
        });
        ctx.enable_accesskit();
        ctx.add_plugin(DriverPlugin(d.clone()));
        d
    }

    /// The app describes itself once a frame, for `ui_state`.
    pub fn set_state(&self, state: String) {
        self.with(|i| i.state = state);
    }

    fn with<T>(&self, f: impl FnOnce(&mut Inner) -> T) -> T {
        f(&mut self.inner.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Runs until `n` more frames have been drawn (desktop: the server's
    /// thread may block; the browser must not, see `web.rs`).
    #[cfg(not(target_arch = "wasm32"))]
    fn frames(&self, n: u64) {
        let start = self.with(|i| i.frame);
        let t = Instant::now();
        while self.with(|i| i.frame) < start + n && t.elapsed() < Duration::from_secs(10) {
            self.ctx.request_repaint();
            std::thread::sleep(Duration::from_millis(8));
        }
    }

    /// Feeds input, a batch per frame, and waits until the app has settled.
    #[cfg(not(target_arch = "wasm32"))]
    fn feed(&self, batches: Vec<Vec<Event>>) {
        self.queue(batches);
        let t = Instant::now();
        while self.with(|i| !i.queue.is_empty()) && t.elapsed() < Duration::from_secs(10) {
            self.ctx.request_repaint();
            std::thread::sleep(Duration::from_millis(8));
        }
        self.frames(2);
    }

    /// Queues input for the next frames.
    pub(crate) fn queue(&self, batches: Vec<Vec<Event>>) {
        self.with(|i| i.queue.extend(batches));
        self.ctx.request_repaint();
    }

    /// The tree as indented text with a ref per widget.
    fn snapshot(&self) -> String {
        self.with(|i| {
            let mut out = String::new();
            if let Some(root) = i.root {
                walk(i, root, 0, &mut out);
            }
            if out.is_empty() { "The window has no accessible content yet.".into() } else { out }
        })
    }

    fn point(&self, a: &Value) -> Result<Pos2, String> {
        if let (Some(x), Some(y)) = (a["x"].as_f64(), a["y"].as_f64()) {
            return Ok(pos2(x as f32, y as f32));
        }
        self.with(|i| {
            let node = if let Some(r) = a["ref"].as_str() {
                let n: u64 = r.trim_start_matches('e').parse().map_err(|_| format!("bad ref {r}"))?;
                i.nodes.get(&i.long.get(&n).copied().unwrap_or(n)).cloned()
            } else if let Some(l) = a["label"].as_str() {
                let l = l.to_lowercase();
                let mut hits: Vec<&Node> = i
                    .nodes
                    .values()
                    .filter(|n| n.rect.is_some() && n.role != "TextRun" && name(i, n).to_lowercase().contains(&l))
                    .collect();
                // The exact name first, then a widget before a plain text
                // (a label names a field), then the smallest.
                hits.sort_by_key(|n| {
                    (name(i, n).to_lowercase() != l, n.role == "Label", n.rect.map_or(0, |r| r.area() as i64))
                });
                hits.first().map(|n| (*n).clone())
            } else {
                return Err("give a ref, a label, or x and y".into());
            };
            node.and_then(|n| n.rect)
                .map(|r| r.center())
                .ok_or_else(|| "no such widget now; take a new ui_snapshot".to_string())
        })
    }

    /// Turns a tool call into what to do: an answer, or input to feed.
    /// Shared by the desktop server and the browser's `window.notesDriver`.
    pub(crate) fn plan(&self, name: &str, a: &Value) -> Result<Plan, String> {
        match name {
            "ui_snapshot" => Ok(Plan::Answer(self.snapshot())),
            "ui_state" => Ok(Plan::Answer(self.with(|i| i.state.clone()))),
            "ui_click" => {
                let p = self.point(a)?;
                let press = |pressed| Event::PointerButton {
                    pos: p,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                };
                Ok(Plan::Input(
                    vec![vec![Event::PointerMoved(p)], vec![press(true)], vec![press(false)]],
                    format!("Clicked at ({:.0}, {:.0}).", p.x, p.y),
                ))
            }
            "ui_type" => {
                let t = a["text"].as_str().ok_or("give text")?;
                Ok(Plan::Input(
                    vec![vec![Event::Text(t.to_string())]],
                    format!("Typed {} characters.", t.chars().count()),
                ))
            }
            other => Err(format!("unknown tool {other}")),
        }
    }

    /// One tool call on the desktop. Errors are text for the agent to read.
    #[cfg(not(target_arch = "wasm32"))]
    fn call(&self, name: &str, a: &Value) -> Result<String, String> {
        // The tree must be fresh before a widget is looked up.
        self.frames(1);
        match self.plan(name, a)? {
            Plan::Answer(text) => Ok(text),
            Plan::Input(batches, text) => {
                self.feed(batches);
                Ok(text)
            }
        }
    }

    /// The tools, as MCP describes them.
    #[cfg(not(target_arch = "wasm32"))]
    fn tools() -> Value {
        let target = json!({
            "ref": {"type": "string", "description": "A node from ui_snapshot, e.g. e12"},
            "label": {"type": "string", "description": "Or: the first widget whose label contains this text"}
        });
        json!([
            {"name": "ui_snapshot", "description": "The window as an accessibility tree: role, label, value and a ref for each widget.", "inputSchema": {"type": "object", "properties": {}}},
            {"name": "ui_click", "description": "Clicks a widget by ref or label, or a point (x, y).", "inputSchema": {"type": "object", "properties": target}},
            {"name": "ui_type", "description": "Types text into the focused widget (click a text field first).", "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"]}},
            {"name": "ui_state", "description": "The app's own account of its state.", "inputSchema": {"type": "object", "properties": {}}}
        ])
    }

    /// Serves MCP (JSON over HTTP POST) on 127.0.0.1:`port` from a thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn serve(self: &Arc<Self>, port: u16) {
        let me = self.clone();
        let server = tiny_http::Server::http(("127.0.0.1", port)).expect("the driver's port is free");
        std::thread::spawn(move || {
            for mut req in server.incoming_requests() {
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let msg: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                let id = msg["id"].clone();
                let result = match msg["method"].as_str().unwrap_or("") {
                    "initialize" => Some(json!({
                        "protocolVersion": "2025-03-26",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "egui-agent-testing", "version": "0.1.0"}
                    })),
                    "tools/list" => Some(json!({"tools": Driver::tools()})),
                    "tools/call" => {
                        let name = msg["params"]["name"].as_str().unwrap_or("");
                        let (text, is_error) = match me.call(name, &msg["params"]["arguments"]) {
                            Ok(t) => (t, false),
                            Err(e) => (e, true),
                        };
                        Some(json!({"content": [{"type": "text", "text": text}], "isError": is_error}))
                    }
                    "ping" => Some(json!({})),
                    _ => None, // notifications get no answer body
                };
                let json_header = tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap();
                let _ = match result {
                    Some(r) => req.respond(
                        tiny_http::Response::from_string(json!({"jsonrpc": "2.0", "id": id, "result": r}).to_string())
                            .with_header(json_header),
                    ),
                    None => req.respond(tiny_http::Response::empty(202)),
                };
            }
        });
    }
}

/// What a widget is called: its own label, the text of a label that names
/// it, or (for plain text) the text itself.
fn name(i: &Inner, n: &Node) -> String {
    if !n.label.is_empty() {
        n.label.clone()
    } else if n.role == "Label" {
        n.value.clone()
    } else if let Some(l) = n.labelled_by.first().and_then(|id| i.nodes.get(id)) {
        name(i, l)
    } else {
        String::new()
    }
}

fn walk(i: &Inner, id: u64, depth: usize, out: &mut String) {
    let Some(n) = i.nodes.get(&id) else { return };
    if n.role == "TextRun" {
        return;
    }
    let name = name(i, n);
    let shown = !name.is_empty() || !matches!(n.role.as_str(), "GenericContainer" | "Unknown" | "Window" | "Pane" | "Label");
    let next = if shown {
        let role = if n.role == "Label" { "text".to_string() } else { n.role.to_ascii_lowercase() };
        out.push_str(&"  ".repeat(depth));
        out.push_str(&format!("- {role}"));
        if !name.is_empty() {
            out.push_str(&format!(" \"{}\"", name.replace('"', "\\\"")));
        }
        if n.role != "Label" && !n.value.is_empty() {
            out.push_str(&format!(" value=\"{}\"", n.value.replace('"', "\\\"")));
        }
        out.push_str(&format!(" [ref=e{}]\n", i.short.get(&n.id).copied().unwrap_or(n.id)));
        depth + 1
    } else {
        depth
    };
    for c in &n.children {
        walk(i, *c, next, out);
    }
}

struct DriverPlugin(Arc<Driver>);

impl egui::Plugin for DriverPlugin {
    fn debug_name(&self) -> &'static str {
        "egui-agent-testing-driver"
    }

    /// Before each frame: put the queued input into egui's.
    fn input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let mut i = self.0.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(batch) = i.queue.pop_front() {
            input.events.extend(batch);
        }
        i.frame += 1;
        if !i.queue.is_empty() {
            ctx.request_repaint();
        }
    }

    /// After each frame: keep the accessibility tree egui built.
    fn output_hook(&mut self, _ctx: &egui::Context, output: &mut egui::FullOutput) {
        let Some(update) = &output.platform_output.accesskit_update else { return };
        let mut i = self.0.inner.lock().unwrap_or_else(|e| e.into_inner());
        i.nodes.clear();
        for (id, n) in &update.nodes {
            i.nodes.insert(
                id.0,
                Node {
                    id: id.0,
                    role: format!("{:?}", n.role()),
                    label: n.label().unwrap_or_default().to_string(),
                    value: n
                        .value()
                        .map(String::from)
                        .or_else(|| n.numeric_value().map(|v| format!("{v}")))
                        .unwrap_or_default(),
                    rect: n.bounds().map(|b| {
                        egui::Rect::from_min_max(pos2(b.x0 as f32, b.y0 as f32), pos2(b.x1 as f32, b.y1 as f32))
                    }),
                    children: n.children().iter().map(|c| c.0).collect(),
                    labelled_by: n.labelled_by().iter().map(|c| c.0).collect(),
                },
            );
        }
        i.root = update.tree.as_ref().map(|t| t.root.0);
        let ids: Vec<u64> = i.nodes.keys().copied().collect();
        for id in ids {
            if !i.short.contains_key(&id) {
                let n = i.short.len() as u64 + 1;
                i.short.insert(id, n);
                i.long.insert(n, id);
            }
        }
    }
}
