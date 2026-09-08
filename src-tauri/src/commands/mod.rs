use crate::auth::authenticate_user;
use crate::database::DbState;
pub mod network_cmds;
use crate::models::{
    CartItem, Category, Customer, CustomerPaymentInput, DashboardStats, Employee, Expense, HeldSale,
    Payroll, EmployeeAdvance, EmployeeAdvanceInput, ProductPackaging, PackagingInput, Product, ProductInput, Purchase, PurchaseItem, CreatePurchaseInput, Sale, Supplier, SupplierPaymentInput, SupplierPaymentRow, Unit, User, UserAccount, Role,
    CashMovement, CashSession, CreateSaleInput, PriceHistoryEntry, QuantityHistoryEntry,
};
use crate::services::{
    cash_service, customer_service, dashboard_service, employee_service, expense_service,
    payroll_service, product_service, purchase_service, sales_service, settings_service,
    supplier_service, scale_service, drawer_service, user_service,
};
use std::collections::HashMap;
use tauri::State;

/// Silent print — 100% native Windows print API (GDI), no dialogs, no
/// viewers, no PDF handlers:
///   1. Rasterize the HTML with the OS-bundled headless Edge (isolated
///      profile — a running Edge can never hijack the job).
///   2. GDI-print the bitmap on a printer DC whose DEVMODE is locked to the
///      exact page (receipts measure their own dynamic height).
/// There is NO browser print and NO PDF/print-verb fallback: on failure the
/// caller gets a real error message instead of a surprise dialog.
#[tauri::command]
pub fn print_html_direct(db: State<'_, DbState>, html: String, title: String, paper: Option<PrintPaperSpec>) -> Result<(), String> {
    let settings = settings_service::get_all_settings(&db).unwrap_or_default();

    let width_mm = paper.as_ref().map(|p| p.width_mm).unwrap_or(80.0);
    let fixed_height_mm = paper.as_ref().and_then(|p| p.height_mm);
    // Receipt printer override (Settings → Printing); empty = Windows default.
    let printer = settings.get("invoice_printer_name").cloned().unwrap_or_default();
    let printer_opt = if printer.trim().is_empty() { None } else { Some(printer.as_str()) };
    let dpi: u32 = settings
        .get("receipt_printer_dpi")
        .and_then(|v| v.parse().ok())
        .unwrap_or(203);

    let outcome = match fixed_height_mm {
        Some(h) => {
            // Fixed-size page (documents/labels): exact-media pipeline.
            let req = crate::printing::label_gdi::LabelPrintRequest {
                html: html.clone(),
                printer: printer_opt.map(|s| s.to_string()),
                width_mm,
                height_mm: h,
                copies: 1,
                dpi: Some(dpi),
                label: title.clone(),
            };
            crate::printing::label_gdi::print_label_job(&req)
        }
        None => crate::printing::label_gdi::print_receipt_job(
            &html, width_mm, printer_opt, dpi, &title,
        ),
    };
    if outcome.ok {
        println!(
            "[silent-print] ok: mode={} {}",
            outcome.diagnostics.mode, outcome.message
        );
        return Ok(());
    }
    Err(format!(
        "Native print failed ({}): {}",
        outcome.diagnostics.mode, outcome.message
    ))
}

/// Paper geometry for a silent print job (frontend `paper` parameter).
#[derive(Debug, serde::Deserialize)]
pub struct PrintPaperSpec {
    pub width_mm: f64,
    #[serde(default)]
    pub height_mm: Option<f64>,
}

/// Silent exact-media label printing: rasterizes one label at the printer's
/// DPI and prints `copies` pages on a DEVMODE locked to width×height mm —
/// no A4/default stock, no gaps, no print dialog. Returns diagnostics
/// (printer, media, copies, page count, raster size) for verification.
#[tauri::command]
pub fn print_label_job(
    db: State<'_, DbState>,
    request: crate::printing::label_gdi::LabelPrintRequest,
) -> Result<crate::printing::label_gdi::LabelPrintResult, String> {
    let _ = db;
    let result = crate::printing::label_gdi::print_label_job(&request);
    // Surface the diagnostics in dev consoles; the UI receives them too.
    if result.ok {
        println!(
            "[label-print] ok: printer={} media={}×{}mm copies={} pages={} dpi={} raster={}×{}px",
            result.diagnostics.printer,
            result.diagnostics.media_width_mm,
            result.diagnostics.media_height_mm,
            result.diagnostics.copies,
            result.diagnostics.page_count,
            result.diagnostics.dpi,
            result.diagnostics.raster_width_px,
            result.diagnostics.raster_height_px,
        );
    } else {
        eprintln!("[label-print] FAILED ({}): {}", result.diagnostics.mode, result.message);
    }
    Ok(result)
}


#[tauri::command]
pub fn login(db: State<'_, DbState>, username: String, password: String) -> Result<Option<User>, String> {
    let user = authenticate_user(&db, &username, &password)?;
    // OFFLINE CLIENT (user decision 2026-09-08): this command only runs
    // locally when the terminal is NOT connected (the wrapper forwards
    // 'login' to the server otherwise). A client-role success here is a
    // local fallback — remember the credentials so the network layer can
    // upgrade the session to a real server token when the server returns.
    if user.is_some() && crate::network::is_offline_fallback_login() {
        crate::network::note_offline_login(&username, &password);
    }
    Ok(user)
}

#[tauri::command]
pub fn get_active_users(db: State<'_, DbState>) -> Result<Vec<User>, String> {
    crate::auth::list_active_users(&db)
}

#[tauri::command]
pub fn change_user_password(
    db: State<'_, DbState>,
    user_id: i64,
    new_password: String,
    old_password: Option<String>,
) -> Result<(), String> {
    employee_service::change_user_password(&db, user_id, &new_password, old_password)
}

