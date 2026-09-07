//! 数据安全：手动导出 / 合并导入 / 启动自动备份。
//!
//! 导出走「写到应用数据目录 + 前端唤起资源管理器定位文件」，而不是浏览器下载：
//! 项目没装 tauri-plugin-dialog，WebView2 下的 `<a download>` 行为也不可控，
//! 写死到 `exports/` 子目录反而稳定、可预期，用户拿到的是一份真实文件。

use chrono::Local;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::db::{Board, Memo, MemoStore, BOARD_NAME_MAX};
use crate::images::validate_path_component;

/// 备份保留多少天的快照。更早的整天删掉
const KEEP_BACKUP_DAYS: usize = 7;
/// 导入文件的体积上限。正常几千条便签的 JSON 也就几 MB，超过这个数多半是选错了文件
const MAX_IMPORT_BYTES: usize = 32 * 1024 * 1024;
const VALID_COLORS: [&str; 5] = ["", "pink", "blue", "green", "yellow"];

fn now_str() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

fn data_root() -> Result<PathBuf, String> {
    dirs::data_dir()
        .map(|d| d.join("sidebar-memo"))
        .ok_or("无法定位应用数据目录".to_string())
}

// ── 导出 ──────────────────────────────────────────────

fn csv_cell(s: &str) -> String {
    // 全部加引号 + 内部引号翻倍：便签正文里的逗号、换行、引号都不会撑破列
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn to_csv(rows: &[Memo], boards: &[Board]) -> String {
    let mut out =
        String::from("\u{feff}内容,创建时间,更新时间,颜色,置顶,完成,提醒时间,重复提醒,回收站,归档,图片,集合\n");
    // 库里存的是集合 id，导出成 id 对人没有意义，换成名称
    let name_of = |id: &str| -> String {
        boards.iter().find(|b| b.id == id).map(|b| b.name.clone()).unwrap_or_default()
    };
    for m in rows {
        let line = [
            m.content.clone(),
            m.created_at.clone(),
            m.updated_at.clone(),
            m.color.clone(),
            if m.is_pinned { "是" } else { "否" }.to_string(),
            if m.is_done { "是" } else { "否" }.to_string(),
            m.remind_at.clone(),
            m.remind_repeat.clone(),
            if m.is_trashed { "是" } else { "否" }.to_string(),
            m.archived_at.clone(),
            m.images.clone(),
            name_of(&m.board),
        ]
        .iter()
        .map(|s| csv_cell(s))
        .collect::<Vec<_>>()
        .join(",");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn to_json(rows: &[Memo], boards: &[Board]) -> Result<String, String> {
    let wrapper = serde_json::json!({
        "app": "sidebar-memo",
        // 2：新增 boards 数组与 memos[].board。
        //    boards[].color 是后来加的装饰字段，没升版本：读侧 serde(default) 会自动补成「不着色」，
        //    老版本读到 color 也只是忽略，双向都不炸
        "schema": 2,
        "exported_at": now_str(),
        "count": rows.len(),
        "boards": boards,
        "memos": rows,
    });
    serde_json::to_string_pretty(&wrapper).map_err(|e| format!("生成导出内容失败: {}", e))
}

/// 导出全量便签（含回收站）与全部集合，返回写好的文件绝对路径
#[tauri::command]
pub(crate) fn export_memos(state: tauri::State<crate::AppState>, format: String) -> Result<String, String> {
    let (rows, boards) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        (
            store.all_rows().map_err(|e| e.to_string())?,
            store.get_boards().map_err(|e| e.to_string())?,
        )
    };
    if rows.is_empty() {
        return Err("没有可导出的便签".to_string());
    }

    let dir = data_root()?.join("exports");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败: {}", e))?;

    let stamp = Local::now().format("%Y%m%d_%H%M%S");
    let (ext, body) = match format.as_str() {
        "csv" => ("csv", to_csv(&rows, &boards)),
        "json" => ("json", to_json(&rows, &boards)?),
        _ => return Err("只支持 json 或 csv 格式".to_string()),
    };

    let path = dir.join(format!("memos_{}.{}", stamp, ext));
    std::fs::write(&path, body).map_err(|e| format!("写入导出文件失败: {}", e))?;
    Ok(path.to_string_lossy().to_string())
}

// ── 导入 ──────────────────────────────────────────────

/// 宽松接收：缺字段按默认值补，避免用户手工删过的 JSON 整体解析失败
#[derive(Default, Deserialize)]
#[serde(default)]
struct RawMemo {
    id: String,
    content: String,
    created_at: String,
    updated_at: String,
    color: String,
    is_pinned: bool,
    is_done: bool,
    sort_order: i32,
    is_trashed: bool,
    trashed_at: String,
    remind_at: String,
    images: String,
    remind_repeat: String,
    archived_at: String,
    board: String,
}

/// 导入用的集合行。名称和创建时间缺失时按默认值补，不让一条坏数据整体解析失败
#[derive(Default, Deserialize)]
#[serde(default)]
struct RawBoard {
    id: String,
    name: String,
    created_at: String,
    color: String,
}

#[derive(Serialize)]
pub(crate) struct ImportReport {
    /// 新插入的条数
    pub inserted: usize,
    /// id 已存在、原样跳过的条数
    pub existing: usize,
    /// 校验不通过被丢弃的条数
    pub invalid: usize,
    /// 新插入的条数里有多少是归档状态：它们不进主列表，不单独报出来会被当成导入没生效
    pub archived: usize,
    pub total: usize,
    /// 新建的集合个数：导入前库里没有的才会算进来
    pub boards: usize,
}

fn valid_dt(s: &str) -> String {
    if MemoStore::is_valid_datetime(s) { s.to_string() } else { now_str() }
}

/// images 列是文件名 JSON 数组。文件名会参与拼路径，导入时必须逐个过路径校验，
/// 否则一个 `../../x.png` 就能让应用去读图片目录之外的文件。
fn sanitize_images(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }
    match serde_json::from_str::<Vec<String>>(raw) {
        Ok(list) => {
            let kept: Vec<String> = list
                .into_iter()
                .filter(|f| validate_path_component(f).is_ok())
                .collect();
            if kept.is_empty() { String::new() } else { serde_json::to_string(&kept).unwrap_or_default() }
        }
        Err(_) => String::new(),
    }
}

/// 重复规则只接受本应用写得出的几种取值，其它一律降级为「不重复」。
/// 没有提醒时间时规则也必须清空，否则库里会留下永远触发不了的半成品。
fn sanitize_repeat(raw: &str, has_remind: bool) -> String {
    if !has_remind {
        return String::new();
    }
    match raw.strip_prefix("monthly:") {
        Some(day) if (1..=31).contains(&day.parse::<u32>().unwrap_or(0)) => raw.to_string(),
        _ if raw == "" || raw == "daily" || raw == "weekly" => raw.to_string(),
        _ => String::new(),
    }
}

fn to_memo(raw: RawMemo) -> Option<Memo> {
    if validate_path_component(&raw.id).is_err() {
        return None;
    }
    let content = raw.content.trim().to_string();
    if content.is_empty() {
        return None;
    }
    let images = sanitize_images(&raw.images);
    let color = if VALID_COLORS.contains(&raw.color.as_str()) { raw.color } else { String::new() };
    let remind_at = if raw.remind_at.is_empty() || !MemoStore::is_valid_datetime(&raw.remind_at) {
        String::new()
    } else {
        raw.remind_at.clone()
    };
    let remind_repeat = sanitize_repeat(&raw.remind_repeat, !remind_at.is_empty());
    let archived_at = if MemoStore::is_valid_datetime(&raw.archived_at) { raw.archived_at } else { String::new() };
    // board 存的是集合 id。这里只挡形状非法的值（比如带路径分隔符的），
    // 至于这个集合在不在库里，交给写库后那条自愈规则统一清成普通便签
    let board = if raw.board.is_empty() || validate_path_component(&raw.board).is_err() {
        String::new()
    } else {
        raw.board
    };
    Some(Memo {
        id: raw.id,
        content,
        created_at: valid_dt(&raw.created_at),
        updated_at: valid_dt(&raw.updated_at),
        color,
        is_pinned: raw.is_pinned,
        is_done: raw.is_done,
        sort_order: raw.sort_order,
        is_trashed: raw.is_trashed,
        trashed_at: if MemoStore::is_valid_datetime(&raw.trashed_at) { raw.trashed_at } else { String::new() },
        remind_at,
        remind_repeat,
        archived_at,
        images,
        board,
    })
}

/// 从 JSON 文本里取出便签与集合：兼容本应用导出的 `{ memos: [...], boards: [...] }`
/// 与裸数组两种写法。老导出没有 boards 就按空处理，那批 board 引用会被写库后的
/// 自愈规则清成普通便签，不会留下看不见也回不去的孤儿。
fn extract_raw_rows(text: &str) -> Result<(Vec<RawMemo>, Vec<Board>), String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("JSON 解析失败: {}", e))?;
    let arr = match &value {
        serde_json::Value::Array(a) => Some(a.clone()),
        serde_json::Value::Object(obj) => obj
            .get("memos")
            .and_then(|v| v.as_array())
            .cloned(),
        _ => None,
    }
    .ok_or("文件里找不到 memos 数组，请确认是本应用导出的 JSON")?;
    let memos = arr
        .into_iter()
        .filter_map(|v| serde_json::from_value::<RawMemo>(v).ok())
        .collect();
    let raw_boards = value
        .get("boards")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let boards = raw_boards
        .into_iter()
        .filter_map(|v| serde_json::from_value::<RawBoard>(v).ok())
        .filter(|b| validate_path_component(&b.id).is_ok())
        .map(|b| {
            let name: String = b.name.trim().chars().take(BOARD_NAME_MAX).collect();
            Board {
                id: b.id,
                name,
                created_at: valid_dt(&b.created_at),
                color: b.color,
            }
        })
        .filter(|b| !b.name.is_empty())
        .collect();
    Ok((memos, boards))
}

