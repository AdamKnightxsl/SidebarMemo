use chrono::{Local, Duration, NaiveDate, NaiveDateTime, Datelike};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 全库统一的时间格式。字典序比较依赖这个格式固定为 YYYY-MM-DD HH:MM:SS
const DT_FMT: &str = "%Y-%m-%d %H:%M:%S";

/// 集合名称的去空白后最大字数，以及同时可存在的集合个数上限（导航栏要能滚动展示）
pub(crate) const BOARD_NAME_MAX: usize = 8;
const BOARD_MAX: usize = 10;

/// 集合归属色的键名，与 style.css 的 --board-* 令牌一一对应（改色值改 CSS，加颜色要两处同步）。
/// 库里存键名不存 hex：深浅主题各取一档亮度，色板调整也不用迁移数据
const BOARD_COLORS: [&str; 10] = [
    "coral", "amber", "lemon", "grass", "teal", "sky", "indigo", "violet", "magenta", "clay",
];

const MIGRATIONS: &[(i32, &str)] = &[
    (1, "ALTER TABLE memos ADD COLUMN is_trashed INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE memos ADD COLUMN trashed_at TEXT NOT NULL DEFAULT '';"),
    (2, "ALTER TABLE memos ADD COLUMN remind_at TEXT NOT NULL DEFAULT '';"),
    (3, "ALTER TABLE memos ADD COLUMN images TEXT NOT NULL DEFAULT '';"),
    (4, "UPDATE memos SET sort_order = (
            SELECT COUNT(*) FROM memos AS m2
            WHERE m2.is_trashed = 0 AND m2.created_at > memos.created_at
         ) WHERE is_trashed = 0;"),
    (5, "ALTER TABLE memos ADD COLUMN remind_repeat TEXT NOT NULL DEFAULT '';
         ALTER TABLE memos ADD COLUMN archived_at TEXT NOT NULL DEFAULT '';"),
    (6, "CREATE TABLE IF NOT EXISTS boards (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
         ALTER TABLE memos ADD COLUMN board TEXT NOT NULL DEFAULT '';"),
    (7, "ALTER TABLE boards ADD COLUMN color TEXT NOT NULL DEFAULT '';"),
];

/// 逐条执行迁移语句。SQLite 不支持 ALTER TABLE ADD COLUMN IF NOT EXISTS，
/// 列已存在时（如建表语句已包含该列）忽略 duplicate column 错误，保证迁移可重复执行不崩溃
fn exec_migration(conn: &Connection, sql: &str) -> rusqlite::Result<()> {
    for stmt in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute_batch(stmt) {
            if e.to_string().contains("duplicate column name") {
                continue;
            }
            return Err(e);
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn schema_version_display(v: i32) -> String {
    let major = v / 100 + 1;
    let minor = (v % 100) / 10;
    let patch = v % 10;
    format!("{}.{}.{}", major, minor, patch)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Memo {
    pub id: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
    pub color: String,
    pub is_pinned: bool,
    pub is_done: bool,
    pub sort_order: i32,
    pub is_trashed: bool,
    pub trashed_at: String,
    pub remind_at: String,
    pub images: String,
    /// 重复提醒规则：'' | daily | weekly | monthly:DD。
    /// 每月这一种把「锚定日」写进规则里，1/31 往后推进才不会永远停在 28 号
    pub remind_repeat: String,
    /// 归档时间，空表示未归档。归档只是从主列表移出，不删数据、也不再提醒
    pub archived_at: String,
    /// 所属集合 id，空串表示普通便签。集合里的便签不参与自动清理，也不进今/昨视图
    pub board: String,
}

/// 导航栏上用户自定义的集合按钮。created_at 只用于展示与恢复，
/// 排列顺序交给 rowid（创建顺序）——同一秒内建两个也不会乱
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Board {
    pub id: String,
    pub name: String,
    pub created_at: String,
    /// 归属色，存的是 BOARD_COLORS 里的键名而不是 hex：色值只写在 CSS 一处，
    /// 深/浅主题各自取亮度。空串＝不着色，跟着皮肤强调色走
    pub color: String,
}

pub struct MemoStore {
    conn: Connection,
}

impl MemoStore {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let db_dir = dirs::data_dir()
            .ok_or("Cannot find data dir")?
            .join("sidebar-memo");
        std::fs::create_dir_all(&db_dir)?;
        let db_path = db_dir.join("memos.db");
        let conn = Connection::open(db_path)?;

        // 开启 WAL 模式，提升并发读写性能
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS _meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS memos (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '',
                is_pinned INTEGER NOT NULL DEFAULT 0,
                is_done INTEGER NOT NULL DEFAULT 0,
                sort_order INTEGER NOT NULL DEFAULT 0,
                is_trashed INTEGER NOT NULL DEFAULT 0,
                trashed_at TEXT NOT NULL DEFAULT '',
                remind_at TEXT NOT NULL DEFAULT '',
                images TEXT NOT NULL DEFAULT '',
                remind_repeat TEXT NOT NULL DEFAULT '',
                archived_at TEXT NOT NULL DEFAULT '',
                board TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS boards (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT ''
            );",
        )?;

        let current: i32 = conn
            .query_row(
                "SELECT CAST(value AS INTEGER) FROM _meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // Handle legacy databases: if _meta is empty but columns exist, skip ahead
        if current == 0 {
            let has_trashed: bool = conn
                .prepare("SELECT is_trashed FROM memos LIMIT 0")
                .is_ok();
            if has_trashed {
                let has_remind: bool = conn
                    .prepare("SELECT remind_at FROM memos LIMIT 0")
                    .is_ok();
                let has_images: bool = conn
                    .prepare("SELECT images FROM memos LIMIT 0")
                    .is_ok();
                let version = if has_images { "4" } else if has_remind { "2" } else { "1" };
                conn.execute(
                    "INSERT OR REPLACE INTO _meta (key, value) VALUES ('schema_version', ?1)",
                    params![version],
                )?;
            }
        }

        for &(version, sql) in MIGRATIONS {
            let db_version: i32 = conn
                .query_row(
                    "SELECT CAST(value AS INTEGER) FROM _meta WHERE key = 'schema_version'",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if db_version < version {
                exec_migration(&conn, sql)?;
                conn.execute(
                    "INSERT OR REPLACE INTO _meta (key, value) VALUES ('schema_version', ?1)",
                    params![version.to_string()],
                )?;
            }
        }

        // 迁移跑完再自愈一次：集合可以整体删除，外部导入的 JSON 也可能带一个库里没有的
        // board id。悬空引用会让这条便签既被今/昨排除、又标不出归属，等于凭空消失，
        // 所以统一退回普通便签。
        conn.execute(
            "UPDATE memos SET board = ''
             WHERE board != '' AND board NOT IN (SELECT id FROM boards)",
            [],
        )?;

        Ok(Self { conn })
    }

    fn row_to_memo(row: &rusqlite::Row) -> rusqlite::Result<Memo> {
        Ok(Memo {
            id: row.get(0)?,
            content: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            color: row.get(4)?,
            is_pinned: row.get::<_, i32>(5)? != 0,
            is_done: row.get::<_, i32>(6)? != 0,
            sort_order: row.get(7)?,
            is_trashed: row.get::<_, i32>(8)? != 0,
            trashed_at: row.get(9)?,
            remind_at: row.get(10)?,
            images: row.get(11)?,
            remind_repeat: row.get(12)?,
            archived_at: row.get(13)?,
            board: row.get(14)?,
        })
    }

    pub fn get_all(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images, remind_repeat, archived_at, board
             FROM memos WHERE is_trashed = 0 AND archived_at = ''
             ORDER BY is_pinned DESC, sort_order ASC",
        )?;
        let rows = stmt.query_map([], Self::row_to_memo)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 归档区：只放还没进垃圾桶的条目，已删除的仍归垃圾桶视图管
    pub fn get_archived(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images, remind_repeat, archived_at, board
             FROM memos
             WHERE is_trashed = 0 AND archived_at != ''
             ORDER BY archived_at DESC",
        )?;
        let rows = stmt.query_map([], Self::row_to_memo)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_trashed(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images, remind_repeat, archived_at, board
             FROM memos WHERE is_trashed = 1
             ORDER BY trashed_at DESC",
        )?;
        let rows = stmt.query_map([], Self::row_to_memo)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 全量快照，含回收站条目：导出与备份用它，才能做到导出后原样导回不丢数据
    pub fn all_rows(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images, remind_repeat, archived_at, board
             FROM memos
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], Self::row_to_memo)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 合并导入：只补进库里没有的 id，已存在的一律跳过。
    /// 刻意不做「按 updated_at 覆盖」——判断错一次就把用户本地改过的内容冲掉了，
    /// 而重复导入最多是少进几条，不会毁数据。
    /// 返回 (新插入条数, 其中归档状态的条数)
    pub fn insert_missing(&self, rows: &[Memo]) -> Result<(usize, usize), Box<dyn std::error::Error>> {
        let mut inserted = 0;
        let mut archived = 0;
        for m in rows {
            let changed = self.conn.execute(
                "INSERT OR IGNORE INTO memos
                 (id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images, remind_repeat, archived_at, board)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    m.id, m.content, m.created_at, m.updated_at, m.color,
                    m.is_pinned as i32, m.is_done as i32, m.sort_order,
                    m.is_trashed as i32, m.trashed_at, m.remind_at, m.images,
                    m.remind_repeat, m.archived_at, m.board
                ],
            )?;
            inserted += changed;
            if changed > 0 && !m.archived_at.is_empty() {
                archived += 1;
            }
        }
        // 导入的 JSON 可以只带 memos 不带 boards，这时 board 会指向一个不存在的集合。
        // 和启动时那条自愈规则一致：清成普通便签，不让它变成看不见也回不去的孤儿。
        self.conn.execute(
            "UPDATE memos SET board = ''
             WHERE board != '' AND board NOT IN (SELECT id FROM boards)",
            [],
        )?;
        Ok((inserted, archived))
    }

    pub fn insert(&self, content: &str, board: &str) -> Result<Memo, Box<dyn std::error::Error>> {
        let id = Uuid::new_v4().to_string();
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let min_order: i32 = self
            .conn
            .query_row(
                "SELECT COALESCE(MIN(sort_order), 0) FROM memos WHERE is_pinned = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        self.conn.execute(
            "INSERT INTO memos (id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images, remind_repeat, archived_at, board)
             VALUES (?1, ?2, ?3, ?4, '', 0, 0, ?5, 0, '', '', '', '', '', ?6)",
            params![id, content, now, now, min_order - 1, board],
        )?;

        Ok(Memo {
            id,
            content: content.to_string(),
            created_at: now.clone(),
            updated_at: now,
            color: String::new(),
            is_pinned: false,
            is_done: false,
            sort_order: min_order - 1,
            is_trashed: false,
            trashed_at: String::new(),
            remind_at: String::new(),
            images: String::new(),
            remind_repeat: String::new(),
            archived_at: String::new(),
            board: board.to_string(),
        })
    }

    pub fn update_content(&self, id: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn.execute(
            "UPDATE memos SET content = ?1, updated_at = ?2 WHERE id = ?3",
            params![content, now, id],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute("DELETE FROM memos WHERE id = ?1", params![id])?;
        let img_dir = dirs::data_dir()
            .ok_or("no data dir")?
            .join("sidebar-memo")
            .join("images")
            .join(id);
        if img_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&img_dir) {
                eprintln!("[delete] 清理图片目录失败 ({}): {}", img_dir.display(), e);
            }
        }
        Ok(())
    }

    pub fn toggle_pin(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute(
            "UPDATE memos SET is_pinned = CASE WHEN is_pinned = 1 THEN 0 ELSE 1 END WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn set_color(&self, id: &str, color: &str) -> Result<(), Box<dyn std::error::Error>> {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn.execute(
            "UPDATE memos SET color = ?1, updated_at = ?2 WHERE id = ?3",
            params![color, now, id],
        )?;
        Ok(())
    }

    pub fn toggle_done(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute(
            "UPDATE memos SET is_done = CASE WHEN is_done = 1 THEN 0 ELSE 1 END WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn reorder(&self, ids: &[String]) -> Result<(), Box<dyn std::error::Error>> {
        for (i, id) in ids.iter().enumerate() {
            self.conn.execute(
                "UPDATE memos SET sort_order = ?1 WHERE id = ?2",
                params![i as i32, id],
            )?;
        }
        Ok(())
    }

    pub fn move_to_trash(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        // 同时清除提醒，防止已删除的备忘仍触发提醒
        self.conn.execute(
            "UPDATE memos SET is_trashed = 1, trashed_at = ?1, remind_at = '' WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    }

    pub fn restore_from_trash(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute(
            "UPDATE memos SET is_trashed = 0, trashed_at = '' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn permanent_delete(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute("DELETE FROM memos WHERE id = ?1", params![id])?;
        // 清理附件目录
        let img_dir = dirs::data_dir()
            .ok_or("no data dir")?
            .join("sidebar-memo")
            .join("images")
            .join(id);
        if img_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&img_dir) {
                eprintln!("[permanent_delete] 清理图片目录失败 ({}): {}", img_dir.display(), e);
            }
        }
        Ok(())
    }

    pub fn clear_trashed(&self) -> Result<u32, Box<dyn std::error::Error>> {
        // 先收集垃圾桶里的 memo ids，以便清理附件
        let mut stmt = self.conn.prepare(
            "SELECT id FROM memos WHERE is_trashed = 1",
        )?;
        let ids: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;

        let affected = self.conn.execute("DELETE FROM memos WHERE is_trashed = 1", [])?;

        // 清理每个 trashed memo 的附件
        if let Some(data_dir) = dirs::data_dir() {
            let img_root = data_dir.join("sidebar-memo").join("images");
            for id in ids {
                let img_dir = img_root.join(&id);
                if img_dir.exists() {
                    if let Err(e) = std::fs::remove_dir_all(&img_dir) {
                        eprintln!("[clear_trashed] 清理图片目录失败 ({}): {}", img_dir.display(), e);
                    }
                }
            }
        }

        Ok(affected as u32)
    }

    pub fn set_reminder(&self, id: &str, remind_at: &str, remind_repeat: &str) -> Result<(), Box<dyn std::error::Error>> {
        // 校验格式必须为 YYYY-MM-DD HH:MM:SS，否则字典序比较会失效
        if !Self::is_valid_datetime(remind_at) {
            return Err(format!("invalid remind_at format: '{}', expected YYYY-MM-DD HH:MM:SS", remind_at).into());
        }
        let rule = Self::normalize_repeat(remind_repeat, remind_at)?;
        self.conn.execute(
            "UPDATE memos SET remind_at = ?1, remind_repeat = ?2 WHERE id = ?3",
            params![remind_at, rule, id],
        )?;
        Ok(())
    }

    /// 校验并归一化重复规则。前端只发 '' / daily / weekly / monthly，
    /// monthly 在这里补上锚定日（取 remind_at 的日）——所以用户改提醒日期时锚点会跟着走，
    /// 而库里存的永远是带锚点的完整形式，推进时不必再猜原来指的是几号
    fn normalize_repeat(rule: &str, remind_at: &str) -> Result<String, String> {
        match rule {
            "" => Ok(String::new()),
            "daily" | "weekly" => Ok(rule.to_string()),
            "monthly" => {
                let n: u32 = remind_at
                    .get(8..10)
                    .and_then(|d| d.parse().ok())
                    .ok_or_else(|| "提醒时间格式不正确".to_string())?;
                if !(1..=31).contains(&n) {
                    return Err("提醒时间格式不正确".to_string());
                }
                Ok(format!("monthly:{:02}", n))
            }
            _ => Err("不支持的重复规则".to_string()),
        }
    }

    fn days_in_month(year: i32, month: u32) -> Option<u32> {
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
        let next_first = NaiveDate::from_ymd_opt(ny, nm, 1)?;
        Some(next_first.ordinal0() - first.ordinal0())
    }

    /// 按规则推进到「晚于 now 的第一个时间点」。
    /// 用 NaiveDateTime 走墙上时钟，保证「每天 09:00」不会随偏移越推越晚；
    /// 错过多天（例如应用没开着）只补发一次，而不是每轮把欠的都通知一遍。
    pub(crate) fn next_remind_at(prev: &str, rule: &str, now: &str) -> Option<String> {
        if rule.is_empty() { return None; }
        let prev_dt = NaiveDateTime::parse_from_str(prev, DT_FMT).ok()?;
        let limit = NaiveDateTime::parse_from_str(now, DT_FMT).ok()?;
        let step_days = match rule {
            "daily" => Some(1),
            "weekly" => Some(7),
            _ => None,
        };
        if let Some(days) = step_days {
            let mut t = prev_dt;
            for _ in 0..4000 {
                t += Duration::days(days);
                if t > limit { return Some(t.format(DT_FMT).to_string()); }
            }
            return None;
        }
        let anchor = rule.strip_prefix("monthly:")?.parse::<u32>().ok()?;
        if !(1..=31).contains(&anchor) { return None; }
        let time = prev_dt.time();
        let (mut year, mut month) = (prev_dt.year(), prev_dt.month());
        for _ in 0..1200 {
            month += 1;
            if month > 12 { month = 1; year += 1; }
            // 小月没有 31 号时就取当月最后一天，锚定日仍留在规则里，下个月照旧回到 31 号
            let day = anchor.min(Self::days_in_month(year, month)?);
            let dt = NaiveDate::from_ymd_opt(year, month, day)?.and_time(time);
            if dt > limit { return Some(dt.format(DT_FMT).to_string()); }
        }
        None
    }

    /// 校验时间字符串是否为合法的 YYYY-MM-DD HH:MM:SS 格式
    pub(crate) fn is_valid_datetime(s: &str) -> bool {
        // 长度必须为 19: "2026-07-30 14:00:00"
        if s.len() != 19 { return false; }
        let b = s.as_bytes();
        // 分隔符位置: 4='-' 7='-' 10=' ' 13=':' 16=':'
        if b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' || b[16] != b':' {
            return false;
        }
        // 其余位置必须为数字
        for &i in &[0,1,2,3, 5,6, 8,9, 11,12, 14,15, 17,18] {
            if !b[i].is_ascii_digit() { return false; }
        }
        true
    }

    pub fn clear_reminder(&self, id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // 规则一并清掉：留着的话，下次随便设个时间就会莫名变成重复提醒
        self.conn.execute(
            "UPDATE memos SET remind_at = '', remind_repeat = '' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn update_images(&self, id: &str, images: &str) -> Result<(), Box<dyn std::error::Error>> {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn.execute(
            "UPDATE memos SET images = ?1, updated_at = ?2 WHERE id = ?3",
            params![images, now, id],
        )?;
        Ok(())
    }

    /// 查询所有已到期的提醒（不清除），由调用方在通知发送成功后调用 ack_reminders 收尾
    pub fn due_reminders(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let now = Local::now().format(DT_FMT).to_string();
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images, remind_repeat, archived_at, board
             FROM memos
             WHERE remind_at != '' AND remind_at <= ?1 AND is_trashed = 0 AND is_done = 0 AND archived_at = ''
             ORDER BY remind_at ASC",
        )?;
        let rows = stmt.query_map(params![now], Self::row_to_memo)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 提醒发送成功后的收尾：一次性提醒清空，重复提醒推进到下一个未来的时间点。
    /// WHERE 带上发送时读到的 remind_at——用户可能正在这几秒里改提醒，
    /// 覆盖回去会让他的操作凭空消失，宁可这一轮少推一次。
    pub fn ack_reminders(&self, due: &[Memo]) -> Result<(), Box<dyn std::error::Error>> {
        let now = Local::now().format(DT_FMT).to_string();
        for m in due {
            let next = if m.remind_repeat.is_empty() {
                None
            } else {
                Self::next_remind_at(&m.remind_at, &m.remind_repeat, &now).or_else(|| {
                    // 规则坏到算不出下一步：清掉重来，也不能让这条提醒每 30 秒重发一次
                    eprintln!("[reminder] {} 的重复规则 {:?} 无法推进，已清除", m.id, m.remind_repeat);
                    None
                })
            };
            match next {
                Some(t) => {
                    let changed = self.conn.execute(
                        "UPDATE memos SET remind_at = ?2 WHERE id = ?1 AND remind_at = ?3",
                        params![m.id, t, m.remind_at],
                    )?;
                    if changed == 0 {
                        eprintln!("[reminder] {} 的提醒时间已被外部修改，跳过本次推进", m.id);
                    }
                }
                None => {
                    self.conn.execute(
                        "UPDATE memos SET remind_at = '', remind_repeat = '' WHERE id = ?1 AND remind_at = ?2",
                        params![m.id, m.remind_at],
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn auto_trash(&self, days: i64) -> Result<u32, Box<dyn std::error::Error>> {
        let cutoff = (Local::now() - Duration::days(days))
            .format("%Y-%m-%d 00:00:00")
            .to_string();
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        // created_at 和 updated_at 都超过 days 天才清理，避免持续编辑的老便签被误删；
        // 已归档的条目是用户主动长期保留的，不参与自动清理；
        // 集合（board 非空）里的便签同理，只能由用户手动删除
        let affected = self.conn.execute(
            "UPDATE memos SET is_trashed = 1, trashed_at = ?1
             WHERE is_pinned = 0 AND is_trashed = 0 AND archived_at = '' AND board = '' AND created_at < ?2 AND updated_at < ?2",
            params![now, cutoff],
        )?;
        Ok(affected as u32)
    }

    /// 撤销「移入垃圾桶」：is_trashed 归零并把删除前的提醒时间写回。
    /// restore_from_trash 不恢复 remind_at（垃圾桶里的条目本就该静默），
    /// 撤销是一次完整的原地复原，所以必须由这个入口走。
    pub fn undo_trash(&self, id: &str, remind_at: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !remind_at.is_empty() && !Self::is_valid_datetime(remind_at) {
            return Err(format!("invalid remind_at format: '{}', expected YYYY-MM-DD HH:MM:SS", remind_at).into());
        }
        self.conn.execute(
            "UPDATE memos SET is_trashed = 0, trashed_at = '', remind_at = ?2 WHERE id = ?1",
            params![id, remind_at],
        )?;
        Ok(())
    }

    /// 归档 / 取消归档。只动 archived_at：置顶、排序、提醒设置全部原样保留，
    /// 所以取消归档后便签会回到它原来的位置和原来的提醒计划。
    /// 垃圾桶里的条目不接受归档（那是两个互斥的状态，允许的话会出现「恢复后凭空消失」）。
    pub fn set_archived(&self, id: &str, archived: bool) -> Result<(), Box<dyn std::error::Error>> {
        let value = if archived { Local::now().format(DT_FMT).to_string() } else { String::new() };
        let sql = if archived {
            "UPDATE memos SET archived_at = ?2 WHERE id = ?1 AND is_trashed = 0"
        } else {
            "UPDATE memos SET archived_at = ?2 WHERE id = ?1"
        };
        self.conn.execute(sql, params![id, value])?;
        Ok(())
    }

    // —— 集合：导航栏上用户自己命名的内容分组 ——

    /// 按创建顺序返回，导航栏照这个顺序渲染：老的在上、新的紧挨着 + 号。
    /// 用 rowid 而不是 created_at，同一秒里连建两个也不会乱。
    pub fn get_boards(&self) -> Result<Vec<Board>, Box<dyn std::error::Error>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, name, created_at, color FROM boards ORDER BY rowid ASC")?;
        let rows = stmt.query_map([], |r| {
            Ok(Board {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
                color: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// 合并导入用的整行写入：只补进库里没有的集合。本地已有的（同 id）一律保留，
    /// 用户可能已经改过名，按导入内容覆盖会把他的命名冲掉。
    /// 这里不查 BOARD_MAX：上限只是新建时的 UI 约束，导入时截断会让被丢集合的便签退回普通便签。
    pub fn insert_boards(&self, boards: &[Board]) -> Result<usize, Box<dyn std::error::Error>> {
        let mut inserted = 0;
        for b in boards {
            inserted += self.conn.execute(
                "INSERT OR IGNORE INTO boards (id, name, created_at, color) VALUES (?1, ?2, ?3, ?4)",
                params![b.id, b.name, b.created_at, normalize_board_color(&b.color)],
            )?;
        }
        Ok(inserted)
    }

    pub fn create_board(&self, name: &str, color: &str) -> Result<Board, Box<dyn std::error::Error>> {
        let name = clean_board_name(name)?;
        let color = valid_board_color(color)?;
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM boards", [], |r| r.get(0))?;
        if count >= BOARD_MAX as i64 {
            return Err(format!("最多只能创建 {} 个集合", BOARD_MAX).into());
        }
        if self.board_name_taken(&name, None)? {
            return Err("已经有同名集合".into());
        }
        let board = Board {
            id: Uuid::new_v4().to_string(),
            name,
            created_at: Local::now().format(DT_FMT).to_string(),
            color,
        };
        self.conn.execute(
            "INSERT INTO boards (id, name, created_at, color) VALUES (?1, ?2, ?3, ?4)",
            params![board.id, board.name, board.created_at, board.color],
        )?;
        Ok(board)
    }

    /// 改名与改色一个入口：命名弹框一次提交两项，拆成两条命令只会让两个字段有机会不一致
    pub fn update_board(&self, id: &str, name: &str, color: &str) -> Result<(), Box<dyn std::error::Error>> {
        let name = clean_board_name(name)?;
        let color = valid_board_color(color)?;
        if self.board_name_taken(&name, Some(id))? {
            return Err("已经有同名集合".into());
        }
        self.conn.execute(
            "UPDATE boards SET name = ?1, color = ?2 WHERE id = ?3",
            params![name, color, id],
        )?;
        Ok(())
    }

    /// 删除集合，返回一起进垃圾桶的便签条数。整个过程必须是一个事务：
    /// 只删掉 boards 行会让便签指向一个不存在的集合，只改 memos 又会留下一个空按钮。
    /// 便签的 board 引用必须和它一起抹掉——集合已经没了，留着会让这条便签
    /// 既被今/昨排除、恢复之后又回不到任何分组。
    pub fn delete_board(&self, id: &str) -> Result<u32, Box<dyn std::error::Error>> {
        let now = Local::now().format(DT_FMT).to_string();
        let tx = self.conn.unchecked_transaction()?;
        let trashed = tx.execute(
            "UPDATE memos SET is_trashed = 1, trashed_at = ?1, remind_at = '', board = ''
             WHERE board = ?2 AND is_trashed = 0",
            params![now, id],
        )?;
        // 早就在垃圾桶里的同名条目只清引用，不能顺手把它也标记一遍
        tx.execute("UPDATE memos SET board = '' WHERE board = ?1", params![id])?;
        tx.execute("DELETE FROM boards WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(trashed as u32)
    }

    /// 集合是否存在。快捷便签在写入前要自己确认一次：
    /// 它缓存的列表可能在窗口打开期间被主窗口删掉，不查就会让自愈规则把这条悄悄清成普通便签
    pub fn board_exists(&self, id: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM boards WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// 把便签移进集合（board 传集合 id）或退回普通便签（传空串）。
    /// 不动 updated_at：它标的是「最后一次编辑完成」，换个分组不算编辑。
    /// 垃圾桶和归档里的条目不接受，那是两个互斥状态，允许的话会出现「移完凭空消失」。
    pub fn set_board(&self, id: &str, board: &str) -> Result<(), Box<dyn std::error::Error>> {
        if !board.is_empty() && !self.board_exists(board)? {
            return Err("集合不存在，可能已被删除".into());
        }
        let hits = self.conn.execute(
            "UPDATE memos SET board = ?2 WHERE id = ?1 AND is_trashed = 0 AND archived_at = ''",
            params![id, board],
        )?;
        // 必须报错而不是静默返回：调用方据此决定是否改本地状态，否则前端显示已移动、库里没动
        if hits == 0 {
            return Err("这条便签已归档或在垃圾桶里，不能移动集合".into());
        }
        Ok(())
    }

    fn board_name_taken(&self, name: &str, except_id: Option<&str>) -> Result<bool, Box<dyn std::error::Error>> {
        let sql = match except_id {
            Some(_) => "SELECT COUNT(*) FROM boards WHERE name = ?1 AND id != ?2",
            None => "SELECT COUNT(*) FROM boards WHERE name = ?1",
        };
        let count: i64 = match except_id {
            Some(skip) => self.conn.query_row(sql, params![name, skip], |r| r.get(0))?,
            None => self.conn.query_row(sql, params![name], |r| r.get(0))?,
        };
        Ok(count > 0)
    }
}

/// 集合名称：去首尾空白后 1～8 个字。导航栏按钮最多显示 4 个字，
/// 8 个是「悬浮能看全、又不至于把 title 撑成一整行」的上限。
fn clean_board_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    let len = trimmed.chars().count();
    if len == 0 {
        return Err("集合名称不能为空".into());
    }
    if len > BOARD_NAME_MAX {
        return Err(format!("集合名称最多 {} 个字", BOARD_NAME_MAX));
    }
    Ok(trimmed.to_string())
}

/// 写入路径（新建 / 命名弹框提交）：空串表示不着色，其余必须命中色板，
/// 免得前端之外的途径塞进一个 CSS 不认识的键名，按钮直接变成没颜色
fn valid_board_color(color: &str) -> Result<String, String> {
    if color.is_empty() {
        return Ok(String::new());
    }
    match BOARD_COLORS.iter().find(|c| **c == color) {
        Some(c) => Ok(c.to_string()),
        None => Err("集合颜色不在色板里".into()),
    }
}

/// 导入路径：认不出的键名（老版本导出、手改过的文件）降级成不着色，
/// 不因为一个装饰字段把整份数据拒掉
fn normalize_board_color(color: &str) -> String {
    valid_board_color(color).unwrap_or_default()
}
