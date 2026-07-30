mod db;
mod ocr;
mod settings;
mod images;
mod util;
mod shortcut;
mod quick_note;

use db::{Memo, MemoStore};
use settings::Settings;
use images::validate_path_component;
use shortcut::{parse_shortcut, register_shortcut_internal};
use util::strip_markdown_for_notify;
use settings::{save_settings, load_settings, save_window_position};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager, Emitter, menu::{Menu, MenuItem}, tray::TrayIconBuilder};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_global_shortcut::GlobalShortcutExt;


#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ViewerPayload {
    pub memo_id: String,
    pub filenames: Vec<String>,
    pub index: usize,
}

pub struct AppState {
    pub store: Mutex<MemoStore>,
    pub settings: Mutex<Settings>,
    pub window_visible: Mutex<bool>,
    pub saved_hide_position: Mutex<Option<(i32, i32)>>,
    pub hover_stop_flag: Mutex<Option<Arc<AtomicBool>>>,
    pub viewer_payload: Mutex<Option<ViewerPayload>>,
}

// Commands
#[tauri::command] fn get_memos(state: tauri::State<AppState>) -> Result<Vec<Memo>, String> {
    let store = state.store.lock().map_err(|e| e.to_string())?;
    if let Err(e) = store.auto_trash() {
        eprintln!("[auto_trash] 执行失败: {}", e);
    }
    store.get_all().map_err(|e| e.to_string())
}
#[tauri::command] fn get_trashed_memos(state: tauri::State<AppState>) -> Result<Vec<Memo>, String> { state.store.lock().map_err(|e| e.to_string())?.get_trashed().map_err(|e| e.to_string()) }
#[tauri::command] fn move_to_trash(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.move_to_trash(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn restore_from_trash(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.restore_from_trash(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn permanent_delete(state: tauri::State<AppState>, id: String) -> Result<(), String> { validate_path_component(&id)?; state.store.lock().map_err(|e| e.to_string())?.permanent_delete(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn add_memo(state: tauri::State<AppState>, content: String) -> Result<Memo, String> { state.store.lock().map_err(|e| e.to_string())?.insert(&content).map_err(|e| e.to_string()) }
#[tauri::command] fn update_memo(state: tauri::State<AppState>, id: String, content: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.update_content(&id, &content).map_err(|e| e.to_string()) }
#[tauri::command] fn delete_memo(state: tauri::State<AppState>, id: String) -> Result<(), String> { validate_path_component(&id)?; state.store.lock().map_err(|e| e.to_string())?.delete(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn toggle_pin(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.toggle_pin(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn set_color(state: tauri::State<AppState>, id: String, color: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.set_color(&id, &color).map_err(|e| e.to_string()) }
#[tauri::command] fn toggle_done(state: tauri::State<AppState>, id: String) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.toggle_done(&id).map_err(|e| e.to_string()) }
#[tauri::command] fn reorder_memos(state: tauri::State<AppState>, ids: Vec<String>) -> Result<(), String> { state.store.lock().map_err(|e| e.to_string())?.reorder(&ids).map_err(|e| e.to_string()) }
#[tauri::command]
fn set_reminder(state: tauri::State<AppState>, id: String, remind_at: String) -> Result<(), String> {
    state.store.lock().map_err(|e| e.to_string())?.set_reminder(&id, &remind_at).map_err(|e| e.to_string())
}

#[tauri::command]
fn clear_reminder(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    state.store.lock().map_err(|e| e.to_string())?.clear_reminder(&id).map_err(|e| e.to_string())
}


