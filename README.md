# TitaouPosCRM — Desktop POS + TitaouCRM Admin Station

TitaouPosCRM is a modern, high-performance Point of Sale (POS) and inventory management desktop application built with **Tauri 2.0**, **Svelte**, **Rust**, and **SQLite**. It is designed for superettes, grocery stores, butcheries, bakeries, and retail stores in Algeria and Francophone/MENA markets.

Developed by **Titaou Bedreddine** (Contact: `0553444057`).

---

## 1. System Architecture

- **Desktop Framework**: Tauri 2.0 (Rust backend with native Windows/Linux OS bindings)
- **Frontend UI**: Svelte 5 with TypeScript & Tailwind CSS
- **Local Database**: Embedded SQLite3 with Write-Ahead Logging (WAL) and strict foreign key integrity
- **Hardware Communications**:
  - **ACLAS Electronic Scale SDK**: Native dynamic loading of `AclasSDK.dll` (Win64/Win32)
  - **Serial Cash Drawer**: Direct Windows COM port file handle communication (`\\.\COM1` to `\\.\COM20`)
  - **Thermal & Label Printing**: Direct ESC/POS and HTML-to-printer thermal pipeline (80mm receipts, 50x30mm stickers, 40x20mm shelf tags)

---

## 1b. TitaouCRM Integration (v0.6.0)

TitaouPosCRM connects a shop to a **TitaouCRM** organization (Supabase):

- **Cloud Sync** (Settings → Cloud Sync): bidirectional sync between this
  terminal (must be the LAN server/standalone PC) and Supabase.
  - *Push*: counter sales, refunds (return movements), purchases, products,
    customers, customer debt payments — through idempotent RPCs (`pos_ref`),
    money converted whole-DZD ↔ centimes.
  - *Pull*: CRM products/clients, field orders (preseller/seller apps) into
    the Field Orders mirror + local stock, product linking by SKU/barcode.
- **Field Team**: invite / edit / delete presellers & sellers (Edge Functions
  `invite-user` / `manage-user`, last-admin guard server-side).
- **Truck Loads**: create a seller's load manifest for a route (auto-filled
  from the route's orders), print the sheet, record undelivered returns
  (+`return` stock movements on the CRM ledger).
- **Deletion Requests**: presellers can only *request* client deletion;
  the admin approves (FK-safe) or rejects here.
- Design + exactly-once/conflict rules: `docs/SYNC_DESIGN.md`.
- Keeping the POS in sync with upstream TitaouPosT fixes: `docs/SYNCING.md`.
- CRM side: `TitaouCRM` repo, branch `feat/pos-integration`
  (migrations 0017 + 0018, `manage-user` function).

## 2. Prerequisites & Development Setup

### Requirements
- **Node.js**: v18+ or v20+ LTS
- **Rust**: 1.75+ with `cargo` and MSVC C++ Build Tools (Windows SDK)
- **Visual Studio C++ Build Tools**: 2022 (Desktop Development with C++)

### Installation & Run
```bash
# Clone and install npm packages
npm install

# Run in Development Mode (Vite + Tauri Rust watcher)
npm run tauri dev

# Build Production Binary
npm run tauri build
```

---

## 3. Database Architecture & Safety

The database file is stored locally in the user's application data directory:
- **Windows**: `%APPDATA%\TitaouPosCRM\titaou_poscrm.db`
- **Linux**: `~/.config/TitaouPosCRM/titaou_poscrm.db`

### Key Safety Guarantees:
- **WAL Mode Enabled**: `PRAGMA journal_mode = WAL;` with `PRAGMA busy_timeout = 5000;` prevents write locks during rapid barcode scanning.
- **Foreign Key Enforcement**: `PRAGMA foreign_keys = ON;`.
- **Database Migrations**: Incremental schema evolution via `src-tauri/src/database/mod.rs` without destructive table drops or data loss.
- **Indexes**: Explicit B-Tree indexes on `barcodes(barcode)`, `products(sku, is_scalable, expiry_date)`, `employees(employee_code, rfid_code)`, and `sales(invoice_number)`.

