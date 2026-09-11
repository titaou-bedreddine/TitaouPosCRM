import { writable, derived, get } from 'svelte/store';
import type { CartItem, Product, HeldSale, PackagingDef } from '../types';
import { invoke } from '@tauri-apps/api/core';
import { selectedCustomerId, DEFAULT_WALKIN_CUSTOMER_ID } from './customers';

export const cartItems = writable<CartItem[]>([]);
export const globalDiscountMode = writable<'none' | 'percent' | 'amount'>('none');
export const globalDiscountValue = writable<number>(0);
export const isRefundMode = writable<boolean>(false);
export const heldSalesList = writable<HeldSale[]>([]);
export const lastAddedProductId = writable<number | null>(null);
export const heldNotification = writable<string | null>(null);
export const cartItemOrder = writable<'top' | 'bottom'>('bottom');
export const allowNegativeStock = writable<boolean>(false);
// Every packaging definition (loaded once per session in PosView) — drives
// the cart unit pickers and the stock decomposition displays.
export const allPackagings = writable<PackagingDef[]>([]);

// POS transaction mode: sale (default), purchase (stock from supplier),
// broken (damaged goods written off as expenses).
export type PosMode = 'sale' | 'purchase' | 'broken';
export const posMode = writable<PosMode>('sale');
// When a sale from history is loaded for editing, this holds its id; the
// POS checkout then UPDATES that sale in place instead of creating a new
// (duplicate) row.
export const originSaleId = writable<number | null>(null);

// Quantity-edit mode (F6): the cart line currently being edited, keyed by
// "productId[_ref]". null = not editing.
export const qtyEditTarget = writable<string | null>(null);

export function itemKey(item: { product_id: number; is_refund?: boolean }): string {
  return `${item.product_id}${item.is_refund ? '_ref' : ''}`;
}

export function startQtyEdit(item: { product_id: number; is_refund?: boolean }) {
  qtyEditTarget.set(itemKey(item));
}

export function stopQtyEdit() {
  qtyEditTarget.set(null);
}

// Kept for backward compatibility: the canonical store now lives in ./customers.
export { selectedCustomerId };

export const stockWarningModal = writable<{
  productName: string;
  available: number;
  requested: number;
} | null>(null);

// Base-quantity helper: a packaging line's stock weight in base units.
export function lineBaseQuantity(item: {
  quantity: number;
  units_per_package?: number;
}): number {
  const upp = item.units_per_package && item.units_per_package > 0
    ? item.units_per_package
    : 1;
  return item.quantity * upp;
}

export function addToCart(
  product: Product,
  quantity = 1,
  asRefund = false,
  packaging?: PackagingDef
): boolean {
  const upp = packaging ? packaging.units_per_package : 1;
  const baseQty = quantity * upp;
  if (!asRefund && !get(allowNegativeStock) && get(posMode) === 'sale') {
    const items = get(cartItems);
    const existing = items.find((i) => i.product_id === product.id && !i.is_refund);
    const existingBase = existing ? lineBaseQuantity(existing) : 0;
    const available = product.current_stock ?? 0;
    if (existingBase + baseQty > available) {
      stockWarningModal.set({
        productName:
          (packaging ? packaging.name + ' — ' : '') +
          (product.name_fr || product.name_ar || product.name_en || 'Product'),
        available,
        requested: existingBase + baseQty,
      });
      return false;
    }
  }

  isCartExplicitlyCleared = false;
  lastAddedProductId.set(product.id);
  setTimeout(() => lastAddedProductId.set(null), 800);

  cartItems.update((items) => {
    const existingIndex = items.findIndex(
      (item) => item.product_id === product.id && item.is_refund === asRefund
    );

    if (existingIndex > -1 && (!packaging || items[existingIndex].sale_unit === packaging.name)) {
      const updated = [...items];
      updated[existingIndex].quantity += quantity;
      const unitNet = Math.max(0, updated[existingIndex].unit_price - updated[existingIndex].discount_amount);
      updated[existingIndex].base_quantity = lineBaseQuantity(updated[existingIndex]);
      updated[existingIndex].total_price = Math.round(updated[existingIndex].quantity * unitNet);
      return updated;
    } else {
      const newItem: CartItem = {
        product_id: product.id,
        sku: product.sku,
        barcode: (product.barcodes && product.barcodes[0]) ? product.barcodes[0] : (product.sku || ''),
        name_ar: product.name_ar,
        name_fr: product.name_fr,
        name_en: product.name_en,
        image_path: product.image_path,
        unit_price: packaging ? packaging.sale_price : product.sale_price,
        quantity,
        discount_amount: 0,
        tax_amount: 0,
        total_price: (packaging ? packaging.sale_price : product.sale_price) * quantity,
        is_refund: asRefund,
        sale_unit: packaging ? packaging.name : undefined,
        units_per_package: packaging ? packaging.units_per_package : 1,
        base_quantity: baseQty,
        expiry_date: (product as any).expiry_date,
        purchase_price: product.purchase_price,
        current_stock: product.current_stock,
        is_scalable: (product as any).is_scalable,
      };
      
      const order = get(cartItemOrder);
      if (order === 'top') {
        return [newItem, ...items];
      } else {
        return [...items, newItem];
      }
    }
  });
  return true;
}

