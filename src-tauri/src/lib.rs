mod db;

use db::{Memo, MemoStore};
use serde::{Deserialize, Serialize};
use std::{sync::Mutex, thread, time::Duration};
use tauri::{AppHandle, Manager, Emitter, menu::{Menu, MenuItem}, tray::TrayIconBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

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
}
fn default_true() -> bool { true }
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
        }
    }
}

pub struct AppState {
    pub store: Mutex<MemoStore>,
    pub settings: Mutex<Settings>,
    pub window_visible: Mutex<bool>,
}

// Commands
#[tauri::command] fn get_memos(state: tauri::State<AppState>) -> Result<Vec<Memo>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    let _ = store.auto_trash();
    store.get_all().map_err(|e| e.to_string())
}
#[tauri::command] fn get_trashed_memos(state: tauri::State<AppState>) -> Result<Vec<Memo>, String> { state.store.lock().map_err(|e| e.to_string())?.get_trashed().map_err(|e| e.to_string()) }
#[tauri::command] fn move_to_trash(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.move_to_trash(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn restore_from_trash(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.restore_from_trash(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn permanent_delete(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.permanent_delete(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn add_memo(state: tauri::State<AppState>, content: String) -> Result<Memo, String> { state.store.lock().map_err(|e| e.to_string())?.insert(&content).map_err(|e| e.to_string()) }
#[tauri::command] fn update_memo(state: tauri::State<AppState>, id: String, content: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.update_content(&id, &content).map_err(|e| e.to_string()) }
#[tauri::command] fn delete_memo(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.delete(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn toggle_pin(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.toggle_pin(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn set_color(state: tauri::State<AppState>, id: String, color: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.set_color(&id, &color).map_err(|e| e.to_string()) }
#[tauri::command] fn toggle_done(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.toggle_done(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn reorder_memos(state: tauri::State<AppState>, ids: Vec<String>) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.reorder(&ids).map_err(|e| e.to_string()) }
#[tauri::command] fn get_settings(state: tauri::State<AppState>) -> Result<Settings, String> { state.settings.lock().map_err(|e| e.to_string())?.clone().pipe(Ok) }
#[tauri::command]
fn set_reminder(state: tauri::State<AppState>, id: String, remind_at: String) -> Result<(), String> {
    state.store.lock().map_err(|e| e.to_string())?.set_reminder(&id, &remind_at).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_reminder(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    state.store.lock().map_err(|e| e.to_string())?.clear_reminder(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_image(state: tauri::State<AppState>, memo_id: String, filename: String, data_base64: String) -> Result<String, String> {
    use base64::Engine;
    let img_data = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("base64 瑙ｇ爜澶辫触: {}", e))?;

    let img_dir = dirs::data_dir()
        .ok_or("鏃犳硶鑾峰彇鏁版嵁鐩綍")?
        .join("sidebar-memo")
        .join("images")
        .join(&memo_id);
    std::fs::create_dir_all(&img_dir).map_err(|e| format!("鍒涘缓鐩綍澶辫触: {}", e))?;

    let file_path = img_dir.join(&filename);
    std::fs::write(&file_path, &img_data).map_err(|e| format!("鍐欏叆鏂囦欢澶辫触: {}", e))?;

    // 鏇存柊鏁版嵁搴?images 瀛楁
    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let memos = store.get_all().map_err(|e| e.to_string())?;
    let memo = memos.iter().find(|m| m.id == memo_id)
        .ok_or("澶囧繕褰曚笉瀛樺湪")?;
    let mut images: Vec<String> = serde_json::from_str(&memo.images).unwrap_or_default();
    if !images.contains(&filename) {
        images.push(filename.clone());
    }
    let images_json = serde_json::to_string(&images).unwrap_or_else(|_| "[]".to_string());
    store.update_images(&memo_id, &images_json).map_err(|e| e.to_string())?;

    Ok(images_json)
}

#[tauri::command]
fn delete_image(state: tauri::State<AppState>, memo_id: String, filename: String) -> Result<String, String> {
    let img_dir = dirs::data_dir()
        .ok_or("鏃犳硶鑾峰彇鏁版嵁鐩綍")?
        .join("sidebar-memo")
        .join("images")
        .join(&memo_id);
    let file_path = img_dir.join(&filename);
    if file_path.exists() {
        let _ = std::fs::remove_file(&file_path);
    }

    let mut store = state.store.lock().map_err(|e| e.to_string())?;
    let memos = store.get_all().map_err(|e| e.to_string())?;
    let memo = memos.iter().find(|m| m.id == memo_id)
        .ok_or("澶囧繕褰曚笉瀛樺湪")?;
    let mut images: Vec<String> = serde_json::from_str(&memo.images).unwrap_or_default();
    images.retain(|f| f != &filename);
    let images_json = serde_json::to_string(&images).unwrap_or_else(|_| "[]".to_string());
    store.update_images(&memo_id, &images_json).map_err(|e| e.to_string())?;

    Ok(images_json)
}

#[tauri::command]
fn get_image_base64(memo_id: String, filename: String) -> Result<String, String> {
    let file_path = dirs::data_dir()
        .ok_or("鏃犳硶鑾峰彇鏁版嵁鐩綍")?
        .join("sidebar-memo")
        .join("images")
        .join(&memo_id)
        .join(&filename);

    if !file_path.exists() {
        return Err("图片文件不存在".to_string());
    }

    let data = std::fs::read(&file_path).map_err(|e| format!("璇诲彇鏂囦欢澶辫触: {}", e))?;
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(&data))
}

#[tauri::command]
fn open_image_viewer(image_data: String) -> Result<(), String> {
    // 从 data URL 提取 MIME 和 base64 数据
    let (mime, b64) = if let Some(rest) = image_data.strip_prefix("data:") {
        if let Some(pos) = rest.find(",") {
            let meta = &rest[..pos];
            let data = &rest[pos + 1..];
            let mime_str = meta.split(';').next().unwrap_or("image/png");
            (mime_str.to_string(), data.to_string())
        } else {
            return Err("无效的 data URL".into());
        }
    } else {
        return Err("不是 data URL".into());
    };

    // 根据 MIME 确定文件扩展名
    let ext = match mime.as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "png",
    };

    // 解码 base64
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&b64)
        .map_err(|e| format!("base64 解码失败: {}", e))?;

    // 写入临时文件
    let temp_dir = std::env::temp_dir();
    let file_name = format!("sidebar_memo_img.{}", ext);
    let file_path = temp_dir.join(&file_name);
    std::fs::write(&file_path, &bytes).map_err(|e| format!("写入临时文件失败: {}", e))?;

    // 调用 Windows 默认图片查看器
    std::process::Command::new("cmd")
        .args(["/c", "start", "", &file_path.to_string_lossy()])
        .spawn()
        .map_err(|e| format!("启动查看器失败: {}", e))?;

    Ok(())
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle, state: tauri::State<AppState>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        *state.window_visible.lock().unwrap() = true;
    }
}

