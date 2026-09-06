use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role_id: Option<i64>,
    pub role_name: Option<String>,
    pub max_discount_percent: f64,
    pub is_active: bool,
    pub permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserAccount {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role_id: Option<i64>,
    pub role_name: Option<String>,
    pub max_discount_percent: f64,
    pub is_active: bool,
    pub last_login: Option<String>,
    pub created_at: Option<String>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Role {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Category {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub color: String,
    pub is_active: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Unit {
    pub id: i64,
    pub name: String,
    pub short_name: String,
    pub allow_decimals: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Product {
    pub id: i64,
    pub sku: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub unit_id: Option<i64>,
    pub unit_name: Option<String>,
    pub purchase_price: i64,
    pub sale_price: i64,
    pub min_sale_price: i64,
    pub tax_rate: i64,
    pub current_stock: f64,
    pub min_stock: f64,
    pub max_stock: Option<f64>,
    pub image_path: Option<String>,
    pub expiry_date: Option<String>,
    pub is_scalable: bool,
    pub scale_code: Option<String>,
    pub scale_plu: Option<i64>,
    pub scale_barcode_type: Option<i64>,
    pub scale_department_id: Option<i64>,
    pub scale_sync_status: Option<String>,
    pub is_bundle: bool,
    pub is_active: bool,
    pub barcodes: Vec<String>,
    /// Lifetime units sold (sales minus refunds) for best/worst sorting.
    #[serde(default)]
    pub total_sold: Option<f64>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub pin_order: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProductInput {
    pub sku: Option<String>,
    pub name_ar: String,
    pub name_fr: String,
    pub name_en: String,
    pub category_id: Option<i64>,
    pub unit_id: Option<i64>,
    pub purchase_price: i64,
    pub sale_price: i64,
    pub min_sale_price: i64,
    pub tax_rate: i64,
    pub current_stock: f64,
    pub min_stock: f64,
    pub image_path: Option<String>,
    pub expiry_date: Option<String>,
    #[serde(default)]
    pub is_scalable: bool,
    #[serde(default)]
    pub scale_code: Option<String>,
    #[serde(default)]
    pub scale_plu: Option<i64>,
    #[serde(default)]
    pub scale_barcode_type: Option<i64>,
    #[serde(default)]
    pub scale_department_id: Option<i64>,
    #[serde(default)]
    pub scale_sync_status: Option<String>,
    #[serde(default)]
    pub is_bundle: bool,
    pub barcodes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CashSession {
    pub id: i64,
    pub register_id: i64,
    pub user_id: i64,
    pub user_name: Option<String>,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub opening_amount: i64,
    pub expected_cash: i64,
    pub actual_cash: Option<i64>,
    pub difference: Option<i64>,
    pub total_sales: Option<i64>,
    pub total_expenses: Option<i64>,
    pub current_balance: Option<i64>,
    pub status: String,
    pub notes: Option<String>,
    /// True when the session was opened on a previous calendar day.
    #[serde(default)]
    pub is_stale: Option<bool>,
    #[serde(default)]
    pub is_archived: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CashMovement {
    pub id: i64,
    pub session_id: i64,
    pub user_id: i64,
    pub user_name: Option<String>,
    pub type_name: String,
    pub amount: i64,
    pub reason: Option<String>,
    pub created_at: String,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CartItem {
    pub product_id: i64,
    #[serde(default)]
    pub sku: Option<String>,
    #[serde(default)]
    pub barcode: Option<String>,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub name_fr: Option<String>,
    #[serde(default)]
    pub name_en: Option<String>,
    #[serde(default)]
    pub image_path: Option<String>,
    #[serde(default)]
    pub unit_price: i64,
    #[serde(default)]
    pub quantity: f64,
    #[serde(default)]
    pub discount_amount: i64,
    #[serde(default)]
    pub tax_amount: i64,
    #[serde(default)]
    pub total_price: i64,
    #[serde(default)]
    pub is_refund: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SalePaymentInput {
    #[serde(default)]
    pub payment_method: String,
    #[serde(default)]
    pub amount: i64,
    #[serde(default)]
    pub reference_code: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CreateSaleInput {
    #[serde(default)]
    pub session_id: i64,
    #[serde(default)]
    pub user_id: i64,
    #[serde(default)]
    pub customer_id: Option<i64>,
    #[serde(default)]
    pub items: Vec<CartItem>,
    #[serde(default)]
    pub subtotal: i64,
    #[serde(default)]
    pub discount_amount: i64,
    #[serde(default)]
    pub discount_percentage: f64,
    #[serde(default)]
    pub discount_reason: Option<String>,
    #[serde(default)]
    pub tax_amount: i64,
    #[serde(default)]
    pub total_amount: i64,
    #[serde(default)]
    pub paid_amount: i64,
    #[serde(default)]
    pub change_amount: i64,
    #[serde(default)]
    pub payments: Vec<SalePaymentInput>,
    #[serde(default)]
    pub payment_method: Option<String>,
    #[serde(default)]
    pub is_refund: Option<bool>,
    #[serde(default)]
    pub notes: Option<String>,
    /// Versement (layaway) sales: goods stay at the shop until fully paid,
    /// so stock is NOT decremented at sale time.
    #[serde(default)]
    pub skip_stock: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Sale {
    pub id: i64,
    pub sale_number: String,
    pub session_id: Option<i64>,
    pub user_id: i64,
    pub user_name: Option<String>,
    pub customer_id: Option<i64>,
    pub customer_name: Option<String>,
    pub subtotal: i64,
    pub discount_amount: i64,
    pub tax_amount: i64,
    pub total_amount: i64,
    pub paid_amount: i64,
    pub change_amount: i64,
    pub payment_method: Option<String>,
    pub payment_status: String,
    pub status: String,
    pub created_at: String,
    pub items: Option<Vec<CartItem>>,
    /// Number of distinct sale LINES (a line = one product row).
    #[serde(default)]
    pub lines_sold: i64,
    /// Total UNITS across all lines (Σ quantity) — lines ≠ units.
    #[serde(default)]
    pub units_sold: f64,
    /// True when the sale was re-checkout-edited in place (replace_sale
    /// stamps "MODIFIED" into notes) — drives the localized EDITED badge.
    #[serde(default)]
    pub is_edited: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Customer {
    pub id: i64,
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub rc: Option<String>,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub ai: Option<String>,
    pub qr_code: Option<String>,
    pub balance: i64,
    pub initial_debt: i64,
    pub total_purchases: Option<i64>,
    pub notes: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub pin_order: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SupplierPaymentInput {
    pub supplier_id: i64,
    pub amount: i64,
    pub payment_method: String,
    pub reference: Option<String>,
    pub session_id: Option<i64>,
    pub user_id: i64,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CustomerPaymentInput {
    pub customer_id: i64,
    pub amount: i64,
    pub payment_method: String,
    pub reference: Option<String>,
    pub session_id: Option<i64>,
    pub user_id: i64,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Supplier {
    pub id: i64,
    pub name: String,
    pub contact_person: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    pub rc: Option<String>,
    pub nif: Option<String>,
    pub nis: Option<String>,
    pub ai: Option<String>,
    pub qr_code: Option<String>,
    pub balance: i64,
    pub notes: Option<String>,
    pub is_active: bool,
    pub created_at: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub pin_order: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PurchaseItemInput {
    pub product_id: i64,
    pub quantity: f64,
    pub unit_cost: i64,
    #[serde(default)]
    pub discount: i64,
    #[serde(default)]
    pub tax: i64,
    #[serde(default)]
    pub total: i64,
    #[serde(default)]
    pub expiry_date: Option<String>,
    #[serde(default)]
    pub batch_number: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PurchaseItem {
    pub id: i64,
    pub purchase_id: i64,
    pub product_id: i64,
    pub product_name: Option<String>,
    pub product_name_ar: Option<String>,
    pub quantity: f64,
    pub unit_cost: i64,
    #[serde(default)]
    pub discount: i64,
    #[serde(default)]
    pub tax: i64,
    #[serde(default)]
    pub total: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreatePurchaseInput {
    pub invoice_number: String,
    pub supplier_id: i64,
    #[serde(default)]
    pub user_id: i64,
    #[serde(default, alias = "purchase_date")]
    pub date: String,
    #[serde(default)]
    pub subtotal: i64,
    #[serde(default)]
    pub discount: i64,
    #[serde(default)]
    pub tax: i64,
    #[serde(default, alias = "total_amount")]
    pub total: i64,
    #[serde(default)]
    pub paid_amount: i64,
    #[serde(default)]
    pub payment_method: String,
    pub items: Vec<PurchaseItemInput>,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScaleSyncLog {
    pub id: i64,
    pub product_id: Option<i64>,
    pub product_name: Option<String>,
    pub scale_plu: Option<i64>,
    pub action: String,
    pub direction: String,
    pub status: String,
    pub error_message: Option<String>,
    pub user_name: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ScaleConfig {
    pub enabled: bool,
    pub model: String,
    pub ip_address: String,
    pub port: u32,
    pub protocol_type: u32,
    pub default_barcode_type: i64,
    pub department_id: i64,
    pub auto_sync_on_change: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Purchase {
    pub id: i64,
    pub invoice_number: String,
    pub supplier_id: i64,
    pub supplier_name: Option<String>,
    pub user_id: i64,
    pub user_name: Option<String>,
    pub date: String,
    pub subtotal: i64,
    pub discount: i64,
    pub tax: i64,
    pub total: i64,
    pub paid_amount: i64,
    pub payment_method: String,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Expense {
    pub id: i64,
    #[serde(default)]
    pub user_name: Option<String>,
    pub expense_number: String,
    pub category_id: i64,
    pub category_name: Option<String>,
    pub amount: i64,
    pub payment_method: String,
    pub session_id: Option<i64>,
    pub user_id: i64,
    pub recipient: Option<String>,
    pub receipt_reference: Option<String>,
    pub date: String,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Employee {
    pub id: i64,
    pub employee_code: String,
    pub full_name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub national_id: Option<String>,
    pub job_title: String,
    pub base_salary: i64,
    pub salary_type: String,
    pub salary_start_date: Option<String>,
    pub hire_date: String,
    pub qr_code: Option<String>,
    pub rfid_code: Option<String>,
    pub is_active: bool,
    pub notes: Option<String>,
    /// Linked auto-created customer (employees ARE customers on the POS).
    #[serde(default)]
    pub customer_id: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Payroll {
    pub id: i64,
    pub payroll_number: String,
    pub employee_id: i64,
    pub employee_name: Option<String>,
    pub period_month: i64,
    pub period_year: i64,
    pub base_salary: i64,
    pub bonuses: i64,
    pub allowances: i64,
    pub deductions: i64,
    pub advances_deducted: i64,
    pub net_salary: i64,
    pub payment_status: String,
    pub payment_method: Option<String>,
    pub paid_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HeldSale {
    pub id: i64,
    pub sale_reference: String,
    pub user_id: i64,
    pub customer_id: Option<i64>,
    pub cart_json: String,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DashboardStats {
    pub today_sales: i64,
    pub today_transactions_count: i64,
    pub today_profit: i64,
    pub low_stock_count: i64,
    pub out_of_stock_count: i64,
    pub active_cash_expected: i64,
    pub today_expenses: i64,
    pub returns_amount: i64,
    pub net_revenue: i64,
    pub cost_of_goods: i64,
    pub gross_profit: i64,
    pub average_basket: i64,
    pub top_products: Vec<TopProductStat>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PriceHistoryEntry {
    pub id: i64,
    pub old_purchase_price: i64,
    pub new_purchase_price: i64,
    pub old_sale_price: i64,
    pub new_sale_price: i64,
    pub user_id: Option<i64>,
    pub created_at: String,
}

/// One row of the product quantity history (sales, purchases, refunds,
/// manual adjustments, broken/damaged): old → new with the signed delta.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuantityHistoryEntry {
    pub id: i64,
    pub movement_type: String,
    pub old_quantity: f64,
    pub new_quantity: f64,
    pub difference: f64,
    pub user_name: Option<String>,
    pub created_at: String,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TopProductStat {
    pub product_name: String,
    pub category_name: String,
    pub sold_qty: f64,
    pub revenue: i64,
    pub cost: i64,
    pub profit: i64,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SupplierPaymentRow {
    pub id: i64,
    pub supplier_id: i64,
    pub amount: i64,
    pub payment_method: String,
    pub reference: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmployeeAdvance {
    pub id: i64,
    pub employee_id: i64,
    pub employee_name: Option<String>,
    pub amount: i64,
    pub reason: Option<String>,
    pub date: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmployeeAdvanceInput {
    pub employee_id: i64,
    pub amount: i64,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub session_id: Option<i64>,
    #[serde(default)]
    pub user_id: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProductPackaging {
    pub id: i64,
    pub product_id: i64,
    pub name: String,
    pub units_per_package: i64,
    pub sale_price: i64,
    pub is_default: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PackagingInput {
    pub name: String,
    pub units_per_package: i64,
    #[serde(default)]
    pub sale_price: i64,
    #[serde(default)]
    pub is_default: bool,
}
