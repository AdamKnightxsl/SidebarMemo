use chrono::{Local, Duration};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MIGRATIONS: &[(i32, &str)] = &[
    (1, "ALTER TABLE memos ADD COLUMN is_trashed INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE memos ADD COLUMN trashed_at TEXT NOT NULL DEFAULT '';"),
    (2, "ALTER TABLE memos ADD COLUMN remind_at TEXT NOT NULL DEFAULT '';"),
    (3, "ALTER TABLE memos ADD COLUMN images TEXT NOT NULL DEFAULT '';"),
    (4, "UPDATE memos SET sort_order = (
            SELECT COUNT(*) FROM memos AS m2
            WHERE m2.is_trashed = 0 AND m2.created_at > memos.created_at
         ) WHERE is_trashed = 0;"),
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
                images TEXT NOT NULL DEFAULT ''
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
        })
    }

    pub fn get_all(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images
             FROM memos WHERE is_trashed = 0
             ORDER BY is_pinned DESC, sort_order ASC",
        )?;
        let rows = stmt.query_map([], Self::row_to_memo)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get_trashed(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images
             FROM memos WHERE is_trashed = 1
             ORDER BY trashed_at DESC",
        )?;
        let rows = stmt.query_map([], Self::row_to_memo)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn insert(&self, content: &str) -> Result<Memo, Box<dyn std::error::Error>> {
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
            "INSERT INTO memos (id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images)
             VALUES (?1, ?2, ?3, ?4, '', 0, 0, ?5, 0, '', '', '')",
            params![id, content, now, now, min_order - 1],
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

    pub fn set_reminder(&self, id: &str, remind_at: &str) -> Result<(), Box<dyn std::error::Error>> {
        // 校验格式必须为 YYYY-MM-DD HH:MM:SS，否则字典序比较会失效
        if !Self::is_valid_datetime(remind_at) {
            return Err(format!("invalid remind_at format: '{}', expected YYYY-MM-DD HH:MM:SS", remind_at).into());
        }
        self.conn.execute(
            "UPDATE memos SET remind_at = ?1 WHERE id = ?2",
            params![remind_at, id],
        )?;
        Ok(())
    }

    /// 校验时间字符串是否为合法的 YYYY-MM-DD HH:MM:SS 格式
    fn is_valid_datetime(s: &str) -> bool {
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
        self.conn.execute(
            "UPDATE memos SET remind_at = '' WHERE id = ?1",
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

    /// 查询所有已到期的提醒（不清除），由调用方在通知发送成功后调用 clear_reminders 清除
    pub fn due_reminders(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images
             FROM memos
             WHERE remind_at != '' AND remind_at <= ?1 AND is_trashed = 0 AND is_done = 0
             ORDER BY remind_at ASC",
        )?;
        let rows = stmt.query_map(params![now], Self::row_to_memo)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn clear_reminders(&self, ids: &[String]) -> Result<(), Box<dyn std::error::Error>> {
        for id in ids {
            self.conn.execute(
                "UPDATE memos SET remind_at = '' WHERE id = ?1",
                params![id],
            )?;
        }
        Ok(())
    }

    pub fn auto_trash(&self) -> Result<u32, Box<dyn std::error::Error>> {
        let cutoff = (Local::now() - Duration::days(3))
            .format("%Y-%m-%d 00:00:00")
            .to_string();
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        // created_at 和 updated_at 都超过 3 天才清理，避免持续编辑的老便签被误删
        let affected = self.conn.execute(
            "UPDATE memos SET is_trashed = 1, trashed_at = ?1
             WHERE is_pinned = 0 AND is_trashed = 0 AND created_at < ?2 AND updated_at < ?2",
            params![now, cutoff],
        )?;
        Ok(affected as u32)
    }
}
