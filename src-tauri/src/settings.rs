use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

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

/// 配置目录：{config_dir}/sidebar-memo。config_dir 在 Windows 上为 %APPDATA%\Roaming。
fn settings_dir() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("sidebar-memo"))
}

/// settings.json 的绝对路径（config_dir 不可用时返回 None）。
fn settings_file() -> Option<std::path::PathBuf> {
    settings_dir().map(|d| d.join("settings.json"))
}

/// 诊断日志：写入 {config_dir}/sidebar-memo/settings-diag.log。
/// 发布版没有控制台，eprintln 不可见，因此把关键路径与读写结果落到文件，
/// 便于在“切换后重启丢失”这类只在个别机器复现的环境问题里抓到真实原因。
/// 文件超过 32KB 时截断重写，避免无限增长。
pub(crate) fn diag(msg: &str) {
    // 去重：每 5 秒的窗口位置自动保存会反复产生相同的 SAVE 行，内容未变就不重复写，
    // 避免把启动时的 LOAD 行等关键信息刷没。
    use std::sync::Mutex;
    static LAST: Mutex<Option<String>> = Mutex::new(None);
    if let Ok(mut last) = LAST.lock() {
        if last.as_deref() == Some(msg) {
            return;
        }
        *last = Some(msg.to_string());
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = format!("[{}] {}\n", ts, msg);
    eprintln!("[settings] {}", msg);
    if let Some(dir) = settings_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let log = dir.join("settings-diag.log");
        if std::fs::metadata(&log).map(|m| m.len() > 32 * 1024).unwrap_or(false) {
            let _ = std::fs::write(&log, &line);
        } else {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
                let _ = f.write_all(line.as_bytes());
            }
        }
    }
}

pub(crate) fn save_settings(s: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    let path = match settings_file() {
        Some(p) => p,
        None => {
            diag("SAVE 失败: config_dir 不可用 (dirs::config_dir() 返回 None)");
            return Err("no config dir".into());
        }
    };
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(s)?)?;
    diag(&format!("SAVE ok theme={} skin={} path={}", s.theme, s.skin, path.display()));
    Ok(())
}

pub(crate) fn load_settings() -> Settings {
    let path = match settings_file() {
        Some(p) => p,
        None => {
            diag("LOAD: config_dir 不可用，回退默认(dark)");
            return Settings::default();
        }
    };
    match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            diag(&format!("LOAD: 文件不存在，回退默认(dark) path={}", path.display()));
            Settings::default()
        }
        Err(e) => {
            // 文件存在却读不出来（权限/占用等）：不静默丢弃，记录后回退默认
            diag(&format!("LOAD 失败: 读取错误 {} path={}", e, path.display()));
            Settings::default()
        }
        Ok(content) => match serde_json::from_str::<Settings>(&content) {
            Ok(s) => {
                diag(&format!("LOAD ok theme={} skin={} path={}", s.theme, s.skin, path.display()));
                s
            }
            Err(e) => {
                // 文件存在但解析失败：备份原文件后回退默认，避免下次保存直接覆盖丢证据
                let _ = std::fs::rename(&path, path.with_extension("json.bak"));
                diag(&format!("LOAD 失败: 解析错误 {}，已备份为 settings.json.bak path={}", e, path.display()));
                Settings::default()
            }
        },
    }
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

/// 主题/皮肤变更后推给快捷便签窗口。该窗口关闭时只 hide 不销毁，
/// 不会重新走 open_quick_note 的同步，所以这里单独发一个不带重置语义的事件。
fn notify_quick_note_theme(app: &AppHandle, theme: &str, skin: &str) {
    if let Some(w) = app.get_webview_window("quick-note") {
        let _ = w.emit("theme-changed", serde_json::json!({ "theme": theme, "skin": skin }));
    }
}

#[tauri::command]
pub(crate) fn set_theme(state: tauri::State<crate::AppState>, app: AppHandle, t: String) -> Result<(), String> {
    let (theme, skin) = {
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.theme = t;
        save_settings(&settings).map_err(|e| e.to_string())?;
        (settings.theme.clone(), settings.skin.clone())
    };
    notify_quick_note_theme(&app, &theme, &skin);
    Ok(())
}

#[tauri::command]
pub(crate) fn set_skin(state: tauri::State<crate::AppState>, app: AppHandle, s: String) -> Result<(), String> {
    let (theme, skin) = {
        let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
        settings.skin = s;
        save_settings(&settings).map_err(|e| e.to_string())?;
        (settings.theme.clone(), settings.skin.clone())
    };
    notify_quick_note_theme(&app, &theme, &skin);
    Ok(())
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
