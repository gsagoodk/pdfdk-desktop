// Configuration management for PDF.dk Flow
//
// Two things live here: the app's own settings (config.json in the OS config
// dir) and the tools catalogue, which the app fetches from GET /api/tools and
// caches next to the config so the app starts with the last known tools
// even before the network answers.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Config directory not found")]
    NoConfigDir,
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("{0}")]
    Invalid(String),
}

/// Saved authentication credentials
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AuthConfig {
    pub token: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
}

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub version: u32,
    pub general: GeneralSettings,
    pub tools: Vec<ToolConfig>,
    /// desktop widgets (0.4.0); missing in older configs
    #[serde(default)]
    pub widgets: Vec<WidgetConfig>,
    #[serde(default)]
    pub auth: Option<AuthConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    pub start_on_login: bool,
    pub start_minimized: bool,
    pub show_notifications: bool,
    /// "da" or "en" — the app follows the website's default, Danish
    pub language: String,
    /// install updates by itself when no job is running (0.4.5)
    #[serde(default = "default_true")]
    pub auto_update: bool,
    /// "system", "light" or "dark"
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_true() -> bool {
    true
}

fn default_theme() -> String {
    "system".to_string()
}

/// A tool the user has switched on, with its watched folder and options.
/// The catalogue fields (endpoint, accepted files, output) are copied in
/// when the tool is enabled so the watcher never needs the catalogue at
/// runtime; they are optional so a config written by v0.2 still loads.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    pub id: String,
    /// One entry per watched folder. A tool can have several (e.g. Farveprofil
    /// "check" in one folder, "convert" in another). Empty in configs written
    /// before 0.3.0 → set to the tool id on load.
    #[serde(default)]
    pub key: String,
    pub enabled: bool,
    pub folder_path: Option<String>,
    pub output_mode: OutputMode,
    pub options: serde_json::Value,
    #[serde(default)]
    pub endpoint: Option<String>,
    #[serde(default)]
    pub file_field: Option<String>,
    #[serde(default)]
    pub accepts: Vec<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub name: Option<Bilingual>,
}

impl ToolConfig {
    pub fn accepts_extension(&self, ext: &str) -> bool {
        if self.accepts.is_empty() {
            return ext.eq_ignore_ascii_case("pdf");
        }
        self.accepts.iter().any(|a| a.eq_ignore_ascii_case(ext))
    }
}

/// A desktop widget (0.4.0): one tool with its own options, sitting on the desktop as a
/// small drop target. Position is logical pixels; None = let the OS place it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetConfig {
    pub id: String,
    pub tool_id: String,
    #[serde(default)]
    pub options: serde_json::Value,
    /// several dropped files → one result (tools with a files[] field)
    #[serde(default)]
    pub combine: bool,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputMode {
    SameFolder,
    Subfolder,
    Custom(String),
}

// ------------------------------------------------------------ catalogue

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Bilingual {
    #[serde(default)]
    pub da: String,
    #[serde(default)]
    pub en: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionChoice {
    pub value: serde_json::Value,
    #[serde(default)]
    pub label: Bilingual,
}

/// One option of a tool, exactly as the API describes it. `kind` is the
/// API's `type`: select | number | boolean | text | hidden.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOption {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub label: Option<Bilingual>,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub choices: Vec<OptionChoice>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub secret: bool,
}

/// A tool as the catalogue describes it (snake_case, same as the API JSON,
/// and the same shape is handed to the frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub id: String,
    pub endpoint: String,
    #[serde(default = "default_file_field")]
    pub file_field: String,
    #[serde(default)]
    pub accepts: Vec<String>,
    #[serde(default = "default_output")]
    pub output: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub category_label: Bilingual,
    #[serde(default)]
    pub name: Bilingual,
    #[serde(default)]
    pub description: Bilingual,
    #[serde(default)]
    pub web_path: Bilingual,
    #[serde(default)]
    pub options: Vec<ToolOption>,
}

fn default_file_field() -> String {
    "file".to_string()
}

fn default_output() -> String {
    "pdf".to_string()
}

impl ToolDefinition {
    /// The options object a fresh enable starts from: every option at its
    /// default, hidden ones included, so the request always carries what
    /// the endpoint's validation expects.
    pub fn default_options(&self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        for opt in &self.options {
            if !opt.default.is_null() {
                map.insert(opt.name.clone(), opt.default.clone());
            }
        }
        serde_json::Value::Object(map)
    }