#[tauri::command]
fn set_shortcut(state: tauri::State<AppState>, s: String, app: tauri::AppHandle) -> Result<(), String> {
    let mut st = state.settings.lock().map_err(|e| e.to_string())?;
    let old_shortcut = st.shortcut.clone();
    st.shortcut = s.clone();
    save_settings(&st).map_err(|e| e.to_string())?;
    let gs = app.global_shortcut();
    if let Ok(old) = parse_shortcut(&old_shortcut) {
        let _ = gs.unregister(old);
    }
    register_shortcut_internal(&app, &s).map_err(|e| e.to_string())
}

fn parse_shortcut(shortcut_str: &str) -> Result<Shortcut, String> {
    let parts: Vec<&str> = shortcut_str.split('+').map(|s| s.trim()).collect();
    let mut mods = Modifiers::empty();
    let mut key_code: Option<Code> = None;
    for part in &parts {
        match part.to_lowercase().as_str() {
            "alt" => mods |= Modifiers::ALT,
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            "super" | "win" | "meta" => mods |= Modifiers::SUPER,
            k => {
                key_code = Some(match k.to_uppercase().as_str() {
                    "SPACE" => Code::Space, "TAB" => Code::Tab,
                    "ENTER" | "RETURN" => Code::Enter, "ESCAPE" | "ESC" => Code::Escape,
                    "BACKSPACE" => Code::Backspace, "DELETE" | "DEL" => Code::Delete,
                    "INSERT" => Code::Insert, "HOME" => Code::Home, "END" => Code::End,
                    "PAGEUP" => Code::PageUp, "PAGEDOWN" => Code::PageDown,
                    "ARROWUP" | "UP" => Code::ArrowUp, "ARROWDOWN" | "DOWN" => Code::ArrowDown,
                    "ARROWLEFT" | "LEFT" => Code::ArrowLeft, "ARROWRIGHT" | "RIGHT" => Code::ArrowRight,
                    "F1" => Code::F1, "F2" => Code::F2, "F3" => Code::F3, "F4" => Code::F4,
                    "F5" => Code::F5, "F6" => Code::F6, "F7" => Code::F7, "F8" => Code::F8,
                    "F9" => Code::F9, "F10" => Code::F10, "F11" => Code::F11, "F12" => Code::F12,
                    "A" => Code::KeyA, "B" => Code::KeyB, "C" => Code::KeyC, "D" => Code::KeyD,
                    "E" => Code::KeyE, "F" => Code::KeyF, "G" => Code::KeyG, "H" => Code::KeyH,
                    "I" => Code::KeyI, "J" => Code::KeyJ, "K" => Code::KeyK, "L" => Code::KeyL,
                    "M" => Code::KeyM, "N" => Code::KeyN, "O" => Code::KeyO, "P" => Code::KeyP,
                    "Q" => Code::KeyQ, "R" => Code::KeyR, "S" => Code::KeyS, "T" => Code::KeyT,
                    "U" => Code::KeyU, "V" => Code::KeyV, "W" => Code::KeyW, "X" => Code::KeyX,
                    "Y" => Code::KeyY, "Z" => Code::KeyZ,
                    "0" => Code::Digit0, "1" => Code::Digit1, "2" => Code::Digit2, "3" => Code::Digit3,
                    "4" => Code::Digit4, "5" => Code::Digit5, "6" => Code::Digit6, "7" => Code::Digit7,
                    "8" => Code::Digit8, "9" => Code::Digit9,
                    _ => Code::Space,
                });
            }
        }
    }
    let code = key_code.ok_or("No valid key code found")?;
    Ok(Shortcut::new(Some(mods), code))
}

