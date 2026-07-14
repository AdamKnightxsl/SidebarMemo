use chrono::{Local, Duration};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MIGRATIONS: &[(i32, &str)] = &[
    (1, "ALTER TABLE memos ADD COLUMN is_trashed INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE memos ADD COLUMN trashed_at TEXT NOT NULL DEFAULT '';"),
    (2, "ALTER TABLE memos ADD COLUMN remind_at TEXT NOT NULL DEFAULT '';"),
    (3, "ALTER TABLE memos ADD COLUMN images TEXT NOT NULL DEFAULT '';"),
];

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
                let version = if has_remind { "2" } else { "1" };
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
                conn.execute_batch(sql)?;
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
             ORDER BY is_pinned DESC, sort_order ASC, created_at DESC",
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
        let max_order: i32 = self
            .conn
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) FROM memos WHERE is_pinned = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(-1);

        self.conn.execute(
            "INSERT INTO memos (id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images)
             VALUES (?1, ?2, ?3, ?4, '', 0, 0, ?5, 0, '', '', '')",
            params![id, content, now, now, max_order + 1],
        )?;

        Ok(Memo {
            id,
            content: content.to_string(),
            created_at: now.clone(),
            updated_at: now,
            color: String::new(),
            is_pinned: false,
            is_done: false,
            sort_order: max_order + 1,
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
        self.conn.execute(
            "UPDATE memos SET is_trashed = 1, trashed_at = ?1 WHERE id = ?2",
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
        Ok(())
    }

    pub fn clear_trashed(&self) -> Result<u32, Box<dyn std::error::Error>> {
        let affected = self.conn.execute("DELETE FROM memos WHERE is_trashed = 1", [])?;
        Ok(affected as u32)
    }

    pub fn set_reminder(&self, id: &str, remind_at: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute(
            "UPDATE memos SET remind_at = ?1 WHERE id = ?2",
            params![remind_at, id],
        )?;
        Ok(())
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

    pub fn take_due_reminders(&self) -> Result<Vec<Memo>, Box<dyn std::error::Error>> {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut stmt = self.conn.prepare(
            "SELECT id, content, created_at, updated_at, color, is_pinned, is_done, sort_order, is_trashed, trashed_at, remind_at, images
             FROM memos
             WHERE remind_at != '' AND remind_at <= ?1 AND is_trashed = 0 AND is_done = 0
             ORDER BY remind_at ASC",
        )?;
        let rows = stmt.query_map(params![now], Self::row_to_memo)?;
        let memos = rows.collect::<Result<Vec<_>, _>>()?;
        for memo in &memos {
            self.conn.execute(
                "UPDATE memos SET remind_at = '' WHERE id = ?1",
                params![memo.id],
            )?;
        }
        Ok(memos)
    }

    pub fn auto_trash(&self) -> Result<u32, Box<dyn std::error::Error>> {
        let cutoff = (Local::now() - Duration::days(3))
            .format("%Y-%m-%d 00:00:00")
            .to_string();
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let affected = self.conn.execute(
            "UPDATE memos SET is_trashed = 1, trashed_at = ?1
             WHERE is_pinned = 0 AND is_trashed = 0 AND created_at < ?2",
            params![now, cutoff],
        )?;
        Ok(affected as u32)
    }
}
