use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use crate::images::validate_path_component;
use crate::settings::save_settings;
use crate::shortcut::parse_shortcut;

/// 获取当前鼠标光标位置（Windows API）
fn get_cursor_position() -> (i32, i32) {
    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        use windows::Win32::Foundation::POINT;
        let mut pt = POINT::default();
        if GetCursorPos(&mut pt).is_ok() {
            return (pt.x, pt.y);
        }
        (0, 0)
    }
    #[cfg(not(target_os = "windows"))]
    { (0, 0) }
}

/// 打开快捷便签窗口：定位到鼠标位置并显示
#[tauri::command]
pub(crate) fn open_quick_note(app: AppHandle) -> Result<(), String> {
    let win = app.get_webview_window("quick-note").ok_or("quick-note window not found")?;

    // 恢复保存的窗口尺寸
    {
        let state = app.state::<crate::AppState>();
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        if let (Some(w), Some(h)) = (s.note_width, s.note_height) {
            let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(w, h)));
        }
    }

    // 获取鼠标位置
    let (mx, my) = get_cursor_position();
    eprintln!("[QuickNote] 鼠标位置: ({}, {})", mx, my);

    // 获取窗口实际尺寸用于边界钳制
    let (win_w, win_h) = win.outer_size()
        .map(|s| (s.width as i32, s.height as i32))
        .unwrap_or((320, 300));

    // 获取显示器工作区进行钳制
    let (mut x, mut y) = (mx, my);
    let monitor = win.primary_monitor().ok().flatten()
        .or_else(|| win.current_monitor().ok().flatten());
    if let Some(mon) = monitor {
        let wa = mon.work_area();
        let wa_x = wa.position.x;
        let wa_y = wa.position.y;
        let wa_w = wa.size.width as i32;
        let wa_h = wa.size.height as i32;
        if x + win_w > wa_x + wa_w { x = wa_x + wa_w - win_w; }
        if y + win_h > wa_y + wa_h { y = wa_y + wa_h - win_h; }
        if x < wa_x { x = wa_x; }
        if y < wa_y { y = wa_y; }
    }

    eprintln!("[QuickNote] 窗口定位: ({}, {})", x, y);
    win.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)))
        .map_err(|e| format!("set_position failed: {}", e))?;
    win.show().map_err(|e| format!("show failed: {}", e))?;
    win.set_focus().map_err(|e| format!("set_focus failed: {}", e))?;

    // 通知前端清空/初始化，携带当前主题信息
    let (theme, skin) = {
        let state = app.state::<crate::AppState>();
        let s = state.settings.lock().map_err(|e| e.to_string())?;
        (s.theme.clone(), s.skin.clone())
    };
    let _ = win.emit("quick-note-opened", serde_json::json!({ "theme": theme, "skin": skin }));
    Ok(())
}

/// 关闭快捷便签窗口（仅隐藏）
#[tauri::command]
pub(crate) fn close_quick_note(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("quick-note") {
        // 保存当前窗口尺寸
        if let Ok(size) = w.outer_size() {
            let state = app.state::<crate::AppState>();
            if let Ok(mut s) = state.settings.lock() {
                s.note_width = Some(size.width);
                s.note_height = Some(size.height);
                let _ = save_settings(&s);
            };
        }
        let _ = w.hide();
    }
    Ok(())
}

