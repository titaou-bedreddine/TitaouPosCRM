use crate::database::DbState;
use crate::models::{Supplier, SupplierPaymentInput, SupplierPaymentRow};
use rusqlite::Result;

pub fn list_suppliers(db: &DbState) -> Result<Vec<Supplier>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, contact_person, phone, email, address, rc, nif, nis, ai, qr_code, balance, notes, is_active, created_at,
                    COALESCE(pinned, 0), COALESCE(pin_order, 0)
             FROM suppliers
             WHERE is_active = 1
             ORDER BY COALESCE(pinned, 0) DESC, COALESCE(pin_order, 0) ASC, id DESC",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Supplier {
                id: row.get(0)?,
                name: row.get(1)?,
                contact_person: row.get(2)?,
                phone: row.get(3)?,
                email: row.get(4)?,
                address: row.get(5)?,
                rc: row.get(6)?,
                nif: row.get(7)?,
                nis: row.get(8)?,
                ai: row.get(9)?,
                qr_code: row.get(10)?,
                balance: row.get(11)?,
                notes: row.get(12)?,
                is_active: row.get(13)?,
                created_at: row.get(14)?,
                pinned: row.get::<_, i64>(15)? == 1,
                pin_order: row.get(16)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Supplier> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}

pub fn save_supplier(
    db: &DbState,
    name: &str,
    contact_person: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    address: Option<String>,
    rc: Option<String>,
    nif: Option<String>,
    nis: Option<String>,
    ai: Option<String>,
    notes: Option<String>,
    supplier_id: Option<i64>,
) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();

    let saved_id = if let Some(sid) = supplier_id {
        conn.execute(
            "UPDATE suppliers
             SET name = ?1, contact_person = ?2, phone = ?3, email = ?4, address = ?5, rc = ?6, nif = ?7, nis = ?8, ai = ?9, notes = ?10
             WHERE id = ?11",
            rusqlite::params![name, contact_person, phone, email, address, rc, nif, nis, ai, notes, sid],
        )
        .map_err(|e| e.to_string())?;
        sid
    } else {
        let qr_code = format!("SUPP-{:04}", chrono::Local::now().timestamp_subsec_millis());
        conn.execute(
            "INSERT INTO suppliers (name, contact_person, phone, email, address, rc, nif, nis, ai, qr_code, balance, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10)",
            rusqlite::params![name, contact_person, phone, email, address, rc, nif, nis, ai, qr_code, notes],
        )
        .map_err(|e| e.to_string())?;
        conn.last_insert_rowid()
    };

    // Cloud sync outbox: the walk-in supplier (id 1) is POS-only.
    if saved_id != 1 {
        let payload = serde_json::json!({
            "name": name,
            "contact_person": contact_person,
            "phone": phone,
            "email": email,
            "address": address,
        });
        let _ = conn.execute(
            "INSERT INTO sync_outbox (entity, local_id, payload, status)
             VALUES ('supplier', ?1, ?2, 'pending')",
            rusqlite::params![saved_id, payload.to_string()],
        );
    }
    Ok(saved_id)
}

pub fn delete_supplier(db: &DbState, supplier_id: i64) -> Result<(), String> {
    // The seeded generic supplier (id 1) backs POS purchase mode: undeletable.
    if supplier_id == 1 {
        return Err("The default supplier cannot be deleted / لا يمكن حذف المورد الافتراضي".to_string());
    }
    let conn = db.conn.lock().unwrap();
    conn.execute("UPDATE suppliers SET is_active = 0 WHERE id = ?1", [supplier_id])
        .map_err(|e| e.to_string())?;
    Ok(())
}
/// Record a payment against a supplier's balance (money we owe them).
/// Cash payments land in the session's cash movements (money OUT of the
/// drawer) so the register and statistics reflect the payment.
pub fn record_supplier_debt_payment(db: &DbState, input: SupplierPaymentInput) -> Result<i64, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT INTO supplier_debt_payments (supplier_id, amount, payment_method, reference, session_id, user_id, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            input.supplier_id, input.amount, input.payment_method,
            input.reference, input.session_id, input.user_id, input.notes
        ],
    )
    .map_err(|e| e.to_string())?;

    let payment_id = tx.last_insert_rowid();

    // Never overpay past what we owe: clamp at 0.
    tx.execute(
        "UPDATE suppliers SET balance = MAX(0, balance - ?1) WHERE id = ?2",
        rusqlite::params![input.amount, input.supplier_id],
    )
    .map_err(|e| e.to_string())?;

    // When the payment settles a specific invoice (reference = invoice
    // number), raise the invoice's paid_amount too — otherwise the DUE
    // button would stay on the invoice list after paying it.
    if let Some(ref invoice_ref) = input.reference {
        let _ = tx.execute(
            "UPDATE purchases
             SET paid_amount = MIN(total, paid_amount + ?1)
             WHERE invoice_number = ?2",
            rusqlite::params![input.amount, invoice_ref],
        );
    }

    if input.payment_method == "cash" {
        if let Some(sid) = input.session_id {
            tx.execute(
                "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, reference_type, reference_id)
                 VALUES (?1, ?2, 'supplier_debt_payment', ?3, ?4, 'supplier_payment', ?5)",
                rusqlite::params![
                    sid,
                    input.user_id,
                    -input.amount,
                    format!("Supplier Debt Payment / \u{062a}\u{0633}\u{062f}\u{064a}\u{062f} \u{062f}\u{064a}\u{0646} \u{0645}\u{0648}\u{0631}\u{062f} #{}", input.supplier_id),
                    payment_id
                ],
            )
            .map_err(|e| e.to_string())?;

            tx.execute(
                "UPDATE cash_sessions SET expected_cash = expected_cash - ?1 WHERE id = ?2",
                rusqlite::params![input.amount, sid],
            )
            .map_err(|e| e.to_string())?;
        }
    }

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    // Telegram "supplier payment" alert (fire-and-forget, lock released),
    // localized to the UI language and attributed.
    {
        let lang = crate::services::notifier_service::ui_language(db);
        let actor = crate::services::notifier_service::actor_label(db, Some(input.user_id));
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("💸 *Supplier Payment*\n💰 Amount: *{} DZD*\n📂 Supplier #{}\n👤 By: {}", input.amount, input.supplier_id, actor),
                format!("💸 *دفعة للمورد*\n💰 المبلغ: *{} دج*\n📂 المورد #{}\n👤 بواسطة: {}", input.amount, input.supplier_id, actor),
                format!("💸 *Paiement Fournisseur*\n💰 Montant : *{} DZD*\n📂 Fournisseur #{}\n👤 Par : {}", input.amount, input.supplier_id, actor),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_supplier_payment", text);
    }

    Ok(payment_id)
}