export function updateItemQuantity(productId: number, isRefund: boolean, newQty: number) {
  if (newQty <= 0) {
    removeFromCart(productId, isRefund);
    return;
  }
  if (!isRefund && !get(allowNegativeStock) && get(posMode) === 'sale') {
    const items = get(cartItems);
    const item = items.find((i) => i.product_id === productId && !i.is_refund);
    if (item && item.current_stock !== undefined && newQty > item.current_stock) {
      stockWarningModal.set({
        productName: item.name_fr || item.name_ar || item.name_en || 'Product',
        available: item.current_stock,
        requested: newQty,
      });
      return;
    }
  }
  cartItems.update((items) =>
    items.map((item) => {
      if (item.product_id === productId && item.is_refund === isRefund) {
        const unitNet = Math.max(0, item.unit_price - item.discount_amount);
        const total = Math.round(newQty * unitNet);
        return { ...item, quantity: newQty, total_price: total };
      }
      return item;
    })
  );
}

export function applyItemDiscount(productId: number, isRefund: boolean, discountPerUnit: number) {
  cartItems.update((items) =>
    items.map((item) => {
      if (item.product_id === productId && item.is_refund === isRefund) {
        const disc = Math.min(Math.max(0, discountPerUnit), item.unit_price);
        item.base_quantity = lineBaseQuantity(item);
      const total = Math.round(item.quantity * (item.unit_price - disc));
        return { ...item, discount_amount: disc, total_price: total };
      }
      return item;
    })
  );
}

export function toggleItemRefund(productId: number, currentRefundState: boolean) {
  cartItems.update((items) =>
    items.map((item) => {
      if (item.product_id === productId && item.is_refund === currentRefundState) {
        return { ...item, is_refund: !currentRefundState };
      }
      return item;
    })
  );
}

export function toggleAllCartRefund() {
  cartItems.update((items) => {
    const anyNormal = items.some((i) => !i.is_refund);
    return items.map((i) => ({ ...i, is_refund: anyNormal }));
  });
}

export function setLineUnit(
  item: CartItem,
  packaging: PackagingDef | null,
  baseSalePrice: number
): boolean {
  const items = get(cartItems);
  const target = items.find(
    (i) => i.product_id === item.product_id && i.sale_unit === item.sale_unit
  );
  if (!target) return false;
  const updated = [...items];
  const idx = updated.indexOf(target);
  const line = { ...updated[idx] };
  const oldBase = lineBaseQuantity(line);
  if (packaging) {
    line.sale_unit = packaging.name;
    line.units_per_package = packaging.units_per_package;
    line.unit_price = packaging.sale_price;
  } else {
    line.sale_unit = undefined;
    line.units_per_package = 1;
    line.unit_price = baseSalePrice;
  }
  const unitNet = Math.max(0, line.unit_price - line.discount_amount);
  line.total_price = Math.round(line.quantity * unitNet);
  line.base_quantity = lineBaseQuantity(line);
  updated[idx] = line;
  cartItems.set(updated);
  return true;
}

export function removeFromCart(productId: number, isRefund: boolean) {
  cartItems.update((items) =>
    items.filter((item) => !(item.product_id === productId && item.is_refund === isRefund))
  );
}

// --- Cart persistence across restarts/crashes -----------------------------
// The active cart (items + discount + customer) is mirrored into
// app_settings under 'active_cart_json'. checkout/clear wipes it; resume
// (app start) restores it, so a shutdown never loses an in-progress sale.

const ACTIVE_CART_KEY = 'active_cart_json';
let hasRestoredOnce = false;
let isCartExplicitlyCleared = false;

