use crate::database::DbState;
use crate::models::{Payroll, EmployeeAdvance, EmployeeAdvanceInput};
use rusqlite::Result;

pub fn list_payrolls(db: &DbState) -> Result<Vec<Payroll>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT p.id, p.payroll_number, p.employee_id, e.full_name, p.period_month, p.period_year,
                    p.base_salary, p.bonuses, p.allowances, p.deductions, p.advances_deducted,
                    p.net_salary, p.payment_status, p.payment_method, p.paid_at, p.notes
             FROM payrolls p
             LEFT JOIN employees e ON p.employee_id = e.id
             ORDER BY p.id DESC LIMIT 200",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([], |row| {
            Ok(Payroll {
                id: row.get(0)?,
                payroll_number: row.get(1)?,
                employee_id: row.get(2)?,
                employee_name: row.get(3)?,
                period_month: row.get(4)?,
                period_year: row.get(5)?,
                base_salary: row.get(6)?,
                bonuses: row.get(7)?,
                allowances: row.get(8)?,
                deductions: row.get(9)?,
                advances_deducted: row.get(10)?,
                net_salary: row.get(11)?,
                payment_status: row.get(12)?,
                payment_method: row.get(13)?,
                paid_at: row.get(14)?,
                notes: row.get(15)?,
            })
        })
        .map_err(|e| e.to_string())?;

    let list: Vec<Payroll> = rows.filter_map(|r| r.ok()).collect();
    Ok(list)
}
/// Record a salary advance: persisted (survives restarts), and when paid
/// from the drawer it is booked as an "Avances Salaires" expense so the
/// Expenses page, register and statistics all reflect it.
pub fn record_employee_advance(db: &DbState, input: EmployeeAdvanceInput) -> Result<i64, String> {
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    let now = chrono::Local::now();
    let date = if input.date.is_empty() {
        now.format("%Y-%m-%d").to_string()
    } else {
        input.date.clone()
    };
    let expense_number = format!("ADV-{}", now.format("%Y%m%d%H%M%S"));

    // Book the expense (category 7 = Avances Salaires). Paid from the
    // session drawer when session_id is provided: the drawer movement and
    // expected_cash update are handled by the same insert.
    // Real employee name for the expense recipient (was "Employee #3").
    let employee_name: String = {
        // full_name is the real column — a past 'SELECT name' always errored
        // and silently booked the recipient as "#<id>", killing the
        // expense→employee link.
        let row = tx.query_row(
            "SELECT full_name FROM employees WHERE id = ?1",
            [input.employee_id],
            |r| r.get::<_, String>(0),
        );
        row.unwrap_or_else(|_| format!("#{}", input.employee_id))
    };

    tx.execute(
        "INSERT INTO expenses (expense_number, category_id, amount, payment_method, session_id, user_id, recipient, date, notes)
         VALUES (?1, 7, ?2, 'cash', ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            expense_number,
            input.amount,
            input.session_id,
            input.user_id,
            employee_name,
            date,
            format!("Salary advance / سلفة راتب: {}", input.reason.as_deref().unwrap_or("Avance sur salaire")),
        ],
    )
    .map_err(|e| e.to_string())?;
    let expense_id = tx.last_insert_rowid();

    // Drawer movement: money OUT of the register.
    if let Some(sid) = input.session_id {
        tx.execute(
            "INSERT INTO cash_movements (session_id, user_id, type, amount, reason, reference_type, reference_id)
             VALUES (?1, ?2, 'salary_payment', ?3, ?4, 'expense', ?5)",
            rusqlite::params![
                sid,
                input.user_id,
                -input.amount,
                format!("Salary Advance / سلفة راتب EXP-{}", expense_number),
                expense_id
            ],
        )
        .map_err(|e| e.to_string())?;

        tx.execute(
            "UPDATE cash_sessions SET expected_cash = expected_cash - ?1 WHERE id = ?2",
            rusqlite::params![input.amount, sid],
        )
        .map_err(|e| e.to_string())?;
    }

    tx.execute(
        "INSERT INTO employee_advances (employee_id, amount, reason, date, expense_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![input.employee_id, input.amount, input.reason, date, expense_id],
    )
    .map_err(|e| e.to_string())?;
    let advance_id = tx.last_insert_rowid();

    tx.commit().map_err(|e| e.to_string())?;
    drop(conn);

    {
        let lang = crate::services::notifier_service::ui_language(db);
        let actor = crate::services::notifier_service::actor_label(db, Some(input.user_id));
        let text = crate::services::notifier_service::tr(
            &lang,
            (
                format!("💵 *Salary Advance* EXP-{}\n💰 Amount: *{} DZD*\n👤 Employee #{}\n👤 By: {}", expense_number, input.amount, input.employee_id, actor),
                format!("💵 *تسبقة على الراتب* EXP-{}\n💰 المبلغ: *{} دج*\n👤 الموظف #{}\n👤 بواسطة: {}", expense_number, input.amount, input.employee_id, actor),
                format!("💵 *Avance Salarie* EXP-{}\n💰 Montant : *{} DZD*\n👤 Employé #{}\n👤 Par : {}", expense_number, input.amount, input.employee_id, actor),
            ),
        );
        crate::services::notifier_service::notify_if_enabled(db, "notify_each_expense", text);
    }

    Ok(advance_id)
}

