use crate::database::DbState;
use crate::models::{CashMovement, CashSession};
use rusqlite::Result;

pub fn get_active_session(db: &DbState, _user_id: i64) -> Result<Option<CashSession>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT cs.id, cs.register_id, cs.user_id, u.display_name, cs.opened_at, cs.closed_at,
                    cs.opening_amount, cs.expected_cash, cs.actual_cash, cs.difference, cs.status, cs.notes,
                    COALESCE(cs.terminal_name, '')
             FROM cash_sessions cs
             LEFT JOIN users u ON cs.user_id = u.id
             WHERE cs.status = 'open'
             ORDER BY cs.id DESC LIMIT 1",
        )
        .map_err(|e| e.to_string())?;

    let session = match stmt.query_row([], |row| {
        Ok(CashSession {
            id: row.get(0)?,
            register_id: row.get(1)?,
            user_id: row.get(2)?,
            user_name: row.get(3)?,
            opened_at: row.get(4)?,
            closed_at: row.get(5)?,
            opening_amount: row.get(6)?,
            expected_cash: row.get(7)?,
            actual_cash: row.get(8)?,
            difference: row.get(9)?,
            total_sales: Some(0),
            total_expenses: Some(0),
            current_balance: Some(row.get(7)?),
            status: row.get(10)?,
            notes: row.get(11)?,
            is_stale: None,
            is_archived: false,
            terminal_name: {
                let t: String = row.get(12)?;
                if t.is_empty() { None } else { Some(t) }
            },
        })
    }) {
        Ok(s) => Some(s),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.to_string()),
    };
    drop(stmt);
    drop(conn);

    finish_active_session(db, session)
}

/// LAN mode: the same active-session lookup but scoped to ONE register so
/// two terminals never see (or auto-close) each other's session. The API
/// layer maps every calling terminal to its own register row.
pub fn get_active_session_for_register(db: &DbState, register_id: i64) -> Result<Option<CashSession>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT cs.id, cs.register_id, cs.user_id, u.display_name, cs.opened_at, cs.closed_at,
                    cs.opening_amount, cs.expected_cash, cs.actual_cash, cs.difference, cs.status, cs.notes,
                    COALESCE(cs.terminal_name, '')
             FROM cash_sessions cs
             LEFT JOIN users u ON cs.user_id = u.id
             WHERE cs.status = 'open' AND cs.register_id = ?1
             ORDER BY cs.id DESC LIMIT 1",
        )
        .map_err(|e| e.to_string())?;

    let session = match stmt.query_row([register_id], |row| {
        Ok(CashSession {
            id: row.get(0)?,
            register_id: row.get(1)?,
            user_id: row.get(2)?,
            user_name: row.get(3)?,
            opened_at: row.get(4)?,
            closed_at: row.get(5)?,
            opening_amount: row.get(6)?,
            expected_cash: row.get(7)?,
            actual_cash: row.get(8)?,
            difference: row.get(9)?,
            total_sales: Some(0),
            total_expenses: Some(0),
            current_balance: Some(row.get(7)?),
            status: row.get(10)?,
            notes: row.get(11)?,
            is_stale: None,
            is_archived: false,
            terminal_name: {
                let t: String = row.get(12)?;
                if t.is_empty() { None } else { Some(t) }
            },
        })
    }) {
        Ok(s) => Some(s),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => return Err(e.to_string()),
    };
    drop(stmt);
    drop(conn);

    finish_active_session(db, session)
}