fn register_shortcut_internal(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut(shortcut_str)?;
    let gs = app.global_shortcut();
    let handle = app.clone();
    let _ = gs.on_shortcut(shortcut, move |_, _, event| {
        if event.state == ShortcutState::Pressed {
            toggle_window(&handle);
        }
    });
    Ok(())
}

#[tauri::command]
fn set_theme(state: tauri::State<AppState>, t: String) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.theme = t;
    save_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_skin(state: tauri::State<AppState>, s: String) -> Result<(), String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.skin = s;
    save_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_trashed(state: tauri::State<AppState>) -> Result<u32, String> {
    state.store.lock().map_err(|e| e.to_string())?.clear_trashed().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_window_visible(state: tauri::State<AppState>, visible: bool) {
    *state.window_visible.lock().unwrap() = visible;
}

#[tauri::command]
fn handle_system_wakeup(app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    let settings = state.settings.lock().map_err(|e| e.to_string())?;
    let gs = app.global_shortcut();
    if let Ok(sh) = parse_shortcut(&settings.shortcut) {
        let _ = gs.unregister(sh);
    }
    drop(settings);
    let settings2 = state.settings.lock().map_err(|e| e.to_string())?;
    register_shortcut_internal(&app, &settings2.shortcut).map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("main") {
        *state.window_visible.lock().unwrap() = w.is_visible().unwrap_or(false);
    }
    Ok(())
}

#[tauri::command]
fn save_current_position(app: tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        save_window_position(&app, &w);
    }
}