/// OCR 识别图片中的文字，返回每个词的文本和边界框（原图坐标）
#[tauri::command]
async fn ocr_image(app: AppHandle, memo_id: String, filename: String) -> Result<ocr::OcrOutput, String> {
    validate_path_component(&memo_id)?;
    validate_path_component(&filename)?;
    // 解析模型目录：优先从打包资源目录，回退到项目目录（开发模式）
    let models_dir = app
        .path()
        .resolve("models", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.join("PP-OCRv6_small_det.mnn").exists())
        .or_else(|| {
            // 开发模式：从当前工作目录查找
            let cwd = std::path::PathBuf::from("models");
            if cwd.join("PP-OCRv6_small_det.mnn").exists() {
                return Some(cwd);
            }
            // 回退：{data_dir}/sidebar-memo/models/
            dirs::data_dir().map(|d| d.join("sidebar-memo").join("models"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("models"));

    eprintln!("[OCR] 模型目录: {}", models_dir.display());
    eprintln!("[OCR] 识别目标: {}/{}", memo_id, filename);

    let result = tokio::task::spawn_blocking(move || ocr::recognize(&models_dir, &memo_id, &filename))
        .await
        .map_err(|e| format!("OCR task panicked: {}", e))?;

    match &result {
        Ok(output) => eprintln!("[OCR] PaddleOCR 识别成功: {} 个文字区域, 图片尺寸 {}x{}", output.words.len(), output.width, output.height),
        Err(e) => eprintln!("[OCR] PaddleOCR 识别失败: {}", e),
    }
    result
}

fn notify_viewer_closed(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.emit("image-viewer-closed", ());
    } else {
        let _ = app.emit("image-viewer-closed", ());
    }
    if let Some(state) = app.try_state::<AppState>() {
        let mut slot = state.viewer_payload.lock().unwrap_or_else(|e| e.into_inner());
        *slot = None;
    }
}

/// 打开图片预览。
/// JS: invoke("open_image_viewer", { payload: { memoId: string, filenames: string[], index: number } })
#[tauri::command]
fn open_image_viewer(
    app: AppHandle,
    state: tauri::State<AppState>,
    payload: ViewerPayload,
) -> Result<(), String> {
    if payload.filenames.is_empty() {
        return Err("no images".into());
    }
    let idx = payload.index.min(payload.filenames.len().saturating_sub(1));
    let payload = ViewerPayload {
        memo_id: payload.memo_id,
        filenames: payload.filenames,
        index: idx,
    };

    // 存储轻量 payload（仅文件名引用，不含图片数据）
    {
        let mut slot = state.viewer_payload.lock().map_err(|e| e.to_string())?;
        *slot = Some(payload.clone());
    }

    // viewer 窗口已在 tauri.conf.json 中预定义（启动时创建，初始隐藏）
    let viewer = app
        .get_webview_window("image-viewer")
        .ok_or("image-viewer window not found")?;
    let _ = viewer.emit("viewer-payload", &payload);
    let _ = viewer.show();
    let _ = viewer.unminimize();
    let _ = viewer.set_focus();
    Ok(())
}

#[tauri::command]
fn close_image_viewer(app: AppHandle) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("image-viewer") {
        let _ = w.hide();
    }
    notify_viewer_closed(&app);
    Ok(())
}

#[tauri::command]
fn get_viewer_payload(state: tauri::State<AppState>) -> Result<Option<ViewerPayload>, String> {
    state
        .viewer_payload
        .lock()
        .map_err(|e| e.to_string())
        .map(|g| g.clone())
}


#[tauri::command]
fn show_main_window(app: tauri::AppHandle, state: tauri::State<AppState>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        let mut v = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
        *v = true;
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


#[tauri::command]
fn clear_trashed(state: tauri::State<AppState>) -> Result<u32, String> {
    state.store.lock().map_err(|e| e.to_string())?.clear_trashed().map_err(|e| e.to_string())
}

#[tauri::command]
fn set_window_visible(state: tauri::State<AppState>, visible: bool) {
    let mut v = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
    *v = visible;
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
        let mut v = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
        *v = w.is_visible().unwrap_or(false);
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
fn frontend_ready(app: tauri::AppHandle, state: tauri::State<AppState>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_skip_taskbar(true);
        let mut v = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
        *v = true;
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
fn set_position_and_size(app: tauri::AppHandle, x: i32, y: i32, w: u32, h: u32) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)));
        let _ = win.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(w, h)));
    }
    Ok(())
}

