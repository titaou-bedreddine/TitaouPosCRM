/**
 * LAN shop network store.
 *
 * When this PC operates as a CLIENT terminal, whitelisted business commands
 * are executed on the shop server — the interception lives in Rust (the
 * invoke_handler wrapper in lib.rs), so the views keep calling the same
 * commands and never need to know. This store only mirrors the live network
 * status and event feed for the UI (sidebar indicator, details popup).
 */
import { writable, get } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';
import { invalidateFromEvent } from './invalidations';

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
  /** True when the session was opened OFFLINE on a client (local fallback login). */
  offline_session: boolean;
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

/** Pull a fresh status snapshot from the backend. */
export async function refreshNetworkStatus(): Promise<NetworkStatus | null> {
  try {
    const status = await invoke<NetworkStatus>('network_get_status');
    networkStatus.set(status);
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
    });
    await listen<NetEvent>('network://event', (e) => {
      if (!e.payload) return;
      networkEvents.update((list) => [e.payload, ...list].slice(0, 50));
      // Live cross-PC sync: server-side mutations invalidate the matching
      // local caches (stock, sessions, settings...) so open views refresh.
      invalidateFromEvent(String(e.payload.type || ''));
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