#[tauri::command]
fn toggle_always_on_top(app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<bool, String> {
    let mut settings = state.settings.lock().map_err(|e| e.to_string())?;
    settings.always_on_top = !settings.always_on_top;
    let new_val = settings.always_on_top;
    save_settings(&settings).map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_always_on_top(new_val);
    }
    Ok(new_val)
}

#[tauri::command]
fn frontend_ready(app: tauri::AppHandle, state: tauri::State<AppState>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_skip_taskbar(true);
        *state.window_visible.lock().unwrap() = true;
    }
}

#[tauri::command]
fn resize_window(app: tauri::AppHandle, width: f64, height: f64) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("main") {
        if let Ok(current) = w.inner_size() {
            let new_w = if width > 0.0 { width.max(200.0) as u32 } else { current.width };
            let new_h = if height > 0.0 { height.max(150.0) as u32 } else { current.height };
            let _ = w.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(new_w, new_h)));
        }
    }
    Ok(())
}

#[tauri::command]
fn close_to_tray(app: tauri::AppHandle, state: tauri::State<AppState>) {
    if let Some(w) = app.get_webview_window("main") {
        save_window_position(&app, &w);
        let _ = w.hide();
    }
    *state.window_visible.lock().unwrap() = false;
}

trait Pipe { fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R, Self: Sized; }
impl<T> Pipe for T { fn pipe<F, R>(self, f: F) -> R where F: FnOnce(Self) -> R { f(self) } }

fn save_settings(s: &Settings) -> Result<(), Box<dyn std::error::Error>> {
    let d = dirs::config_dir().ok_or("no config dir")?.join("sidebar-memo");
    std::fs::create_dir_all(&d)?;
    std::fs::write(d.join("settings.json"), serde_json::to_string_pretty(s)?)?;
    Ok(())
}
fn load_settings() -> Settings {
    dirs::config_dir().unwrap_or_default().join("sidebar-memo").join("settings.json")
        .pipe(|p| std::fs::read_to_string(p).ok())
        .and_then(|d| serde_json::from_str(&d).ok()).unwrap_or_default()
}

fn save_window_position(app: &AppHandle, window: &tauri::WebviewWindow) {
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
            let state = app.state::<AppState>();
            let mut settings = state.settings.lock().unwrap();
            settings.window_x = Some(pos.x);
            settings.window_y = Some(pos.y);
            settings.window_width = Some(size.width);
            settings.window_height = Some(size.height);
            let _ = save_settings(&settings);
        }
    }
}

fn start_reminder_worker(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(30));
        let state = app.state::<AppState>();
        let due = match state.store.lock() {
            Ok(store) => store.take_due_reminders().unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        if let Some(w) = app.get_webview_window("main") {
            for memo in due {
                let _ = w.emit("memo-reminder-due", &memo);
            }
        }
    });
}