/// Advances for an employee (optionally only the given month), newest first.
pub fn list_employee_advances(db: &DbState, employee_id: Option<i64>, month: Option<String>) -> Result<Vec<EmployeeAdvance>, String> {
    let conn = db.conn.lock().unwrap();

    let mut sql = String::from(
        "SELECT ea.id, ea.employee_id, e.full_name, ea.amount, ea.reason, ea.date, ea.created_at
         FROM employee_advances ea
         LEFT JOIN employees e ON ea.employee_id = e.id
         WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(eid) = employee_id {
        sql.push_str(&format!(" AND ea.employee_id = {}", eid));
    }
    if let Some(m) = &month {
        if !m.is_empty() {
            // Month filter "YYYY-MM"
            sql.push_str(" AND substr(ea.date, 1, 7) = ?");
            params.push(Box::new(m.clone()));
        }
    }
    sql.push_str(" ORDER BY ea.id DESC LIMIT 200");

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())), |row| {
            Ok(EmployeeAdvance {
                id: row.get(0)?,
                employee_id: row.get(1)?,
                employee_name: row.get(2)?,
                amount: row.get(3)?,
                reason: row.get(4)?,
                date: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Record an absence (in days) for an employee — persisted, unlike the old
/// UI-only in-memory map that lost absences on reload.
pub fn record_employee_absence(
    db: &DbState,
    employee_id: i64,
    days: i64,
    reason: Option<String>,
    date: String,
) -> Result<i64, String> {
    let conn = db.conn.lock().unwrap();
    conn.execute(
        "INSERT INTO employee_absences (employee_id, days, reason, date)
         VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![employee_id, days, reason, date],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

pub fn list_employee_absences(
    db: &DbState,
    employee_id: Option<i64>,
    month: Option<String>,
) -> Result<Vec<(i64, i64, i64, Option<String>, String)>, String> {
    let conn = db.conn.lock().unwrap();
    let mut sql = String::from(
        "SELECT id, employee_id, days, reason, date FROM employee_absences WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(eid) = employee_id {
        sql.push_str(" AND employee_id = ?");
        params.push(Box::new(eid));
    }
    if let Some(m) = &month {
        if !m.is_empty() {
            sql.push_str(" AND substr(date, 1, 7) = ?");
            params.push(Box::new(m.clone()));
        }
    }
    sql.push_str(" ORDER BY id DESC LIMIT 500");
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}
