//! Transactional outbox: business services call [enqueue] INSIDE their write
//! transaction, so a rolled-back sale never syncs. The pusher (push.rs)
//! drains pending rows on the coordinator; CRM RPCs are idempotent on
//! pos_ref, so a crash between "CRM wrote" and "row marked done" replays
//! safely.

use rusqlite::{params, Connection, OptionalExtension};

/// What kind of entity event this is + the payload the pusher will send to
/// the CRM RPC. Payloads are JSON strings built by the service at enqueue
/// time (they snapshot the transaction's committed values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutboxEntity {
    Sale,
    Refund,
    Purchase,
    Product,
    StockAdjustment,
    Customer,
    CustomerPayment,
    Supplier,
}

impl OutboxEntity {
    pub fn as_str(self) -> &'static str {
        match self {
            OutboxEntity::Sale => "sale",
            OutboxEntity::Refund => "refund",
            OutboxEntity::Purchase => "purchase",
            OutboxEntity::Product => "product",
            OutboxEntity::StockAdjustment => "stock_adjustment",
            OutboxEntity::Customer => "customer",
            OutboxEntity::CustomerPayment => "customer_payment",
            OutboxEntity::Supplier => "supplier",
        }
    }

    pub fn parse(s: &str) -> Option<OutboxEntity> {
        Some(match s {
            "sale" => OutboxEntity::Sale,
            "refund" => OutboxEntity::Refund,
            "purchase" => OutboxEntity::Purchase,
            "product" => OutboxEntity::Product,
            "stock_adjustment" => OutboxEntity::StockAdjustment,
            "customer" => OutboxEntity::Customer,
            "customer_payment" => OutboxEntity::CustomerPayment,
            "supplier" => OutboxEntity::Supplier,
            _ => return None,
        })
    }
}

/// Enqueue an event inside the caller's open transaction. NEVER call this
/// outside a transaction — that's the whole point of the outbox.
pub fn enqueue_tx(
    tx: &Connection,
    entity: OutboxEntity,
    local_id: i64,
    payload: &str,
) -> rusqlite::Result<()> {
    tx.execute(
        "INSERT INTO sync_outbox (entity, local_id, payload, status)
         VALUES (?1, ?2, ?3, 'pending')",
        params![entity.as_str(), local_id, payload],
    )?;
    Ok(())
}

/// Pending rows in order (oldest first) — the pusher drains these.
pub fn pending(tx: &Connection, limit: i64) -> rusqlite::Result<Vec<PendingEvent>> {
    let mut stmt = tx.prepare(
        "SELECT id, entity, local_id, payload, attempts FROM sync_outbox
          WHERE status = 'pending' ORDER BY id LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| {
        Ok(PendingEvent {
            outbox_id: r.get(0)?,
            entity: r.get::<_, String>(1)?,
            local_id: r.get::<_, i64>(2)?,
            payload: r.get(3)?,
            attempts: r.get(4)?,
        })
    })?;
    rows.collect()
}

#[derive(Debug, Clone)]
pub struct PendingEvent {
    pub outbox_id: i64,
    pub entity: String,
    pub local_id: i64,
    pub payload: String,
    pub attempts: i64,
}

/// Mark a row done (CRM accepted).
pub fn mark_done(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_outbox SET status = 'done', pushed_at = datetime('now','localtime'), last_error = NULL
          WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

/// Mark permanently failed (4xx the RPC will never accept) — surfaced in the
/// Cloud Sync status panel. Transient errors leave the row pending for retry.
pub fn mark_failed(conn: &Connection, id: i64, error: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_outbox SET status = 'failed', attempts = attempts + 1, last_error = ?2
          WHERE id = ?1",
        params![id, error],
    )?;
    Ok(())
}

/// Bump attempts + record the error, but keep the row pending (transient).
pub fn record_retry(conn: &Connection, id: i64, error: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE sync_outbox SET attempts = attempts + 1, last_error = ?2
          WHERE id = ?1",
        params![id, error],
    )?;
    Ok(())
}