#[tauri::command]
pub fn get_user_by_qr(db: State<'_, DbState>, qr_code: String) -> Result<Option<User>, String> {
    employee_service::get_user_by_qr(&db, &qr_code)
}

#[tauri::command]
pub fn get_dashboard_stats(db: State<'_, DbState>, start_date: Option<String>, end_date: Option<String>) -> Result<DashboardStats, String> {
    dashboard_service::get_stats(&db, start_date, end_date)
}

// Cash
#[tauri::command]
pub fn get_active_cash_session(db: State<'_, DbState>, _user_id: i64) -> Result<Option<CashSession>, String> {
    cash_service::get_active_session(&db, _user_id)
}

#[tauri::command]
pub fn open_cash_session(
    db: State<'_, DbState>,
    user_id: i64,
    register_id: i64,
    opening_amount: i64,
    notes: Option<String>,
) -> Result<CashSession, String> {
    cash_service::open_session(&db, user_id, register_id, opening_amount, notes)
}

#[tauri::command]
pub fn add_cash_movement(
    db: State<'_, DbState>,
    session_id: i64,
    user_id: i64,
    movement_type: String,
    amount: i64,
    reason: Option<String>,
) -> Result<(), String> {
    cash_service::add_cash_movement(&db, session_id, user_id, &movement_type, amount, reason)
}

#[tauri::command]
pub fn close_cash_session(
    db: State<'_, DbState>,
    session_id: i64,
    actual_cash: i64,
    notes: Option<String>,
) -> Result<(), String> {
    cash_service::close_session(&db, session_id, actual_cash, notes)
}

#[tauri::command]
pub fn list_cash_movements(db: State<'_, DbState>, session_id: i64) -> Result<Vec<CashMovement>, String> {
    cash_service::list_movements(&db, session_id)
}

#[tauri::command]
pub fn list_session_history(
    db: State<'_, DbState>,
    from_date: Option<String>,
    to_date: Option<String>,
    include_archived: Option<bool>,
) -> Result<Vec<CashSession>, String> {
    cash_service::list_session_history(&db, from_date, to_date, include_archived)
}

#[tauri::command]
pub fn edit_opening_balance(
    db: State<'_, DbState>,
    session_id: i64,
    new_amount: i64,
    reason: String,
    admin_password: Option<String>,
) -> Result<(), String> {
    cash_service::edit_opening_balance(&db, session_id, new_amount, reason, admin_password)
}

#[tauri::command]
pub fn edit_cash_session(
    db: State<'_, DbState>,
    session_id: i64,
    opening_amount: Option<i64>,
    actual_cash: Option<i64>,
    notes: Option<String>,
    admin_password: Option<String>,
) -> Result<(), String> {
    cash_service::edit_cash_session(&db, session_id, opening_amount, actual_cash, notes, admin_password)
}

#[tauri::command]
pub fn archive_cash_session(
    db: State<'_, DbState>,
    session_id: i64,
    archived: bool,
    admin_password: Option<String>,
) -> Result<(), String> {
    cash_service::archive_cash_session(&db, session_id, archived, admin_password)
}

#[tauri::command]
pub fn delete_cash_session(
    db: State<'_, DbState>,
    session_id: i64,
    admin_password: Option<String>,
) -> Result<(), String> {
    cash_service::delete_cash_session(&db, session_id, admin_password)
}

// Products & Categories
#[tauri::command]
pub fn search_products(
    db: State<'_, DbState>,
    query: String,
    category_id: Option<i64>,
    search_type: String,
) -> Result<Vec<Product>, String> {
    product_service::search_products(&db, &query, category_id, &search_type)
}

#[tauri::command]
pub fn save_product(
    db: State<'_, DbState>,
    input: ProductInput,
    product_id: Option<i64>,
    user_id: Option<i64>,
) -> Result<i64, String> {
    product_service::save_product(&db, input, product_id, user_id)
}

#[tauri::command]
pub fn delete_product(db: State<'_, DbState>, product_id: i64) -> Result<(), String> {
    product_service::delete_product(&db, product_id)
}

#[tauri::command]
pub fn get_categories(db: State<'_, DbState>) -> Result<Vec<Category>, String> {
    product_service::get_categories(&db)
}

#[tauri::command]
pub fn save_category(
    db: State<'_, DbState>,
    name_ar: String,
    name_fr: String,
    name_en: String,
    color: String,
    category_id: Option<i64>,
) -> Result<i64, String> {
    product_service::save_category(&db, &name_ar, &name_fr, &name_en, &color, category_id)
}

#[tauri::command]
pub fn delete_category(db: State<'_, DbState>, category_id: i64) -> Result<(), String> {
    product_service::delete_category(&db, category_id)
}

#[tauri::command]
pub fn get_units(db: State<'_, DbState>) -> Result<Vec<Unit>, String> {
    product_service::get_units(&db)
}

#[tauri::command]
pub fn list_packagings(db: State<'_, DbState>, product_id: i64) -> Result<Vec<ProductPackaging>, String> {
    product_service::list_packagings(&db, product_id)
}

#[tauri::command]
pub fn save_packagings(db: State<'_, DbState>, product_id: i64, inputs: Vec<PackagingInput>) -> Result<(), String> {
    product_service::save_packagings(&db, product_id, inputs)
}

#[tauri::command]
pub fn toggle_product_pin(db: State<'_, DbState>, product_id: i64, pinned: bool) -> Result<(), String> {
    product_service::toggle_product_pin(&db, product_id, pinned)
}

#[tauri::command]
pub fn reorder_pinned_products(db: State<'_, DbState>, ordered_ids: Vec<i64>) -> Result<(), String> {
    product_service::reorder_pinned_products(&db, ordered_ids)
}

// Sales & Held Sales
#[tauri::command]
pub fn process_sale(db: State<'_, DbState>, input: CreateSaleInput) -> Result<String, String> {
    sales_service::process_sale(&db, input)
}