export function persistActiveCart() {
  if (isCartExplicitlyCleared) return;
  try {
    const payload = JSON.stringify({
      items: get(cartItems),
      discountMode: get(globalDiscountMode),
      discountValue: get(globalDiscountValue),
      customerId: get(selectedCustomerId),
      mode: get(posMode),
      savedAt: Date.now(),
    });
    invoke('set_setting', { key: ACTIVE_CART_KEY, value: payload }).catch(() => {});
  } catch {
    // Persistence must never interrupt selling.
  }
}

export function clearPersistedCart() {
  try {
    invoke('set_setting', { key: ACTIVE_CART_KEY, value: '' }).catch(() => {});
  } catch {}
}

export async function restoreActiveCart() {
  if (isCartExplicitlyCleared) return false;
  if (hasRestoredOnce && get(cartItems).length === 0) return false;
  try {
    const payload = await invoke<string | null>('get_setting', { key: ACTIVE_CART_KEY });
    hasRestoredOnce = true;
    if (!payload || payload.trim() === '') return false;
    const parsed = JSON.parse(payload);
    if (!parsed || !Array.isArray(parsed.items) || parsed.items.length === 0) return false;
    cartItems.set(parsed.items);
    if (parsed.discountMode === 'percent' || parsed.discountMode === 'amount') {
      globalDiscountMode.set(parsed.discountMode);
      globalDiscountValue.set(Number(parsed.discountValue) || 0);
    }
    if (parsed.customerId) selectedCustomerId.set(parsed.customerId);
    if (parsed.mode === 'purchase' || parsed.mode === 'broken' || parsed.mode === 'sale') {
      posMode.set(parsed.mode);
    }
    return true;
  } catch (e) {
    console.warn('Could not restore active cart:', e);
    return false;
  }
}

function mirrorCartToDb() {
  const items = get(cartItems);
  if (items.length > 0) {
    isCartExplicitlyCleared = false;
    persistActiveCart();
  }
}

// Re-mirror after every cart mutation.
cartItems.subscribe(() => mirrorCartToDb());
globalDiscountMode.subscribe(() => mirrorCartToDb());
globalDiscountValue.subscribe(() => mirrorCartToDb());

export function clearCart() {
  isCartExplicitlyCleared = true;
  cartItems.set([]);
  globalDiscountMode.set('none');
  globalDiscountValue.set(0);
  // New sale resets to the walk-in customer, not "no customer".
  selectedCustomerId.set(DEFAULT_WALKIN_CUSTOMER_ID);
  isRefundMode.set(false);
  posMode.set('sale');
  originSaleId.set(null);
  stopQtyEdit();
  clearPersistedCart();
}

// Effective cart-level discount in DZD; never exceeds the cart amount so the total can't go negative.
export function computeCartDiscount(subtotal: number, mode: 'none' | 'percent' | 'amount', value: number): number {
  const base = Math.max(0, subtotal);
  if (mode === 'percent' && value > 0) {
    return Math.min(base, Math.round((base * Math.min(100, value)) / 100));
  }
  if (mode === 'amount' && value > 0) {
    return Math.min(base, Math.round(value));
  }
  return 0;
}

// Held carts saved with a cart-level remise store { items, discountMode, discountValue };
// older rows are plain CartItem[] arrays, so parse both shapes.
export function parseHeldCart(json: string): { items: CartItem[]; discountMode: 'none' | 'percent' | 'amount'; discountValue: number } {
  try {
    const parsed = JSON.parse(json);
    if (Array.isArray(parsed)) {
      return { items: parsed as CartItem[], discountMode: 'none', discountValue: 0 };
    }
    if (parsed && Array.isArray(parsed.items)) {
      const mode = parsed.discountMode === 'percent' || parsed.discountMode === 'amount' ? parsed.discountMode : 'none';
      return { items: parsed.items as CartItem[], discountMode: mode, discountValue: Number(parsed.discountValue) || 0 };
    }
    return { items: [], discountMode: 'none', discountValue: 0 };
  } catch {
    return { items: [], discountMode: 'none', discountValue: 0 };
  }
}

