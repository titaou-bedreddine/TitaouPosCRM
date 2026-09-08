# TitaouPOS CRM ↔ TitaouCRM Cloud Sync — Design

```
TitaouPosCRM (Tauri, admin station)          Flutter field apps
  coordinator PC ── cloudsync (Rust) ────►  Supabase  ◄── direct + PowerSync
  client terminals ── LAN axum (unchanged)   (preseller / seller shells)
```

## Principles

1. **Supabase is the source of truth for org-wide data.** The POS keeps its own
   authoritative SQLite for the shop LAN (unchanged) and syncs bidirectionally.
2. **Cloud sync runs ONLY on the LAN coordinator.** Client terminals already
   forward all business ops to the coordinator through the axum API, so there is
   exactly one writer to Supabase. No split-brain, no multi-writer conflicts.
   When this PC is a LAN client, the Cloud Sync tab shows "runs on the server
   terminal".
3. **Only movements sync, never quantities.** Stock is an append-only ledger on
   both sides. POS sales push `sale` movements; POS purchases push `entry`
   movements; POS refunds push `return` movements; field orders pull into POS as
   `sale` movements (reference `crm_order:<uuid>`). Product stock fields are
   always recomputed from the ledger, never synced as raw numbers.
4. **Exactly-once.** POS-side: transactional outbox (enqueue inside each service
   transaction — a rolled-back sale never syncs) + `sync_map` (local int id ↔
   remote uuid). CRM-side: idempotent RPCs keyed on `pos_ref`
   (`orders.pos_ref`, `invoices.pos_ref`, `payments.pos_ref`) — a replayed push
   returns the existing row instead of duplicating.
5. **Conflict policy: last-write-wins** on `updated_at` (pull first, then push,
   each cycle → POS edits win for POS-origin rows on tie). Rare in a single
   shop; documented, not negotiated per-field in v1.

## Mappings

| POS (SQLite, whole DZD) | CRM (Postgres, centimes) | Rule |
|---|---|---|
| `sale.total_amount` = 1500 (DZD) | `orders.total_amount` = 150000 (centimes) | push ×100, pull ÷100 — single constant `DZD_TO_CENTIMES`, unit-tested |
| `quantity` REAL (kg/L/pieces) | `quantity numeric(10,3)` | migration 0017; direct mapping |
| payment `cash` → `cash` | `payments.method = 'cash'` | direct |
| payment `tpe`, `versement` → `transfer` | `payments.method = 'transfer'` | map |
| payment `credit` | no payment row | unpaid remainder → client balance |
| product names (ar/fr/en) | `products.name` | `name_fr`, fallback `name_ar` |
| customers (walk-in id 1) | `ensure_pos_client()` walk-in client | linked at first connect |
| sale number `POS-…` | `orders.pos_ref` | unique per org (partial index) |

## Entity flows

**Push (POS → Supabase)**, from the outbox:
- Counter sale → `create_pos_order` (idempotent on pos_ref; `source='pos'`;
  status `delivered`; sale movements unless versement; payments; client balance
  adjusts by total − payments, netting 0 for fully paid sales).
- Refund → `record_pos_refund` (`return` movements only — refund financials are
  a POS cash-drawer concept and stay local).
- Purchase → `create_purchase` with `p_number` (idempotent).
- Product create/update → `upsert_pos_product` (match on SKU, then primary
  barcode, then insert).
- Stock adjustment (inc/dec) → `adjustment` movement via `upsert_pos_product`
  event type or direct movement insert path (see push.rs).
- Customer create/update → `clients` upsert (POS customers map to CRM clients;
  walk-in linked to `ensure_pos_client`).
- Customer debt payment → `record_client_payment`.
- Supplier create/update → `suppliers` upsert.

**Pull (Supabase → POS)**, cursor batches on `updated_at` (200 rows/batch):
- CRM clients → POS customers (balance = CRM `current_balance`; CRM is the
  balance authority — SET, not add).
- CRM products → POS products (prices ÷100; linked via sync_map/SKU).
- Field orders (`orders.source != 'pos'`) → `crm_orders` + `crm_order_items`
  mirror tables + POS `inventory_movements` (type `sale`,
  ref `crm_order:<uuid>`) + `current_stock` recompute.
- Field payments → customer balance mirror refresh.
- CRM purchases / stock adjustments (admin-created in the Flutter admin) →
  POS `inventory_movements` (`entry`/`adjustment`, ref `crm_invoice:<uuid>`).

**First connect (bootstrap):**
1. `ensure_pos_client()` → link walk-in customer ↔ walk-in client.
2. Product **linking pass**: existing POS products match CRM products by SKU,
   then by primary barcode (recorded in `sync_map` — prevents duplicates).
3. Initial pull for clients/products/orders.

## Admin-station features (not sync, direct Supabase calls from POS)

- **Field Team**: list/create/edit/delete presellers & sellers.
  Create → `invite-user` edge function; update/delete → `manage-user` edge
  function (delete refuses the last active admin of the org).
- **Truck Loads**: document-style load sheets (`truck_loads` + items).
  Stock is NOT moved at load time — the preseller's orders already decremented
  it at order time (create_order writes the `sale` movement immediately).
  Undelivered goods → admin records a **truck return** (`record_truck_return`)
  which appends `return` movements (+qty). If goods were refused against a
  specific order, the admin separately cancels/edits that order via the
  existing admin CRUD (documented workflow).
- **Deletion Requests**: presellers request client deletion
  (`client_deletion_requests`); admin approves (`approve_client_deletion` —
  FK-safe, errors surface "client has orders — deactivate instead") or
  rejects.
- **Field Orders**: read-only mirror of pulled field orders with lines,
  auto-refreshed after each pull cycle.

## Trust boundary

The POS is the owner's admin station. `create_pos_order` trusts POS totals —
the same trust level as the existing Flutter admin (create_order trusts the
client-submitted totals). RLS still scopes everything to the org; the
service-role key never ships with the POS.

## Out of scope v1 (stays POS-local)

Cash sessions/movements, expenses, payroll/employees/advances, held carts,
supplier debt balances, POS user accounts, scale/drawer/printer hardware flows,
Telegram notifications. CRM invoices are not pulled into the POS (the field
orders mirror covers admin visibility).
