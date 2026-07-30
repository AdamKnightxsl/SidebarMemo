use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub shortcut: String,
    pub theme: String,
    #[serde(default)]
    pub skin: String,
    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
    #[serde(default = "default_true")]
    pub always_on_top: bool,
    #[serde(default = "default_note_shortcut")]
    pub note_shortcut: String,
    #[serde(default)]
    pub note_width: Option<u32>,
    #[serde(default)]
    pub note_height: Option<u32>,
}

fn default_true() -> bool { true }
fn default_note_shortcut() -> String { "Alt+N".into() }

impl Default for Settings {
    fn default() -> Self {
        Self {
            shortcut: "Alt+M".into(),
            theme: "dark".into(),
            skin: String::new(),
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
            always_on_top: true,
            note_shortcut: "Alt+N".into(),
            note_width: None,
            note_height: None,
        }
    }
}

trait Pipe { fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R, Self: Sized; }
impl<T> Pipe for T { fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) } }

pub(crate) fn save_settings(s: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    let d = dirs::config_dir().ok_or("no config dir")?.join("sidebar-memo");
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join("settings.json"), serde_json::to_string_pretty(s)?)?;
    Ok(())
}

pub(crate) fn load_settings() -> Settings {
    dirs::config_dir().unwrap_or_default().join("sidebar-memo").join("settings.json")
        .pipe(|p| std::fs::read_to_string(p).ok())
        .and_then(|d| serde_json::from_str(&d).ok()).unwrap_or_default()
}

pub(crate) fn save_window_position(app: &AppHandle, window: &tauri::WebviewWindow) {
    if let Ok(pos) = window.outer_position() {
        if let Ok(size) = window.inner_size() {
            if let Some(monitor) = window.primary_monitor().ok().flatten() {
                let wa = monitor.work_area();
                if pos.x < wa.position.x || pos.y < wa.position.y
                    || pos.x > wa.position.x + wa.size.width as i32
                    || pos.y > wa.position.y + wa.size.height as i32 {
                    return;
                }
            }
            let state = app.state::<crate::AppState>();
            let _ = state.settings.lock().map(|mut settings| {
                settings.window_x = Some(pos.x);
                settings.window_y = Some(pos.y);
                settings.window_width = Some(size.width);
                settings.window_height = Some(size.height);
                let _ = save_settings(&settings);
            });
        }
    }
}

#[tauri::command]
pub(crate) fn get_settings(state: tauri::State<crate::AppState>) -> Result<Settings, String> {
    state.settings.lock().map_err(|e| e.to_string())?.clone().pipe(Ok)
}

#[tauri::command]
pub(crate) fn set_theme(state: tauri::State<crate::AppState>, t: String) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.theme = t;
    save_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn set_skin(state: tauri::State<crate::AppState>, s: String) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.skin = s;
    save_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn toggle_always_on_top(app: AppHandle, state: tauri::State<crate::AppState>) -> Result<bool, String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.always_on_top = !settings.always_on_top;
    let new_val = settings.always_on_top;
    save_settings(&settings).map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(new_val);
    }
    Ok(new_val)
}