#[tauri::command]
pub fn create_sale(db: State<'_, DbState>, input: CreateSaleInput) -> Result<String, String> {
    sales_service::process_sale(&db, input)
}

#[tauri::command]
pub fn list_sales(
    db: State<'_, DbState>,
    start_date: Option<String>,
    end_date: Option<String>,
    user_id: Option<i64>,
    limit: i64,
) -> Result<Vec<Sale>, String> {
    sales_service::list_sales(&db, start_date, end_date, user_id, limit)
}

#[tauri::command]
pub fn get_sale_items(db: State<'_, DbState>, sale_id: i64) -> Result<Vec<CartItem>, String> {
    sales_service::get_sale_items(&db, sale_id)
}

#[tauri::command]
pub fn hold_sale(
    db: State<'_, DbState>,
    user_id: Option<i64>,
    customer_id: Option<i64>,
    cart_json: Option<String>,
    cart_data_json: Option<String>,
    _total_amount: Option<i64>,
    note: Option<String>,
    notes: Option<String>,
) -> Result<i64, String> {
    let uid = user_id.unwrap_or(1);
    let raw_json = cart_json.or(cart_data_json).unwrap_or_else(|| "[]".to_string());
    let final_note = note.or(notes);
    sales_service::hold_sale(&db, uid, customer_id, &raw_json, final_note)
}

#[tauri::command]
pub fn list_held_sales(db: State<'_, DbState>) -> Result<Vec<HeldSale>, String> {
    sales_service::list_held_sales(&db)
}

#[tauri::command]
pub fn delete_held_sale(db: State<'_, DbState>, held_id: i64) -> Result<(), String> {
    sales_service::delete_held_sale(&db, held_id)
}

// Customers & Debt
#[tauri::command]
pub fn list_customers(db: State<'_, DbState>) -> Result<Vec<Customer>, String> {
    customer_service::list_customers(&db)
}

#[tauri::command]
pub fn save_customer(
    db: State<'_, DbState>,
    name: String,
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
    customer_service::save_customer(
        &db, &name, phone, email, address, rc, nif, nis, ai, initial_debt, notes, customer_id,
    )
}

#[tauri::command]
pub fn delete_customer(db: State<'_, DbState>, customer_id: i64) -> Result<(), String> {
    customer_service::delete_customer(&db, customer_id)
}

#[tauri::command]
pub fn toggle_customer_pin(db: State<'_, DbState>, customer_id: i64, pinned: bool) -> Result<(), String> {
    customer_service::toggle_customer_pin(&db, customer_id, pinned)
}

#[tauri::command]
pub fn record_customer_debt_payment(db: State<'_, DbState>, input: CustomerPaymentInput) -> Result<i64, String> {
    customer_service::record_customer_debt_payment(&db, input)
}

// Suppliers & Purchases
#[tauri::command]
pub fn list_suppliers(db: State<'_, DbState>) -> Result<Vec<Supplier>, String> {
    supplier_service::list_suppliers(&db)
}

#[tauri::command]
pub fn save_supplier(
    db: State<'_, DbState>,
    name: String,
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
    supplier_service::save_supplier(
        &db, &name, contact_person, phone, email, address, rc, nif, nis, ai, notes, supplier_id,
    )
}

#[tauri::command]
pub fn delete_supplier(db: State<'_, DbState>, supplier_id: i64) -> Result<(), String> {
    supplier_service::delete_supplier(&db, supplier_id)
}

#[tauri::command]
pub fn toggle_supplier_pin(db: State<'_, DbState>, supplier_id: i64, pinned: bool) -> Result<(), String> {
    supplier_service::toggle_supplier_pin(&db, supplier_id, pinned)
}

#[tauri::command]
pub fn record_supplier_debt_payment(db: State<'_, DbState>, input: SupplierPaymentInput) -> Result<i64, String> {
    supplier_service::record_supplier_debt_payment(&db, input)
}

#[tauri::command]
pub fn list_supplier_debt_payments(db: State<'_, DbState>, supplier_id: i64) -> Result<Vec<SupplierPaymentRow>, String> {
    supplier_service::list_supplier_debt_payments(&db, supplier_id)
}

#[tauri::command]
pub fn create_purchase(db: State<'_, DbState>, input: CreatePurchaseInput) -> Result<String, String> {
    purchase_service::create_purchase(&db, input)
}

#[tauri::command]
pub fn get_purchase_items(db: State<'_, DbState>, purchase_id: i64) -> Result<Vec<PurchaseItem>, String> {
    purchase_service::get_purchase_items(&db, purchase_id)
}

#[tauri::command]
pub fn delete_purchase(db: State<'_, DbState>, purchase_id: i64, user_id: Option<i64>) -> Result<(), String> {
    purchase_service::delete_purchase(&db, purchase_id, user_id)
}

#[tauri::command]
pub fn list_purchases(db: State<'_, DbState>) -> Result<Vec<Purchase>, String> {
    purchase_service::list_purchases(&db)
}

// Expenses
#[tauri::command]
pub fn add_expense(
    db: State<'_, DbState>,
    category_id: i64,
    amount: i64,
    payment_method: String,
    session_id: Option<i64>,
    user_id: i64,
    recipient: Option<String>,
    receipt_reference: Option<String>,
    notes: Option<String>,
    date: Option<String>,
) -> Result<String, String> {
    expense_service::add_expense(
        &db, category_id, amount, &payment_method, session_id, user_id, recipient, receipt_reference, notes, date,
    )
}

#[tauri::command]
pub fn update_expense(
    db: State<'_, DbState>,
    expense_id: i64,
    category_id: i64,
    amount: i64,
    payment_method: String,
    recipient: Option<String>,
    receipt_reference: Option<String>,
    notes: Option<String>,
    date: String,
) -> Result<(), String> {
    expense_service::update_expense(
        &db, expense_id, category_id, amount, &payment_method, recipient, receipt_reference, notes, date,
    )
}

