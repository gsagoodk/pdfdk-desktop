//! Desktop widgets (0.4.0): a small frameless window per tool that sits on the desktop
//! like a macOS widget. Drop files on it → the tool runs → the results can be dragged
//! straight out again (into Mail, a browser, Finder …).
//!
//! Each widget is one tool with its own options snapshot (`WidgetConfig`), persisted in
//! the app config next to the watched folders. Windows are labelled `widget-<id>` and
//! load the same SPA with `?widget=<id>`, which mounts `WidgetApp.vue`.

use crate::config::{self, ToolConfig, ToolDefinition, WidgetConfig};
use crate::{find_tool, run_files, AppState, FileResult};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

/// Logical size of a widget window (the macOS "small" widget is ~170 pt, we need a little
/// more room for a file list).
pub const WIDTH: f64 = 196.0;
pub const HEIGHT: f64 = 196.0;

pub(crate) fn label(id: &str) -> String {
    format!("widget-{}", id)
}

/// Open (or re-show) the window for one widget.
pub fn open_window<R: Runtime>(app: &AppHandle<R>, w: &WidgetConfig) -> tauri::Result<()> {
    let label = label(&w.id);
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.show();
        return Ok(());
    }
    let mut builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(format!("index.html?widget={}", w.id).into()))
        .title("PDF.dk")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .always_on_bottom(true)
        .skip_taskbar(true)
        .visible(true);
    if let (Some(x), Some(y)) = (w.x, w.y) {
        builder = builder.position(x, y);
    }
    let win = builder.build()?;

    // Sit on the desktop like Apple's own widgets: desktop-icon level (under every app window,
    // still there when "click wallpaper to reveal desktop" pushes windows aside), on all Spaces.
    // tao's always_on_bottom is only "below normal" and gets hidden with the rest (0.4.1).
    #[cfg(target_os = "macos")]
    {
        let win2 = win.clone();
        let _ = win.run_on_main_thread(move || unsafe { place_on_desktop(&win2) });
    }

    // the frosted-glass look of the OS's own widgets
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
        if let Err(e) = apply_vibrancy(&win, NSVisualEffectMaterial::HudWindow, None, Some(22.0)) {
            crate::add_log(&format!("Widget {}: no vibrancy ({})", w.id, e));
        }
    }
    #[cfg(target_os = "windows")]
    {
        if window_vibrancy::apply_acrylic(&win, Some((18, 18, 22, 150))).is_err() {
            let _ = window_vibrancy::apply_blur(&win, Some((18, 18, 22, 150)));
        }
    }

    // remember where the user puts it
    let app2 = app.clone();
    let id = w.id.clone();
    let win2 = win.clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::Moved(pos) = event {
            let scale = win2.scale_factor().unwrap_or(1.0);
            let logical: tauri::LogicalPosition<f64> = pos.to_logical(scale);
            save_position(&app2, &id, logical.x, logical.y);
        }
    });
    Ok(())
}

#[cfg(target_os = "macos")]
unsafe fn place_on_desktop<R: Runtime>(win: &tauri::WebviewWindow<R>) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowLevelForKey(key: i32) -> i32;
    }
    let Ok(ptr) = win.ns_window() else { return };
    if ptr.is_null() {
        return;
    }
    let ns = ptr as *mut AnyObject;
    let level = CGWindowLevelForKey(18) as isize + 1; // kCGDesktopIconWindowLevelKey, one above Finder's icons
    let _: () = msg_send![ns, setLevel: level];
    // canJoinAllSpaces (1<<0) | stationary (1<<4) | ignoresCycle (1<<6)
    let behaviour: usize = (1 << 0) | (1 << 4) | (1 << 6);
    let _: () = msg_send![ns, setCollectionBehavior: behaviour];
    let _: () = msg_send![ns, setHidesOnDeactivate: false];
}

fn save_position<R: Runtime>(app: &AppHandle<R>, id: &str, x: f64, y: f64) {
    let Some(state) = app.try_state::<AppState>() else { return };
    let cfg = state.config.clone();
    let id = id.to_string();
    tauri::async_runtime::spawn(async move {
        let mut config = cfg.write().await;
        if let Some(w) = config.widgets.iter_mut().find(|w| w.id == id) {
            if w.x != Some(x) || w.y != Some(y) {
                w.x = Some(x);
                w.y = Some(y);
                let _ = config::save_config(&config);
            }
        }
    });
}

