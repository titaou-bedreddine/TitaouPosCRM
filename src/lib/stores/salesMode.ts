import { writable } from 'svelte/store';
import { invoke } from '@tauri-apps/api/core';

/** Business workflow: 'pre_sale' | 'direct_sale'. Loaded once at startup
 * (App) and updated live by the Settings screen, so the sidebar reacts
 * without an app restart. */
export const salesMode = writable<string>('pre_sale');

export async function loadSalesMode(): Promise<void> {
  try {
    const mode = (await invoke<string>('get_setting', { key: 'sales_mode' })) || 'pre_sale';
    salesMode.set(mode);
  } catch {
    salesMode.set('pre_sale');
  }
}