#[tauri::command]
pub fn list_expenses(db: State<'_, DbState>) -> Result<Vec<Expense>, String> {
    expense_service::list_expenses(&db)
}

#[tauri::command]
pub fn delete_expense(db: State<'_, DbState>, expense_id: i64) -> Result<(), String> {
    expense_service::delete_expense(&db, expense_id)
}

// Employees & Payroll
#[tauri::command]
pub fn list_employees(db: State<'_, DbState>) -> Result<Vec<Employee>, String> {
    employee_service::list_employees(&db)
}

#[tauri::command]
pub fn save_employee(
    db: State<'_, DbState>,
    code: String,
    name: String,
    phone: Option<String>,
    email: Option<String>,
    national_id: Option<String>,
    job_title: String,
    base_salary: i64,
    salary_type: String,
    salary_start_date: Option<String>,
    hire_date: String,
    notes: Option<String>,
    rfid_code: Option<String>,
    employee_id: Option<i64>,
) -> Result<i64, String> {
    employee_service::save_employee(
        &db, &code, &name, phone, email, national_id, &job_title, base_salary, &salary_type, salary_start_date, &hire_date, notes, rfid_code, employee_id,
    )
}

/// Look up an employee by a scanned RFID tag (also matches their QR code).
#[tauri::command]
pub fn find_employee_by_rfid(db: State<'_, DbState>, rfid: String) -> Result<Option<Employee>, String> {
    employee_service::find_employee_by_rfid(&db, &rfid)
}

/// RFID login: resolve a scanned card to the employee's active user account.
#[tauri::command]
pub fn login_with_rfid(db: State<'_, DbState>, rfid: String) -> Result<Option<User>, String> {
    let user = employee_service::login_with_rfid(&db, &rfid)?;
    // Offline client fallback: badge logins upgrade on reconnect too.
    if user.is_some() && crate::network::is_offline_fallback_login() {
        crate::network::note_offline_badge_login(&rfid);
    }
    Ok(user)
}

/// Resolve a scanned scale barcode (ACLAS price/weight-embedded EAN) to the
/// scalable product. The WEIGHT rides back as the suggested quantity.
#[tauri::command]
pub fn resolve_scale_scan(db: State<'_, DbState>, code: String) -> Result<Option<ScaleScanResult>, String> {
    let settings = settings_service::get_all_settings(&db)?;
    let btype: i64 = settings
        .get("scale_default_barcode_type")
        .and_then(|v| v.parse().ok())
        .unwrap_or(97);
    let Some((item_code, weight, price)) =
        scale_service::parse_scale_barcode_code(&code, btype)
    else {
        return Ok(None);
    };
    let Some(product) = scale_service::resolve_scale_scan(&db, &code) else {
        return Ok(None);
    };
    Ok(Some(ScaleScanResult {
        product_id: product.id,
        name: Some(product.name_fr.clone()),
        unit_price: if price > 0 { price } else { product.sale_price },
        weight,
    }))
}

#[derive(Debug, serde::Serialize)]
pub struct ScaleScanResult {
    pub product_id: i64,
    pub name: Option<String>,
    pub unit_price: i64,
    pub weight: f64,
}

/// Browser-free printing fallback: send a ready-made ESC/POS byte payload
/// (text commands, cut) RAW to the Windows spooler — no driver dialog, no
/// browser, no PDF. Works on any thermal receipt printer even on machines
/// with NO Chrome/Edge installed.
#[tauri::command]
pub fn print_escpos_raw(payload: String, printer: Option<String>) -> Result<(), String> {
    crate::printing::escpos::print_raw(&payload, printer.as_deref())
}

/// Next free employee code (EMP-NN) — accounts for soft-deleted rows.
#[tauri::command]
pub fn next_employee_code(db: State<'_, DbState>) -> Result<String, String> {
    employee_service::next_employee_code(&db)
}

#[tauri::command]
pub fn delete_employee(db: State<'_, DbState>, employee_id: i64) -> Result<(), String> {
    employee_service::delete_employee(&db, employee_id)
}

#[tauri::command]
pub fn record_employee_advance(db: State<'_, DbState>, input: EmployeeAdvanceInput) -> Result<i64, String> {
    payroll_service::record_employee_advance(&db, input)
}

#[tauri::command]
pub fn list_employee_advances(db: State<'_, DbState>, employee_id: Option<i64>, month: Option<String>) -> Result<Vec<EmployeeAdvance>, String> {
    payroll_service::list_employee_advances(&db, employee_id, month)
}

#[tauri::command]
pub fn list_payrolls(db: State<'_, DbState>) -> Result<Vec<Payroll>, String> {
    payroll_service::list_payrolls(&db)
}

// Settings & Activation
#[tauri::command]
pub fn get_all_settings(db: State<'_, DbState>) -> Result<HashMap<String, String>, String> {
    settings_service::get_all_settings(&db)
}

#[tauri::command]
pub fn set_setting(db: State<'_, DbState>, key: String, value: String) -> Result<(), String> {
    settings_service::set_setting(&db, &key, &value)
}