#[tauri::command]
fn close_to_tray(app: tauri::AppHandle, state: tauri::State<AppState>) {
    if let Some(w) = app.get_webview_window("main") {
        save_window_position(&app, &w);
        let _ = w.hide();
    }
    let mut v = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
    *v = false;
}

// Frontend calls this before hiding to store the correct position in memory
#[tauri::command]
fn save_hide_position(app: tauri::AppHandle, x: i32, y: i32) {
    let state = app.state::<AppState>();
    let mut pos = state.saved_hide_position.lock().unwrap_or_else(|e| e.into_inner());
    *pos = Some((x, y));
}

// ── Rust 端窗口动画 ──────────────────────────────────────

/// 在 Rust 端执行窗口位置动画，不受浏览器 rAF 暂停影响。
/// 前端只需调用一次 invoke，动画在 Rust 异步任务中完成。
/// 获取显示器刷新率，用于计算动画帧间隔
#[cfg(target_os = "windows")]
fn get_monitor_refresh_rate_for_window(window: &tauri::WebviewWindow) -> u32 {
    use windows::Win32::Graphics::Gdi::{GetDC, GetDeviceCaps, ReleaseDC, VREFRESH};
    use windows::Win32::Foundation::HWND;
    unsafe {
        // 优先使用窗口所在显示器的 HWND 获取刷新率
        let hdc = if let Ok(hwnd) = window.hwnd() {
            GetDC(HWND(hwnd.0 as _))
        } else {
            GetDC(HWND(std::ptr::null_mut()))
        };
        if !hdc.0.is_null() {
            let rate = GetDeviceCaps(hdc, VREFRESH);
            ReleaseDC(HWND(std::ptr::null_mut()), hdc);
            if rate > 0 { return rate as u32; }
        }
        60 // fallback
    }
}

// 动画代次计数：每次新动画启动或显式取消时自增。运行中的动画线程每帧比对代次，
// 发现被超越立即退出且不强制落到本次目标位——否则旧动画（如吸附对齐）结束时的强制落位
// 会把已被新动画（如失焦隐藏）移走的窗口拉回可见位置，造成"前端认为已隐藏、窗口却留在屏幕上"的卡死
static ANIM_GENERATION: AtomicU64 = AtomicU64::new(0);

/// 立即取消当前正在运行的窗口动画（若有）。前端在需要马上接管窗口位置时调用，
/// 例如弹出动画中收到失焦/隐藏请求时提前终止弹出，让隐藏尽快开始
#[tauri::command]
fn cancel_window_animation() {
    ANIM_GENERATION.fetch_add(1, Ordering::SeqCst);
}

