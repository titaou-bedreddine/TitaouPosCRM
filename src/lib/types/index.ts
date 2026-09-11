export interface User {
  id: number;
  username: string;
  display_name: string;
  role_id?: number;
  role_name?: string;
  max_discount_percent: number;
  is_active: boolean;
  permissions: string[];
}

export interface Category {
  id: number;
  parent_id?: number;
  name_ar: string;
  name_fr: string;
  name_en: string;
  color: string;
  is_active: boolean;
}

export interface Unit {
  id: number;
  name: string;
  short_name: string;
  allow_decimals: boolean;
}

export interface Product {
  id: number;
  sku?: string;
  name_ar: string;
  name_fr: string;
  name_en: string;
  category_id?: number;
  category_name?: string;
  unit_id?: number;
  unit_name?: string;
  purchase_price: number;
  sale_price: number;
  min_sale_price: number;
  tax_rate: number;
  current_stock: number;
  min_stock: number;
  max_stock?: number;
  image_path?: string;
  expiry_date?: string;
  is_scalable: boolean;
  scale_code?: string;
  scale_plu?: number;
  scale_barcode_type?: number;
  scale_department_id?: number;
  scale_sync_status?: string;
  is_bundle: boolean;
  is_active: boolean;
  barcodes: string[];
  total_sold?: number;
  /** Pinned products float to the top of the POS grid and stock page. */
  pinned?: boolean;
  pin_order?: number;
}

export interface ProductInput {
  sku?: string;
  name_ar: string;
  name_fr: string;
  name_en: string;
  category_id?: number | null;
  unit_id?: number | null;
  purchase_price: number;
  sale_price: number;
  min_sale_price: number;
  tax_rate: number;
  current_stock: number;
  min_stock: number;
  image_path?: string;
  expiry_date?: string;
  is_scalable: boolean;
  scale_code?: string;
  scale_plu?: number;
  scale_barcode_type?: number;
  scale_department_id?: number;
  scale_sync_status?: string;
  is_bundle: boolean;
  barcodes: string[];
}

export interface CartItem {
  product_id: number;
  sku?: string;
  barcode?: string;
  name_ar: string;
  name_fr: string;
  name_en: string;
  image_path?: string;
  unit_price: number;
  quantity: number;
  discount_amount: number;
  tax_amount: number;
  total_price: number;
  is_refund: boolean;
  // Carried from the product so the cart line can show an EXPIRED badge
  // the moment an expired item is added.
  expiry_date?: string;
  // Product's purchase cost per BASE unit (cart profit indicators).
  purchase_price?: number;
  // Product's available inventory balance for negative-stock enforcement.
  current_stock?: number;
  // Scale product: enables weight-suggestion chips on the cart line.
  is_scalable?: boolean;
  // Packaging sale: the unit this line is billed in ("Palette", "Fardeau"…).
  // null/absent = base unit (bottle). base_quantity = quantity ×
  // units_per_package — the stock truth in base units.
  sale_unit?: string;
  units_per_package?: number;
  base_quantity?: number;
}

export interface PackagingDef {
  id: number;
  product_id: number;
  name: string;
  units_per_package: number;
  sale_price: number;
  purchase_price: number;
}

export interface CashSession {
  id: number;
  register_id: number;
  user_id: number;
  user_name?: string;
  opened_at: string;
  closed_at?: string;
  opening_amount: number;
  expected_cash: number;
  actual_cash?: number;
  difference?: number;
  total_sales?: number;
  total_expenses?: number;
  current_balance?: number;
  status: 'open' | 'closed';
  notes?: string;
  is_archived?: boolean;
  /** LAN: PC name of the terminal that opened this session. */
  terminal_name?: string;
}

export interface CashMovement {
  id: number;
  session_id: number;
  user_id: number;
  user_name?: string;
  type_name: string;
  amount: number;
  reason?: string;
  created_at: string;
  notes?: string;
  /** LAN: PC name of the terminal that recorded this movement. */
  terminal_name?: string;
}

