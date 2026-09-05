//! Employee payroll payment reminders.
//!
//! Notifies the administrator one day before each employee's payroll
//! payment date and again on the date itself — via the in-app feed and
//! (when enabled) Telegram. Rules:
//!   * The next payment date derives from the employee's own pay-cycle
//!     start day (salary_start_date, falling back to hire_date), clamped
//!     per month length (handles February, leap years, end-of-month).
//!   * An occurrence already PAID (a matching paid payroll row for that
//!     employee + period) never triggers reminders.
//!   * A reminder is uniquely identified by (employee, payment date,
//!     reminder type) in payroll_reminder_log — restarts, settings
//!     reloads or repeated scheduler ticks never duplicate it.
//!   * One employee's failure never stops the others.

use crate::database::DbState;
use chrono::Datelike;

/// Local calendar date string "YYYY-MM-DD".
fn today_str() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

/// Days in a (year, month) — leap-year aware.
fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1u32) } else { (year, month + 1) };
    chrono::NaiveDate::from_ymd_opt(ny, nm, 1)
        .and_then(|d| d.pred_opt())
        .map(|d| d.day())
        .unwrap_or(31)
}

/// The next payroll payment date for an employee, from their cycle start
/// day. Monthly cycle: payment lands on the same day-of-month each cycle;
/// when the start day exceeds the target month's length it clamps to that
/// month's last day (e.g. Jan 31 → Feb 28/29).
pub fn next_payroll_payment_date(salary_start_date: &str, today: &str) -> String {
    let today_date = parse_date(today)
        .unwrap_or_else(|| chrono::Local::now().date_naive());

    let start_date = parse_date(salary_start_date);
    let start_day = start_date.map(|d| d.day()).unwrap_or(1).clamp(1, 31);

    let (t_y, t_m) = (today_date.year(), today_date.month());
    let pay_this_day = start_day.min(days_in_month(t_y, t_m));
    let pay_this = chrono::NaiveDate::from_ymd_opt(t_y, t_m, pay_this_day)
        .unwrap_or(today_date);

    if pay_this >= today_date {
        return pay_this.format("%Y-%m-%d").to_string();
    }

    // Next month's payment date, clamped to that month's length.
    let (ny, nm) = if t_m == 12 { (t_y + 1, 1u32) } else { (t_y, t_m + 1) };
    let pay_next_day = start_day.min(days_in_month(ny, nm));
    chrono::NaiveDate::from_ymd_opt(ny, nm, pay_next_day)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| today.to_string())
}

/// Calendar day BEFORE a date ("YYYY-MM-DD"), handling month/year wraps,
/// month lengths and leap years.
fn day_before(date: &str) -> Option<String> {
    parse_date(date)?.pred_opt().map(|p| p.format("%Y-%m-%d").to_string())
}

/// Whether this employee's pay period ending on `payment_date` is already
/// paid: any PAID payroll row for this employee stamped within the 31
/// days before the payment date counts as this cycle's payment.
fn payroll_period_paid(conn: &rusqlite::Connection, employee_id: i64, payment_date: &str) -> bool {
    let Some(pay_day) = parse_date(payment_date) else { return false };
    let Some(window_start) = pay_day
        .checked_sub_signed(chrono::Duration::days(31))
        .map(|d| d.format("%Y-%m-%d").to_string())
    else {
        return false;
    };
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM payrolls
             WHERE employee_id = ?1
               AND payment_status = 'paid'
               AND date(COALESCE(paid_at, created_at)) BETWEEN ?2 AND ?3",
            rusqlite::params![employee_id, window_start, payment_date],
            |r| r.get(0),
        )
        .unwrap_or(0);
    count > 0
}

/// True when (employee, date, reminder type) was already delivered.
fn reminder_already_sent(conn: &rusqlite::Connection, employee_id: i64, payment_date: &str, kind: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM payroll_reminder_log
         WHERE employee_id = ?1 AND payment_date = ?2 AND reminder_type = ?3",
        rusqlite::params![employee_id, payment_date, kind],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n > 0)
    .unwrap_or(false)
}

fn mark_reminder_sent(conn: &rusqlite::Connection, employee_id: i64, payment_date: &str, kind: &str) {
    let _ = conn.execute(
        "INSERT OR IGNORE INTO payroll_reminder_log (employee_id, payment_date, reminder_type)
         VALUES (?1, ?2, ?3)",
        rusqlite::params![employee_id, payment_date, kind],
    );
}

/// Employee row used by the reminder scan.
struct EmpRow {
    id: i64,
    name: String,
    base_salary: i64,
    salary_start_date: Option<String>,
    hire_date: String,
}

