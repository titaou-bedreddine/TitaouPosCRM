import { writable, derived } from 'svelte/store';
import type { User } from '../types';

export const currentUser = writable<User | null>(null);

export const isAuthenticated = derived(currentUser, ($u) => !!$u);

export function logout() {
  currentUser.set(null);
  // A fresh login always starts in SALE mode — the previous user may have
  // left the POS in purchase/broken mode.
  import('./cart').then(({ posMode }) => posMode.set('sale')).catch(() => {});
  // LAN client mode: drop the user's API token from memory (the device
  // stays registered). Never blocks logout.
  import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke('network_logout').catch(() => {}))
    .catch(() => {});
}

export function hasPermission(user: User | null, permissionCode: string): boolean {
  if (!user) return false;
  if (user.role_name === 'Administrator' || user.role_id === 1) return true;
  return user.permissions.includes(permissionCode);
}