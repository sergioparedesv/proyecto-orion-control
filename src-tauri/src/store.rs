//! Persistencia mínima de F2: redes, líneas base y ajustes en SQLite.
//! El resto del esquema del §6.4.1 (samples_1m, events, bufferbloat_runs, actions_log) llega en F3.

use std::path::Path;

use orion_engine::{Baseline, BaselineKey, BaselineScope};
use rusqlite::{params, Connection, OptionalExtension};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS networks (
    id INTEGER PRIMARY KEY,
    ssid_hash TEXT,
    gateway_ip TEXT,
    kind TEXT,
    first_seen INTEGER,
    last_seen INTEGER
);
CREATE TABLE IF NOT EXISTS baselines (
    network_id INTEGER NOT NULL,
    target_id TEXT NOT NULL,
    slot INTEGER NOT NULL,
    mu REAL NOT NULL,
    var REAL NOT NULL,
    n INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    PRIMARY KEY (network_id, target_id, slot)
);
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn })
    }

    pub fn upsert_network(&self, id: i64, ssid_hash: Option<&str>, gateway: Option<&str>, kind: &str, now: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO networks (id, ssid_hash, gateway_ip, kind, first_seen, last_seen) VALUES (?1, ?2, ?3, ?4, ?5, ?5)
             ON CONFLICT(id) DO UPDATE SET last_seen = excluded.last_seen, gateway_ip = excluded.gateway_ip",
            params![id, ssid_hash, gateway, kind, now],
        )?;
        Ok(())
    }

    pub fn load_baselines(&self) -> rusqlite::Result<Vec<(BaselineKey, Baseline)>> {
        let mut stmt = self.conn.prepare("SELECT network_id, target_id, slot, mu, var, n FROM baselines")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                BaselineKey { network_id: r.get(0)?, target_id: r.get(1)?, slot: r.get::<_, i64>(2)? as u8 },
                Baseline { mu: r.get(3)?, var: r.get(4)?, n: r.get::<_, i64>(5)? as u32 },
            ))
        })?;
        rows.collect()
    }

    pub fn save_baselines<'a>(&mut self, items: impl Iterator<Item = (&'a BaselineKey, &'a Baseline)>, now: i64) -> rusqlite::Result<usize> {
        let tx = self.conn.transaction()?;
        let mut n = 0;
        {
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO baselines (network_id, target_id, slot, mu, var, n, updated) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for (k, b) in items {
                stmt.execute(params![k.network_id, k.target_id, k.slot as i64, b.mu, b.var, b.n as i64, now])?;
                n += 1;
            }
        }
        tx.commit()?;
        Ok(n)
    }

    pub fn delete_baselines(&self, scope: &BaselineScope, network_id: i64) -> rusqlite::Result<usize> {
        match scope {
            BaselineScope::All => self.conn.execute("DELETE FROM baselines", []),
            BaselineScope::Network => self.conn.execute("DELETE FROM baselines WHERE network_id = ?1", params![network_id]),
            BaselineScope::Target { target_id } => self
                .conn
                .execute("DELETE FROM baselines WHERE network_id = ?1 AND target_id = ?2", params![network_id, target_id]),
        }
    }

    pub fn get_setting(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).optional()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baselines_roundtrip_and_scoped_delete() {
        let mut s = Store::in_memory().unwrap();
        let k1 = BaselineKey { network_id: 1, target_id: "relay:lim".into(), slot: 5 };
        let k2 = BaselineKey { network_id: 2, target_id: "gateway".into(), slot: 0 };
        let b = Baseline { mu: 41.5, var: 2.25, n: 320 };
        s.save_baselines([(&k1, &b), (&k2, &b)].into_iter(), 0).unwrap();
        let loaded = s.load_baselines().unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(loaded.contains(&(k1.clone(), b)));
        s.delete_baselines(&BaselineScope::Network, 1).unwrap();
        assert_eq!(s.load_baselines().unwrap(), vec![(k2, b)]);
    }

    #[test]
    fn settings_kv() {
        let s = Store::in_memory().unwrap();
        assert_eq!(s.get_setting("x").unwrap(), None);
        s.set_setting("x", "1").unwrap();
        s.set_setting("x", "2").unwrap();
        assert_eq!(s.get_setting("x").unwrap().as_deref(), Some("2"));
    }
}