    pub fn has_visible_options(&self) -> bool {
        self.options.iter().any(|o| o.kind != "hidden")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CatalogCache {
    pub version: Option<String>,
    pub fetched_at: Option<String>,
    pub tools: Vec<ToolDefinition>,
}

/// The two tools the very first release had — used only when there is no
/// network and no cached catalogue yet, so the app is never empty.
pub fn builtin_catalog() -> Vec<ToolDefinition> {
    let da_en = |da: &str, en: &str| Bilingual { da: da.to_string(), en: en.to_string() };
    vec![
        ToolDefinition {
            id: "compress".into(), endpoint: "/api/compress".into(), file_field: "file".into(),
            accepts: vec!["pdf".into()], output: "pdf".into(), category: "Optimér".into(),
            category_label: da_en("Optimér", "Optimise"), name: da_en("Komprimer PDF", "Compress PDF"),
            description: da_en("Gør din PDF mindre uden synligt kvalitetstab.", "Shrink a PDF without visible quality loss."),
            web_path: da_en("/tools/compress/", "/en/tools/compress/"),
            options: vec![ToolOption {
                name: "quality".into(), kind: "select".into(), label: Some(da_en("Kvalitet", "Quality")),
                default: serde_json::json!("default"),
                choices: vec![
                    OptionChoice { value: serde_json::json!("low"), label: da_en("Lav", "Low") },
                    OptionChoice { value: serde_json::json!("default"), label: da_en("Standard", "Default") },
                    OptionChoice { value: serde_json::json!("high"), label: da_en("Høj", "High") },
                ],
                min: None, max: None, step: None, required: false, secret: false,
            }],
        },
        ToolDefinition {
            id: "outline".into(), endpoint: "/api/outline".into(), file_field: "file".into(),
            accepts: vec!["pdf".into()], output: "pdf".into(), category: "Tryk".into(),
            category_label: da_en("Tryk", "Print"), name: da_en("Tekst til kurver", "Fonts to outlines"),
            description: da_en("Konverter al tekst til kurver, så ingen fonts mangler hos trykkeriet.", "Convert all text to curves so no fonts are missing at the printer."),
            web_path: da_en("/tools/outline/", "/en/tools/outline/"),
            options: vec![],
        },
    ]
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            version: 2,
            general: GeneralSettings {
                start_on_login: true,
                start_minimized: true,
                show_notifications: true,
                auto_update: true,
                language: "da".to_string(),
                theme: "system".to_string(),
            },
            tools: vec![],
            widgets: Vec::new(),
            auth: None,
        }
    }
}

impl AppConfig {
    /// Enable a watched folder for `tool`. `key` = change the folder of that
    /// existing entry; `None` = reuse the tool's disabled entry (options holder)
    /// or add a new one. Returns the entry key.
    pub fn enable_tool(&mut self, tool: &ToolDefinition, folder_path: &str, key: Option<&str>) -> Result<String, ConfigError> {
        let path = PathBuf::from(folder_path);
        if !path.exists() {
            fs::create_dir_all(&path)?;
        }
        let processed_path = path.join("Processed");
        if !processed_path.exists() {
            fs::create_dir_all(&processed_path)?;
        }
        if let Some(other) = self.tools.iter().find(|t| {
            t.enabled && t.folder_path.as_deref() == Some(folder_path) && key.map_or(true, |k| t.key != k)
        }) {
            return Err(ConfigError::Invalid(format!("{} is already watched by {}", folder_path, other.id)));
        }

        let idx = match key {
            Some(k) => self.tools.iter().position(|t| t.key == k),
            None => self.tools.iter().position(|t| t.id == tool.id && !t.enabled),
        };
        let idx = match idx {
            Some(i) => i,
            None => {
                self.tools.push(ToolConfig {
                    id: tool.id.clone(),
                    key: self.new_key(&tool.id),
                    enabled: true,
                    folder_path: Some(folder_path.to_string()),
                    output_mode: OutputMode::Subfolder,
                    options: tool.default_options(),
                    endpoint: Some(tool.endpoint.clone()),
                    file_field: Some(tool.file_field.clone()),
                    accepts: tool.accepts.clone(),
                    output: Some(tool.output.clone()),
                    name: Some(tool.name.clone()),
                });
                self.tools.len() - 1
            }
        };
        let tc = &mut self.tools[idx];
        tc.enabled = true;
        tc.folder_path = Some(folder_path.to_string());
        tc.endpoint = Some(tool.endpoint.clone());
        tc.file_field = Some(tool.file_field.clone());
        tc.accepts = tool.accepts.clone();
        tc.output = Some(tool.output.clone());
        tc.name = Some(tool.name.clone());
        // keep the user's options, but fill in anything the schema added since
        if let (Some(existing), Some(defaults)) = (tc.options.as_object_mut(), tool.default_options().as_object()) {
            for (k, v) in defaults {
                existing.entry(k.clone()).or_insert(v.clone());
            }
        }
        Ok(tc.key.clone())
    }