/// 保存快捷便签内容为新备忘录
#[tauri::command]
pub(crate) fn save_quick_note(
    app: AppHandle,
    state: tauri::State<crate::AppState>,
    content: String,
    color: String,
    is_pinned: bool,
    is_done: bool,
    remind_at: String,
    images: Vec<String>,
) -> Result<String, String> {
    let trimmed = content.trim().to_string();
    // 空内容且无图片 → 不创建
    if trimmed.is_empty() && images.is_empty() {
        // 保存尺寸并隐藏窗口
        if let Some(w) = app.get_webview_window("quick-note") {
            if let Ok(size) = w.outer_size() {
                if let Ok(mut s) = state.settings.lock() {
                    s.note_width = Some(size.width);
                    s.note_height = Some(size.height);
                    let _ = save_settings(&s);
                }
            }
            let _ = w.hide();
        }
        return Ok(String::new());
    }

    let memo_content = if trimmed.is_empty() { "（图片）".to_string() } else { trimmed };

    // 插入备忘录
    let memo = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        store.insert(&memo_content).map_err(|e| e.to_string())?
    };

    // 设置颜色
    if !color.is_empty() {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let _ = store.set_color(&memo.id, &color);
    }
    // 设置置顶
    if is_pinned {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let _ = store.toggle_pin(&memo.id);
    }
    // 设置完成
    if is_done {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let _ = store.toggle_done(&memo.id);
    }
    // 设置提醒
    if !remind_at.is_empty() {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let _ = store.set_reminder(&memo.id, &remind_at);
    }
    // 保存图片（从临时目录移动到正式目录）
    if !images.is_empty() {
        // 移动文件
        let base = dirs::data_dir().ok_or("no data dir")?.join("sidebar-memo").join("images");
        let src_dir = base.join("_quick_note");
        let dst_dir = base.join(&memo.id);
        let _ = std::fs::create_dir_all(&dst_dir);
        for f in &images {
            let src = src_dir.join(f);
            let dst = dst_dir.join(f);
            if src.exists() {
                let _ = std::fs::rename(&src, &dst);
            }
        }
        let images_json = serde_json::to_string(&images).unwrap_or_else(|_| "[]".to_string());
        let store = state.store.lock().map_err(|e| e.to_string())?;
        let _ = store.update_images(&memo.id, &images_json);
    }

    // 保存尺寸并隐藏窗口
    if let Some(w) = app.get_webview_window("quick-note") {
        if let Ok(size) = w.outer_size() {
            if let Ok(mut s) = state.settings.lock() {
                s.note_width = Some(size.width);
                s.note_height = Some(size.height);
                let _ = save_settings(&s);
            }
        }
        let _ = w.hide();
    }

    // 通知主窗口刷新
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.emit("quick-note-saved", &memo.id);
    }

    Ok(memo.id)
}

/// 快捷便签中保存图片（先存到临时 memo 目录，关闭时再关联）
#[tauri::command]
pub(crate) fn save_quick_note_image(
    filename: String,
    data_base64: String,
) -> Result<String, String> {
    validate_path_component(&filename)?;
    use base64::Engine;
    let img_data = base64::engine::general_purpose::STANDARD
        .decode(&data_base64)
        .map_err(|e| format!("base64 decode failed: {}", e))?;

    // 存到临时目录 _quick_note_images/
    let img_dir = dirs::data_dir()
        .ok_or("no data dir")?
        .join("sidebar-memo")
        .join("images")
        .join("_quick_note");
    std::fs::create_dir_all(&img_dir).map_err(|e| format!("create dir failed: {}", e))?;

    let file_path = img_dir.join(&filename);
    std::fs::write(&file_path, &img_data).map_err(|e| format!("write file failed: {}", e))?;

    Ok(filename)
}

/// 将临时图片移动到正式备忘录目录
#[tauri::command]
pub(crate) fn move_quick_note_images(
    filenames: Vec<String>,
    memo_id: String,
) -> Result<(), String> {
    validate_path_component(&memo_id)?;
    for f in &filenames {
        validate_path_component(f)?;
    }
    let base = dirs::data_dir().ok_or("no data dir")?.join("sidebar-memo").join("images");
    let src_dir = base.join("_quick_note");
    let dst_dir = base.join(&memo_id);
    std::fs::create_dir_all(&dst_dir).map_err(|e| format!("create dst dir: {}", e))?;

    for f in &filenames {
        let src = src_dir.join(f);
        let dst = dst_dir.join(f);
        if src.exists() {
            let _ = std::fs::rename(&src, &dst);
        }
    }
    Ok(())
}

/// 设置快捷便签快捷键
#[tauri::command]
pub(crate) fn set_note_shortcut(state: tauri::State<crate::AppState>, s: String, app: AppHandle) -> Result<(), String> {
    let mut st = state.settings.lock().map_err(|e| e.to_string())?;
    let old = st.note_shortcut.clone();
    st.note_shortcut = s.clone();
    save_settings(&st).map_err(|e| e.to_string())?;
    let gs = app.global_shortcut();
    if let Ok(old_sc) = parse_shortcut(&old) {
        let _ = gs.unregister(old_sc);
    }
    register_note_shortcut_internal(&app, &s).map_err(|e| e.to_string())
}

pub(crate) fn register_note_shortcut_internal(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let shortcut = parse_shortcut(shortcut_str)?;
    let gs = app.global_shortcut();
    let handle = app.clone();
    gs.on_shortcut(shortcut, move |_, _, event| {
        if event.state == ShortcutState::Pressed {
            eprintln!("[QuickNote] 快捷键触发");
            if let Err(e) = open_quick_note(handle.clone()) {
                eprintln!("[QuickNote] 打开失败: {}", e);
            }
        }
    }).map_err(|e| format!("register note shortcut failed: {}", e))?;
    eprintln!("[QuickNote] 快捷键注册成功: {}", shortcut_str);
    Ok(())
}