/// One reminder the scan decided to emit (after the DB work is done).
struct PendingReminder {
    employee_id: i64,
    employee_name: String,
    amount: i64,
    pay_date: String,
    kind: &'static str,
}

/// One scheduler tick: scan every active employee, compute their next
/// payment date, and fire DAY_BEFORE / DUE_TODAY reminders when due.
/// Returns the number of NEW reminders emitted (for diagnostics).
///
/// LOCK DISCIPLINE (the v0.5.16 regression): db.conn is a NON-REENTRANT
/// mutex. The first version kept the lock for the whole loop and then
/// called ui_language / push_inapp_notification / notify_if_enabled —
/// each re-locks db.conn, self-deadlocking the backend so even login
/// hung ("backend connection stuck"). Now ALL reads/decisions happen
/// inside ONE short lock scope, the guard is released, and only then
/// are notifications emitted lock-free.
pub fn run_payroll_reminder_scan(db: &DbState) -> i64 {
    let settings = crate::services::settings_service::get_all_settings(db).unwrap_or_default();
    // Master OFF disables the whole payroll reminder feature.
    if settings.get("notify_payroll_enabled").map(|v| v == "false").unwrap_or(false) {
        return 0;
    }
    let inapp_on = settings.get("notify_payroll_inapp").map(|v| v == "true").unwrap_or(true);
    let telegram_on = settings.get("notify_payroll_telegram").map(|v| v == "true").unwrap_or(false);
    if !inapp_on && !telegram_on {
        return 0;
    }
    // Resolve the UI language BEFORE taking the connection lock below —
    // ui_language locks the same mutex internally.
    let lang = crate::services::notifier_service::ui_language(db);

    let today = today_str();
    let tomorrow = parse_date(&today)
        .and_then(|d| d.succ_opt())
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();

    // --- Single short lock scope: read employees, decide, mark, release. ---
    let pending: Vec<PendingReminder> = {
        let conn = db.conn.lock().unwrap();
        // Ensure the dedup table exists even on legacy DBs.
        let _ = conn.execute(
            "CREATE TABLE IF NOT EXISTS payroll_reminder_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                employee_id INTEGER NOT NULL,
                payment_date TEXT NOT NULL,
                reminder_type TEXT NOT NULL CHECK(reminder_type IN ('DAY_BEFORE', 'DUE_TODAY')),
                created_at TEXT DEFAULT (datetime('now','localtime')),
                UNIQUE(employee_id, payment_date, reminder_type)
            )",
            [],
        );

        let emps: Vec<EmpRow> = match conn.prepare(
            "SELECT id, full_name, base_salary, salary_start_date, hire_date
             FROM employees WHERE is_active = 1",
        ) {
            Ok(mut stmt) => stmt
                .query_map([], |r| {
                    Ok(EmpRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        base_salary: r.get(2)?,
                        salary_start_date: r.get(3)?,
                        hire_date: r.get(4)?,
                    })
                })
                .map(|rows| rows.filter_map(|x| x.ok()).collect())
                .unwrap_or_default(),
            Err(e) => {
                eprintln!("[payroll-remind] prepare failed: {}", e);
                Vec::new()
            }
        };

        let mut due: Vec<PendingReminder> = Vec::new();
        for emp in emps {
            let start = emp
                .salary_start_date
                .clone()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| emp.hire_date.clone());
            let pay_date = next_payroll_payment_date(&start, &today);

            // Which reminder (if any) is due today for this occurrence?
            let kind = if pay_date == today {
                "DUE_TODAY"
            } else if tomorrow == pay_date {
                "DAY_BEFORE"
            } else {
                continue;
            };
            // Already-paid occurrence or already-delivered → skip.
            if payroll_period_paid(&conn, emp.id, &pay_date) {
                continue;
            }
            if reminder_already_sent(&conn, emp.id, &pay_date, kind) {
                continue;
            }
            // Claim the reminder NOW (while still holding the lock) so the
            // 60s scheduler can never double-fire between scan steps.
            mark_reminder_sent(&conn, emp.id, &pay_date, kind);
            due.push(PendingReminder {
                employee_id: emp.id,
                employee_name: emp.name,
                amount: emp.base_salary,
                pay_date,
                kind,
            });
        }
        due
    };
    // --- Lock released: notifications below may lock db.conn freely. ---

    let mut emitted: i64 = 0;
    for r in &pending {
        let (title, message) = if r.kind == "DAY_BEFORE" {
            (
                crate::services::notifier_service::tr(&lang, (
                    "💼 Employee Payroll Reminder".to_string(),
                    "💼 تذكير براتب موظف".to_string(),
                    "💼 Rappel de paie employé".to_string(),
                )),
                crate::services::notifier_service::tr(&lang, (
                    format!("Employee: {}\nAmount Due: {} DZD\nPayment Date: {}\nReminder: Payroll payment is due tomorrow.", r.employee_name, r.amount, r.pay_date),
                    format!("الموظف: {}\nالمبلغ المستحق: {} دج\nتاريخ الدفع: {}\nتذكير: دفع الراتب غداً.", r.employee_name, r.amount, r.pay_date),
                    format!("Employé : {}\nMontant dû : {} DZD\nDate de paiement : {}\nRappel : la paie est due demain.", r.employee_name, r.amount, r.pay_date),
                )),
            )
        } else {
            (
                crate::services::notifier_service::tr(&lang, (
                    "📅 Employee Payroll Due Today".to_string(),
                    "📅 راتب موظف مستحق اليوم".to_string(),
                    "📅 Paie employée due aujourd'hui".to_string(),
                )),
                crate::services::notifier_service::tr(&lang, (
                    format!("Employee: {}\nAmount Due: {} DZD\nPayment Date: {}\nStatus: Payment due today.", r.employee_name, r.amount, r.pay_date),
                    format!("الموظف: {}\nالمبلغ المستحق: {} دج\nتاريخ الدفع: {}\nالحالة: الدفع مستحق اليوم.", r.employee_name, r.amount, r.pay_date),
                    format!("Employé : {}\nMontant dû : {} DZD\nDate de paiement : {}\nStatut : paiement dû aujourd'hui.", r.employee_name, r.amount, r.pay_date),
                )),
            )
        };

        // A failure on one employee's notification must not stop the others.
        if inapp_on {
            crate::services::notifier_service::push_inapp_notification(
                db, "payroll", &title, &message, Some(r.employee_id),
            );
        }
        if telegram_on {
            // The per-event switch IS notify_payroll_telegram (already
            // checked above), so passing it as the gate key is correct.
            crate::services::notifier_service::notify_if_enabled(
                db,
                "notify_payroll_telegram",
                format!("{}\n{}", title, message),
            );
        }
        emitted += 1;
    }

    if emitted > 0 {
        println!("[payroll-remind] emitted {} reminder(s) for {}", emitted, today);
    }
    emitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    // The v0.5.16 regression: the scan used to hold db.conn across calls
    // that re-lock the same NON-REENTRANT mutex (ui_language, in-app push,
    // telegram config) — self-deadlocking the backend so even login hung
    // ("backend connection stuck"). This runs the scan on a worker thread
    // with a hard timeout: a deadlock hangs the test, not the POS.
    #[test]
    fn test_scan_completes_without_deadlock() {
        let db = std::sync::Arc::new(crate::database::DbState::new().expect("db open"));
        let (tx, rx) = mpsc::channel();
        let db2 = std::sync::Arc::clone(&db);
        let handle = std::thread::spawn(move || {
            let emitted = run_payroll_reminder_scan(&db2);
            let _ = tx.send(emitted);
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(_) => {
                handle.join().unwrap();
                // Scan finished: the mutex must be free again — prove it by
                // taking the lock right now.
                let _guard = db.conn.lock().unwrap();
            }
            Err(_) => {
                panic!("DEADLOCK in run_payroll_reminder_scan: it re-locks db.conn while holding it");
            }
        }
    }

    // Payment date math must survive month lengths: cycle day 31 in a
    // non-leap February clamps to the 28th.
    #[test]
    fn clamps_to_month_length() {
        assert_eq!(next_payroll_payment_date("2026-01-31", "2026-02-01"), "2026-02-28");
    }

    // Same-day payment: today IS the payment date.
    #[test]
    fn today_is_payday() {
        assert_eq!(next_payroll_payment_date("2026-05-06", "2026-09-06"), "2026-09-06");
    }

    // After this month's payday passed → next month's payday (leap Feb 29).
    #[test]
    fn rolls_to_next_month_leap() {
        assert_eq!(next_payroll_payment_date("2024-01-29", "2024-02-01"), "2024-02-29");
    }

    // Day-before math handles month wrap (Mar 1 → Feb 29 leap).
    #[test]
    fn day_before_wraps_month() {
        assert_eq!(day_before("2024-03-01").unwrap(), "2024-02-29");
    }

    // Standard mid-month cycle rolls to the next month after payday.
    #[test]
    fn rolls_after_payday() {
        assert_eq!(next_payroll_payment_date("2026-03-15", "2026-03-16"), "2026-04-15");
    }
}