    fn new_key(&self, tool_id: &str) -> String {
        if !self.tools.iter().any(|t| t.key == tool_id) {
            return tool_id.to_string();
        }
        let mut n = 2;
        loop {
            let k = format!("{}-{}", tool_id, n);
            if !self.tools.iter().any(|t| t.key == k) {
                return k;
            }
            n += 1;
        }
    }

    /// Stop watching one entry. The tool's last entry is kept (disabled) so its
    /// options survive; extra entries are removed. Returns the folder to unwatch.
    pub fn disable_tool(&mut self, key: &str) -> Option<PathBuf> {
        let idx = self.tools.iter().position(|t| t.key == key)?;
        let folder = self.tools[idx].folder_path.clone().map(PathBuf::from);
        let id = self.tools[idx].id.clone();
        let others = self.tools.iter().filter(|t| t.id == id && t.key != key).count();
        if others > 0 {
            self.tools.remove(idx);
        } else {
            self.tools[idx].enabled = false;
        }
        folder
    }

    /// Old configs have no entry keys: use the tool id, made unique.
    pub fn migrate_keys(&mut self) {
        let mut seen: Vec<String> = Vec::new();
        for i in 0..self.tools.len() {
            if self.tools[i].key.is_empty() {
                self.tools[i].key = self.tools[i].id.clone();
            }
            if seen.contains(&self.tools[i].key) {
                let base = self.tools[i].id.clone();
                let mut n = 2;
                while seen.contains(&format!("{}-{}", base, n)) { n += 1; }
                self.tools[i].key = format!("{}-{}", base, n);
            }
            seen.push(self.tools[i].key.clone());
        }
    }

    #[allow(dead_code)]
    pub fn get_enabled_tools(&self) -> Vec<&ToolConfig> {
        self.tools.iter().filter(|t| t.enabled).collect()
    }
}

// ---------------------------------------------------------------- files

fn app_config_dir() -> Result<PathBuf, ConfigError> {
    let config_dir = dirs::config_dir().ok_or(ConfigError::NoConfigDir)?;
    let app_config_dir = config_dir.join("dk.pdf.desktop");
    if !app_config_dir.exists() {
        fs::create_dir_all(&app_config_dir)?;
    }
    Ok(app_config_dir)
}

fn get_config_path() -> Result<PathBuf, ConfigError> {
    Ok(app_config_dir()?.join("config.json"))
}

fn get_catalog_path() -> Result<PathBuf, ConfigError> {
    Ok(app_config_dir()?.join("tools_catalog.json"))
}

/// First start: follow the OS language (macOS, Windows, Linux). Danish stays Danish;
/// everything else gets English. The user can still change it under Indstillinger → Sprog,
/// and a saved config is never overridden (0.4.4).
pub fn os_language() -> String {
    let loc = sys_locale::get_locale().unwrap_or_default().to_ascii_lowercase();
    if loc.starts_with("da") { "da".to_string() } else { "en".to_string() }
}

pub fn load_config() -> Result<AppConfig, ConfigError> {
    let path = get_config_path()?;
    if path.exists() {
        make_private(&path);
        let content = fs::read_to_string(&path)?;
        let mut config: AppConfig = serde_json::from_str(&content)?;
        config.migrate_keys();
        Ok(config)
    } else {
        let mut config = AppConfig::default();
        config.general.language = os_language();
        Ok(config)
    }
}

pub fn save_config(config: &AppConfig) -> Result<(), ConfigError> {
    let path = get_config_path()?;
    let content = serde_json::to_string_pretty(config)?;
    write_private(&path, content.as_bytes())?;
    Ok(())
}

/// The config holds the login token: written readable by this user only, from the first byte (a
/// temp file created 0600, then moved into place – also never a half-written config).
fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let _ = fs::remove_file(&tmp);
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(data)?;
    f.sync_all()?;
    drop(f);
    fs::rename(&tmp, path)
}

/// A config written by 0.4.6 or older is readable by other users: made private once.
fn make_private(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = fs::metadata(path) {
            if m.permissions().mode() & 0o077 != 0 {
                let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
            }
        }
    }
    let _ = path;
}

pub fn load_cached_catalog() -> Option<CatalogCache> {
    let path = get_catalog_path().ok()?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_cached_catalog(cache: &CatalogCache) -> Result<(), ConfigError> {
    let path = get_catalog_path()?;
    fs::write(&path, serde_json::to_string_pretty(cache)?)?;
    Ok(())
}

/// Get the default base folder path
#[allow(dead_code)]
pub fn get_default_base_folder() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("PDF.dk")
}
