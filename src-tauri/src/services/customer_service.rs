use crate::database::DbState;
use crate::models::{Customer, CustomerPaymentInput};
use rusqlite::Result;

pub fn list_customers(db: &DbState) -> Result<Vec<Customer>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.name, c.phone, c.email, c.address, c.rc, c.nif, c.nis, c.ai, c.qr_code,
                    c.balance, c.initial_debt, c.notes, c.is_active, c.created_at,
                    COALESCE(SUM(s.total_amount), 0) as total_purchases,
                    COALESCE(c.pinned, 0), COALESCE(c.pin_order, 0)
             FROM customers c
             LEFT JOIN sales s ON c.id = s.customer_id AND s.status = 'completed'
             WHERE c.is_active = 1
             GROUP BY c.id
             ORDER BY COALESCE(c.pinned, 0) DESC, COALESCE(c.pin_order, 0) ASC, c.id DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Customer {
                id: row.get(0)?,
                name: row.get(1)?,
                phone: row.get(2)?,
                email: row.get(3)?,
                address: row.get(4)?,
                rc: row.get(5)?,
                nif: row.get(6)?,
                nis: row.get(7)?,
                ai: row.get(8)?,
                qr_code: row.get(9)?,
                balance: row.get(10)?,
                initial_debt: row.get(11)?,
                notes: row.get(12)?,
                is_active: row.get(13)?,
                created_at: row.get(14)?,
                total_purchases: Some(row.get(15)?),
                pinned: row.get::<_, i64>(16)? == 1,
                pin_order: row.get(17)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Customer> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn save_customer(
    db: &DbState,
    name: &str,
    phone: Option<String>,
    email: Option<String>,
    address: Option<String>,
    rc: Option<String>,
    nif: Option<String>,
    nis: Option<String>,
    ai: Option<String>,
    initial_debt: i64,
    notes: Option<String>,
    customer_id: Option<i64>,
) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();

    if let Some(cid) = customer_id {
        conn.execute(
            "UPDATE customers
             SET name = ?1, phone = ?2, email = ?3, address = ?4, rc = ?5, nif = ?6, nis = ?7, ai = ?8, notes = ?9
             WHERE id = ?10",
            rusqlite::params![name, phone, email, address, rc, nif, nis, ai, notes, cid],
        )
        .map_err(|e| e.to_string())?;
        Ok(cid)
    } else {
        let qr_code = format!("CUST-{:04}", chrono::Local::now().timestamp_subsec_millis());
        conn.execute(
            "INSERT INTO customers (name, phone, email, address, rc, nif, nis, ai, qr_code, balance, initial_debt, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11)",
            rusqlite::params![name, phone, email, address, rc, nif, nis, ai, qr_code, initial_debt, notes],
        )
        .map_err(|e| e.to_string())?;
        Ok(conn.last_insert_rowid())
    }
}

