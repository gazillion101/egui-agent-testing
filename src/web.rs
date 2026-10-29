//! The driver in the browser. Nothing may block there, so there is no server:
//! with `?driver` in the URL the page gets `window.notesDriver.call(tool, args)`,
//! which answers at once (`ui_snapshot`, `ui_state`) or queues the input for
//! the next frames (`ui_click`, `ui_type`). The caller waits a frame or two
//! (`await new Promise(r => setTimeout(r, 100))`) before it looks again.

use std::sync::Arc;

use eframe::wasm_bindgen::{JsValue, closure::Closure};

use crate::driver::{Driver, Plan};

pub fn expose(driver: Arc<Driver>) {
    let call = Closure::wrap(Box::new(move |tool: String, args: String| -> String {
        let args: serde_json::Value = serde_json::from_str(&args).unwrap_or(serde_json::Value::Null);
        match driver.plan(&tool, &args) {
            Ok(Plan::Answer(text)) => text,
            Ok(Plan::Input(batches, text)) => {
                driver.queue(batches);
                text
            }
            Err(e) => format!("error: {e}"),
        }
    }) as Box<dyn Fn(String, String) -> String>);
    let api = js_sys::Object::new();
    js_sys::Reflect::set(&api, &JsValue::from_str("call"), call.as_ref()).unwrap();
    call.forget();
    let window = eframe::web_sys::window().expect("a browser window");
    js_sys::Reflect::set(&window, &JsValue::from_str("notesDriver"), &api).unwrap();
}