/// Clear Sales & Purchase History: wipes transactional history ONLY —
/// products, prices, stock, customers, suppliers and their debts stay.
#[tauri::command]
pub fn clear_transaction_history(db: State<'_, DbState>, confirm_text: String) -> Result<String, String> {
    if confirm_text.trim() != "CLEAR HISTORY" {
        return Err("Type CLEAR HISTORY to confirm / اكتب CLEAR HISTORY للتأكيد".to_string());
    }
    let mut conn = db.conn.lock().unwrap();
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    // Order matters: expenses & the debt-payment tables carry session_id
    // FKs into cash_sessions and employee_advances reference expenses —
    // deleting cash_sessions first failed the whole transaction with
    // "foreign key constraint failed".
    let tables = [
        "sale_payments",
        "sale_items",
        "sales",
        "purchase_items",
        "purchases",
        "employee_advances",
        "expenses",
        "customer_debt_payments",
        "supplier_debt_payments",
        "cash_movements",
        "cash_sessions",
        "inventory_movements",
        "held_sales",
        "product_price_history",
    ];
    let mut cleared = 0;
    for t in tables {
        let n = tx.execute(&format!("DELETE FROM {}", t), []).map_err(|e| e.to_string())?;
        cleared += n;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(format!("Cleared {} history rows", cleared))
}

#[tauri::command]
pub fn get_setting(db: State<'_, DbState>, key: String) -> Result<Option<String>, String> {
    let all = settings_service::get_all_settings(&db)?;
    Ok(all.get(&key).cloned().filter(|v| !v.is_empty()))
}

/// Price change log for a product, newest first.
#[tauri::command]
pub fn get_price_history(db: State<'_, DbState>, product_id: i64) -> Result<Vec<PriceHistoryEntry>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, old_purchase_price, new_purchase_price, old_sale_price, new_sale_price, user_id, created_at
             FROM product_price_history WHERE product_id = ?1 ORDER BY id DESC LIMIT 100",
        )
        .map_err(|e| e.to_string())?;

    let rows = stmt
        .query_map([product_id], |row| {
            Ok(PriceHistoryEntry {
                id: row.get(0)?,
                old_purchase_price: row.get(1)?,
                new_purchase_price: row.get(2)?,
                old_sale_price: row.get(3)?,
                new_sale_price: row.get(4)?,
                user_id: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| e.to_string())?;

    Ok(rows.filter_map(|r| r.ok()).collect())
}

/// Quantity change log for a product: old → new, delta, type, user, time.
#[tauri::command]
pub fn get_quantity_history(db: State<'_, DbState>, product_id: i64) -> Result<Vec<QuantityHistoryEntry>, String> {
    crate::services::product_service::get_quantity_history(&db, product_id)
}

#[tauri::command]
pub fn send_telegram_message(db: State<'_, DbState>, text: String) -> Result<(), String> {
    // Manual/test send — blocking is fine here so the UI shows the result.
    let (token, chat_id, _) = crate::services::notifier_service::get_telegram_config(&db)
        .ok_or("Telegram bot token / chat ID not configured")?;
    crate::services::notifier_service::send_telegram_blocking(&token, &chat_id, &text)
}

#[tauri::command]
pub fn send_telegram_recap(db: State<'_, DbState>) -> Result<String, String> {
    crate::services::notifier_service::send_periodic_recap(&db)
}

#[tauri::command]
pub fn set_multiple_settings(db: State<'_, DbState>, settings: HashMap<String, String>) -> Result<(), String> {
    settings_service::set_multiple_settings(&db, settings)
}

#[tauri::command]
pub fn get_hwid() -> String {
    settings_service::get_hwid()
}

#[tauri::command]
pub fn verify_license(db: State<'_, DbState>, code: String) -> Result<bool, String> {
    settings_service::verify_license(&db, &code)
}

/// Online activation: checks the GitHub-hosted license registry for this
/// machine's HWID (license files live in licenses/<HWID>.json on the repo).
#[tauri::command]
pub fn activate_online(db: State<'_, DbState>) -> Result<bool, String> {
    let hwid = settings_service::get_hwid();
    settings_service::activate_online_github(&db, &hwid, "titaou-bedreddine", "TitaouPosCRM-licenses")
}

#[tauri::command]
pub fn factory_reset(db: State<'_, DbState>, reset_type: String) -> Result<(), String> {
    settings_service::factory_reset(&db, &reset_type)
}

// Scale Integration (ACLAS Native SDK)
#[tauri::command]
pub fn test_scale_connection(ip: String, port: u32, protocol_type: u32) -> Result<String, String> {
    scale_service::test_scale_connection(&ip, port, protocol_type)
}

#[tauri::command]
pub fn upload_product_to_scale(
    db: State<'_, DbState>,
    product_id: i64,
    ip: String,
    port: u32,
    protocol_type: u32,
    default_dept: i64,
    default_barcode_type: i64,
    user_name: Option<String>,
) -> Result<usize, String> {
    let products = product_service::search_products(&db, "", None, "all")?;
    let target: Vec<Product> = products.into_iter().filter(|p| p.id == product_id).collect();
    if target.is_empty() {
        return Err("Product not found".to_string());
    }
    scale_service::upload_products_to_scale(
        &db, &target, &ip, port, protocol_type, default_dept, default_barcode_type, user_name,
    )
}

#[tauri::command]
pub fn upload_all_scalable_to_scale(
    db: State<'_, DbState>,
    ip: String,
    port: u32,
    protocol_type: u32,
    default_dept: i64,
    default_barcode_type: i64,
    user_name: Option<String>,
) -> Result<usize, String> {
    let products = product_service::search_products(&db, "", None, "all")?;
    let target: Vec<Product> = products.into_iter().filter(|p| p.is_scalable).collect();
    if target.is_empty() {
        return Err("No scalable products found to synchronize".to_string());
    }
    scale_service::upload_products_to_scale(
        &db, &target, &ip, port, protocol_type, default_dept, default_barcode_type, user_name,
    )
}

#[tauri::command]
pub fn fetch_products_from_scale(
    db: State<'_, DbState>,
    ip: String,
    port: u32,
    protocol_type: u32,
    user_name: Option<String>,
) -> Result<usize, String> {
    scale_service::fetch_products_from_scale(&db, &ip, port, protocol_type, user_name)
}

#[tauri::command]
pub fn get_scale_sync_logs(db: State<'_, DbState>) -> Result<Vec<crate::models::ScaleSyncLog>, String> {
    scale_service::get_sync_logs(&db)
}

// Direct Serial Cash Drawer
#[tauri::command]
pub fn open_serial_cash_drawer(com_port: u32, baud_rate: u32) -> Result<String, String> {
    drawer_service::open_serial_cash_drawer(com_port, baud_rate)
}

// Additional commands

#[tauri::command]
pub fn delete_sale(db: State<'_, DbState>, sale_id: i64, user_id: Option<i64>) -> Result<(), String> {
    sales_service::delete_sale(&db, sale_id, user_id)
}

#[tauri::command]
pub fn verify_admin_password(db: State<'_, DbState>, password: String) -> Result<bool, String> {
    crate::auth::verify_admin_password(&db, &password)
}

#[tauri::command]
pub fn save_unit(
    db: State<'_, DbState>,
    name: String,
    short_name: String,
    allow_decimals: bool,
    unit_id: Option<i64>,
) -> Result<i64, String> {
    product_service::save_unit(&db, &name, &short_name, allow_decimals, unit_id)
}

#[tauri::command]
pub fn backup_database(destination_path: String) -> Result<String, String> {
    settings_service::backup_database(&destination_path)
}

/// Native Windows FOLDER picker (System.Windows.Forms.FolderBrowserDialog
/// via PowerShell) — no browser dialog, no webview input.
#[tauri::command]
pub fn pick_backup_folder() -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms | Out-Null
$f = New-Object System.Windows.Forms.FolderBrowserDialog
$f.Description = 'Select backup folder'
$f.ShowNewFolderButton = $true
if ($f.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  Write-Output $f.SelectedPath
}
"#;
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW for the shell host
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }
    #[cfg(not(windows))]
    Ok(None)
}

/// Native Windows FILE picker for the backup to restore (.sqlite).
#[tauri::command]
pub fn pick_backup_file() -> Result<Option<String>, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = r#"
Add-Type -AssemblyName System.Windows.Forms | Out-Null
$o = New-Object System.Windows.Forms.OpenFileDialog
$o.Filter = 'SQLite backup (*.sqlite)|*.sqlite|All files (*.*)|*.*'
$o.Title = 'Select the backup file to restore'
if ($o.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  Write-Output $o.FileName
}
"#;
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", script])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            Ok(None)
        } else {
            Ok(Some(text))
        }
    }
    #[cfg(not(windows))]
    Ok(None)
}