#[tauri::command]
async fn animate_window_position(
    app: tauri::AppHandle,
    target_x: i32,
    target_y: i32,
    duration_ms: u64,
    expand: Option<bool>,
) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("no main window")?;
    // 领取新代次，同时使仍在运行的旧动画失效
    let my_gen = ANIM_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let start_pos = window.outer_position().map_err(|e| e.to_string())?;
    let dx = target_x - start_pos.x;
    let dy = target_y - start_pos.y;

    // 如果目标位置和当前位置相同，直接返回
    if dx == 0 && dy == 0 {
        return Ok(());
    }

    // 根据显示器刷新率计算帧间隔（须在把 window 移入闭包前取得）
    #[cfg(target_os = "windows")]
    let refresh_rate = get_monitor_refresh_rate_for_window(&window);
    #[cfg(not(target_os = "windows"))]
    let refresh_rate = 60u32;
    let is_expand = expand.unwrap_or(false);

    // 逐帧动画放到独立阻塞线程执行。
    // 原实现跑在 Tauri 的 tokio 异步 worker 上，每帧 sleep().await 会与弹出/隐藏期间的其它
    // IPC 命令（set_window_visible / 悬停检测 / 焦点查询等）抢占同一批 worker，负载高时唤醒被
    // 推迟，表现为「偶发掉帧、忽快忽慢」。改为 spawn_blocking + std sleep 自主控帧，与异步调度解耦。
    tokio::task::spawn_blocking(move || {
        // 提升 Windows 定时器分辨率到 1ms
        #[cfg(target_os = "windows")]
        unsafe {
            use windows::Win32::Media::timeBeginPeriod;
            timeBeginPeriod(1);
        }
        let frame_interval = std::time::Duration::from_secs_f64(1.0 / refresh_rate as f64);
        let start_time = std::time::Instant::now();
        let duration = std::time::Duration::from_millis(duration_ms);
        let mut last_x = start_pos.x;
        let mut last_y = start_pos.y;

        // 末段最小步长推进：easing 尾部每帧位移不足 min_step 时强制按 min_step 前进，
        // 避免窗口以 1px/帧龟速爬行时 WebView 内容与窗框异步刷新造成的左右抖动
        fn advance_toward(last: i32, desired: i32, target: i32, min_step: i32) -> i32 {
            if last == target {
                return target;
            }
            let dir = (target - last).signum();
            let mut next = desired;
            if (next - last) * dir < min_step {
                next = last + dir * min_step;
            }
            // 不越过目标
            if (target - next) * dir <= 0 { target } else { next }
        }

        loop {
            // 被新动画或显式取消超越：立即退出，且不落位到本次目标（位置已归新动画管辖）
            if ANIM_GENERATION.load(Ordering::SeqCst) != my_gen {
                break;
            }
            let elapsed = start_time.elapsed();
            if elapsed >= duration {
                // 确保最终位置精确
                let _ = window.set_position(tauri::PhysicalPosition::new(target_x, target_y));
                break;
            }
            let t = elapsed.as_secs_f64() / duration.as_secs_f64();
            // 弹出与隐藏使用不同曲线：expand=Some(true) 为弹出（用户单独调速），否则为隐藏及吸附对齐（保持原参数）。
            // 两者均为三段二次缓动，各段交界处位置与速度连续（无突变不顿挫）；尾端由最小步长机制平稳落位不抖动
            let ease = if is_expand {
                // 弹出：前 80ms 走 70%，中 100ms 走 70%~90%，后 100ms 走最后 10%（总 280ms），速度 4.2→0.70→0.42→0.14
                if t < 80.0 / 280.0 {
                    4.2 * t - 6.125 * t * t
                } else if t < 180.0 / 280.0 {
                    let u = t - 80.0 / 280.0;
                    0.70 + 0.70 * u - 0.392 * u * u
                } else {
                    let w = t - 180.0 / 280.0;
                    0.90 + 0.42 * w - 0.392 * w * w
                }
            } else {
                // 隐藏：前 80ms 走 70%，中 100ms 走 70%~90%，后 140ms 走最后 10%（总 320ms），速度 4.6→1.0→0.28→0.18
                if t < 80.0 / 320.0 {
                    4.6 * t - 7.2 * t * t
                } else if t < 180.0 / 320.0 {
                    let u = t - 80.0 / 320.0;
                    0.70 + 1.0 * u - 1.152 * u * u
                } else {
                    let w = t - 180.0 / 320.0;
                    0.90 + 0.28 * w - 0.117551 * w * w
                }
            };
            let desired_x = start_pos.x + (dx as f64 * ease).round() as i32;
            let desired_y = start_pos.y + (dy as f64 * ease).round() as i32;
            let x = advance_toward(last_x, desired_x, target_x, 2);
            let y = advance_toward(last_y, desired_y, target_y, 2);
            let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
            last_x = x;
            last_y = y;
            // 提前到达目标就直接收尾，不再空转到时长结束
            if x == target_x && y == target_y {
                break;
            }

            // 关键：用「当前真实时间」而非循环顶部的旧 elapsed 计算下一帧边界。
            // 若沿用旧 elapsed，当某帧 set_position 抖动、耗时超过一帧时，算出的 next_frame 会
            // 落在 now 之前 → 跳过等待、连渲两帧再突然空一拍，正是「偶发掉帧」的直接来源。
            // floor+1 保证边界严格位于未来，节奏始终对齐刷新率（个别超时帧只会规律地跳过一帧）。
            let now = std::time::Instant::now();
            let cur = now.duration_since(start_time).as_secs_f64();
            let next_idx = (cur / frame_interval.as_secs_f64()).floor() as u64 + 1;
            let next_frame = start_time + frame_interval * (next_idx.min(u32::MAX as u64) as u32);
            if next_frame > now {
                let remaining = next_frame - now;
                if remaining > std::time::Duration::from_millis(2) {
                    // 大于 2ms 的部分用 sleep 让出 CPU
                    std::thread::sleep(remaining - std::time::Duration::from_millis(1));
                }
                // 剩余部分 spin 到精确边界（sleep 可能过冲，spin 补偿）
                while std::time::Instant::now() < next_frame {
                    std::thread::yield_now();
                }
            }
        }

        // 恢复默认定时器分辨率
        #[cfg(target_os = "windows")]
        unsafe {
            use windows::Win32::Media::timeEndPeriod;
            timeEndPeriod(1);
        }
    })
    .await
    .map_err(|e| format!("animation task panicked: {}", e))?;

    Ok(())
}