/// Where a new widget lands: bottom-left of the main screen, cascading to the right — the
/// top-left corner is where macOS keeps its own widgets. (0.4.0 used 40,80 and sat under them.)
fn default_position<R: Runtime>(app: &AppHandle<R>, n: f64) -> (f64, f64) {
    let (sw, sh, scale) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| (m.size().width as f64, m.size().height as f64, m.scale_factor()))
        .unwrap_or((1440.0, 900.0, 1.0));
    let (sw, sh) = (sw / scale, sh / scale);
    let per_row = ((sw - 80.0) / (WIDTH + 16.0)).floor().max(1.0);
    let x = 40.0 + (n % per_row) * (WIDTH + 16.0);
    let y = (sh - HEIGHT - 60.0 - (n / per_row).floor() * (HEIGHT + 16.0)).max(40.0);
    (x, y)
}

/// Open every saved widget (app start).
pub async fn open_all<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let widgets = {
        let mut config = state.config.write().await;
        let mut moved = false;
        for i in 0..config.widgets.len() {
            if config.widgets[i].x == Some(40.0) && config.widgets[i].y == Some(80.0) {
                let (x, y) = default_position(app, i as f64);
                config.widgets[i].x = Some(x);
                config.widgets[i].y = Some(y);
                moved = true;
            }
        }
        if moved {
            let _ = config::save_config(&config);
        }
        config.widgets.clone()
    };
    for w in &widgets {
        if let Err(e) = open_window(app, w) {
            crate::add_log(&format!("Widget {} could not open: {}", w.id, e));
        }
    }
}

