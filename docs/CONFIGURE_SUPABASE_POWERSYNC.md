# TitaouCRM — Supabase + PowerSync Configuration Guide

Complete setup for the integrated system:

```
TitaouPosCRM (Windows POS / admin station)
     │  anon key + owner's admin login (Cloud Sync tab)
     ▼
Supabase (Postgres + RLS + RPCs + Edge Functions + Storage)  ◄── single source of truth
     ▲
     │  anon key + JWT (PowerSync connector)
PowerSync (offline sync for the field apps)
     ▲
     │  local SQLite, offline-first
Flutter apps (Android — preseller & seller)
```

---

## Part 1 — Supabase (backend)

### 1.1 Create the project
1. Go to https://supabase.com/dashboard → **New project**.
2. Name it (e.g. `titaoucrm`), set a strong DB password, region close to you (e.g. `eu-west` / `me-central`).
3. Wait for provisioning. Note the **Project URL** and **anon/public key** (Settings → API).

### 1.2 Apply the schema (16 + 2 migrations)
The migrations live in `TitaouCRM/supabase/migrations/`. Easiest with the CLI:

```bash
# One-time: link your checkout of TitaouCRM to your project
cd TitaouCRM
supabase login
supabase link --project-ref <your-project-ref>

# Apply everything in order (0001…0016 + 0017_pos_sync + 0018_field_ops)
supabase db push

# Verify key objects exist
supabase db sql --project-ref <ref> --file - <<'SQL'
select proname from pg_proc where proname in
  ('create_pos_order','record_client_payment','approve_client_deletion','create_truck_load');
select column_type from information_schema.columns
  where table_name='order_items' and column_name='quantity';
SQL
-- Expect 4 RPC names + column_type = numeric(10,3)
```

> No CLI? Supabase Dashboard → **SQL Editor** → paste each migration file's
> contents in numeric order (0001…0018) and run.