/// Backup Now — timestamped copy into the configured folder + retention.
#[tauri::command]
pub fn create_backup(db: State<'_, DbState>, tag: Option<String>) -> Result<String, String> {
    // A connected CLIENT terminal has no authoritative database: backups of
    // the shop data belong to the server PC (spec §51). Everything else
    // (single PC, standalone, server) backs up its own SQLite as before.
    let status = crate::network::status_snapshot();
    if status.get("role").and_then(|r| r.as_str()) == Some("client") {
        return Err("This terminal is a client — the shop database is backed up on the server PC.".to_string());
    }
    settings_service::create_backup(&db, tag.as_deref().unwrap_or("manual"))
}

/// Scheduled daily backup tick ("every day at HH:MM"): the backend decides
/// whether the window has been reached and stamps the day — safe to call
/// every minute. The UI must NOT call create_backup directly for the
/// schedule, or it would back up on every tick.
#[tauri::command]
pub fn run_scheduled_backup(db: State<'_, DbState>) -> Result<(), String> {
    // Client terminals never back up the (server-owned) shop database.
    let status = crate::network::status_snapshot();
    if status.get("role").and_then(|r| r.as_str()) == Some("client") {
        return Ok(());
    }
    settings_service::run_scheduled_backup(&db);
    Ok(())
}

/// List existing backups (newest first) for the Settings UI.
#[tauri::command]
pub fn list_backups(db: State<'_, DbState>) -> Result<Vec<settings_service::BackupInfo>, String> {
    settings_service::list_backups(&db)
}

/// Pre-restore validation: is the picked file a real SQLite backup?
#[tauri::command]
pub fn validate_backup_file(path: String) -> Result<String, String> {
    settings_service::validate_backup_file(&path)
}

#[tauri::command]
pub fn restore_database(source_backup_path: String) -> Result<String, String> {
    settings_service::restore_database(&source_backup_path)
}

/// Restore ONLY application settings from a backup's settings snapshot —
/// business data stays untouched.
#[tauri::command]
pub fn restore_settings_only(source_backup_path: String) -> Result<usize, String> {
    settings_service::restore_settings_only(&source_backup_path)
}

// ---------------------------------------------------------------------------
// Debt clearing (admin-gated forgiveness, no cash movement)
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn clear_customer_debt(
    db: State<'_, DbState>,
    customer_id: i64,
    reason: Option<String>,
    admin_password: String,
    user_id: Option<i64>,
) -> Result<i64, String> {
    customer_service::clear_customer_debt(&db, customer_id, reason, &admin_password, user_id)
}

#[tauri::command]
pub fn clear_supplier_debt(
    db: State<'_, DbState>,
    supplier_id: i64,
    reason: Option<String>,
    admin_password: String,
    user_id: Option<i64>,
) -> Result<i64, String> {
    supplier_service::clear_supplier_debt(&db, supplier_id, reason, &admin_password, user_id)
}

