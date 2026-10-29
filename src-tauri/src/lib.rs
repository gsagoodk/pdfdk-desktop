// PDF.dk Flow - Main library
// Watched folders, "open with", drag-and-drop: every file goes through the
// website's API with the tool the user chose.

mod api;
mod auth;
mod config;
mod processor;
mod watcher;
mod widgets;

use config::{AppConfig, ToolConfig, ToolDefinition};
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent, Runtime,
};
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_notification::NotificationExt;
use tokio::sync::RwLock;
use tracing::{error, info};

// ------------------------------------------------------------ log buffer

static LOG_BUFFER: Lazy<Mutex<Vec<String>>> = Lazy::new(|| Mutex::new(Vec::new()));

pub fn add_log(message: &str) {
    let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
    let log_entry = format!("[{}] {}", timestamp, message);
    println!("{}", log_entry);
    if let Ok(mut logs) = LOG_BUFFER.lock() {
        logs.push(log_entry);
        if logs.len() > 500 {
            logs.remove(0);
        }
    }
}

// ------------------------------------------------------------- job list

static JOBS: Lazy<Mutex<Vec<processor::Job>>> = Lazy::new(|| Mutex::new(Vec::new()));

fn job_add(job: processor::Job) -> String {
    let id = job.id.clone();
    if let Ok(mut jobs) = JOBS.lock() {
        jobs.insert(0, job);
        jobs.truncate(200);
    }
    id
}

pub fn job_find(id: &str) -> Option<processor::Job> {
    JOBS.lock().ok().and_then(|jobs| jobs.iter().find(|j| j.id == id).cloned())
}

fn job_update(id: &str, f: impl FnOnce(&mut processor::Job)) {
    if let Ok(mut jobs) = JOBS.lock() {
        if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
            f(j);
        }
    }
}

// ------------------------------------------------------------- app state

pub struct AppState {
    pub config: Arc<RwLock<AppConfig>>,
    pub auth: Arc<RwLock<auth::AuthState>>,
    pub watcher: Arc<RwLock<Option<watcher::FolderWatcher>>>,
    pub catalog: Arc<RwLock<Vec<ToolDefinition>>>,
    /// files handed to us by the OS before the window was ready
    pub pending_files: Mutex<Vec<String>>,
    /// browser sign-in in progress: (random state, started at)
    pub login_state: Mutex<Option<(String, std::time::Instant)>>,
}

fn notify<R: Runtime>(app: &AppHandle<R>, cfg: &Arc<RwLock<AppConfig>>, title: &str, body: &str) {
    let enabled = cfg.try_read().map(|c| c.general.show_notifications).unwrap_or(true);
    if enabled {
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

fn tool_label(tc: &ToolConfig, lang: &str) -> String {
    match &tc.name {
        Some(b) if lang == "en" && !b.en.is_empty() => b.en.clone(),
        Some(b) if !b.da.is_empty() => b.da.clone(),
        _ => tc.id.clone(),
    }
}

/// Consume watcher events: run the job, track it, notify.
fn spawn_event_loop(app: AppHandle, state_auth: Arc<RwLock<auth::AuthState>>, state_cfg: Arc<RwLock<AppConfig>>, mut rx: tokio::sync::broadcast::Receiver<watcher::FileEvent>) {
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let file_name = event.path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
            let lang = state_cfg.read().await.general.language.clone();
            let label = tool_label(&event.tool_config, &lang);
            add_log(&format!("{} → {}", file_name, label));

            let job_id = job_add(processor::Job::new(&event.tool_id, &label, &event.path.to_string_lossy()).with_key(&event.tool_config.key));
            job_update(&job_id, |j| j.set_processing());

            let token = state_auth.read().await.token.clone();
            match watcher::process_file_event(event.clone(), token).await {
                Ok(output_path) => {
                    add_log(&format!("OK: {} → {:?}", file_name, output_path));
                    job_update(&job_id, |j| j.set_completed(&output_path.to_string_lossy()));
                    notify(&app, &state_cfg, &format!("{} · PDF.dk", label), &if lang == "en" { format!("{} is done", file_name) } else { format!("{} er færdig", file_name) });
                }
                Err(e) => {
                    let msg = e.to_string();
                    add_log(&format!("FAILED: {} — {}", file_name, msg));
                    // Out of the hot folder into Failed/, with a job file PDF.dk Client can show.
                    let parked = watcher::park_failed(&event.path, &event.tool_config, &label, &msg).await;
                    job_update(&job_id, |j| {
                        j.set_failed(&msg);
                        if let Some(p) = &parked {
                            j.input_file = p.to_string_lossy().into_owned();
                        }
                    });
                    notify(&app, &state_cfg, &format!("{} · PDF.dk", label), &format!("{}: {}", file_name, msg));
                }
            }
        }
    });
}