export async function holdCurrentSale(note?: string): Promise<boolean> {
  const items = get(cartItems);
  if (items.length === 0) return false;

  try {
    const total = get(cartGrandTotal);
    const customerId = get(selectedCustomerId);
    const timeStr = new Date().toLocaleTimeString();
    const finalNote = note?.trim()
      ? `${note.trim()} • ${total.toLocaleString()} DZD`
      : `${total.toLocaleString()} DZD (${items.length} items - ${timeStr})`;

    await invoke('hold_sale', {
      customerId,
      cartDataJson: JSON.stringify({
        items,
        discountMode: get(globalDiscountMode),
        discountValue: get(globalDiscountValue),
        // The cart's POS mode (sale/purchase/broken) rides in the JSON so
        // resuming returns the user to the mode they held from — the note
        // string is display-only now.
        mode: get(posMode),
      }),
      totalAmount: total,
      notes: finalNote,
    });

    clearCart();
    await refreshHeldSales();

    heldNotification.set(`Cart #${total.toLocaleString()} DZD (${items.length} items) held!`);
    setTimeout(() => heldNotification.set(null), 4000);
    return true;
  } catch (e) {
    console.error('Failed to hold sale:', e);
    return false;
  }
}

export async function refreshHeldSales() {
  try {
    const list = await invoke<HeldSale[]>('list_held_sales');
    heldSalesList.set(list);
  } catch (e) {
    console.error(e);
  }
}

function sumCartLines($items: CartItem[]): number {
  return $items.reduce((sum, item) => {
    const lineVal = item.total_price;
    return item.is_refund ? sum - lineVal : sum + lineVal;
  }, 0);
}

export const cartSubtotal = derived(cartItems, ($items) => sumCartLines($items));

// Cart-level remise actually applied, in DZD (clamped so it can never exceed the cart amount).
export const globalDiscountAmount = derived(
  [cartItems, globalDiscountMode, globalDiscountValue],
  ([$items, $mode, $val]) => computeCartDiscount(sumCartLines($items), $mode, $val)
);

export const globalDiscountPercent = derived(
  [globalDiscountMode, globalDiscountValue],
  ([$mode, $val]) => ($mode === 'percent' && $val > 0 ? Math.min(100, $val) : 0)
);

// Checkout sale-total rounding (Settings → POS Rules): 'off' | '50' | '100'.
// OFF by default; 50/100 round the GRAND TOTAL for quick cash handling.
// Never applied to negative (refund) totals, purchase prices or history.
export const saleTotalRoundingStep = writable<number>(0);

// ---------------------------------------------------------------------------
// Learned-weight suggestions (v0.5.20): the POS remembers which quantities a
// product is usually sold in (e.g. Lait → 1, Œufs → 30/15/10, Eau → 6) and
// surfaces them as one-tap chips on scalable cart lines. Stored in
// localStorage per product; checkout records what was actually sold.
// ---------------------------------------------------------------------------

const QTY_HISTORY_KEY = 'pos_qty_history_v1';

type QtyHistory = Record<string, number[]>; // productId → recent quantities

function loadQtyHistory(): QtyHistory {
  try {
    return JSON.parse(localStorage.getItem(QTY_HISTORY_KEY) || '{}') as QtyHistory;
  } catch {
    return {};
  }
}

/** Record a sold quantity at checkout (max 8 most-recent per product). */
export function recordSoldQuantities(items: { product_id: number; quantity: number }[]) {
  if (typeof localStorage === 'undefined') return;
  const hist = loadQtyHistory();
  let changed = false;
  for (const it of items) {
    const key = String(it.product_id);
    const qty = Math.round(it.quantity * 1000) / 1000;
    if (!Number.isFinite(qty) || qty <= 0) continue;
    const list = hist[key] || [];
    hist[key] = [qty, ...list.filter((q) => q !== qty)].slice(0, 8);
    changed = true;
  }
  if (changed) {
    try {
      localStorage.setItem(QTY_HISTORY_KEY, JSON.stringify(hist));
    } catch { /* storage full: suggestions degrade silently */ }
  }
}

/** Suggested quick-quantities for a scalable product (learned first, then defaults). */
export function suggestedQuantities(productId: number): number[] {
  const hist = loadQtyHistory();
  const learned = (hist[String(productId)] || []).slice(0, 4);
  const defaults = [1, 0.5, 0.25, 2];
  for (const d of defaults) {
    if (learned.length >= 4) break;
    if (!learned.includes(d)) learned.push(d);
  }
  return learned;
}

export function applySaleRounding(total: number, step: number): number {
  if (!step || step <= 0 || total <= 0) return total;
  return Math.round(total / step) * step;
}

// Rounded total shown in the POS and charged at checkout.
export const cartGrandTotal = derived(
  [cartItems, globalDiscountAmount, saleTotalRoundingStep],
  ([$items, $discount, $step]) =>
    applySaleRounding(sumCartLines($items) - $discount, $step)
);