/// The tool config a widget runs with: the catalogue entry + the widget's own options.
fn tool_config(def: &ToolDefinition, w: &WidgetConfig) -> ToolConfig {
    let mut options = def.default_options();
    if let (Some(base), Some(over)) = (options.as_object_mut(), w.options.as_object()) {
        for (k, v) in over {
            base.insert(k.clone(), v.clone());
        }
    }
    ToolConfig {
        id: def.id.clone(),
        key: format!("widget:{}", w.id),
        enabled: true,
        folder_path: None,
        output_mode: config::OutputMode::SameFolder,
        options,
        endpoint: Some(def.endpoint.clone()),
        file_field: Some(def.file_field.clone()),
        accepts: def.accepts.clone(),
        output: Some(def.output.clone()),
        name: Some(def.name.clone()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetInfo {
    pub widget: WidgetConfig,
    pub tool: ToolDefinition,
    pub language: String,
    pub theme: String,
}

// ------------------------------------------------------------------ commands

#[tauri::command]
pub async fn widget_list(state: tauri::State<'_, AppState>) -> Result<Vec<WidgetConfig>, String> {
    Ok(state.config.read().await.widgets.clone())
}

#[tauri::command]
pub async fn widget_get(state: tauri::State<'_, AppState>, id: String) -> Result<WidgetInfo, String> {
    let (widget, language, theme) = {
        let config = state.config.read().await;
        let w = config.widgets.iter().find(|w| w.id == id).cloned().ok_or_else(|| format!("Unknown widget: {}", id))?;
        (w, config.general.language.clone(), config.general.theme.clone())
    };
    let tool = find_tool(&state, &widget.tool_id).await.ok_or_else(|| format!("Unknown tool: {}", widget.tool_id))?;
    Ok(WidgetInfo { widget, tool, language, theme })
}

/// Put a tool on the desktop. Options start as the tool's saved defaults (what drag-and-drop
/// in the main window uses) and can be changed per widget afterwards.
#[tauri::command]
pub async fn widget_create(app: AppHandle, state: tauri::State<'_, AppState>, tool_id: String) -> Result<WidgetConfig, String> {
    create(&app, &state, &tool_id).await
}

/// `PDFDK_WIDGET_TEST=<tool id>` at launch: put that tool on the desktop right away and log the
/// outcome — lets a terminal run prove that widget windows build on this machine.
pub fn start_test_hook(app: &AppHandle) {
    let Ok(tool_id) = std::env::var("PDFDK_WIDGET_TEST") else { return };
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let Some(state) = handle.try_state::<AppState>() else { return };
        match create(&handle, &state, &tool_id).await {
            Ok(w) => println!("WIDGET_TEST ok: {} at {:?},{:?} window={}", w.id, w.x, w.y, handle.get_webview_window(&label(&w.id)).is_some()),
            Err(e) => println!("WIDGET_TEST failed: {}", e),
        }
    });
}

/// The ToolConfig a widget would run with right now (its own options over the catalogue defaults).
pub(crate) async fn widget_tool_config(state: &AppState, id: &str) -> Option<ToolConfig> {
    let widget = state.config.read().await.widgets.iter().find(|w| w.id == id).cloned()?;
    let def = find_tool(state, &widget.tool_id).await?;
    Some(tool_config(&def, &widget))
}

async fn create(app: &AppHandle, state: &AppState, tool_id: &str) -> Result<WidgetConfig, String> {
    let def = find_tool(state, tool_id).await.ok_or_else(|| format!("Unknown tool: {}", tool_id))?;
    let widget = {
        let mut config = state.config.write().await;
        let saved = config.tools.iter().find(|t| t.id == tool_id).map(|t| t.options.clone()).unwrap_or(serde_json::json!({}));
        let n = config.widgets.len() as f64;
        let (x, y) = default_position(app, n);
        let w = WidgetConfig {
            id: uuid::Uuid::new_v4().to_string()[..8].to_string(),
            tool_id: def.id.clone(),
            options: saved,
            combine: def.file_field.ends_with("[]"),
            x: Some(x),
            y: Some(y),
        };
        config.widgets.push(w.clone());
        config::save_config(&config).map_err(|e| e.to_string())?;
        w
    };
    open_window(app, &widget).map_err(|e| e.to_string())?;
    crate::add_log(&format!("Widget added: {} ({})", def.name.da, widget.id));
    Ok(widget)
}

#[tauri::command]
pub async fn widget_remove(app: AppHandle, state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(&label(&id)) {
        let _ = win.close();
    }
    let mut config = state.config.write().await;
    config.widgets.retain(|w| w.id != id);
    config::save_config(&config).map_err(|e| e.to_string())?;
    crate::add_log(&format!("Widget removed: {}", id));
    Ok(())
}

/// Bring a widget back (e.g. after its window was closed by the OS) — also used by "Vis".
#[tauri::command]
pub async fn widget_show(app: AppHandle, state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let w = state.config.read().await.widgets.iter().find(|w| w.id == id).cloned().ok_or_else(|| format!("Unknown widget: {}", id))?;
    open_window(&app, &w).map_err(|e| e.to_string())
}

/// Per-widget options (from the Tools view). The widget window is told to reload.
#[tauri::command]
pub async fn widget_update(app: AppHandle, state: tauri::State<'_, AppState>, id: String, options: serde_json::Value, combine: Option<bool>) -> Result<(), String> {
    {
        let mut config = state.config.write().await;
        let w = config.widgets.iter_mut().find(|w| w.id == id).ok_or_else(|| format!("Unknown widget: {}", id))?;
        w.options = options;
        if let Some(c) = combine {
            w.combine = c;
        }
        config::save_config(&config).map_err(|e| e.to_string())?;
    }
    let _ = app.emit_to(label(&id), "widget-changed", ());
    Ok(())
}

/// Files dropped on a widget. Same pipeline as the main window's drop zone (jobs show up
/// in Aktivitet, retries and rate-limit waits included); output lands next to the input.
#[tauri::command]
pub async fn widget_process(app: AppHandle, state: tauri::State<'_, AppState>, id: String, paths: Vec<String>) -> Result<Vec<FileResult>, String> {
    let (widget, lang) = {
        let config = state.config.read().await;
        let w = config.widgets.iter().find(|w| w.id == id).cloned().ok_or_else(|| format!("Unknown widget: {}", id))?;
        (w, config.general.language.clone())
    };
    let def = find_tool(&state, &widget.tool_id).await.ok_or_else(|| format!("Unknown tool: {}", widget.tool_id))?;
    let tc = tool_config(&def, &widget);
    let target = label(&id);
    Ok(run_files(&app, &state, tc, &lang, paths, widget.combine, Some(&target)).await)
}

#[tauri::command]
pub fn widget_show_main(app: AppHandle) {
    crate::show_main(&app);
}