/// Shared tail of the active-session lookups: midnight-rollover auto-close
/// plus sales/expense totals. (Extracted verbatim from the original
/// `get_active_session` so both lookup variants behave identically.)
fn finish_active_session(db: &DbState, session: Option<CashSession>) -> Result<Option<CashSession>, String> {
    match session {
        Some(mut s) => {
            // A session left open from a previous calendar day is stale: the
            // POS day ends at midnight, so it auto-closes here (cash counted
            // as expected) and the caller opens today's fresh session.
            // Closing the app must NEVER close a session.
            //
            // opened_at comes from SQLite CURRENT_TIMESTAMP — that is UTC.
            // Naively comparing it to the LOCAL date made sessions opened
            // between 00:00 and 00:59 local (still "yesterday" in UTC, e.g.
            // UTC+1 Algeria) look stale and instantly auto-close. Convert
            // the stored UTC time to local FIRST, then compare dates.
            let opened_local = chrono::NaiveDateTime::parse_from_str(
                &s.opened_at,
                "%Y-%m-%d %H:%M:%S",
            )
            .map(|naive| {
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(naive, chrono::Utc)
                    .with_timezone(&chrono::Local)
            })
            .or_else(|_| {
                chrono::DateTime::parse_from_rfc3339(&s.opened_at)
                    .map(|d| d.with_timezone(&chrono::Local))
            });
            let opened_date = match opened_local {
                Ok(dt) => dt.date_naive(),
                // Unparseable/absent timestamp: treat as TODAY so a fresh
                // session is never falsely closed.
                Err(_) => chrono::Local::now().date_naive(),
            };
            let is_stale = opened_date < chrono::Local::now().date_naive();

            if is_stale {
                // Previous-day session: close it now (midnight rollover);
                // the caller sees no active session and opens a fresh one.
                let conn = db.conn.lock().unwrap();
                let _ = conn.execute(
                    "UPDATE cash_sessions
                     SET closed_at = datetime('now','localtime'), actual_cash = expected_cash,
                         difference = 0, status = 'closed',
                         notes = COALESCE(notes, '') || ' | Auto-closed at midnight'
                     WHERE id = ?1 AND status = 'open'",
                    [s.id],
                );
                return Ok(None);
            }
            s.is_stale = Some(false);

            // Calculate total sales and expenses for this session
            let conn = db.conn.lock().unwrap();
            let sales_sum: i64 = conn.query_row(
                "SELECT COALESCE(SUM(amount), 0) FROM cash_movements WHERE session_id = ?1 AND type = 'cash_sale'",
                [s.id],
                |r| r.get(0),
            ).unwrap_or(0);

            let exp_sum: i64 = conn.query_row(
                "SELECT COALESCE(SUM(amount), 0) FROM cash_movements WHERE session_id = ?1 AND (type = 'expense_payment' OR type = 'cash_out')",
                [s.id],
                |r| r.get(0),
            ).unwrap_or(0);

            s.total_sales = Some(sales_sum);
            s.total_expenses = Some(exp_sum.abs());
            s.current_balance = Some(s.expected_cash);
            Ok(Some(s))
        }
        None => Ok(None),
    }
}

/// LAN mode: open a session for ONE register only — other terminals' open
/// sessions are left untouched (single-PC behavior still closes all).
pub fn open_session_for_register(db: &DbState, user_id: i64, register_id: i64, opening_amount: i64, notes: Option<String>) -> Result<CashSession, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "UPDATE cash_sessions SET status = 'closed', closed_at = datetime('now','localtime') WHERE status = 'open' AND register_id = ?1",
        [register_id],
    )
    .map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT INTO cash_sessions (register_id, user_id, opening_amount, expected_cash, status, notes, opened_at, terminal_name)
         VALUES (?1, ?2, ?3, ?3, 'open', ?4, datetime('now','localtime'), ?5)",
        rusqlite::params![register_id, user_id, opening_amount, notes, crate::network::current_stamp_terminal()],
    )
    .map_err(|e| e.to_string())?;

    let session_id = tx.last_insert_rowid();

    tx.execute(
        "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, notes, terminal_name)
         VALUES (?1, ?2, 'opening_balance', ?3, 'Startup Cash / رصيد افتتاحي', ?4, ?5)",
        rusqlite::params![session_id, user_id, opening_amount, notes, crate::network::current_stamp_terminal()],
    )
    .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;

    Ok(CashSession {
        id: session_id,
        register_id,
        user_id,
        user_name: Some("Cashier".to_string()),
        opened_at: chrono::Local::now().to_rfc3339(),
        closed_at: None,
        opening_amount,
        expected_cash: opening_amount,
        actual_cash: None,
        difference: None,
        total_sales: Some(0),
        total_expenses: Some(0),
        current_balance: Some(opening_amount),
        status: "open".to_string(),
        notes,
        is_stale: Some(false),
        is_archived: false,
        terminal_name: Some(crate::network::current_stamp_terminal()),
    })
}