async fn ensure_watcher(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let mut guard = state.watcher.write().await;
    if guard.is_none() {
        let (w, rx) = watcher::FolderWatcher::new().map_err(|e| format!("Failed to create file watcher: {}", e))?;
        spawn_event_loop(app.clone(), state.auth.clone(), state.config.clone(), rx);
        *guard = Some(w);
    }
    Ok(())
}

// -------------------------------------------------------------- commands

#[tauri::command]
async fn get_config(state: tauri::State<'_, AppState>) -> Result<AppConfig, String> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn save_config(app: AppHandle, state: tauri::State<'_, AppState>, new_config: AppConfig) -> Result<(), String> {
    {
        let mut config = state.config.write().await;
        *config = new_config.clone();
    }
    config::save_config(&new_config).map_err(|e| e.to_string())?;
    // start-at-login follows the setting
    let al = app.autolaunch();
    let res = if new_config.general.start_on_login { al.enable() } else { al.disable() };
    if let Err(e) = res {
        add_log(&format!("autostart: {}", e));
    }
    Ok(())
}

#[tauri::command]
async fn get_auth_state(state: tauri::State<'_, AppState>) -> Result<auth::AuthState, String> {
    Ok(state.auth.read().await.clone())
}

async fn fill_usage(result: &mut auth::AuthState) {
    if let Some(ref token) = result.token {
        let client = api::PdfDkClient::new(Some(token.clone()));
        if let Ok(usage) = client.get_usage_status().await {
            result.apply_plan(&usage.plan);
            result.jobs_limit = Some(usage.limit);
            result.jobs_used = Some(usage.used);
            result.jobs_remaining = Some(usage.limit - usage.used);
            result.max_file_size_mb = usage.max_file_size_mb.or(Some(100));
            result.is_unlimited = Some(usage.is_unlimited);
        }
    }
}

#[tauri::command]
async fn login(app: AppHandle, state: tauri::State<'_, AppState>, email: String, password: String, remember: Option<bool>) -> Result<auth::AuthState, String> {
    let mut result = auth::login(&email, &password).await.map_err(|e| e.to_string())?;
    fill_usage(&mut result).await;
    {
        let mut auth_state = state.auth.write().await;
        *auth_state = result.clone();
    }
    auth::save_token(&result.token.clone().unwrap_or_default()).map_err(|e| e.to_string())?;
    let _ = app.emit("auth-changed", ());
    if remember.unwrap_or(false) {
        if let Err(e) = auth::save_credentials(&email, &password) {
            error!("Failed to save credentials: {}", e);
        }
    } else {
        let _ = auth::clear_credentials();
    }
    Ok(result)
}

/// Browser sign-in, step 1: open pdf.dk/desktop-login with a one-time state. The page
/// signs the user in (e-mail, Google, Microsoft — whatever they use) and sends the
/// token back through pdfdk://auth?token=…&state=… (handled in `handle_deep_link`).
#[tauri::command]
async fn start_browser_login(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    if let Ok(mut g) = state.login_state.lock() {
        *g = Some((nonce.clone(), std::time::Instant::now()));
    }
    let lang = state.config.read().await.general.language.clone();
    let prefix = if lang == "en" { "/en" } else { "" };
    let url = format!("{}{}/desktop-login?state={}", api::site_base(), prefix, nonce);
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| e.to_string())?;
    Ok(url)
}

/// Browser sign-in, step 2 (also usable by the "paste code" fallback on the login page).
#[tauri::command]
async fn login_with_token(app: AppHandle, state: tauri::State<'_, AppState>, token: String) -> Result<auth::AuthState, String> {
    let mut result = auth::validate_token(token.trim()).await.map_err(|e| e.to_string())?;
    fill_usage(&mut result).await;
    {
        let mut auth_state = state.auth.write().await;
        *auth_state = result.clone();
    }
    auth::save_token(token.trim()).map_err(|e| e.to_string())?;
    let _ = auth::clear_credentials();
    let _ = app.emit("auth-changed", ());
    Ok(result)
}