/// 启动鼠标边缘悬停检测，在后台线程运行。
/// 检测到鼠标在屏幕边缘时，向前端发送 "hover-edge-detected" 事件。
/// 使用 AtomicBool 停止标志防止线程泄漏，检测到后自动退出。
#[tauri::command]
fn start_hover_detection(
    app: tauri::AppHandle,
    edge: String,
    win_x: i32,
    win_y: i32,
    win_w: u32,
    win_h: u32,
    wa_x: i32,
    wa_y: i32,
    wa_w: u32,
    wa_h: u32,
    hidden_px: i32,
    hover_range: i32,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    // 停止上一次检测（如果还在运行）
    {
        let mut flag = state.hover_stop_flag.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = flag.as_ref() {
            old.store(true, Ordering::SeqCst);
        }
        let stop_flag = Arc::new(AtomicBool::new(false));
        *flag = Some(stop_flag.clone());

        let handle = app.clone();
        std::thread::spawn(move || {
            let edge_str = edge.as_str();
            let mut last_triggered = false;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));

                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }

                let st = handle.state::<AppState>();
                let visible = *st.window_visible.lock().unwrap_or_else(|e| e.into_inner());
                if visible {
                    break;
                }

                // 实时获取窗口位置，避免使用静态传入值
                let (cur_win_x, cur_win_y, cur_win_w, cur_win_h) = {
                    if let Some(w) = handle.get_webview_window("main") {
                        if let Ok(pos) = w.outer_position() {
                            if let Ok(size) = w.outer_size() {
                                (pos.x, pos.y, size.width as i32, size.height as i32)
                            } else {
                                (win_x, win_y, win_w as i32, win_h as i32)
                            }
                        } else {
                            (win_x, win_y, win_w as i32, win_h as i32)
                        }
                    } else {
                        (win_x, win_y, win_w as i32, win_h as i32)
                    }
                };

                #[cfg(target_os = "windows")]
                let (mx, my) = unsafe {
                    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
                    use windows::Win32::Foundation::POINT;
                    let mut pt = POINT::default();
                    if GetCursorPos(&mut pt).is_ok() {
                        (pt.x, pt.y)
                    } else {
                        continue;
                    }
                };
                #[cfg(not(target_os = "windows"))]
                { break; }

                let (edge_match, axis_match) = match edge_str {
                    "left" => (
                        mx <= wa_x + hidden_px + hover_range,
                        my >= cur_win_y && my <= cur_win_y + cur_win_h,
                    ),
                    "right" => (
                        mx >= wa_x + wa_w as i32 - hidden_px - hover_range,
                        my >= cur_win_y && my <= cur_win_y + cur_win_h,
                    ),
                    "top" => (
                        my <= wa_y + hidden_px + hover_range,
                        mx >= cur_win_x && mx <= cur_win_x + cur_win_w,
                    ),
                    "bottom" => (
                        my >= wa_y + wa_h as i32 - hidden_px - hover_range,
                        mx >= cur_win_x && mx <= cur_win_x + cur_win_w,
                    ),
                    _ => break,
                };

                if edge_match && axis_match {
                    if !last_triggered {
                        if let Some(w) = handle.get_webview_window("main") {
                            let _ = w.emit("hover-edge-detected", ());
                        }
                        last_triggered = true;
                    }
                } else {
                    last_triggered = false;
                }
            }
        });
    }
    Ok(())
}