fn toggle_window(app: &AppHandle) {
    let state = app.state::<AppState>();
    let mut visible = state.window_visible.lock().unwrap();
    if let Some(w) = app.get_webview_window("main") {
        let actually_visible = *visible && w.is_visible().unwrap_or(true);
        if actually_visible {
            save_window_position(app, &w);
            let _ = w.hide();
            *visible = false;
        } else {
            let _ = w.show();
            let _ = w.set_focus();
            let _ = w.emit("toggle-window", ());
            *visible = true;
        }
    } else {
        *visible = false;
    }
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "显示/隐藏", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    TrayIconBuilder::new().icon(app.default_window_icon().unwrap().clone()).menu(&menu).tooltip("Sidebar Memo")
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, e| match e.id.as_ref() { "show" => toggle_window(app), "quit" => app.exit(0), _ => {} })
        .on_tray_icon_event(|tray, e| {
            match e {
                tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } => {
                    toggle_window(tray.app_handle());
                }
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 尝试从 Windows 注册表读取系统代理配置
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("reg")
            .args(["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings", "/v", "ProxyEnable"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("0x1") {
                if let Ok(output) = std::process::Command::new("reg")
                    .args(["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings", "/v", "ProxyServer"])
                    .output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        if line.contains("ProxyServer") {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if let Some(proxy) = parts.last() {
                                let proxy_url = if proxy.starts_with("http") { proxy.to_string() } else { format!("http://{}", proxy) };
                                std::env::set_var("HTTPS_PROXY", &proxy_url);
                                std::env::set_var("HTTP_PROXY", &proxy_url);
                            }
                        }
                    }
                }
            }
        }
    }

    let settings = load_settings();
    let shortcut_str = settings.shortcut.clone();
    let saved_settings = settings.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(move |app| {
            let store = MemoStore::new().expect("Failed to init database");
            app.manage(AppState { store: Mutex::new(store), settings: Mutex::new(settings.clone()), window_visible: Mutex::new(false) });
            setup_tray(app.handle())?;
            let _ = register_shortcut_internal(app.handle(), &shortcut_str);
            start_reminder_worker(app.handle().clone());
            if let Some(w) = app.get_webview_window("main") {
                // 消除 DWM 在无边框窗口周围渲染的 1px 黑线
                #[cfg(target_os = "windows")]
                {
                    use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
                    use windows::Win32::UI::Controls::MARGINS;
                    use windows::Win32::Foundation::HWND;
                    if let Ok(hwnd) = w.hwnd() {
                        unsafe {
                            let margins = MARGINS {
                                cxLeftWidth: -1,
                                cxRightWidth: -1,
                                cyTopHeight: -1,
                                cyBottomHeight: -1,
                            };
                            let _ = DwmExtendFrameIntoClientArea(HWND(hwnd.0 as _), &margins);
                        }
                    }
                }
                let mut reset_pos = true;
                if let (Some(x), Some(y)) = (saved_settings.window_x, saved_settings.window_y) {
                    if let Some(monitor) = w.primary_monitor().ok().flatten() {
                        let wa = monitor.work_area();
                        if x >= wa.position.x && y >= wa.position.y
                            && x < wa.position.x + wa.size.width as i32 - 100
                            && y < wa.position.y + wa.size.height as i32 - 100 {
                            let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
                            reset_pos = false;
                        }
                    }
                }
                if let (Some(width), Some(height)) = (saved_settings.window_width, saved_settings.window_height) {
                    if width > 100 && height > 100 {
                        let _ = w.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(width, height)));
                    }
                }
                let _ = w.set_always_on_top(saved_settings.always_on_top);
                if reset_pos {
                    if let Some(monitor) = w.primary_monitor().ok().flatten() {
                        let wa = monitor.work_area();
                        let x = wa.position.x + (wa.size.width as i32 - 400) / 2;
                        let y = wa.position.y + (wa.size.height as i32 - 600) / 2;
                        let _ = w.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![close_to_tray, resize_window, toggle_always_on_top, frontend_ready, get_memos, get_trashed_memos, add_memo, update_memo, delete_memo, toggle_pin, set_color, toggle_done, reorder_memos, get_settings, set_window_visible, move_to_trash, restore_from_trash, permanent_delete, save_current_position, set_shortcut, set_theme, set_skin, clear_trashed, set_reminder, clear_reminder, show_main_window, handle_system_wakeup, save_image, delete_image, get_image_base64, open_image_viewer])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