fn handle_deep_link(app: &AppHandle, url: &str) {
    let Ok(parsed) = url::Url::parse(url) else { return };
    if parsed.scheme() != "pdfdk" || parsed.host_str() != Some("auth") {
        return;
    }
    let mut token = None;
    let mut state_param = None;
    for (k, v) in parsed.query_pairs() {
        match k.as_ref() {
            "token" => token = Some(v.to_string()),
            "state" => state_param = Some(v.to_string()),
            _ => {}
        }
    }
    let (Some(token), Some(state_param)) = (token, state_param) else { return };
    let Some(state) = app.try_state::<AppState>() else { return };
    let expected = state.login_state.lock().ok().and_then(|g| g.clone());
    match expected {
        Some((nonce, started)) if nonce == state_param && started.elapsed().as_secs() < 900 => {}
        _ => {
            add_log("Ignored a sign-in link that was not requested by this app");
            return;
        }
    }
    if let Ok(mut g) = state.login_state.lock() {
        *g = None;
    }
    show_main(app);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app2.state::<AppState>();
        match login_with_token(app2.clone(), state, token).await {
            Ok(auth) => {
                add_log(&format!("Signed in via browser as {}", auth.user.as_ref().map(|u| u.email.clone()).unwrap_or_default()));
                let _ = app2.emit("auth-changed", ());
            }
            Err(e) => {
                add_log(&format!("Browser sign-in failed: {}", e));
                let _ = app2.emit("auth-error", e);
            }
        }
    });
}

/// Where a watched folder's results go: "subfolder" (Processed/), "same" or a custom folder.
#[tauri::command]
async fn update_tool_output(state: tauri::State<'_, AppState>, key: String, mode: String, path: Option<String>) -> Result<(), String> {
    let new_mode = match mode.as_str() {
        "same" => config::OutputMode::SameFolder,
        "custom" => config::OutputMode::Custom(path.filter(|p| !p.is_empty()).ok_or("A folder is required")?),
        _ => config::OutputMode::Subfolder,
    };
    let tc = {
        let mut config = state.config.write().await;
        let tc = config.tools.iter_mut().find(|t| t.key == key).ok_or("Unknown folder entry")?;
        tc.output_mode = new_mode;
        let copy = tc.clone();
        config::save_config(&config).map_err(|e| e.to_string())?;
        copy
    };
    // the watcher keeps its own copy of the entry
    let mut guard = state.watcher.write().await;
    if let (Some(w), Some(folder)) = (guard.as_mut(), tc.folder_path.clone()) {
        let _ = w.remove_folder(Path::new(&folder)).await;
        let _ = w.add_folder(tc).await;
    }
    Ok(())
}

/// Run a failed job again. A watched-folder file that is still in its folder goes
/// through the folder's settings (result in Processed/, original to Originals/);
/// anything else is treated like a dropped file (result next to the source).
#[tauri::command]
async fn retry_job(app: AppHandle, state: tauri::State<'_, AppState>, job_id: String) -> Result<(), String> {
    let job = processor::job_get(&job_id).ok_or("Unknown job")?;
    let input = PathBuf::from(&job.input_file);
    if !input.is_file() {
        return Err(if state.config.read().await.general.language == "en" { "The original file is no longer there".into() } else { "Originalfilen findes ikke længere".into() });
    }
    // The hot folder it came from (also for a file waiting in Failed/).
    let parent = Some(watcher::hot_folder_of(&input));
    // 0.4.6: rerun with the CURRENT options of whatever ran the job — its widget, its folder
    // entry — not a stale copy. Falls back to the folder-by-path lookup and the tool card.
    let widget_tc = match job.key.as_deref().and_then(|k| k.strip_prefix("widget:")) {
        Some(id) => widgets::widget_tool_config(&state, id).await,
        None => None,
    };
    let entry = match widget_tc {
        Some(tc) => Some(tc),
        None => {
            let config = state.config.read().await;
            job.key.as_deref().and_then(|k| config.tools.iter().find(|t| t.key == k && t.enabled).cloned())
                .or_else(|| config.tools.iter().find(|t| t.id == job.tool_id && t.enabled && t.folder_path.as_deref().map(Path::new) == parent.as_deref()).cloned())
        }
    };
    let token = state.auth.read().await.token.clone();
    job_update(&job_id, |j| { j.status = processor::JobStatus::Processing; j.error = None; j.output_file = None; });
    let result = match entry {
        Some(tc) => watcher::process_file_event(watcher::FileEvent { path: input.clone(), tool_id: tc.id.clone(), tool_config: tc }, token).await,
        None => {
            let out = process_files(app.clone(), state.clone(), job.tool_id.clone(), vec![job.input_file.clone()], None).await?;
            match out.into_iter().next() {
                Some(r) if r.output.is_some() => Ok(PathBuf::from(r.output.unwrap())),
                Some(r) => Err(api::ApiError::ServerError(r.error.unwrap_or_default())),
                None => Err(api::ApiError::ServerError("no result".into())),
            }
        }
    };
    let file_name = input.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
    match result {
        Ok(output) => {
            job_update(&job_id, |j| j.set_completed(&output.to_string_lossy()));
            add_log(&format!("Retry OK: {} → {:?}", file_name, output));
            Ok(())
        }
        Err(e) => {
            job_update(&job_id, |j| j.set_failed(&e.to_string()));
            add_log(&format!("Retry FAILED: {} — {}", file_name, e));
            Err(e.to_string())
        }
    }
}

