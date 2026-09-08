use crate::database::DbState;
use crate::models::Expense;
use rusqlite::Result;

pub fn add_expense(
    db: &DbState,
    category_id: i64,
    amount: i64,
    payment_method: &str,
    session_id: Option<i64>,
    user_id: i64,
    recipient: Option<String>,
    receipt_reference: Option<String>,
    notes: Option<String>,
    date: Option<String>,
) -> Result<String, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let now = chrono::Local::now();
    let expense_number = format!("EXP-{}", now.format("%Y%m%d%H%M%S"));
    // Caller-provided expense date; defaults to today.
    let today_date = date
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| now.format("%Y-%m-%d").to_string());

    tx.execute(
        "INSERT INTO expenses (expense_number, category_id, amount, payment_method, session_id, user_id, recipient, receipt_reference, date, notes, terminal_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            expense_number, category_id, amount, payment_method,
            session_id, user_id, recipient, receipt_reference, today_date, notes,
            crate::network::current_stamp_terminal()
        ],
    )
    .map_err(|e| e.to_string())?;

    let expense_id = tx.last_insert_rowid();

    // If paid from active cash session drawer, deduct from cash movements
    if payment_method == "cash" {
        if let Some(sid) = session_id {
            tx.execute(
                "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, reference_type, reference_id)
                 VALUES (?1, ?2, 'expense_payment', ?3, ?4, 'expense', ?5)",
                rusqlite::params![
                    sid, user_id, -amount,
                    format!("Expense Payment / دفع مصروف {}", expense_number), expense_id
                ],
            )
            .map_err(|e| e.to_string())?;

            tx.execute(
                "UPDATE cash_sessions SET expected_cash = expected_cash - ?1 WHERE id = ?2",
                rusqlite::params![amount, sid],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    {
        let lang = crate::services::notifier_service::ui_language(db);
        let actor = crate::services::notifier_service::actor_label(db, Some(user_id));
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("💸 *Expense* {}\n💰 Amount: *{} DZD*\n👤 By: {}", expense_number, amount, actor),
                format!("💸 *مصروف* {}\n💰 المبلغ: *{} دج*\n👤 بواسطة: {}", expense_number, amount, actor),
                format!("💸 *Dépense* {}\n💰 Montant : *{} DZD*\n👤 Par : {}", expense_number, amount, actor),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_each_expense", text);
    }

    Ok(expense_number)
}

pub fn list_expenses(db: &DbState) -> Result<Vec<Expense>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT e.id, e.expense_number, e.category_id, ec.name_ar, e.amount,
                    e.payment_method, e.session_id, e.user_id, e.recipient, e.receipt_reference,
                    e.date, e.notes, e.created_at, u.display_name, COALESCE(e.terminal_name, '')
             FROM expenses e
             LEFT JOIN expense_categories ec ON e.category_id = ec.id
             LEFT JOIN users u ON e.user_id = u.id
             ORDER BY e.id DESC LIMIT 200",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Expense {
                id: row.get(0)?,
                user_name: row.get::<_, Option<String>>(13)?,
                expense_number: row.get(1)?,
                category_id: row.get(2)?,
                category_name: row.get(3)?,
                amount: row.get(4)?,
                payment_method: row.get(5)?,
                session_id: row.get(6)?,
                user_id: row.get(7)?,
                recipient: row.get(8)?,
                receipt_reference: row.get(9)?,
                date: row.get(10)?,
                notes: row.get(11)?,
                created_at: row.get(12)?,
            terminal_name: {
                let t: String = row.get(14)?;
                if t.is_empty() { None } else { Some(t) }
            },
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Expense> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

/// Reverse the drawer effect of one expense: restore expected_cash and
/// remove its linked cash_movements row (reference_type='expense').
/// Caller must hold an open transaction. Returns the old expense row
/// (amount, payment_method, session_id) for rebooking decisions.
fn reverse_expense_cash(tx: &rusqlite::Transaction, expense_id: i64) -> Result<(i64, String, Option<i64>), String> {
    let old: (i64, String, Option<i64>) = tx
        .query_row(
            "SELECT amount, payment_method, session_id FROM expenses WHERE id = ?1",
            [expense_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;
    let (old_amount, old_method, old_session) = &old;
    if old_method == "cash" {
        if let Some(sid) = old_session {
            // Restore the drawer the original payment deducted.
            tx.execute(
                "UPDATE cash_sessions SET expected_cash = expected_cash + ?1 WHERE id = ?2",
                rusqlite::params![old_amount, sid],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "DELETE FROM cash_movements
                 WHERE reference_type = 'expense' AND reference_id = ?1 AND type = 'expense_payment'",
                [expense_id],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(old)
}

pub fn delete_expense(db: &DbState, expense_id: i64) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    // The drawer must give the money back: reverse the linked movement and
    // expected_cash BEFORE removing the row (field-found: deleting an
    // expense left the register and the session totals stale).
    let _ = reverse_expense_cash(&tx, expense_id)?;
    tx.execute("DELETE FROM expenses WHERE id = ?1", [expense_id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

/// Edit a voucher IN PLACE (the replace_sale pattern): reverse the old
/// drawer effect, rewrite the same row (same id/number — never a
/// duplicate), stamp MODIFIED + timestamp into notes, and rebook the cash
/// movement for the NEW values. The expense keeps its ORIGINAL date, so a
/// yesterday-expense edited today counts on yesterday (dashboard groups
/// by expenses.date) — no leakage into today's totals.
pub fn update_expense(
    db: &DbState,
    expense_id: i64,
    category_id: i64,
    amount: i64,
    payment_method: &str,
    recipient: Option<String>,
    receipt_reference: Option<String>,
    notes: Option<String>,
    date: String,
) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let (_old_amount, _old_method, _old_session) = reverse_expense_cash(&tx, expense_id)?;

    tx.execute(
        "UPDATE expenses SET
                category_id = ?2, amount = ?3, payment_method = ?4, recipient = ?5,
                receipt_reference = ?6,
                notes = COALESCE(COALESCE(?7, notes), '') || ' | MODIFIED ' || datetime('now','localtime'),
                date = ?8, session_id = CASE WHEN ?4 = 'cash' THEN COALESCE(session_id, (SELECT id FROM cash_sessions WHERE status = 'open' ORDER BY id DESC LIMIT 1)) ELSE NULL END
         WHERE id = ?1",
        rusqlite::params![
            expense_id, category_id, amount, payment_method, recipient,
            receipt_reference, notes, date
        ],
    )
    .map_err(|e| e.to_string())?;

    // Rebook the drawer for the new values (cash + an open session).
    if payment_method == "cash" {
        let sid: Option<i64> = tx
            .query_row(
                "SELECT session_id FROM expenses WHERE id = ?1",
                [expense_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if let Some(sid) = sid {
            let user_id: i64 = tx
                .query_row(
                    "SELECT COALESCE(user_id, 1) FROM expenses WHERE id = ?1",
                    [expense_id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let number: String = tx
                .query_row("SELECT expense_number FROM expenses WHERE id = ?1", [expense_id], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, reference_type, reference_id, terminal_name)
                 VALUES (?1, ?2, 'expense_payment', ?3, ?4, 'expense', ?5, ?6)",
                rusqlite::params![
                    sid, user_id, -amount,
                    format!("Expense Payment / دفع مصروف {}", number), expense_id,
                    crate::network::current_stamp_terminal()
                ],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE cash_sessions SET expected_cash = expected_cash - ?1 WHERE id = ?2",
                rusqlite::params![amount, sid],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod edit_delete_tests {
    use super::*;

    fn fresh_db() -> DbState {
        let dir = std::env::temp_dir().join("titaou_expense_tests");
        let _ = std::fs::create_dir_all(&dir);
        // Unique per test invocation: tests run in parallel threads and a
        // shared filename makes two fixtures race on the same SQLite file.
        let path = dir.join(format!(
            "exp_{}_{}.sqlite",
            std::process::id(),
            std::thread::current().name().unwrap_or("t").replace("::", "_")
        ));
        let _ = std::fs::remove_file(&path);
        let state = DbState { conn: std::sync::Mutex::new(rusqlite::Connection::open(&path).unwrap()) };
        state.run_migrations().unwrap();
        state.seed_default_admin().unwrap();
        {
            let conn = state.conn.lock().unwrap();
            // FK target for cash_sessions.register_id.
            let _ = conn.execute(
                "INSERT OR IGNORE INTO registers (id, name, identifier) VALUES (1, 'Caisse 1', 'REG1')",
                [],
            );
        }
        state
    }

    fn cash_movements_total(db: &DbState, session_id: i64) -> i64 {
        let conn = db.conn.lock().unwrap();
        conn.query_row(
            "SELECT COALESCE(SUM(amount), 0) FROM cash_movements WHERE session_id = ?1 AND type = 'expense_payment'",
            [session_id],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    fn expected_cash(db: &DbState, session_id: i64) -> i64 {
        let conn = db.conn.lock().unwrap();
        conn.query_row("SELECT expected_cash FROM cash_sessions WHERE id = ?1", [session_id], |r| r.get(0))
            .unwrap_or(0)
    }

    #[test]
    fn edit_updates_in_place_and_rebooks_drawer() {
        let db = fresh_db();
        // Open a cash session with 10 000.
        crate::services::cash_service::open_session(&db, 1, 1, 10000, None).unwrap();
        let session_id: i64 = {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT id FROM cash_sessions WHERE status = 'open' ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap()
        };
        // Yesterday's expense of 5000 paid cash.
        let yesterday = chrono::Local::now() - chrono::Duration::days(1);
        let number = add_expense(&db, 1, 5000, "cash", Some(session_id), 1, None, None, None,
            Some(yesterday.format("%Y-%m-%d").to_string())).unwrap();
        let expense_id: i64 = {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT id FROM expenses WHERE expense_number = ?1", [&number], |r| r.get(0)).unwrap()
        };

        // EDIT: 5000 -> 6000 (same date, yesterday).
        update_expense(&db, expense_id, 1, 6000, "cash", None, None, None,
            yesterday.format("%Y-%m-%d").to_string()).unwrap();

        let conn = db.conn.lock().unwrap();
        // Exactly ONE row (no duplicate!), amount updated, MODIFIED stamped,
        // date kept on yesterday.
        let (count, amount, date, notes): (i64, i64, String, Option<String>) = conn
            .query_row("SELECT COUNT(*), MIN(amount), MIN(date), MIN(notes) FROM expenses WHERE expense_number = ?1", [&number], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
            .unwrap();
        assert_eq!(count, 1, "edit must not duplicate the expense");
        assert_eq!(amount, 6000);
        assert_eq!(date, yesterday.format("%Y-%m-%d").to_string(), "date must stay on yesterday");
        assert!(notes.unwrap_or_default().contains("MODIFIED"), "MODIFIED tag missing");
        drop(conn);

        // Drawer: exactly ONE movement of -6000 (old -5000 reversed, new booked).
        assert_eq!(cash_movements_total(&db, session_id), -6000);
        // expected_cash = 10000 - 6000.
        assert_eq!(expected_cash(&db, session_id), 4000);
    }

    #[test]
    fn delete_reverses_the_drawer() {
        let db = fresh_db();
        crate::services::cash_service::open_session(&db, 1, 1, 10000, None).unwrap();
        let session_id: i64 = {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT id FROM cash_sessions WHERE status = 'open' ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).unwrap()
        };
        let number = add_expense(&db, 1, 3000, "cash", Some(session_id), 1, None, None, None, None).unwrap();
        let expense_id: i64 = {
            let conn = db.conn.lock().unwrap();
            conn.query_row("SELECT id FROM expenses WHERE expense_number = ?1", [&number], |r| r.get(0)).unwrap()
        };
        assert_eq!(expected_cash(&db, session_id), 7000);

        delete_expense(&db, expense_id).unwrap();

        // Drawer fully refunded: no movement left, expected_cash restored.
        assert_eq!(cash_movements_total(&db, session_id), 0);
        assert_eq!(expected_cash(&db, session_id), 10000);
        let conn = db.conn.lock().unwrap();
        let remaining: i64 = conn.query_row("SELECT COUNT(*) FROM expenses WHERE id = ?1", [expense_id], |r| r.get(0)).unwrap();
        assert_eq!(remaining, 0);
    }
}