pub fn delete_customer(db: &DbState, customer_id: i64) -> Result<(), String> {
    // The seeded walk-in customer (id 1) is the POS default: undeletable.
    if customer_id == 1 {
        return Err("The default walk-in customer cannot be deleted / لا يمكن حذف الزبون الافتراضي".to_string());
    }
    let conn = db.conn.lock().unwrap();
    conn.execute("UPDATE customers SET is_active = 0 WHERE id = ?1", [customer_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn record_customer_debt_payment(db: &DbState, input: CustomerPaymentInput) -> Result<i64, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT INTO customer_debt_payments (customer_id, amount, payment_method, reference, session_id, user_id, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            input.customer_id, input.amount, input.payment_method,
            input.reference, input.session_id, input.user_id, input.notes
        ],
    )
    .map_err(|e| e.to_string())?;

    let payment_id = tx.last_insert_rowid();

    tx.execute(
        "UPDATE customers SET balance = balance - ?1 WHERE id = ?2",
        rusqlite::params![input.amount, input.customer_id],
    )
    .map_err(|e| e.to_string())?;

    if input.payment_method == "cash" {
        if let Some(sid) = input.session_id {
            tx.execute(
                "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, reference_type, reference_id)
                 VALUES (?1, ?2, 'customer_debt_payment', ?3, 'Customer Debt Payment / تسديد دين زبون', 'customer_payment', ?4)",
                rusqlite::params![sid, input.user_id, input.amount, payment_id],
            )
            .map_err(|e| e.to_string())?;

            tx.execute(
                "UPDATE cash_sessions SET expected_cash = expected_cash + ?1 WHERE id = ?2",
                rusqlite::params![input.amount, sid],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    Ok(payment_id)
}
/// Pin/unpin a customer: pinned customers float to the top of the list.
pub fn toggle_customer_pin(db: &DbState, customer_id: i64, pinned: bool) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    if pinned {
        let max_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(pin_order), 0) FROM customers WHERE pinned = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "UPDATE customers SET pinned = 1, pin_order = ?1 WHERE id = ?2",
            rusqlite::params![max_order + 1, customer_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "UPDATE customers SET pinned = 0, pin_order = 0 WHERE id = ?1",
            [customer_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Clear (forgive) a customer's outstanding debt — admin-only.
///
/// Accounting rule: this is debt FORGIVENESS, not a payment. It must NOT
/// create any cash movement, IN/OUT entry or fake payment row: no money
/// entered the drawer. The balance becomes 0 and the cleared amount is
/// archived in debt_clear_log (entity, previous debt, user, reason, time).
pub fn clear_customer_debt(
    db: &DbState,
    customer_id: i64,
    reason: Option<String>,
    admin_password: &str,
    user_id: Option<i64>,
) -> Result<i64, String> {
    if !crate::auth::verify_admin_password(db, admin_password)? {
        return Err("Mot de passe administrateur incorrect / Invalid admin password".to_string());
    }

    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let (name, previous_debt): (String, i64) = tx
        .query_row(
            "SELECT name, balance FROM customers WHERE id = ?1 AND is_active = 1",
            [customer_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("Client introuvable: {}", e))?;

    if previous_debt <= 0 {
        return Err("This customer has no outstanding debt / لا يوجد دين مستحق".to_string());
    }

    tx.execute(
        "UPDATE customers SET balance = 0 WHERE id = ?1",
        [customer_id],
    )
    .map_err(|e| e.to_string())?;

    let actor = crate::services::notifier_service::actor_label_from_conn(&tx, user_id);
    tx.execute(
        "INSERT INTO debt_clear_log (entity_type, entity_id, entity_name, previous_debt, new_debt, reason, user_name)
         VALUES ('customer', ?1, ?2, ?3, 0, ?4, ?5)",
        rusqlite::params![customer_id, name, previous_debt, reason, actor],
    )
    .map_err(|e| e.to_string())?;
    let log_id = tx.last_insert_rowid();

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    // Audit-style Telegram alert (gated by the debt-clear switch).
    {
        let lang = crate::services::notifier_service::ui_language(db);
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("🧹 *Customer Debt Cleared*\n👤 Customer: *{}* (#{})\n💰 Previous debt: *{} DZD* ➔ 0\n📝 Reason: {}\n🛡 By: {}", name, customer_id, previous_debt, reason.as_deref().unwrap_or("-"), actor),
                format!("🧹 *تمت مصالحة دين الزبون*\n👤 الزبون: *{}* (#{})\n💰 الدين السابق: *{} دج* ➔ 0\n📝 السبب: {}\n🛡 بواسطة: {}", name, customer_id, previous_debt, reason.as_deref().unwrap_or("-"), actor),
                format!("🧹 *Dette client effacée*\n👤 Client : *{}* (#{})\n💰 Dette précédente : *{} DZD* ➔ 0\n📝 Motif : {}\n🛡 Par : {}", name, customer_id, previous_debt, reason.as_deref().unwrap_or("-"), actor),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_debt_cleared", text);
    }

    Ok(log_id)
}