pub fn open_session(db: &DbState, user_id: i64, register_id: i64, opening_amount: i64, notes: Option<String>) -> Result<CashSession, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // Close any previous open sessions
    let _ = tx.execute("UPDATE cash_sessions SET status = 'closed', closed_at = datetime('now','localtime') WHERE status = 'open'", []);

    // opened_at is written explicitly in LOCAL time — the schema default
    // (CURRENT_TIMESTAMP) stores UTC, which showed every timestamp one hour
    // behind on UTC+ machines and fed the midnight stale-check bug.
    tx.execute(
        "INSERT INTO cash_sessions (register_id, user_id, opening_amount, expected_cash, status, notes, opened_at, terminal_name)
         VALUES (?1, ?2, ?3, ?3, 'open', ?4, datetime('now','localtime'), ?5)",
        rusqlite::params![register_id, user_id, opening_amount, notes, crate::network::current_stamp_terminal()],
    )
    .map_err(|e| e.to_string())?;

    let session_id = tx.last_insert_rowid();

    tx.execute(
        "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, notes, terminal_name)
         VALUES (?1, ?2, 'opening_balance', ?3, 'Startup Cash / رصيد افتتاحي', ?4, ?5)",
        rusqlite::params![session_id, user_id, opening_amount, notes, crate::network::current_stamp_terminal()],
    )
    .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;

    Ok(CashSession {
        id: session_id,
        register_id,
        user_id,
        user_name: Some("Cashier".to_string()),
        opened_at: chrono::Local::now().to_rfc3339(),
        closed_at: None,
        opening_amount,
        expected_cash: opening_amount,
        actual_cash: None,
        difference: None,
        total_sales: Some(0),
        total_expenses: Some(0),
        current_balance: Some(opening_amount),
        status: "open".to_string(),
        notes,
        is_stale: Some(false),
        is_archived: false,
        terminal_name: Some(crate::network::current_stamp_terminal()),
    })
}

