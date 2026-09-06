pub mod auth;
pub mod commands;
pub mod database;
pub mod models;
pub mod network;
pub mod printing;
pub mod server;
pub mod services;

use database::DbState;
use tauri::Manager;

/// Wrap the generated Tauri command handler with LAN client forwarding:
/// whitelisted business commands are executed on the shop server while this
/// PC is a connected client; everything else runs locally. The wrapper is a
/// named function so the `generate_handler!` closure gets its expected type.
fn lan_wrap_invoke_handler(
    generated: impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static,
) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
        let cmd = invoke.message.command().to_string();
        if network::should_forward_ipc(&cmd) {
            let payload = match invoke.message.payload() {
                tauri::ipc::InvokeBody::Json(v) => {
                    serde_json::to_string(v).unwrap_or_else(|_| "null".to_string())
                }
                tauri::ipc::InvokeBody::Raw(_) => "null".to_string(),
            };
            let resolver = invoke.resolver;
            resolver.respond_async(async move {
                match network::forward_ipc(cmd, payload).await {
                    Ok(v) => Ok(tauri::ipc::InvokeResponseBody::Json(v.to_string())),
                    Err(e) => Err(tauri::ipc::InvokeError::from(e)),
                }
            });
            return true;
        }
        generated(invoke)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {    let db_state = DbState::new().expect("Failed to initialize database");

    // Startup backup (only when the setting is ON; once per day).
    crate::services::settings_service::run_startup_backup(&db_state);

    server::set_diag_db(DbState::new().expect("diag db"));
    // The LAN shop API shares the same authoritative SQLite file (its own
    // WAL connection) — set BEFORE the server starts serving /api/v1.
    network::server_api::set_api_db(DbState::new().expect("lan api db"));
    server::start_local_api_server();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // A second launch just focuses the existing window.
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .manage(db_state)
        .setup(|app| {
            // LAN shop networking: discovery, election, client/server roles.
            network::init(app.handle().clone(), DbState::new().expect("network db"));
            Ok(())
        })
        .invoke_handler(lan_wrap_invoke_handler(tauri::generate_handler![
            commands::login,
            commands::get_active_users,
            commands::change_user_password,
            commands::get_user_by_qr,
            commands::get_dashboard_stats,
            commands::get_active_cash_session,
            commands::open_cash_session,
            commands::add_cash_movement,
            commands::close_cash_session,
            commands::list_cash_movements,
            commands::list_session_history,
            commands::edit_opening_balance,
            commands::edit_cash_session,
            commands::archive_cash_session,
            commands::delete_cash_session,
            commands::search_products,
            commands::save_product,
            commands::delete_product,
            commands::get_categories,
            commands::save_category,
            commands::delete_category,
            commands::get_units,
            commands::toggle_product_pin,
            commands::list_packagings,
            commands::save_packagings,
            commands::reorder_pinned_products,
            commands::process_sale,
            commands::create_sale,
            commands::replace_sale,
            commands::list_sales,
            commands::get_sale_items,
            commands::get_last_sale,
            commands::get_sale_by_number,
            commands::hold_sale,
            commands::list_held_sales,
            commands::delete_held_sale,
            commands::list_customers,
            commands::save_customer,
            commands::delete_customer,
            commands::toggle_customer_pin,
            commands::record_customer_debt_payment,
            commands::clear_customer_debt,
            commands::clear_supplier_debt,
            commands::list_debt_clear_log,
            commands::list_suppliers,
            commands::save_supplier,
            commands::delete_supplier,
            commands::toggle_supplier_pin,
            commands::record_supplier_debt_payment,
            commands::list_supplier_debt_payments,
            commands::create_purchase,
            commands::get_purchase_items,
            commands::delete_purchase,
            commands::list_purchases,
            commands::add_expense,
            commands::update_expense,
            commands::list_expenses,
            commands::delete_expense,
            commands::list_employees,
            commands::save_employee,
            commands::find_employee_by_rfid,
            commands::login_with_rfid,
            commands::resolve_scale_scan,
            commands::print_escpos_raw,
            commands::next_employee_code,
            commands::delete_employee,
            commands::list_payrolls,
            commands::record_employee_advance,
            commands::record_employee_absence,
            commands::list_employee_absences,
            commands::toggle_user_pin,
            commands::list_employee_advances,
            commands::get_all_settings,
            commands::set_setting,
            commands::set_multiple_settings,
            commands::send_telegram_message,
            commands::send_telegram_recap,
            commands::print_html_direct,
            commands::print_label_job,
            commands::get_setting,
            commands::get_price_history,
            commands::get_quantity_history,
            commands::clear_transaction_history,
            commands::get_hwid,
            commands::verify_license,
            commands::activate_online,
            commands::factory_reset,
            commands::test_scale_connection,
            commands::upload_product_to_scale,
            commands::upload_all_scalable_to_scale,
            commands::fetch_products_from_scale,
            commands::get_scale_sync_logs,
            commands::open_serial_cash_drawer,
            commands::delete_sale,
            commands::verify_admin_password,
            commands::save_unit,
            commands::backup_database,
            commands::restore_database,
            commands::restore_settings_only,
            commands::create_backup,
            commands::run_scheduled_backup,
            commands::list_backups,
            commands::validate_backup_file,
            commands::pick_backup_folder,
            commands::pick_backup_file,
            commands::get_app_version,
            commands::get_all_users,
            commands::get_all_roles,
            commands::create_user,
            commands::update_user,
            commands::delete_user,
            commands::check_github_update,
            commands::set_autostart,
            commands::get_autostart,
            commands::list_printers,
            commands::run_payroll_reminder_scan,
            commands::list_app_notifications,
            commands::dismiss_app_notification,
            commands::get_server_status,
            // LAN shop network
            commands::network_cmds::network_get_status,
            commands::network_cmds::network_get_log,
            commands::network_cmds::network_discovered_servers,
            commands::network_cmds::network_set_setup,
            commands::network_cmds::network_set_role,
            commands::network_cmds::network_set_pc_name,
            commands::network_cmds::network_set_flags,
            commands::network_cmds::network_become_server,
            commands::network_cmds::network_probe_server,
            commands::network_cmds::network_join_server,
            commands::network_cmds::network_leave_shop,
            commands::network_cmds::network_block_device,
            commands::network_cmds::network_remove_device,
            commands::network_cmds::network_rename_device,
            commands::network_cmds::network_logout,
            commands::network_cmds::network_open_firewall,
            commands::network_cmds::network_forward,
        ]))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}