/// Count of pending rows (status panel).
pub fn pending_count(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM sync_outbox WHERE status = 'pending'",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Count of failed rows (status panel).
pub fn failed_count(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM sync_outbox WHERE status = 'failed'",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// sync_map: remember that a local entity is linked to a CRM uuid.
pub fn map_local(conn: &Connection, entity: &str, local_id: i64, remote_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO sync_map (entity, local_id, remote_id) VALUES (?1, ?2, ?3)",
        params![entity, local_id, remote_id],
    )?;
    Ok(())
}

/// sync_map lookup: CRM uuid for a local id (None when not linked yet).
pub fn remote_id(conn: &Connection, entity: &str, local_id: i64) -> Option<String> {
    conn.query_row(
        "SELECT remote_id FROM sync_map WHERE entity = ?1 AND local_id = ?2",
        params![entity, local_id],
        |r| r.get(0),
    )
    .optional()
    .unwrap_or(None)
}

/// sync_map reverse lookup: local id for a CRM uuid.
pub fn local_id(conn: &Connection, entity: &str, remote_id: &str) -> Option<i64> {
    conn.query_row(
        "SELECT local_id FROM sync_map WHERE entity = ?1 AND remote_id = ?2",
        params![entity, remote_id],
        |r| r.get(0),
    )
    .optional()
    .unwrap_or(None)
}

/// Persisted pull cursor (ISO timestamp of the last row seen per stream).
pub fn get_cursor(conn: &Connection, stream: &str) -> Option<String> {
    conn.query_row(
        "SELECT cursor_value FROM sync_cursors WHERE stream = ?1",
        params![stream],
        |r| r.get(0),
    )
    .optional()
    .unwrap_or(None)
}

pub fn set_cursor(conn: &Connection, stream: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO sync_cursors (stream, cursor_value) VALUES (?1, ?2)",
        params![stream, value],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sync_outbox (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity TEXT NOT NULL,
                local_id INTEGER,
                payload TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                created_at TEXT,
                pushed_at TEXT
            );
            CREATE TABLE sync_map (
                entity TEXT NOT NULL,
                local_id INTEGER NOT NULL,
                remote_id TEXT NOT NULL,
                PRIMARY KEY (entity, local_id)
            );
            CREATE UNIQUE INDEX idx_sync_map_remote ON sync_map(entity, remote_id);
            CREATE TABLE sync_cursors (stream TEXT PRIMARY KEY, cursor_value TEXT NOT NULL);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn enqueue_and_pending_order() {
        let conn = mem();
        enqueue_tx(&conn, OutboxEntity::Sale, 5, "{}").unwrap();
        enqueue_tx(&conn, OutboxEntity::Product, 7, "{}").unwrap();
        let p = pending(&conn, 10).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].entity, "sale");
        assert_eq!(p[1].local_id, 7);
        assert_eq!(pending_count(&conn), 2);
    }

    #[test]
    fn done_retries_and_failures() {
        let conn = mem();
        enqueue_tx(&conn, OutboxEntity::Sale, 1, "{}").unwrap();
        let id = pending(&conn, 1).unwrap()[0].outbox_id;
        record_retry(&conn, id, "timeout").unwrap();
        assert_eq!(pending(&conn, 10).unwrap()[0].attempts, 1);
        mark_done(&conn, id).unwrap();
        assert_eq!(pending_count(&conn), 0);

        enqueue_tx(&conn, OutboxEntity::Refund, 2, "{}").unwrap();
        let id2 = pending(&conn, 1).unwrap()[0].outbox_id;
        mark_failed(&conn, id2, "400 bad").unwrap();
        assert_eq!(pending_count(&conn), 0);
        assert_eq!(failed_count(&conn), 1);
    }

    #[test]
    fn sync_map_roundtrip() {
        let conn = mem();
        map_local(&conn, "product", 12, "uuid-a").unwrap();
        assert_eq!(remote_id(&conn, "product", 12).as_deref(), Some("uuid-a"));
        assert_eq!(local_id(&conn, "product", "uuid-a"), Some(12));
        assert_eq!(remote_id(&conn, "product", 99), None);
        // Remap replaces cleanly.
        map_local(&conn, "product", 12, "uuid-b").unwrap();
        assert_eq!(remote_id(&conn, "product", 12).as_deref(), Some("uuid-b"));
    }

    #[test]
    fn cursors() {
        let conn = mem();
        assert_eq!(get_cursor(&conn, "clients"), None);
        set_cursor(&conn, "clients", "2026-09-08T10:00:00Z").unwrap();
        set_cursor(&conn, "clients", "2026-09-08T11:00:00Z").unwrap();
        assert_eq!(
            get_cursor(&conn, "clients").as_deref(),
            Some("2026-09-08T11:00:00Z")
        );
    }
}