pub fn add_cash_movement(db: &DbState, session_id: i64, user_id: i64, movement_type: &str, amount: i64, reason: Option<String>) -> Result<(), String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let signed_amount = if movement_type == "cash_in" || movement_type == "opening_balance" || movement_type == "customer_debt_payment" {
        amount.abs()
    } else {
        -amount.abs()
    };

    let reason_for_log = reason.clone();
    tx.execute(
        "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, terminal_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![session_id, user_id, movement_type, signed_amount, reason, crate::network::current_stamp_terminal()],
    )
    .map_err(|e| e.to_string())?;

    tx.execute(
        "UPDATE cash_sessions SET expected_cash = expected_cash + ?1 WHERE id = ?2",
        rusqlite::params![signed_amount, session_id],
    )
    .map_err(|e| e.to_string())?;

    // A withdrawal from the drawer (POS drawer modal or register page) is
    // money leaving the shop: book it as an expense too, so the Expenses
    // page and dashboard statistics include it. Without this, cash_out
    // only showed on the register page.
    if movement_type == "cash_out" && amount > 0 {
        let now = chrono::Local::now();
        let expense_number = format!("EXP-{}", now.format("%Y%m%d%H%M%S"));
        tx.execute(
            "INSERT INTO expenses (expense_number, category_id, amount, payment_method, session_id, user_id, recipient, receipt_reference, date, notes, terminal_name)
             VALUES (?1, 6, ?2, 'cash', ?3, ?4, 'Cash Register', NULL, ?5, ?6, ?7)",
            rusqlite::params![
                expense_number,
                amount,
                session_id,
                user_id,
                now.format("%Y-%m-%d").to_string(),
                reason.unwrap_or_else(|| "Cash withdrawal from drawer / سحب نقدي من الصندوق".to_string()),
                crate::network::current_stamp_terminal(),
            ],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    // Telegram alerts for drawer money movement (fire-and-forget),
    // localized to the UI language and attributed to the acting user.
    {
        let lang = crate::services::notifier_service::ui_language(db);
        let actor = crate::services::notifier_service::actor_label(db, Some(user_id));
        let reason = reason_for_log.clone().unwrap_or_else(|| "-".to_string());
        let text = match movement_type {
            "cash_in" => crate::services::notifier_service::tr(
                &lang,
                (
                    format!("💰 *Cash In*\nAmount: *{} DZD*\nReason: {}\n👤 By: {}", amount, reason, actor),
                    format!("💰 *دخول نقدي*\nالمبلغ: *{} دج*\nالسبب: {}\n👤 بواسطة: {}", amount, reason, actor),
                    format!("💰 *Entrée Caisse*\nMontant : *{} DZD*\nMotif : {}\n👤 Par : {}", amount, reason, actor),
                ),
            ),
            "cash_out" => crate::services::notifier_service::tr(
                &lang,
                (
                    format!("💸 *Cash Out*\nAmount: *{} DZD*\nReason: {}\n👤 By: {}", amount, reason, actor),
                    format!("💸 *خروج نقدي*\nالمبلغ: *{} دج*\nالسبب: {}\n👤 بواسطة: {}", amount, reason, actor),
                    format!("💸 *Sortie Caisse*\nMontant : *{} DZD*\nMotif : {}\n👤 Par : {}", amount, reason, actor),
                ),
            ),
            "opening_balance" => crate::services::notifier_service::tr(
                &lang,
                (
                    format!("🏦 *Register Opened*\nOpening float: *{} DZD*\n👤 By: {}", amount, actor),
                    format!("🏦 *فتح الصندوق*\nرصيد البداية: *{} دج*\n👤 بواسطة: {}", amount, actor),
                    format!("🏦 *Ouverture Caisse*\nFond de caisse : *{} DZD*\n👤 Par : {}", amount, actor),
                ),
            ),
            _ => String::new(),
        };
        let switch = match movement_type {
            "cash_in" => Some("notify_cash_in"),
            "cash_out" => Some("notify_cash_out"),
            "opening_balance" => Some("notify_opening_cash"),
            _ => None,
        };
        if let (Some(sw), true) = (switch, !text.is_empty()) {
            crate::services::notifier_service::notify_if_enabled(db, sw, text);
        }
    }

    Ok(())
}

pub fn close_session(db: &DbState, session_id: i64, actual_cash: i64, notes: Option<String>) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    
    let expected: i64 = conn
        .query_row(
            "SELECT expected_cash FROM cash_sessions WHERE id = ?1",
            [session_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;

    let difference = actual_cash - expected;

    conn.execute(
        "UPDATE cash_sessions
         SET closed_at = datetime('now','localtime'), actual_cash = ?1, difference = ?2, status = 'closed', notes = ?3
         WHERE id = ?4",
        rusqlite::params![actual_cash, difference, notes, session_id],
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn list_movements(db: &DbState, session_id: i64) -> Result<Vec<CashMovement>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT cm.id, cm.session_id, cm.user_id, u.display_name, cm.type, cm.amount, cm.reason, cm.created_at, cm.notes,
                    COALESCE(cm.terminal_name, '')
             FROM cash_movements cm
             LEFT JOIN users u ON cm.user_id = u.id
             WHERE cm.session_id = ?1
             ORDER BY cm.id DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([session_id], |row| {
            Ok(CashMovement {
                id: row.get(0)?,
                session_id: row.get(1)?,
                user_id: row.get(2)?,
                user_name: row.get(3)?,
                type_name: row.get(4)?,
                amount: row.get(5)?,
                reason: row.get(6)?,
                created_at: row.get(7)?,
                notes: row.get(8)?,
                terminal_name: {
                    let t: String = row.get(9)?;
                    if t.is_empty() { None } else { Some(t) }
                },
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<CashMovement> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn list_session_history(
    db: &DbState,
    from_date: Option<String>,
    to_date: Option<String>,
    include_archived: Option<bool>,
) -> Result<Vec<CashSession>, String> {
    let conn = db.conn.lock().unwrap();
    let mut sql = String::from(
        "SELECT cs.id, cs.register_id, cs.user_id, u.display_name, cs.opened_at, cs.closed_at,
                cs.opening_amount, cs.expected_cash, cs.actual_cash, cs.difference, cs.status, cs.notes,
                COALESCE(cs.is_archived, 0), COALESCE(cs.terminal_name, '')
         FROM cash_sessions cs
         LEFT JOIN users u ON cs.user_id = u.id
         WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if !include_archived.unwrap_or(false) {
        sql.push_str(" AND COALESCE(cs.is_archived, 0) = 0");
    }

    if let Some(from) = from_date {
        if !from.trim().is_empty() {
            sql.push_str(" AND date(cs.opened_at) >= date(?)");
            params.push(Box::new(from));
        }
    }

    if let Some(to) = to_date {
        if !to.trim().is_empty() {
            sql.push_str(" AND date(cs.opened_at) <= date(?)");
            params.push(Box::new(to));
        }
    }

    sql.push_str(" ORDER BY cs.id DESC LIMIT 200");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())), |row| {
            let archived_int: i64 = row.get(12).unwrap_or(0);
            Ok(CashSession {
                id: row.get(0)?,
                register_id: row.get(1)?,
                user_id: row.get(2)?,
                user_name: row.get(3)?,
                opened_at: row.get(4)?,
                closed_at: row.get(5)?,
                opening_amount: row.get(6)?,
                expected_cash: row.get(7)?,
                actual_cash: row.get(8)?,
                difference: row.get(9)?,
                total_sales: None,
                total_expenses: None,
                current_balance: Some(row.get(7)?),
                status: row.get(10)?,
                notes: row.get(11)?,
                is_stale: None,
                is_archived: archived_int == 1,
                terminal_name: {
                    let t: String = row.get(13)?;
                    if t.is_empty() { None } else { Some(t) }
                },
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<CashSession> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn edit_opening_balance(
    db: &DbState,
    session_id: i64,
    new_amount: i64,
    reason: String,
    admin_password: Option<String>,
) -> Result<(), String> {
    if let Some(pwd) = &admin_password {
        if !crate::auth::verify_admin_password(db, pwd)? {
            return Err("Mot de passe administrateur incorrect / Invalid admin password".to_string());
        }
    }

    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let (old_opening, old_expected, actual_opt): (i64, i64, Option<i64>) = tx
        .query_row(
            "SELECT opening_amount, expected_cash, actual_cash FROM cash_sessions WHERE id = ?1",
            [session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| format!("Session introuvable: {}", e))?;

    let delta = new_amount - old_opening;
    let new_expected = old_expected + delta;
    let new_diff = actual_opt.map(|act| act - new_expected);

    tx.execute(
        "UPDATE cash_sessions
         SET opening_amount = ?1,
             expected_cash = ?2,
             difference = ?3,
             notes = COALESCE(notes, '') || ' | Solde ouv. modifié: ' || ?4 || ' DZD (' || ?5 || ')'
         WHERE id = ?6",
        rusqlite::params![new_amount, new_expected, new_diff, new_amount, reason, session_id],
    )
    .map_err(|e| e.to_string())?;

    // Update opening_balance cash_movement if exists, else insert audit movement
    let updated = tx.execute(
        "UPDATE cash_movements SET amount = ?1, reason = 'Opening balance updated: ' || ?2 WHERE session_id = ?3 AND type = 'opening_balance'",
        rusqlite::params![new_amount, reason, session_id],
    ).unwrap_or(0);

    if updated == 0 {
        let _ = tx.execute(
            "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, reference_type, reference_id)
             VALUES (?1, 1, 'opening_balance', ?2, ?3, 'adjustment', ?4)",
            rusqlite::params![session_id, delta, format!("Ajustement solde d'ouverture: {}", reason), session_id],
        );
    }

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    let lang = crate::services::notifier_service::ui_language(db);
    let text = crate::services::notifier_service::tr(
        &lang,
        (
            format!("⚠️ *Cash Session #{} Opening Balance Edited*\nOld: {} DZD ➔ New: {} DZD\nReason: {}", session_id, old_opening, new_amount, reason),
            format!("⚠️ *تعديل الرصيد الافتتاحي للجلسة #{}*\nالسابق: {} دج ➔ الجديد: {} دج\nالسبب: {}", session_id, old_opening, new_amount, reason),
            format!("⚠️ *Session #{} : Solde d'ouverture modifié*\nAncien : {} DZD ➔ Nouveau : {} DZD\nMotif : {}", session_id, old_opening, new_amount, reason),
        ),
    );
    crate::services::notifier_service::notify_if_enabled(db, "notify_cash_edited", text);

    Ok(())
}

pub fn edit_cash_session(
    db: &DbState,
    session_id: i64,
    opening_amount: Option<i64>,
    actual_cash: Option<i64>,
    notes: Option<String>,
    admin_password: Option<String>,
) -> Result<(), String> {
    if let Some(pwd) = &admin_password {
        if !crate::auth::verify_admin_password(db, pwd)? {
            return Err("Mot de passe administrateur incorrect / Invalid admin password".to_string());
        }
    }

    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let (curr_opening, curr_expected, curr_actual): (i64, i64, Option<i64>) = tx
        .query_row(
            "SELECT opening_amount, expected_cash, actual_cash FROM cash_sessions WHERE id = ?1",
            [session_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| format!("Session introuvable: {}", e))?;

    let final_opening = opening_amount.unwrap_or(curr_opening);
    let delta = final_opening - curr_opening;
    let final_expected = curr_expected + delta;
    let final_actual = actual_cash.or(curr_actual);
    let final_diff = final_actual.map(|act| act - final_expected);

    tx.execute(
        "UPDATE cash_sessions
         SET opening_amount = ?1,
             expected_cash = ?2,
             actual_cash = ?3,
             difference = ?4,
             notes = COALESCE(?5, notes)
         WHERE id = ?6",
        rusqlite::params![final_opening, final_expected, final_actual, final_diff, notes, session_id],
    )
    .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    let lang = crate::services::notifier_service::ui_language(db);
    let text = crate::services::notifier_service::tr(
        &lang,
        (
            format!("⚠️ *Cash Session #{} Details Edited by Admin*", session_id),
            format!("⚠️ *تم تعديل بيانات جلسة الصندوق #{} من قبل المسؤول*", session_id),
            format!("⚠️ *Session de caisse #{} modifiée par l'administrateur*", session_id),
        ),
    );
    crate::services::notifier_service::notify_if_enabled(db, "notify_cash_edited", text);

    Ok(())
}

pub fn archive_cash_session(
    db: &DbState,
    session_id: i64,
    archived: bool,
    admin_password: Option<String>,
) -> Result<(), String> {
    if let Some(pwd) = &admin_password {
        if !crate::auth::verify_admin_password(db, pwd)? {
            return Err("Mot de passe administrateur incorrect / Invalid admin password".to_string());
        }
    }

    let conn = db.conn.lock().unwrap();
    conn.execute(
        "UPDATE cash_sessions SET is_archived = ?1 WHERE id = ?2",
        rusqlite::params![if archived { 1 } else { 0 }, session_id],
    )
    .map_err(|e| e.to_string())?;
    drop(conn);

    // Audit/notify: archive hides the session but never destroys records.
    {
        let lang = crate::services::notifier_service::ui_language(db);
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("📦 *Cash Session #{} Archived*\nHidden from the active history list (records kept).", session_id),
                format!("📦 *أُرشفت جلسة الصندوق #{}*\nأُخفيت من قائمة السجل النشطة (البيانات محفوظة).", session_id),
                format!("📦 *Session de caisse #{} archivée*\nMasquée de l'historique actif (données conservées).", session_id),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_session_archived", text);
    }

    Ok(())
}

pub fn delete_cash_session(
    db: &DbState,
    session_id: i64,
    admin_password: Option<String>,
) -> Result<(), String> {
    if let Some(pwd) = &admin_password {
        if !crate::auth::verify_admin_password(db, pwd)? {
            return Err("Mot de passe administrateur incorrect / Invalid admin password".to_string());
        }
    }
    // Admin password is REQUIRED for permanent session deletion (the modal
    // always collects it; a missing password means a direct API bypass).
    if admin_password.as_deref().map(|p| p.trim().is_empty()).unwrap_or(true) {
        return Err("Admin password required / كلمة مرور المسؤول مطلوبة".to_string());
    }

    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // Sessions are referenced by sales, expenses, purchases' debt payments
    // and payrolls. Deleting only the cash_movements used to abort with
    // "FOREIGN KEY constraint failed" as soon as any of those rows existed.
    // The session's own movements are removed; every OTHER dependent row
    // keeps its financial record but is detached (session_id = NULL), so
    // the ledger totals survive the deletion. No PRAGMA foreign_keys=OFF.
    tx.execute("DELETE FROM cash_movements WHERE session_id = ?1", [session_id])
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE sales SET session_id = NULL WHERE session_id = ?1", [session_id])
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE expenses SET session_id = NULL WHERE session_id = ?1", [session_id])
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE customer_debt_payments SET session_id = NULL WHERE session_id = ?1", [session_id])
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE supplier_debt_payments SET session_id = NULL WHERE session_id = ?1", [session_id])
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE salary_advances SET session_id = NULL WHERE session_id = ?1", [session_id])
        .map_err(|e| e.to_string())?;
    tx.execute("UPDATE payrolls SET session_id = NULL WHERE session_id = ?1", [session_id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM cash_sessions WHERE id = ?1", [session_id])
        .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    // Audit + Telegram alert for the destructive admin action (gated by the
    // session-edit switch, consistent with archive/edit notifications).
    {
        let lang = crate::services::notifier_service::ui_language(db);
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("🗑 *Cash Session #{} Deleted*\nAll its cash movements were removed; linked sales/expenses keep their records (session detached).", session_id),
                format!("🗑 *حُذفت جلسة الصندوق #{}*\nأُزيلت حركاتها النقدية؛ بقيت المبيعات والمصاريف المرتبطة في السجل (فُصلت عن الجلسة).", session_id),
                format!("🗑 *Session de caisse #{} supprimée*\nSes mouvements ont été supprimés ; les ventes/dépenses liées conservent leur historique (détachées).", session_id),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_session_deleted", text);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn test_db_with_open_session(opened_at: &str) -> DbState {
        let conn = Connection::open_in_memory().unwrap();
        let state = DbState { conn: std::sync::Mutex::new(conn) };
        state.run_migrations().unwrap();
        state.seed_default_admin().unwrap();
        {
            let conn = state.conn.lock().unwrap();
            let _ = conn.execute(
                "INSERT INTO cash_sessions (register_id, user_id, opening_amount, expected_cash, status, opened_at)
                 VALUES (1, 1, 500, 500, 'open', ?1)",
                [opened_at],
            );
        }
        state
    }

    // A same-day session MUST be returned open: closing/reopening the app
    // never closes the register. Regression: the app used to force-close
    // still-valid sessions on every restart.
    #[test]
    fn same_day_session_survives() {
        // Dynamic "this morning" date — a hardcoded one goes stale past midnight.
        let this_morning = chrono::Local::now()
            .format("%Y-%m-%d 09:00:00")
            .to_string();
        let db = test_db_with_open_session(&this_morning);
        let s = get_active_session(&db, 1).unwrap().expect("session must be open");
        assert_eq!(s.status, "open");
        assert_eq!(s.is_stale, Some(false));
    }

    // A previous-day session is auto-closed at the midnight rollover so the
    // cashier starts a fresh one.
    #[test]
    fn previous_day_session_auto_closes() {
        // Dynamic "yesterday noon" so timezone offset never rolls into today.
        let yesterday = (chrono::Local::now() - chrono::Duration::days(1))
            .format("%Y-%m-%d 12:00:00")
            .to_string();
        let db = test_db_with_open_session(&yesterday);
        let s = get_active_session(&db, 1).unwrap();
        assert!(s.is_none(), "stale session must be closed (None)");
        let conn = db.conn.lock().unwrap();
        let status: String = conn
            .query_row("SELECT status FROM cash_sessions ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "closed");
    }

    // No open session at all returns None (not an error) so the UI can offer
    // "Open New Session".
    #[test]
    fn no_open_session_returns_none() {
        let conn = Connection::open_in_memory().unwrap();
        let state = DbState { conn: std::sync::Mutex::new(conn) };
        state.run_migrations().unwrap();
        state.seed_default_admin().unwrap();
        assert!(get_active_session(&state, 1).unwrap().is_none());
    }
}

#[cfg(test)]
mod login_probe {
    use super::*;
    use crate::auth::authenticate_user;

    // Reproduce the login-then-session chain the UI runs after sign-in,
    // against the REAL user database. If this hangs, we found the bug.
    #[test]
    fn probe_login_chain_real_db() {
        let state = crate::database::DbState::new().expect("db open");
        let t0 = std::time::Instant::now();
        let user = authenticate_user(&state, "admin", "admin");
        println!("login -> {:?} in {:?}", user.as_ref().map(|u| u.is_some()), t0.elapsed());
        assert!(t0.elapsed().as_secs() < 10, "login hung");

        let t1 = std::time::Instant::now();
        let s = get_active_session(&state, 1);
        println!("session -> ok={} in {:?}", s.is_ok(), t1.elapsed());
        assert!(t1.elapsed().as_secs() < 10, "session fetch hung");

        let t2 = std::time::Instant::now();
        let g = crate::services::settings_service::get_all_settings(&state);
        println!("settings -> ok={} in {:?}", g.is_ok(), t2.elapsed());
        assert!(t2.elapsed().as_secs() < 10, "settings fetch hung");
    }
}

#[cfg(test)]
mod fresh_db_probe {
    // Simulate a FRESH INSTALL (empty APPDATA db): migrations + seeds must
    // all succeed, or the app opens with every invoke failing = the login
    // hang + missing wizard the user saw after uninstall/reinstall.
    #[test]
    fn probe_fresh_install_boot() {
        let state = crate::database::DbState::new().expect("FRESH BOOT: DbState::new failed");
        let t0 = std::time::Instant::now();
        let users = crate::auth::list_active_users(&state);
        println!("fresh users -> {:?}", users.as_ref().map(|u| u.len()));
        assert!(users.map(|u| !u.is_empty()).unwrap_or(false), "fresh db has no users to log in with");
        let login = crate::auth::authenticate_user(&state, "admin", "admin");
        println!("fresh login -> {:?} in {:?}", login.as_ref().map(|u| u.is_some()), t0.elapsed());
        assert!(t0.elapsed().as_secs() < 10, "fresh login hung");
        let settings = crate::services::settings_service::get_all_settings(&state);
        println!("fresh settings -> ok={} in {:?}", settings.is_ok(), t0.elapsed());
        assert!(settings.is_ok(), "fresh settings fetch failed");
    }
}