export interface Sale {
  id: number;
  sale_number: string;
  session_id?: number;
  user_id: number;
  user_name?: string;
  customer_id?: number;
  customer_name?: string;
  subtotal: number;
  discount_amount: number;
  tax_amount: number;
  total_amount: number;
  paid_amount: number;
  change_amount: number;
  payment_method?: string;
  payment_status: string;
  status: string;
  created_at: string;
  /** Distinct sale LINES (A×2 + B×5 = 2 lines). */
  lines_sold?: number;
  /** Total UNITS across lines (A×2 + B×5 = 7 units). */
  units_sold?: number;
  /** True when the sale was edited in place (localized EDITED badge). */
  is_edited?: boolean;
  /** LAN: PC name of the terminal that recorded this sale. */
  terminal_name?: string;
}

export interface Customer {
  id: number;
  name: string;
  phone?: string;
  email?: string;
  address?: string;
  rc?: string;
  nif?: string;
  nis?: string;
  ai?: string;
  qr_code?: string;
  balance: number;
  initial_debt: number;
  total_purchases?: number;
  notes?: string;
  is_active: boolean;
  created_at: string;  /** Pinned customers float to the top of the list. */
  pinned?: boolean;
  pin_order?: number;
}

export interface Supplier {
  id: number;
  name: string;
  contact_person?: string;
  phone?: string;
  email?: string;
  address?: string;
  rc?: string;
  nif?: string;
  nis?: string;
  ai?: string;
  qr_code?: string;
  balance: number;
  notes?: string;
  is_active: boolean;
  created_at: string;  /** Pinned suppliers float to the top of the list. */
  pinned?: boolean;
  pin_order?: number;
}

export interface Purchase {
  id: number;
  invoice_number: string;
  supplier_id: number;
  supplier_name?: string;
  user_id: number;
  user_name?: string;
  date: string;
  subtotal: number;
  discount: number;
  tax: number;
  total: number;
  paid_amount: number;
  payment_method: string;
  status: string;
  notes?: string;
  created_at: string;
  /** LAN: PC name of the terminal that recorded this purchase. */
  terminal_name?: string;
}

export interface Expense {
  id: number;
  user_name?: string;
  expense_number: string;
  category_id: number;
  category_name?: string;
  amount: number;
  payment_method: string;
  session_id?: number;
  user_id: number;
  recipient?: string;
  receipt_reference?: string;
  date: string;
  notes?: string;
  created_at: string;
  /** LAN: PC name of the terminal that recorded this expense. */
  terminal_name?: string;
}

export interface Employee {
  id: number;
  employee_code: string;
  full_name: string;
  phone?: string;
  email?: string;
  national_id?: string;
  job_title: string;
  base_salary: number;
  salary_type: string;
  salary_start_date?: string;
  hire_date: string;
  qr_code?: string;
  is_active: boolean;
  notes?: string;
}

export interface HeldSale {
  id: number;
  sale_reference: string;
  user_id: number;
  customer_id?: number;
  cart_json: string;
  note?: string;
  created_at: string;
}

export interface DashboardStats {
  today_sales: number;
  today_transactions_count: number;
  today_profit: number;
  low_stock_count: number;
  out_of_stock_count: number;
  active_cash_expected: number;
  today_expenses: number;
  returns_amount: number;
  net_revenue: number;
  cost_of_goods: number;
  gross_profit: number;
  average_basket: number;
  top_products: Array<{
    product_name: string;
    category_name: string;
    sold_qty: number;
    revenue: number;
    cost: number;
    profit: number;
  }>;
  /** LAN: today's sales split per terminal (PC) — which counter sold what. */
  sales_by_terminal?: Array<{
    terminal: string;
    total: number;
    count: number;
  }>;
  /** Newly added stock in the period: qty, cost, possible profit. */
  new_stock?: {
    qty_added: number;
    cost_total: number;
    sale_value: number;
    possible_profit: number;
    product_count: number;
  };
}