/// 合并导入：按 id 补进缺失的条目，已存在的原样保留
#[tauri::command]
pub(crate) fn import_memos(
    state: tauri::State<crate::AppState>,
    payload: String,
) -> Result<ImportReport, String> {
    if payload.len() > MAX_IMPORT_BYTES {
        return Err("导入文件过大，请确认选的是导出文件".to_string());
    }
    let (raws, boards) = extract_raw_rows(&payload)?;
    let total = raws.len();
    let mut invalid = 0;
    let mut cleaned: Vec<Memo> = Vec::with_capacity(total);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for r in raws {
        match to_memo(r) {
            // 同一份文件内部重复的 id 只取第一条，否则第二条会报成「已存在」误导用户
            Some(m) if seen.insert(m.id.clone()) => cleaned.push(m),
            _ => invalid += 1,
        }
    }

    let (inserted, archived, boards_added) = {
        let store = state.store.lock().map_err(|e| e.to_string())?;
        // 必须先建集合再写便签：insert_missing 结尾会把指向不存在集合的
        // board 引用清成普通便签，反过来做就等于白导
        let added = store.insert_boards(&boards).map_err(|e| format!("写入集合失败: {}", e))?;
        let (i, a) = store.insert_missing(&cleaned).map_err(|e| format!("写入数据库失败: {}", e))?;
        (i, a, added)
    };
    crate::settings::diag(&format!(
        "IMPORT total={} inserted={} archived={} invalid={} boards={}",
        total, inserted, archived, invalid, boards_added
    ));
    Ok(ImportReport {
        inserted,
        existing: cleaned.len() - inserted,
        invalid,
        archived,
        total,
        boards: boards_added,
    })
}