/// 发送 Windows 原生系统通知
fn send_native_notification(app: &AppHandle, title: &str, body: &str) {
    let result = app.notification()
        .builder()
        .title(title)
        .body(body)
        .show();
    if let Err(e) = result {
        eprintln!("[reminder] 发送系统通知失败: {}", e);
    }
}

fn start_reminder_worker(app: AppHandle) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(30));
        let state = app.state::<AppState>();
        let due = match state.store.lock() {
            Ok(store) => store.take_due_reminders().unwrap_or_default(),
            Err(e) => { eprintln!("Reminder worker lock error: {}", e); Vec::new() }
        };
        // 发送原生系统通知（即使窗口隐藏也能看到）
        for memo in &due {
            let body = strip_markdown_for_notify(&memo.content);
            let body = if body.is_empty() { "（空内容）".to_string() } else { body };
            send_native_notification(&app, "备忘录提醒", &body);
        }
        // 同时发送事件到前端（用于 toast 和应用内提示）
        if let Some(w) = app.get_webview_window("main") {
            for memo in due {
                let _ = w.emit("memo-reminder-due", &memo);
            }
        }
    });
}

pub(crate) fn toggle_window(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Some(w) = app.get_webview_window("main") {
        let actually_visible = {
            let visible = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
            *visible && w.is_visible().unwrap_or(true)
        };
        if actually_visible {
            // 非置顶且未聚焦时窗口可能被其它窗口遮挡：此时快捷键应把窗口提到前台而不是隐藏，
            // 否则用户看不见窗口却触发了隐藏，需要按两次才能呼出；
            // 置顶时窗口必然可见，无论是否聚焦都直接隐藏（一次按键即隐藏）
            let on_top = state.settings.lock().unwrap_or_else(|e| e.into_inner()).always_on_top;
            let focused = w.is_focused().unwrap_or(false);
            if !on_top && !focused {
                let _ = w.show();
                let _ = w.set_focus();
                return; // 仍为可见状态，window_visible 不变
            }
            save_window_position(app, &w);
            // 隐藏/显示动画由前端状态机唯一负责，Rust 端只发事件不直接 hide/show/set_position，
            // 否则会与前端动画争抢窗口（曾因此把未贴边窗口传送回旧吸附位）
            let _ = w.emit("request-hide", ());
            let mut visible = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
            *visible = false;
        } else {
            let _ = w.emit("toggle-window", ());
            let mut visible = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
            *visible = true;
        }
    } else {
        let mut visible = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
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
    let settings = load_settings();
    let shortcut_str = settings.shortcut.clone();
    let note_shortcut_str = settings.note_shortcut.clone();
    let saved_settings = settings.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let store = MemoStore::new().expect("Failed to init database");
            app.manage(AppState {
                store: Mutex::new(store),
                settings: Mutex::new(settings.clone()),
                window_visible: Mutex::new(false),
                saved_hide_position: Mutex::new(None),
                hover_stop_flag: Mutex::new(None),
                viewer_payload: Mutex::new(None),
            });

            // 配置 asset 协议作用域：允许前端通过 asset:// 协议访问图片目录
            if let Some(data_dir) = dirs::data_dir() {
                let img_dir = data_dir.join("sidebar-memo").join("images");
                let _ = std::fs::create_dir_all(&img_dir);
                let _ = app.handle().asset_protocol_scope().allow_directory(&img_dir, true);
            }
            setup_tray(app.handle())?;
            let _ = shortcut::register_shortcut_internal(app.handle(), &shortcut_str);
            if let Err(e) = quick_note::register_note_shortcut_internal(app.handle(), &note_shortcut_str) {
                eprintln!("[QuickNote] 快捷键注册失败: {}", e);
            }
            start_reminder_worker(app.handle().clone());

            // viewer 窗口：拦截关闭请求，改为隐藏（复用窗口，避免销毁/重建问题）
            if let Some(viewer_win) = app.get_webview_window("image-viewer") {
                let viewer_handle = app.handle().clone();
                viewer_win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(w) = viewer_handle.get_webview_window("image-viewer") {
                            let _ = w.hide();
                        }
                        notify_viewer_closed(&viewer_handle);
                    }
                });
            }

            // Listen for second instance show signal
            {
                let app_handle = app.handle().clone();
                #[cfg(target_os = "windows")]
                thread::spawn(move || {
                    use windows::Win32::System::Threading::{OpenEventW, WaitForSingleObject, ResetEvent, SYNCHRONIZATION_ACCESS_RIGHTS};
                    use windows_core::PCWSTR;

                    let event_name: Vec<u16> = "Global\\SidebarMemoShowEvent\0"
                        .encode_utf16()
                        .collect();
                    let access = SYNCHRONIZATION_ACCESS_RIGHTS(0x100002);
                    unsafe {
                        if let Ok(event) = OpenEventW(access, false, PCWSTR(event_name.as_ptr())) {
                            loop {
                                WaitForSingleObject(event, 0xFFFFFFFF);
                                let _ = ResetEvent(event);
                                // 通过 toggle-window 事件同步前端状态机，避免只 show 不同步
                                if let Some(w) = app_handle.get_webview_window("main") {
                                    let state = app_handle.state::<AppState>();
                                    let was_visible = {
                                        let v = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
                                        *v
                                    };
                                    if !was_visible {
                                        let _ = w.emit("toggle-window", ());
                                        let mut v = state.window_visible.lock().unwrap_or_else(|e| e.into_inner());
                                        *v = true;
                                    } else {
                                        let _ = w.show();
                                        let _ = w.set_focus();
                                    }
                                }
                            }
                        }
                    }
                });
            }

            if let Some(w) = app.get_webview_window("main") {
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
        .invoke_handler(tauri::generate_handler![close_to_tray, resize_window, set_position_and_size, settings::toggle_always_on_top, frontend_ready, get_memos, get_trashed_memos, add_memo, update_memo, delete_memo, toggle_pin, set_color, toggle_done, reorder_memos, settings::get_settings, set_window_visible, move_to_trash, restore_from_trash, permanent_delete, save_current_position, set_shortcut, settings::set_theme, settings::set_skin, clear_trashed, set_reminder, clear_reminder, show_main_window, handle_system_wakeup, images::save_image, images::delete_image, images::get_image_base64, images::get_image_path, open_image_viewer, close_image_viewer, get_viewer_payload, save_hide_position, animate_window_position, cancel_window_animation, start_hover_detection, ocr_image, quick_note::open_quick_note, quick_note::close_quick_note, quick_note::save_quick_note, quick_note::save_quick_note_image, quick_note::move_quick_note_images, quick_note::set_note_shortcut])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}