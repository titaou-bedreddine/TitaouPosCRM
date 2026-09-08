/**
 * Live cross-PC cache invalidation (LAN).
 *
 * The shop server broadcasts a mutation event to every connected terminal
 * (product_updated, session_*, settings_updated...). This store turns those
 * events into per-kind invalidation counters: any view that displays the
 * affected data subscribes to the counter it cares about and re-fetches —
 * so stock changes made on the SERVER (or another client) appear on this
 * terminal's POS/Inventory/Dashboard without navigating anywhere.
 */
import { writable } from 'svelte/store';

export interface Invalidations {
  products: number;
  sessions: number;
  settings: number;
  expenses: number;
  sales: number;
}

export const invalidations = writable<Invalidations>({
  products: 0,
  sessions: 0,
  settings: 0,
  expenses: 0,
  sales: 0,
});

/** Bump the counter for every kind an event type makes stale. */
export function invalidateFromEvent(type: string): void {
  const kind =
    type === 'product_updated'
      ? 'products'
      : type.startsWith('session_')
        ? 'sessions'
        : type === 'settings_updated'
          ? 'settings'
          : type === 'expense_updated' || type === 'expense_added'
            ? 'expenses'
            : type.startsWith('sale_') || type === 'sale_updated'
              ? 'sales'
              : null;
  if (kind) {
    invalidations.update((i) => ({ ...i, [kind]: i[kind] + 1 }));
  }
}