### 1.3 Deploy the two Edge Functions
These handle user management (RLS can't create auth users):

```bash
cd TitaouCRM
supabase functions deploy invite-user
supabase functions deploy manage-user
```

Then set their secrets (Dashboard → Edge Functions → Secrets):
```
SUPABASE_URL=https://<ref>.supabase.co
SUPABASE_ANON_KEY=<anon key>
SUPABASE_SERVICE_ROLE_KEY=<service role key — NEVER ships with any app>
```

### 1.4 Create your organization + users
Run once in the SQL editor (or adapt the demo seed):

```sql
-- 1) The tenant
insert into organizations (name, currency, tax_rate)
values ('Titaou Distribution SARL', 'DZD', 0.19);

-- 2) The owner: create the auth user in Dashboard → Authentication → Add user
--    (email + password, auto-confirm). The handle_new_user trigger creates
--    their profile row. Then attach it to the org:
update profiles set role='admin', organization_id=(select id from organizations order by created_at limit 1)
  where email='owner@yourdomain.dz';
```

> Field staff (presellers/sellers) should NOT be created by hand — invite them
> from TitaouPosCRM → **Field Team → Invite member**, or from the Flutter
> admin's Users screen. Both call the `invite-user` function.

### 1.5 Security checklist (do once)
- [ ] Rotate the service-role key if it was ever pasted anywhere (Settings → API → Reset).
- [ ] Confirm RLS on: run `select tablename from pg_tables where schemaname='public'` — every table must have policies (`pg_policies`).
- [ ] In Supabase → Authentication → Providers → Email: disable "Allow new users to sign up" once your team exists (invites still work via the functions).

---

## Part 2 — TitaouPosCRM (the POS / admin station)

### 2.1 Install
Run the installer (`TitaouPosCRM_x64-setup.exe` or `.msi`). It installs
side-by-side with the original TitaouPOS — **separate everything**:

| | TitaouPOS | TitaouPosCRM |
|---|---|---|
| Data folder | `%APPDATA%\TitaouPosT\` | `%APPDATA%\TitaouPosCRM\` |
| Database | `titaou_post.db` | `titaou_poscrm.db` |
| App identifier | `com.titaou.pos` | `com.titaou.poscrm` |
| LAN API port | **8080** (default) | **8090** (default) |
| mDNS discovery | `_titaoupos._tcp.local.` | `_titaouposcrm._tcp.local.` |
| Autostart key | `HKCU\...\Run\TitaouPOS` | `HKCU\...\Run\TitaouPosCRM` |
| Updater feed | TitaouPosT releases | TitaouPosCRM releases |

Both apps can run on the same PC at the same time: different processes,
different data files, different ports, different discovery namespaces, and
single-instance locks are per-identifier (opening TitaouPosCRM twice focuses
its own window, and never touches a running TitaouPOS).

### 2.2 Connect to your org (Cloud Sync)
In TitaouPosCRM: **Settings → Cloud Sync**:

| Field | What to paste |
|---|---|
| Supabase URL | `https://<ref>.supabase.co` |
| Anon key | `sb_publishable_…` / the project anon key |
| CRM admin email | the owner account from 1.4 |
| CRM password | its password |

- **Test connection** first — it prints the account name + role + org id.
- **Save & Connect** signs in, verifies the account is an active admin, and
  runs the first cycle: the walk-in client is created (`ensure_pos_client`),
  products link by SKU/barcode (no duplicates), and the initial pull runs.
- The password is used once — only a refresh token is stored locally.
- The sync loop then runs every 5 seconds, **on the LAN server terminal
  only** (client terminals of your shop LAN forward their operations to it).

### 2.3 What flows where
- **Pushed** (idempotent — safe on flaky networks): counter sales
  (`orders source='pos'`), refunds (return movements), purchases (bon d'achat),
  product creates/updates, customers, customer debt payments.
- **Pulled**: CRM products (price updates), clients → POS customers with
  balances (Supabase is the balance authority), field orders → read-only
  **Field Orders** mirror + local stock movements.
- Money: POS whole DZD ↔ CRM centimes (×100 / ÷100) — automatic.

### 2.4 Admin-station duties
- **Field Team**: invite presellers/sellers, edit names/roles/passwords,
  delete (refuses to delete the last active admin).
- **Truck Loads**: pick a seller + route → quantities auto-fill from the
  route's orders → create the manifest, print the load sheet, record
  undelivered goods as returns (+stock movements on the CRM ledger).
- **Deletion Requests**: presellers can only *request* a client deletion;
  approve (FK-safe — a client with orders is rejected with a clear message;
  deactivate instead) or reject here.

---

## Part 3 — PowerSync (offline field apps)

PowerSync gives the Flutter preseller/seller apps offline SQLite + sync,
reusing the Supabase JWT. **The POS is not involved in this leg** — it talks
to Supabase directly; the field apps get POS data via Supabase.

### 3.1 Create the PowerSync instance
1. Sign up at https://powersync.com → create a free instance
   (e.g. name it `titaoucrm`).
2. Note the **PowerSync URL**:
   `https://<instance-id>.powersync.journeyapps.com`.
3. Under **Authentication**: enable **Supabase JWT** validation
   (paste your Supabase **JWT secret** — Dashboard → Settings → API → JWT Settings).
4. Under **Replication → Postgres**: connect your Supabase connection string
   (`Settings → Database → Connection string`, use the **pooled** 6543 URI
   with `?pgbouncer=true`).

### 3.2 Sync rules (Dashboard → Sync rules → Edit)
Mirror the org-scoping of your RLS. Each field user gets only their org's
data (and their own assigned clients):

```yaml
bucket_definitions:
  by_org:
    parameters:
      - select organization_id from profiles where id = request.user_id()
    data:
      - select * from sync_clients where organization_id = bucket.organization_id
      - select * from products where organization_id = bucket.organization_id
      - select * from orders
          where organization_id = bucket.organization_id
            and (preseller_id = request.user_id() or seller_id = request.user_id())
      - select * from order_items where organization_id = bucket.organization_id
      - select * from payments where organization_id = bucket.organization_id
      - select * from visits where organization_id = bucket.organization_id
      - select * from routes where organization_id = bucket.organization_id
      - select * from truck_loads
          where organization_id = bucket.organization_id
            and seller_id = request.user_id()
      - select * from truck_load_items ti
          join truck_loads tl on tl.id = ti.load_id
          where ti.organization_id = bucket.organization_id
            and tl.seller_id = request.user_id()
```

> `sync_clients` is the migration-0015 view (lat/lng split out — PowerSync
> can't parse PostGIS geography). Deploy + save; it shows "Deployed" when valid.

### 3.3 Flutter app configuration
`app/.env` (see `.env.example`):

```
SUPABASE_URL=https://<ref>.supabase.co
SUPABASE_ANON_KEY=<anon key>
POWERSYNC_URL=https://<instance>.powersync.journeyapps.com
```

Then:
```bash
cd app
flutter pub get
flutter gen-l10n
flutter run -d <android-device-id>
```

### 3.4 Verify offline works (the acceptance test)
1. Sign in as a preseller, confirm the nearest-first list loads.
2. Turn on airplane mode → add a client + take an order — both must save
   locally (PowerSync queues the writes).
3. Back online → within seconds the order appears in Supabase, in
   TitaouPosCRM's **Field Orders**, and stock drops in the POS.
4. In the POS, record a truck load for that preseller's route → the seller's
   app shows the manifest banner.

---

## Part 4 — Day-2 operations cheat-sheet

| Task | Where |
|---|---|
| Add a preseller/seller | POS → Field Team → Invite |
| Change a price | POS product editor → pushes to CRM → field apps pull it |
| Counter sale | POS → syncs as `orders.source='pos'` (visible in CRM stats) |
| Client can't be deleted (has orders) | Deletion request is rejected by the RPC → deactivate instead |
| Undelivered goods back from a route | POS → Truck Loads → Record returns |
| Field user forgot password | POS → Field Team → Edit → new password |
| Stop cloud sync | POS → Cloud Sync → Disconnect (outbox keeps pending events for next connect) |
| Restore POS from backup | POS Settings → Backups (unchanged from TitaouPOS) |

## Troubleshooting

- **"product not linked to CRM yet"** in sync status: the sale stays queued;
  it syncs automatically after the product pull links it (check that the
  product exists in the CRM with the same SKU/barcode).
- **Login fails from POS**: the account must be an **active admin** — the
  POS is the owner's station by design.
- **Client terminal shows "runs on the server terminal"**: expected — configure
  Cloud Sync on the PC that is the shop server (or a standalone PC).
- **mDNS cross-talk**: impossible by construction — TitaouPOS advertises
  `_titaoupos._tcp.local.`, TitaouPosCRM advertises `_titaouposcrm._tcp.local.`.