// ── 启动自动备份 ──────────────────────────────────────

/// 后台线程做整目录备份，不阻塞窗口显示
pub(crate) fn spawn_startup_backup() {
    std::thread::spawn(|| {
        if let Err(e) = run_backup() {
            eprintln!("[backup] 自动备份失败: {}", e);
        }
    });
}

fn run_backup() -> Result<(), String> {
    let root = data_root()?;
    let day = Local::now().format("%Y-%m-%d").to_string();
    let backups = root.join("backups");
    let final_dir = backups.join(&day);
    // 一天一份；重跑（如同一天内反复启动）不重做当天已完成的快照
    if final_dir.exists() {
        return Ok(());
    }
    // 上次被中途结束的备份会留下 .tmp 半成品，正式目录没有原子写入保证，先清干净
    if let Ok(entries) = std::fs::read_dir(&backups) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".tmp") {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }

    // 先写 .tmp 再整体改名：进程在备份中途被结束时，不会留下一个「当天已存在但不完整」的快照
    let tmp = backups.join(format!("{}.tmp", day));
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建备份临时目录失败: {}", e))?;

    if let Err(e) = backup_db(&root, &tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    // 图片失败不影响已经写好的数据库快照，降级为保住数据、继续改名
    if let Err(e) = backup_images(&root, &tmp) {
        eprintln!("[backup] 图片备份失败: {}", e);
    }
    std::fs::rename(&tmp, &final_dir).map_err(|e| format!("备份落位失败: {}", e))?;
    if let Err(e) = prune_backups(&backups) {
        eprintln!("[backup] 清理过期备份失败: {}", e);
    }
    crate::settings::diag(&format!("BACKUP ok dir={}", final_dir.display()));
    Ok(())
}

