/**
 * LAN shop network store + the client-mode invoke patch.
 *
 * When this PC operates as a CLIENT terminal, every whitelisted business
 * command (`list_sales`, `process_sale`, …) is transparently rerouted to the
 * shop server through `network_forward` — the views keep calling the same
 * commands and never need to know. Hardware commands (printers, drawer,
 * scale), backups, licensing and the network commands themselves are NOT in
 * the forwardable set: they stay local to each PC by design.
 */
import { writable, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

export interface NetPeerInfo {
  node_id: string;
  pc_name: string;
  role: string;
  is_coordinator: boolean;
  shop_name: string;
  ip: string;
  term: number;
}

export interface NetDeviceInfo {
  node_id: string;
  pc_name: string;
  role_pref: string;
  ip: string;
  app_version: string;
  online: boolean;
  last_seen_secs_ago: number;
}

export interface NetworkStatus {
  enabled: boolean;
  role: 'server' | 'client' | 'automatic' | string;
  mode:
    | 'disabled'
    | 'server'
    | 'standalone'
    | 'connected'
    | 'searching'
    | 'reconnecting'
    | 'offline'
    | string;
  serving: boolean;
  node_id: string;
  pc_name: string;
  shop_id: string;
  shop_name: string;
  coordinator: { node_id: string; pc_name: string } | null;
  server_url: string | null;
  term: number;
  devices: NetDeviceInfo[] | null;
  devices_count: number;
  lan_ips: string[];
  port: number;
  autodiscovery: boolean;
  autoreconnect: boolean;
  manual_server: string;
  logged_in: boolean;
  last_event: NetEvent | null;
  events: NetEvent[];
  known_peers: NetPeerInfo[];
}

export interface NetEvent {
  type: string;
  data: any;
  ts: number;
  source?: string;
}

/** Live network status (null until the first snapshot arrives). */
export const networkStatus = writable<NetworkStatus | null>(null);

/** Recent real-time events from the shop server (client mode). */
export const networkEvents = writable<NetEvent[]>([]);

// ---------------------------------------------------------------------------
// The forwarding whitelist — must mirror network::invoke_registry
// (a Rust test asserts every entry exists in the dispatch table).
// ---------------------------------------------------------------------------
const FORWARDABLE = new Set([
  'get_active_users', 'get_user_by_qr', 'login_with_rfid', 'verify_admin_password',
  'change_user_password', 'get_all_users', 'get_all_roles', 'create_user', 'update_user',
  'delete_user', 'toggle_user_pin', 'get_dashboard_stats', 'get_active_cash_session',
  'open_cash_session', 'add_cash_movement', 'close_cash_session', 'list_cash_movements',
  'list_session_history', 'edit_opening_balance', 'edit_cash_session', 'archive_cash_session',
  'delete_cash_session', 'search_products', 'save_product', 'delete_product', 'get_categories',
  'save_category', 'delete_category', 'get_units', 'save_unit', 'toggle_product_pin',
  'reorder_pinned_products', 'list_packagings', 'save_packagings', 'get_price_history',
  'get_quantity_history', 'resolve_scale_scan', 'process_sale', 'create_sale', 'replace_sale',
  'list_sales', 'get_sale_items', 'get_last_sale', 'get_sale_by_number', 'hold_sale',
  'list_held_sales', 'delete_held_sale', 'delete_sale', 'list_customers', 'save_customer',
  'delete_customer', 'toggle_customer_pin', 'record_customer_debt_payment',
  'clear_customer_debt', 'list_debt_clear_log', 'list_suppliers', 'save_supplier',
  'delete_supplier', 'toggle_supplier_pin', 'record_supplier_debt_payment',
  'list_supplier_debt_payments', 'clear_supplier_debt', 'create_purchase', 'get_purchase_items',
  'delete_purchase', 'list_purchases', 'add_expense', 'update_expense', 'list_expenses',
  'delete_expense', 'list_employees', 'save_employee', 'find_employee_by_rfid',
  'next_employee_code', 'delete_employee', 'list_payrolls', 'record_employee_advance',
  'list_employee_advances', 'record_employee_absence', 'list_employee_absences',
  'get_all_settings', 'get_setting', 'set_setting', 'set_multiple_settings',
  'list_app_notifications', 'dismiss_app_notification', 'run_payroll_reminder_scan',
  'send_telegram_message', 'send_telegram_recap',
]);

let clientForwarding = false;

/** Flip client-mode forwarding on/off (driven by the network mode). */
export function setClientForwarding(on: boolean) {
  clientForwarding = on;
}

export function isForwardableCommand(cmd: string): boolean {
  return FORWARDABLE.has(cmd);
}

/**
 * Install the invoke interceptor. MUST run before any view issues a command
 * (called synchronously at the top of main.ts). Idempotent.
 */
export function installInvokePatch(): void {
  const internals = (window as any).__TAURI_INTERNALS__;
  if (!internals || typeof internals.invoke !== 'function') return;
  if ((internals as any).__titaouNetPatched) return;
  const orig = internals.invoke.bind(internals);
  internals.invoke = (
    cmd: string,
    args?: Record<string, unknown>,
    options?: unknown
  ): Promise<unknown> => {
    if (clientForwarding && FORWARDABLE.has(cmd)) {
      return orig('network_forward', { command: cmd, args: args ?? {} }, options);
    }
    return orig(cmd, args, options);
  };
  (internals as any).__titaouNetPatched = true;
}

/** Pull a fresh status snapshot from the backend. */
export async function refreshNetworkStatus(): Promise<NetworkStatus | null> {
  try {
    const status = await invoke<NetworkStatus>('network_get_status');
    networkStatus.set(status);
    setClientForwarding(status?.mode === 'connected');
    return status;
  } catch {
    return null;
  }
}

let initialized = false;

/** Subscribe to backend status/event pushes; call once at startup. */
export async function initNetwork(): Promise<void> {
  if (initialized) return;
  initialized = true;
  try {
    const { listen } = await import('@tauri-apps/api/event');
    await listen<NetworkStatus>('network://status', (e) => {
      networkStatus.set(e.payload);
      setClientForwarding(e.payload?.mode === 'connected');
    });
    await listen<NetEvent>('network://event', (e) => {
      if (!e.payload) return;
      networkEvents.update((list) => [e.payload, ...list].slice(0, 50));
    });
  } catch {
    // Event API unavailable (non-Tauri context) — status polling still works.
  }
  await refreshNetworkStatus();
}

/** Fire-and-forget actions used by the UI. */
export async function becomeServer(shopName?: string) {
  const r = await invoke<any>('network_become_server', { shopName: shopName ?? null });
  await refreshNetworkStatus();
  return r;
}

export async function setNetworkRole(role: string) {
  await invoke('network_set_role', { role });
  await refreshNetworkStatus();
}

export async function leaveShop() {
  await invoke('network_leave_shop');
  await refreshNetworkStatus();
}

export async function probeServer(addr: string) {
  return invoke<any>('network_probe_server', { addr });
}

export async function joinServer(addr: string, shopId: string) {
  const r = await invoke<any>('network_join_server', { addr, shopId });
  await refreshNetworkStatus();
  return r;
}

/** Recent events helper for the popup. */
export function recentEvents(): NetEvent[] {
  return get(networkEvents);
}