---

## 4. Hardware Integrations

### A. ACLAS Electronic Scale Real SDK Integration
TitaouPosCRM features native integration with ACLAS electronic barcode scales (LH51, LS M3, TS Series, etc.) via dynamic library linking:
- **Reference SDK & Demo**:
  - C# Demo: `AclasSDK_Demo_C#`
  - Native DLLs: `SDK-Balance_Aclas/.../DLL66_Demo_EN200612/DLL&SO/Win64/AclasSDK.dll`
  - Specification Document: `Aclas Sync SDK_V2.0_EN.pdf`
- **Supported Barcode Types**:
  - **Type 97**: 18-Code format (`DD IIII PPPPPP WW.WWW C` — 2-digit Dept + 4-digit Item + 6-digit Price + Weight + Checksum)
  - **Type 02 / 22**: EAN-13 Price Embedded (`DD IIIII PPPPP C` / `D IIIIII PPPPP C`)
  - **Type 07 / 27**: EAN-13 Weight Embedded (`DD IIIII WWWWW C` / `D IIIIII WWWWW C`)
  - **Type 12 / 17**: Fixed prefix 22/27 EAN-13 Price/Weight Embedded
- **PLU File Format**: Generated on-the-fly as standard **UTF-16LE with BOM (`0xFF, 0xFE`)** tab-separated records containing PLU ID, Item Code, Department ID, Name (Ar/Fr), Unit Price, and Barcode Type.
- **Commands**:
  - `test_scale_connection(ip, port, protocol_type)`
  - `upload_product_to_scale(product_id, ip, port, ...)`
  - `upload_all_scalable_to_scale(ip, port, ...)`
  - `get_scale_sync_logs()`

### B. Serial Cash Drawer Integration
Cash drawers connected via RS-232 COM ports or USB-to-Serial adapters are opened directly through Windows kernel file handles:
- **Port Path**: `\\.\COM1` through `\\.\COM20`
- **Baud Rates**: 9600 (default), 19200, 38400, 115200
- **Kick Pulse**: Exact 5-byte sequence `[0x1B, 0x70, 0x00, 0x19, 0xFA]`
- **Commands**: `open_serial_cash_drawer(com_port, baud_rate)`

### C. Thermal Receipt & Label Printing
- **Receipts**: Standard 80mm / 58mm thermal rolls with dynamic shop header, legal tax numbers (RC, NIF, NIS, AI), itemized lines, and payment breakdowns.
- **Shelf Tags**: 40x20mm and 60x40mm shelf price labels with shop name, large bold price in DZD, and barcode.
- **Stickers**: 50x30mm product barcode stickers formatted with Modulo-10 checksum-validated EAN-13 codes.

---

## 5. POS Operational Features & Workflows

1. **4-Mode Held Carts**:
   - `Sale`, `Purchases`, `Refund`, and `Return`.
   - Clicking "New Sale" or switching cart modes automatically holds the active cart without prompt popups or data loss.
2. **Instant 1-Character / 1-Digit Live Search**:
   - In Purchases and POS search bars, typing a single letter or digit immediately displays live matching product cards with real-time stock and prices.
   - Auto-tabbing flow: `Quantity` $
ightarrow$ `Enter` $
ightarrow$ `Cost` $
ightarrow$ `Enter` $
ightarrow$ `Sale Price` $
ightarrow$ `Enter`.
3. **Margin & Profit Toggle**:
   - Switch between **Percentage (%)** and **Amount (DZD)** calculation in product editor with bidirectional synchronization.
4. **Quick Family & Unit Creation**:
   - Add new product categories and units directly from within the Product modal without closing it.
5. **Telegram Bot Notifications**:
   - Send automated end-of-day revenue summaries, refund alerts, low-stock warnings, and expired product notices via Telegram Bot API.

---

## 6. License & Author

- **Application**: TitaouPosCRM Retail System
- **Author**: Titaou Bedreddine
- **Phone**: +213 553 444 057
- **License**: Proprietary / Perpetual Offline Commercial License