/// VACUUM INTO 用独立连接取事务一致的快照。
/// 直接拷 memos.db 在 WAL 模式下会漏掉 -wal 里未合并的写入，拷出来的库可能是旧的或坏的。
fn backup_db(root: &Path, dir: &Path) -> Result<(), String> {
    let target = dir.join("memos.db");
    let conn = Connection::open(root.join("memos.db")).map_err(|e| format!("打开数据库失败: {}", e))?;
    conn.busy_timeout(std::time::Duration::from_secs(10))
        .map_err(|e| format!("设置等待超时失败: {}", e))?;
    let literal = target.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{}'", literal))
        .map_err(|e| format!("VACUUM INTO 失败: {}", e))
}

/// 图片优先硬链接：同一份内容只占一份磁盘，却能挡住「应用内误删」这个真实风险
/// （删掉原文件后目录项还在，备份里的链接依然读得到）。非 NTFS 等场景回退成复制。
fn backup_images(root: &Path, dir: &Path) -> Result<(), String> {
    let src = root.join("images");
    if !src.is_dir() {
        return Ok(());
    }
    link_dir_all(&src, &dir.join("images"))
}

fn link_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("创建 {} 失败: {}", dst.display(), e))?;
    let entries = std::fs::read_dir(src).map_err(|e| format!("读取 {} 失败: {}", src.display(), e))?;
    for entry in entries.flatten() {
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            link_dir_all(&from, &to)?;
            continue;
        }
        if std::fs::hard_link(&from, &to).is_err() {
            std::fs::copy(&from, &to)
                .map_err(|e| format!("备份 {} 失败: {}", from.display(), e))?;
        }
    }
    Ok(())
}

/// 只删名字严格是 YYYY-MM-DD 的目录，其他内容一律不碰
fn prune_backups(backups: &Path) -> Result<(), String> {
    let mut days: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(backups) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.len() == 10 && name.as_bytes()[4] == b'-' && name.as_bytes()[7] == b'-' {
                days.push(name);
            }
        }
    }
    if days.len() <= KEEP_BACKUP_DAYS {
        return Ok(());
    }
    days.sort();
    days.reverse();
    for day in days.into_iter().skip(KEEP_BACKUP_DAYS) {
        std::fs::remove_dir_all(backups.join(&day))
            .map_err(|e| format!("删除过期备份 {} 失败: {}", day, e))?;
    }
    Ok(())
}