/// Payment history for a supplier, newest first.
pub fn list_supplier_debt_payments(db: &DbState, supplier_id: i64) -> Result<Vec<SupplierPaymentRow>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT s.id, s.supplier_id, s.amount, s.payment_method, s.reference, s.created_at
             FROM supplier_debt_payments s
             WHERE s.supplier_id = ?1
             ORDER BY s.id DESC LIMIT 100",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([supplier_id], |row| {
            Ok(SupplierPaymentRow {
                id: row.get(0)?,
                supplier_id: row.get(1)?,
                amount: row.get(2)?,
                payment_method: row.get(3)?,
                reference: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Pin/unpin a supplier: pinned suppliers float to the top of the list.
pub fn toggle_supplier_pin(db: &DbState, supplier_id: i64, pinned: bool) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    if pinned {
        let max_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(pin_order), 0) FROM suppliers WHERE pinned = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "UPDATE suppliers SET pinned = 1, pin_order = ?1 WHERE id = ?2",
            rusqlite::params![max_order + 1, supplier_id],
        )
        .map_err(|e| e.to_string())?;
    } else {
        conn.execute(
            "UPDATE suppliers SET pinned = 0, pin_order = 0 WHERE id = ?1",
            [supplier_id],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Clear (forgive) a supplier's outstanding balance — admin-only.
///
/// Accounting rule: debt forgiveness, NOT a payment. No OUT cash movement,
/// no drawer/expected_cash change, no fake payment row. Balance becomes 0
/// and the cleared amount is archived in debt_clear_log.
pub fn clear_supplier_debt(
    db: &DbState,
    supplier_id: i64,
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
            "SELECT name, balance FROM suppliers WHERE id = ?1 AND is_active = 1",
            [supplier_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| format!("Fournisseur introuvable: {}", e))?;

    if previous_debt <= 0 {
        return Err("This supplier has no outstanding balance / لا يوجد دين مستحق".to_string());
    }

    tx.execute(
        "UPDATE suppliers SET balance = 0 WHERE id = ?1",
        [supplier_id],
    )
    .map_err(|e| e.to_string())?;

    let actor = crate::services::notifier_service::actor_label_from_conn(&tx, user_id);
    tx.execute(
        "INSERT INTO debt_clear_log (entity_type, entity_id, entity_name, previous_debt, new_debt, reason, user_name)
         VALUES ('supplier', ?1, ?2, ?3, 0, ?4, ?5)",
        rusqlite::params![supplier_id, name, previous_debt, reason, actor],
    )
    .map_err(|e| e.to_string())?;
    let log_id = tx.last_insert_rowid();

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    {
        let lang = crate::services::notifier_service::ui_language(db);
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("🧹 *Supplier Debt Cleared*\n🚚 Supplier: *{}* (#{})\n💰 Previous debt: *{} DZD* ➔ 0\n📝 Reason: {}\n🛡 By: {}", name, supplier_id, previous_debt, reason.as_deref().unwrap_or("-"), actor),
                format!("🧹 *تمت مصالحة دين المورد*\n🚚 المورد: *{}* (#{})\n💰 الدين السابق: *{} دج* ➔ 0\n📝 السبب: {}\n🛡 بواسطة: {}", name, supplier_id, previous_debt, reason.as_deref().unwrap_or("-"), actor),
                format!("🧹 *Dette fournisseur effacée*\n🚚 Fournisseur : *{}* (#{})\n💰 Dette précédente : *{} DZD* ➔ 0\n📝 Motif : {}\n🛡 Par : {}", name, supplier_id, previous_debt, reason.as_deref().unwrap_or("-"), actor),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_debt_cleared", text);
    }

    Ok(log_id)
}