/// Test hook: PDFDK_AUTO_UPDATE_TEST=1 makes the app install an available update
/// right after the start-up check (used to reproduce updater failures from a terminal).
#[tauri::command]
fn auto_update_test() -> bool {
    std::env::var("PDFDK_AUTO_UPDATE_TEST").map(|v| v == "1").unwrap_or(false)
}

/// The webview's errors end up in the in-app log (Activity → logs), where a user can see them.
#[tauri::command]
fn log_client(message: String) {
    add_log(&format!("UI: {}", message.chars().take(500).collect::<String>()));
}

#[tauri::command]
async fn get_saved_credentials() -> Result<Option<serde_json::Value>, String> {
    match auth::load_credentials() {
        Ok((email, password)) => Ok(Some(serde_json::json!({ "email": email, "password": password }))),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
async fn logout(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let token = {
        let mut auth_state = state.auth.write().await;
        let t = auth_state.token.clone();
        *auth_state = auth::AuthState::default();
        t
    };
    auth::clear_token().map_err(|e| e.to_string())?;
    let _ = app.emit("auth-changed", ());
    // revoke on the server too (0.3.4); failing that is logged, not fatal — the token is gone locally
    if let Some(t) = token {
        match auth::revoke_token(&t).await {
            Ok(()) => add_log("Signed out — the desktop token was revoked on the server"),
            Err(e) => add_log(&format!("Signed out locally; the server could not revoke the token: {}", e)),
        }
    }
    Ok(())
}

/// One Pro account, one computer (shared with PDF.dk Client on the same machine): where this
/// account's Pro is. pdf.dk answers the licence call with 409 `device_limit` when it's on another
/// computer; then hot folders and widgets don't run here until the user moves it.
#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct Seat {
    /// "here", "elsewhere" or "unknown" (offline, not signed in).
    state: String,
    name: Option<String>,
    last_seen: Option<String>,
    next_allowed: Option<String>,
    message: Option<String>,
}

fn seat_from(status: reqwest::StatusCode, v: &serde_json::Value) -> Seat {
    let s = |x: &serde_json::Value| x.as_str().map(|s| s.chars().take(80).collect::<String>());
    let active = api::field(v, "active");
    let code = api::field(v, "code").as_str();
    let state = if status == reqwest::StatusCode::CONFLICT && code == Some("device_limit") {
        "elsewhere"
    } else if status.is_success() {
        "here"
    } else {
        "unknown"
    };
    Seat { state: state.into(), name: s(&active["name"]), last_seen: s(&active["last_seen"]), next_allowed: s(api::field(v, "next_allowed")), message: None }
}

#[tauri::command]
async fn seat_status() -> Seat {
    let Ok(token) = auth::load_token() else { return Seat { state: "unknown".into(), ..Default::default() } };
    match api::http_client().get(format!("{}/licence/client", api::api_base())).bearer_auth(token).header("Accept", "application/json").send().await {
        Ok(r) => {
            let st = r.status();
            seat_from(st, &r.json().await.unwrap_or_default())
        }
        Err(_) => Seat { state: "unknown".into(), ..Default::default() },
    }
}

/// "Brug Pro her i stedet": this computer takes the account's Pro (a few moves a month).
#[tauri::command]
async fn seat_activate() -> Seat {
    let Ok(token) = auth::load_token() else { return Seat { state: "unknown".into(), ..Default::default() } };
    let r = match api::http_client().post(format!("{}/licence/client/activate", api::api_base())).bearer_auth(token).header("Accept", "application/json").send().await {
        Ok(r) => r,
        Err(e) => return Seat { state: "unknown".into(), message: Some(e.to_string()), ..Default::default() },
    };
    let st = r.status();
    let v: serde_json::Value = r.json().await.unwrap_or_default();
    if st == reqwest::StatusCode::TOO_MANY_REQUESTS || api::field(&v, "code").as_str() == Some("move_limit") {
        let mut s = seat_status().await;
        s.next_allowed = api::field(&v, "next_allowed").as_str().map(String::from);
        s.message = Some("move_limit".into());
        return s;
    }
    if st.is_success() {
        return Seat { state: "here".into(), ..Default::default() };
    }
    let mut s = seat_status().await;
    s.message = api::field(&v, "message").as_str().map(String::from).or(Some(format!("pdf.dk {st}")));
    s
}

#[tauri::command]
async fn check_auth(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<auth::AuthState, String> {
    if let Ok(token) = auth::load_token() {
        match auth::validate_token(&token).await {
            Ok(mut result) => {
                fill_usage(&mut result).await;
                {
                    let mut auth_state = state.auth.write().await;
                    *auth_state = result.clone();
                }
                // widgets open before the main window has validated the session — tell them (0.4.3)
                let _ = app.emit("auth-changed", ());
                return Ok(result);
            }
            Err(e) => add_log(&format!("Saved session not valid: {}", e)),
        }
    }
    Ok(auth::AuthState::default())
}

/// The tools catalogue: live from the API, else the cached copy, else the
/// two built-in tools. Never empty.
#[tauri::command]
async fn get_available_tools(state: tauri::State<'_, AppState>, refresh: Option<bool>) -> Result<Vec<ToolDefinition>, String> {
    if !refresh.unwrap_or(false) {
        let current = state.catalog.read().await;
        if !current.is_empty() {
            return Ok(current.clone());
        }
    }
    let token = state.auth.read().await.token.clone();
    let client = api::PdfDkClient::new(token);
    let tools = match client.fetch_catalog().await {
        Ok(cache) => {
            add_log(&format!("Tools catalogue {} loaded: {} tools", cache.version.clone().unwrap_or_default(), cache.tools.len()));
            if let Err(e) = config::save_cached_catalog(&cache) {
                add_log(&format!("Could not cache catalogue: {}", e));
            }
            cache.tools
        }
        Err(e) => {
            add_log(&format!("Catalogue fetch failed ({}), using cached copy", e));
            match config::load_cached_catalog() {
                Some(c) if !c.tools.is_empty() => c.tools,
                _ => config::builtin_catalog(),
            }
        }
    };
    let mut cat = state.catalog.write().await;
    *cat = tools.clone();
    Ok(tools)
}

pub(crate) async fn find_tool(state: &AppState, tool_id: &str) -> Option<ToolDefinition> {
    let cat = state.catalog.read().await;
    cat.iter().find(|t| t.id == tool_id).cloned()
}

#[tauri::command]
async fn enable_tool(app: AppHandle, state: tauri::State<'_, AppState>, tool_id: String, folder_path: String, key: Option<String>) -> Result<(), String> {
    let def = find_tool(&state, &tool_id).await.ok_or_else(|| format!("Unknown tool: {}", tool_id))?;
    // changing an existing entry's folder: stop watching the old one first
    let old_folder = match key.as_deref() {
        Some(k) => state.config.read().await.tools.iter().find(|t| t.key == k).and_then(|t| t.folder_path.clone()).map(PathBuf::from),
        None => None,
    };
    let tool_config = {
        let mut config = state.config.write().await;
        let k = config.enable_tool(&def, &folder_path, key.as_deref()).map_err(|e| e.to_string())?;
        config::save_config(&config).map_err(|e| e.to_string())?;
        config.tools.iter().find(|t| t.key == k).cloned()
    };
    if let Some(tc) = tool_config {
        ensure_watcher(&app, &state).await?;
        let mut guard = state.watcher.write().await;
        if let Some(w) = guard.as_mut() {
            if let Some(old) = old_folder.filter(|o| o.as_os_str() != std::ffi::OsStr::new(&folder_path)) {
                let _ = w.remove_folder(&old).await;
            }
            w.add_folder(tc).await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn disable_tool(state: tauri::State<'_, AppState>, key: String) -> Result<(), String> {
    let folder_path = {
        let mut config = state.config.write().await;
        let f = config.disable_tool(&key);
        config::save_config(&config).map_err(|e| e.to_string())?;
        f
    };
    if let Some(path) = folder_path {
        let mut guard = state.watcher.write().await;
        if let Some(w) = guard.as_mut() {
            let _ = w.remove_folder(&path).await;
        }
    }
    Ok(())
}

/// `key` = one watched folder's options; without it the tool's first entry
/// (also what drag-and-drop / Open with use), created disabled if needed.
#[tauri::command]
async fn update_tool_options(app: AppHandle, state: tauri::State<'_, AppState>, tool_id: String, options: serde_json::Value, key: Option<String>) -> Result<(), String> {
    if let Some(id) = key.as_deref().and_then(|k| k.strip_prefix("widget:")) {
        return widgets::widget_update(app, state, id.to_string(), options, None).await;
    }
    let mut config = state.config.write().await;
    let found = match key.as_deref() {
        Some(k) => config.tools.iter_mut().find(|t| t.key == k),
        None => config.tools.iter_mut().find(|t| t.id == tool_id),
    };
    if let Some(tc) = found {
        // 0.4.6: widgets of this tool that still carry the card's previous options (never
        // customised on their own) follow the card. A widget edited separately keeps its own.
        let previous = tc.options.clone();
        tc.options = options.clone();
        if key.is_none() {
            let mut changed = Vec::new();
            for w in config.widgets.iter_mut().filter(|w| w.tool_id == tool_id) {
                let own = w.options.as_object().map(|o| o.is_empty()).unwrap_or(true);
                if own || w.options == previous {
                    w.options = options.clone();
                    changed.push(w.id.clone());
                }
            }
            for id in changed {
                let _ = app.emit_to(widgets::label(&id), "widget-changed", ());
            }
        }
    } else if key.is_some() {
        return Err("Unknown folder entry".to_string());
    } else {
        let def = { let cat = state.catalog.read().await; cat.iter().find(|t| t.id == tool_id).cloned() };
        let Some(def) = def else { return Err(format!("Unknown tool: {}", tool_id)) };
        config.tools.push(ToolConfig {
            id: def.id.clone(), key: def.id.clone(), enabled: false, folder_path: None, output_mode: config::OutputMode::Subfolder,
            options: options.clone(), endpoint: Some(def.endpoint.clone()), file_field: Some(def.file_field.clone()),
            accepts: def.accepts.clone(), output: Some(def.output.clone()), name: Some(def.name.clone()),
        });
        // first card save: widgets of this tool created with empty options follow it
        let mut changed = Vec::new();
        for w in config.widgets.iter_mut().filter(|w| w.tool_id == tool_id) {
            if w.options.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                w.options = options.clone();
                changed.push(w.id.clone());
            }
        }
        for id in changed {
            let _ = app.emit_to(widgets::label(&id), "widget-changed", ());
        }
    }
    config::save_config(&config).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn start_watchers(app: AppHandle, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let enabled: Vec<ToolConfig> = {
        let config = state.config.read().await;
        config.tools.iter().filter(|t| t.enabled && t.folder_path.is_some()).cloned().collect()
    };
    if enabled.is_empty() {
        return Ok(());
    }
    ensure_watcher(&app, &state).await?;
    let mut guard = state.watcher.write().await;
    if let Some(w) = guard.as_mut() {
        for tool in enabled {
            if let Err(e) = w.add_folder(tool.clone()).await {
                add_log(&format!("Could not watch folder for {}: {}", tool.id, e));
            }
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileResult {
    pub input: String,
    pub output: Option<String>,
    pub error: Option<String>,
}

/// Run files through one tool config: jobs are tracked (Aktivitet), the user is notified,
/// output lands next to each input. Shared by the drop zone, Open with and the widgets.
/// Per-file progress for a window (the widgets): {done, total, file, ok}.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunProgress {
    pub done: usize,
    pub total: usize,
    pub file: String,
    pub ok: bool,
}

pub(crate) async fn run_files(app: &AppHandle, state: &AppState, tc: ToolConfig, lang: &str, paths: Vec<String>, combine: bool, progress_to: Option<&str>) -> Vec<FileResult> {
    let label = tool_label(&tc, lang);
    let token = state.auth.read().await.token.clone();
    let mut results = Vec::new();
    // "Saml i én fil": tools with a files[] field take all dropped files in one request
    let multi_tool = tc.file_field.as_deref().map(|f| f.ends_with("[]")).unwrap_or(false);
    if combine && multi_tool && paths.len() > 1 {
        let inputs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        let job_id = job_add(processor::Job::new(&tc.id, &label, &paths[0]).with_key(&tc.key));
        job_update(&job_id, |j| j.set_processing());
        let parent = inputs[0].parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        return match watcher::run_tool_multi(&inputs, &tc, token.clone(), Some(&parent)).await {
            Ok(out) => {
                job_update(&job_id, |j| j.set_completed(&out.to_string_lossy()));
                notify(app, &state.config, &format!("{} · PDF.dk", label), &if lang == "en" { format!("{} files combined", paths.len()) } else { format!("{} filer samlet", paths.len()) });
                vec![FileResult { input: paths.join(", "), output: Some(out.to_string_lossy().to_string()), error: None }]
            }
            Err(e) => {
                job_update(&job_id, |j| j.set_failed(&e.to_string()));
                vec![FileResult { input: paths.join(", "), output: None, error: Some(e.to_string()) }]
            }
        };
    }
    let total = paths.len();
    for p in paths {
        let path = PathBuf::from(&p);
        let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if let Some(label) = progress_to {
            let _ = app.emit_to(label, "widget-progress", RunProgress { done: results.len(), total, file: file_name.clone(), ok: true });
        }
        let job_id = job_add(processor::Job::new(&tc.id, &label, &p).with_key(&tc.key));
        job_update(&job_id, |j| j.set_processing());
        let parent = path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        let ok = match watcher::run_tool(&path, &tc, token.clone(), Some(&parent)).await {
            Ok(out) => {
                job_update(&job_id, |j| j.set_completed(&out.to_string_lossy()));
                results.push(FileResult { input: p, output: Some(out.to_string_lossy().to_string()), error: None });
                true
            }
            Err(e) => {
                job_update(&job_id, |j| j.set_failed(&e.to_string()));
                results.push(FileResult { input: p, output: None, error: Some(e.to_string()) });
                false
            }
        };
        if let Some(label) = progress_to {
            let _ = app.emit_to(label, "widget-progress", RunProgress { done: results.len(), total, file: file_name, ok });
        }
    }
    let ok = results.iter().filter(|r| r.output.is_some()).count();
    notify(app, &state.config, &format!("{} · PDF.dk", label), &if lang == "en" { format!("{} of {} files done", ok, results.len()) } else { format!("{} af {} filer færdige", ok, results.len()) });
    results
}

/// Run files the user dropped on the window / opened with the app through
/// one tool. Output lands next to the source; the original stays put.
#[tauri::command]
async fn process_files(app: AppHandle, state: tauri::State<'_, AppState>, tool_id: String, paths: Vec<String>, combine: Option<bool>) -> Result<Vec<FileResult>, String> {
    let def = find_tool(&state, &tool_id).await.ok_or_else(|| format!("Unknown tool: {}", tool_id))?;
    let (tc, lang) = {
        let config = state.config.read().await;
        let saved = config.tools.iter().find(|t| t.id == tool_id).cloned();
        let mut options = def.default_options();
        if let Some(s) = saved {
            if let (Some(base), Some(over)) = (options.as_object_mut(), s.options.as_object()) {
                for (k, v) in over { base.insert(k.clone(), v.clone()); }
            }
        }
        (ToolConfig {
            id: def.id.clone(),
            key: def.id.clone(), enabled: true, folder_path: None, output_mode: config::OutputMode::SameFolder,
            options, endpoint: Some(def.endpoint.clone()), file_field: Some(def.file_field.clone()),
            accepts: def.accepts.clone(), output: Some(def.output.clone()), name: Some(def.name.clone()),
        }, config.general.language.clone())
    };
    Ok(run_files(&app, &state, tc, &lang, paths, combine.unwrap_or(false), None).await)
}

#[tauri::command]
fn take_pending_files(state: tauri::State<'_, AppState>) -> Vec<String> {
    state.pending_files.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

#[tauri::command]
fn get_jobs() -> Vec<processor::Job> {
    JOBS.lock().map(|j| j.clone()).unwrap_or_default()
}

/// Opens a job's file in PDF.dk Client (the separate viewer/editor) – to see why a hot folder
/// failed on it, or to check a result. Only a file one of this app's jobs read or wrote: the
/// window can't use it to open anything else. Err("not_installed") when Client isn't there.
#[tauri::command]
fn open_in_client(path: String) -> Result<(), String> {
    let known = JOBS.lock().map(|jobs| jobs.iter().any(|j| j.input_file == path || j.output_file.as_deref() == Some(path.as_str()))).unwrap_or(false);
    if !known || !Path::new(&path).is_file() {
        return Err("unknown".into());
    }
    #[cfg(target_os = "macos")]
    {
        // By bundle id: the installed Client, wherever it is; the path as a document to open.
        let ok = std::process::Command::new("/usr/bin/open").args(["-b", "dk.pdf.client", "--", &path]).status().map(|s| s.success()).unwrap_or(false);
        return if ok { Ok(()) } else { Err("not_installed".into()) };
    }
    #[cfg(target_os = "windows")]
    {
        // Where the installers put it: per user (NSIS default), or for all users.
        let mut candidates = Vec::new();
        for var in ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(base) = std::env::var(var) {
                candidates.push(PathBuf::from(&base).join("PDF.dk Client").join("pdfdk-client.exe"));
                candidates.push(PathBuf::from(&base).join("Programs").join("PDF.dk Client").join("pdfdk-client.exe"));
            }
        }
        let Some(exe) = candidates.into_iter().find(|p| p.is_file()) else { return Err("not_installed".into()) };
        return std::process::Command::new(exe).arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string());
    }
    #[allow(unreachable_code)]
    Err("not_installed".into())
}

#[tauri::command]
fn get_site_base() -> String {
    api::site_base()
}

#[tauri::command]
fn get_logs() -> Vec<String> {
    LOG_BUFFER.lock().map(|logs| logs.clone()).unwrap_or_default()
}

#[tauri::command]
fn clear_logs() {
    if let Ok(mut logs) = LOG_BUFFER.lock() {
        logs.clear();
    }
}

// ------------------------------------------------------------------ tray

fn setup_tray<R: Runtime>(app: &tauri::App<R>) -> Result<(), Box<dyn std::error::Error>> {
    let show = tauri::menu::MenuItem::with_id(app, "show", "Vis PDF.dk Flow", true, None::<&str>)?;
    let site = tauri::menu::MenuItem::with_id(app, "site", "Åbn pdf.dk", true, None::<&str>)?;
    let quit = tauri::menu::MenuItem::with_id(app, "quit", "Afslut", true, None::<&str>)?;
    let menu = tauri::menu::Menu::with_items(app, &[&show, &site, &quit])?;

    let tray = match app.tray_by_id("main") {
        Some(t) => t,
        None => TrayIconBuilder::with_id("main").icon(app.default_window_icon().cloned().ok_or("no icon")?).build(app)?,
    };
    tray.set_menu(Some(menu))?;
    tray.set_show_menu_on_left_click(false)?;

    tray.on_menu_event(|app, event| match event.id.as_ref() {
        "show" => show_main(app),
        "site" => {
            let _ = tauri_plugin_opener::open_url(api::site_base(), None::<&str>);
        }
        "quit" => app.exit(0),
        _ => {}
    });
    tray.on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            show_main(tray.app_handle());
        }
    });
    Ok(())
}

pub(crate) fn show_main<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Files handed to us by the OS (dock drop, "Open with", command line).
fn deliver_files<R: Runtime>(app: &AppHandle<R>, paths: Vec<String>) {
    let paths: Vec<String> = paths.into_iter().filter(|p| Path::new(p).is_file()).collect();
    if paths.is_empty() {
        return;
    }
    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(mut pending) = state.pending_files.lock() {
            pending.extend(paths.clone());
        }
    }
    show_main(app);
    let _ = app.emit("files-opened", paths);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt::init();

    let cli_files: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with('-') && Path::new(a).is_file()).collect();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // second launch (Windows deep link / double-click): hand over and show the window
            let files: Vec<String> = args.iter().skip(1).filter(|a| !a.starts_with('-') && Path::new(a).is_file()).cloned().collect();
            if !files.is_empty() {
                deliver_files(app, files);
            } else {
                show_main(app);
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .setup(move |app| {
            let config = config::load_config().unwrap_or_default();
            let start_minimized = config.general.start_minimized && std::env::args().any(|a| a == "--minimized");
            let cached = config::load_cached_catalog().map(|c| c.tools).unwrap_or_default();

            app.manage(AppState {
                config: Arc::new(RwLock::new(config)),
                auth: Arc::new(RwLock::new(auth::AuthState::default())),
                watcher: Arc::new(RwLock::new(None)),
                catalog: Arc::new(RwLock::new(cached)),
                pending_files: Mutex::new(cli_files.clone()),
                login_state: Mutex::new(None),
            });

            // pdfdk://auth?token=…&state=… comes back from the website after a browser sign-in
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                #[cfg(any(target_os = "linux", windows))]
                let _ = app.deep_link().register_all();
                let handle = app.handle().clone();
                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        handle_deep_link(&handle, url.as_str());
                    }
                });
            }

            if let Err(e) = setup_tray(app) {
                error!("Failed to setup tray: {}", e);
            }

            if let Some(window) = app.get_webview_window("main") {
                if start_minimized && cli_files.is_empty() {
                    let _ = window.hide();
                }
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }

            // saved desktop widgets come back with the app
            {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(state) = handle.try_state::<AppState>() {
                        widgets::open_all(&handle, &state).await;
                    }
                });
                widgets::start_test_hook(app.handle());
            }

            info!("PDF.dk Flow started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_auth_state,
            login,
            logout,
            check_auth,
            get_available_tools,
            enable_tool,
            disable_tool,
            update_tool_options,
            start_watchers,
            process_files,
            take_pending_files,
            get_jobs,
            get_site_base,
            get_saved_credentials,
            log_client,
            auto_update_test,
            start_browser_login,
            login_with_token,
            update_tool_output,
            retry_job,
            open_in_client,
            seat_status,
            seat_activate,
            get_logs,
            clear_logs,
            widgets::widget_list,
            widgets::widget_get,
            widgets::widget_create,
            widgets::widget_remove,
            widgets::widget_show,
            widgets::widget_update,
            widgets::widget_process,
            widgets::widget_show_main,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        #[cfg(target_os = "macos")]
        if let RunEvent::Opened { urls } = &event {
            let paths: Vec<String> = urls.iter().filter_map(|u| u.to_file_path().ok()).map(|p| p.to_string_lossy().to_string()).collect();
            deliver_files(app_handle, paths);
        }
        #[cfg(target_os = "macos")]
        if let RunEvent::Reopen { .. } = &event {
            show_main(app_handle);
        }
        let _ = (&event, app_handle);
    });
}