/// Cleared-debt archive (both customers and suppliers), newest first.
#[tauri::command]
pub fn list_debt_clear_log(db: State<'_, DbState>) -> Result<Vec<DebtClearEntry>, String> {
    let conn = db.conn.lock().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, entity_type, entity_id, entity_name, previous_debt, new_debt, reason, user_name, created_at
             FROM debt_clear_log ORDER BY id DESC LIMIT 200",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok(DebtClearEntry {
                id: row.get(0)?,
                entity_type: row.get(1)?,
                entity_id: row.get(2)?,
                entity_name: row.get(3)?,
                previous_debt: row.get(4)?,
                new_debt: row.get(5)?,
                reason: row.get(6)?,
                user_name: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[derive(Debug, serde::Serialize)]
pub struct DebtClearEntry {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub entity_name: Option<String>,
    pub previous_debt: i64,
    pub new_debt: i64,
    pub reason: Option<String>,
    pub user_name: Option<String>,
    pub created_at: String,
}

/// The most recent sale + its items — "Reopen/Reprint Last Receipt".
#[tauri::command]
pub fn get_last_sale(db: State<'_, DbState>) -> Result<Option<LastSalePayload>, String> {
    let Some((sale, items)) = sales_service::get_last_sale(&db)? else {
        return Ok(None);
    };
    Ok(Some(LastSalePayload { sale, items }))
}

/// One sale by receipt number + its items â receipt QR scan in the POS
/// (`SALE:<sale_number>`) opens the sale here for reprint/edit.
#[tauri::command]
pub fn get_sale_by_number(db: State<'_, DbState>, saleNumber: String) -> Result<Option<LastSalePayload>, String> {
    let Some((sale, items)) = sales_service::get_sale_by_number(&db, &saleNumber)? else {
        return Ok(None);
    };
    Ok(Some(LastSalePayload { sale, items }))
}

#[derive(Debug, serde::Serialize)]
pub struct LastSalePayload {
    pub sale: crate::models::Sale,
    pub items: Vec<crate::models::CartItem>,
}

/// One payroll-reminder scheduler tick (called periodically by the UI).
#[tauri::command]
pub fn run_payroll_reminder_scan(db: State<'_, DbState>) -> Result<i64, String> {
    Ok(crate::services::payroll_reminder_service::run_payroll_reminder_scan(&db))
}

/// Persistent in-app notification feed (payroll reminders etc.), newest
/// first; dismissed rows are filtered out.
#[tauri::command]
pub fn list_app_notifications(db: State<'_, DbState>, limit: Option<i64>) -> Result<Vec<AppNotification>, String> {
    let conn = db.conn.lock().unwrap();
    let _ = conn.execute(
        "CREATE TABLE IF NOT EXISTS app_notification_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            type TEXT NOT NULL,
            title TEXT NOT NULL,
            message TEXT NOT NULL,
            related_id INTEGER,
            is_dismissed INTEGER DEFAULT 0,
            created_at TEXT DEFAULT (datetime('now','localtime'))
        )",
        [],
    );
    let mut stmt = conn
        .prepare(
            "SELECT id, type, title, message, related_id, created_at
             FROM app_notification_log WHERE is_dismissed = 0
             ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([limit.unwrap_or(100)], |row| {
            Ok(AppNotification {
                id: row.get(0)?,
                ntype: row.get(1)?,
                title: row.get(2)?,
                message: row.get(3)?,
                related_id: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| e.to_string())?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

#[derive(Debug, serde::Serialize)]
pub struct AppNotification {
    pub id: i64,
    #[serde(rename = "type")]
    pub ntype: String,
    pub title: String,
    pub message: String,
    pub related_id: Option<i64>,
    pub created_at: String,
}

/// Dismiss one in-app notification.
#[tauri::command]
pub fn dismiss_app_notification(db: State<'_, DbState>, id: i64) -> Result<(), String> {
    let conn = db.conn.lock().unwrap();
    conn.execute("UPDATE app_notification_log SET is_dismissed = 1 WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Real embedded-server status (port, LAN IPs, uptime, live devices) for
/// the Network settings tab + mobile pairing QR.
#[tauri::command]
pub fn get_server_status() -> Result<serde_json::Value, String> {
    Ok(crate::server::server_status())
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// User & Role Management Commands
#[tauri::command]
pub fn get_all_users(db: State<'_, DbState>) -> Result<Vec<UserAccount>, String> {
    user_service::get_all_users(&db)
}

#[tauri::command]
pub fn get_all_roles(db: State<'_, DbState>) -> Result<Vec<Role>, String> {
    user_service::get_all_roles(&db)
}

#[tauri::command]
pub fn create_user(
    db: State<'_, DbState>,
    username: String,
    display_name: String,
    password: String,
    role_id: Option<i64>,
    max_discount_percent: f64,
) -> Result<i64, String> {
    user_service::create_user(&db, &username, &display_name, &password, role_id, max_discount_percent)
}

#[tauri::command]
pub fn update_user(
    db: State<'_, DbState>,
    user_id: i64,
    username: String,
    display_name: String,
    role_id: Option<i64>,
    max_discount_percent: f64,
    is_active: bool,
    new_password: Option<String>,
) -> Result<(), String> {
    user_service::update_user(
        &db,
        user_id,
        &username,
        &display_name,
        role_id,
        max_discount_percent,
        is_active,
        new_password,
    )
}

/// Pin/unpin a login user (pinned users list first on the login screen).
#[tauri::command]
pub fn toggle_user_pin(db: State<'_, DbState>, user_id: i64, pinned: bool) -> Result<(), String> {
    user_service::toggle_user_pin(&db, user_id, pinned)
}

/// Record an employee absence (persisted).
#[tauri::command]
pub fn record_employee_absence(
    db: State<'_, DbState>,
    employee_id: i64,
    days: i64,
    reason: Option<String>,
    date: String,
) -> Result<i64, String> {
    payroll_service::record_employee_absence(&db, employee_id, days, reason, date)
}

/// Absence history rows (id, employee_id, days, reason, date).
#[tauri::command]
pub fn list_employee_absences(
    db: State<'_, DbState>,
    employee_id: Option<i64>,
    month: Option<String>,
) -> Result<Vec<(i64, i64, i64, Option<String>, String)>, String> {
    payroll_service::list_employee_absences(&db, employee_id, month)
}

/// Re-checkout a sale edited from history in place (no duplicate row).
#[tauri::command]
pub fn replace_sale(
    db: State<'_, DbState>,
    original_sale_id: i64,
    input: CreateSaleInput,
) -> Result<String, String> {
    sales_service::replace_sale(&db, original_sale_id, input)
}

#[tauri::command]
pub fn delete_user(db: State<'_, DbState>, user_id: i64) -> Result<(), String> {
    user_service::delete_user(&db, user_id)
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AppUpdateResult {
    pub has_update: bool,
    pub current_version: String,
    pub latest_version: String,
    pub tag_name: String,
    pub release_name: String,
    pub release_notes: String,
    pub release_url: String,
    pub download_url: String,
    pub published_at: String,
}

#[tauri::command]
pub async fn check_github_update(app_handle: tauri::AppHandle) -> Result<AppUpdateResult, String> {
    let current_version = app_handle.package_info().version.to_string();

    let client = reqwest::Client::builder()
        .user_agent("TitaouPosCRM-Desktop")
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let res = client
        .get("https://api.github.com/repos/titaou-bedreddine/TitaouPosCRM/releases")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| format!("Failed to reach GitHub: {}", e))?;

    if !res.status().is_success() {
        return Err(format!("GitHub API returned HTTP {}", res.status()));
    }

    let releases: Vec<serde_json::Value> = res
        .json()
        .await
        .map_err(|e| format!("Failed to parse GitHub response: {}", e))?;

    if releases.is_empty() {
        return Ok(AppUpdateResult {
            has_update: false,
            current_version: current_version.clone(),
            latest_version: current_version.clone(),
            tag_name: format!("v{}", current_version),
            release_name: "No releases found".to_string(),
            release_notes: "".to_string(),
            release_url: "https://github.com/titaou-bedreddine/TitaouPosCRM/releases".to_string(),
            download_url: "".to_string(),
            published_at: "".to_string(),
        });
    }

    let latest = &releases[0];
    let tag_name = latest["tag_name"].as_str().unwrap_or("").trim().to_string();
    let clean_latest = tag_name.trim_start_matches('v').trim();
    let clean_current = current_version.trim_start_matches('v').trim();

    let release_name = latest["name"].as_str().unwrap_or(&tag_name).to_string();
    let release_notes = latest["body"].as_str().unwrap_or("").to_string();
    let release_url = latest["html_url"]
        .as_str()
        .unwrap_or("https://github.com/titaou-bedreddine/TitaouPosCRM/releases")
        .to_string();
    let published_at = latest["published_at"].as_str().unwrap_or("").to_string();

    let mut download_url = release_url.clone();
    if let Some(assets) = latest["assets"].as_array() {
        // Prefer the NSIS setup bundle (small, silent-installable) over the
        // raw 18MB portable exe; MSI is the last resort.
        let pick_asset = assets.iter().find(|a| {
            let name = a["name"].as_str().unwrap_or("");
            name.ends_with("-setup.exe")
        });
        let asset = pick_asset.or_else(|| {
            assets.iter().find(|a| {
                let name = a["name"].as_str().unwrap_or("");
                name.ends_with(".exe") && !name.starts_with("titaou-poscrm")
            })
        });
        if let Some(asset) = asset {
            if let Some(browser_url) = asset["browser_download_url"].as_str() {
                download_url = browser_url.to_string();
            }
        }
    }

    // Version-AWARE comparison: a published release OLDER than the running
    // build (e.g. running 0.5.18 while GitHub's latest release is 0.5.15)
    // must NOT offer a "new update". Compare each numeric segment.
    fn is_newer_version(candidate: &str, current: &str) -> bool {
        let parse = |v: &str| -> Vec<u64> {
            v.split('.')
                .map(|p| p.trim().parse::<u64>().unwrap_or(0))
                .collect()
        };
        let (cand, cur) = (parse(candidate), parse(current));
        let len = cand.len().max(cur.len());
        for i in 0..len {
            let a = cand.get(i).copied().unwrap_or(0);
            let b = cur.get(i).copied().unwrap_or(0);
            if a != b {
                return a > b;
            }
        }
        false
    }
    let has_update = !tag_name.is_empty() && is_newer_version(&clean_latest, &clean_current);

    Ok(AppUpdateResult {
        has_update,
        current_version,
        latest_version: clean_latest.to_string(),
        tag_name,
        release_name,
        release_notes,
        release_url,
        download_url,
        published_at,
    })
}
/// Launch TitaouPosCRM automatically when Windows starts (HKCU Run key -
/// per-user, no admin rights needed).
#[tauri::command]
pub fn set_autostart(enable: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        let exe = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .to_string();

        let output = if enable {
            std::process::Command::new("reg")
                .args(["add", KEY, "/v", "TitaouPosCRM", "/t", "REG_SZ", "/d", &exe, "/f"])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()
                .map_err(|e| e.to_string())?
        } else {
            std::process::Command::new("reg")
                .args(["delete", KEY, "/v", "TitaouPosCRM", "/f"])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()
                .map_err(|e| e.to_string())?
        };

        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "Autostart registry update failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enable;
        Err("Autostart is only supported on Windows".to_string())
    }
}

/// Read the current autostart state for the settings toggle.
#[tauri::command]
pub fn get_autostart() -> Result<bool, String> {
    #[cfg(windows)]
    {
        let output = std::process::Command::new("reg")
            .args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run", "/v", "TitaouPOS"])
            .output()
            .map_err(|e| e.to_string())?;
        Ok(output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("TitaouPosCRM"))
    }
    #[cfg(not(windows))]
    {
        Ok(false)
    }
}

static PRINTERS_CACHE: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

/// List installed printer names so Settings can offer a choice.
#[tauri::command]
pub fn list_printers() -> Result<Vec<String>, String> {
    if let Ok(guard) = PRINTERS_CACHE.lock() {
        if let Some(cached) = guard.as_ref() {
            return Ok(cached.clone());
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut names: Vec<String> = Vec::new();
        if let Ok(output) = std::process::Command::new("wmic")
            .args(["printer", "get", "name"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            names = text
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("name"))
                .map(|l| l.to_string())
                .collect();
        }
        if let Ok(mut guard) = PRINTERS_CACHE.lock() {
            *guard = Some(names.clone());
        }
        Ok(names)
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}
