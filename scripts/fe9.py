import io

def edit(path, subs):
    with io.open(path, 'r', encoding='utf-8') as f:
        s = f.read()
    for old, new, label in subs:
        if old not in s:
            print('MISS', label)
            continue
        s = s.replace(old, new, 1)
        print('ok:', label)
    with io.open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(s)

# ===== 1) CartItem type: is_scalable flag rides into the cart =====
edit('src/lib/types/index.ts', [
    ('''  // Product's available inventory balance for negative-stock enforcement.
  current_stock?: number;
}''',
     '''  // Product's available inventory balance for negative-stock enforcement.
  current_stock?: number;
  // Scale product: enables weight-suggestion chips on the cart line.
  is_scalable?: boolean;
}''', 'CartItem is_scalable'),
])

# ===== 2) cart.ts: stamp is_scalable on add + learned-weights helpers =====
edit('src/lib/stores/cart.ts', [
    ('''        is_refund: asRefund,
        expiry_date: (product as any).expiry_date,
        purchase_price: product.purchase_price,
        current_stock: product.current_stock,
      };''',
     '''        is_refund: asRefund,
        expiry_date: (product as any).expiry_date,
        purchase_price: product.purchase_price,
        current_stock: product.current_stock,
        is_scalable: (product as any).is_scalable,
      };''', 'stamp is_scalable'),
    ('''export function applySaleRounding(total: number, step: number): number {''',
     '''// ---------------------------------------------------------------------------
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

export function applySaleRounding(total: number, step: number): number {''', 'learned weights helpers'),
])

# ===== 3) record quantities on checkout success =====
edit('src/routes/pos/PosView.svelte', [
    ('''      lastSaleSuccessNumber = saleNumber;
      clearCart();
      await loadProducts();''',
     '''      // Learn the sold quantities so scalable products get smart
      // weight-suggestion chips next time (1kg, 0.5, eggs 30…).
      recordSoldQuantities($cartItems.map((i) => ({ product_id: i.product_id, quantity: i.quantity })));

      lastSaleSuccessNumber = saleNumber;
      clearCart();
      await loadProducts();''', 'record at checkout'),
    ('''  import { cartItems, cartGrandTotal, cartSubtotal, globalDiscountAmount, globalDiscountMode, globalDiscountValue, globalDiscountPercent, isRefundMode, addToCart, clearCart, cartItemOrder, qtyEditTarget, itemKey, stopQtyEdit, posMode, originSaleId, restoreActiveCart, holdCurrentSale, allowNegativeStock, saleTotalRoundingStep } from '../../lib/stores/cart';''',
     '''  import { cartItems, cartGrandTotal, cartSubtotal, globalDiscountAmount, globalDiscountMode, globalDiscountValue, globalDiscountPercent, isRefundMode, addToCart, clearCart, cartItemOrder, qtyEditTarget, itemKey, stopQtyEdit, posMode, originSaleId, restoreActiveCart, holdCurrentSale, allowNegativeStock, saleTotalRoundingStep, recordSoldQuantities } from '../../lib/stores/cart';''', 'import record'),
])

# ===== 4) CartItemCard: suggestion chips for scalable lines =====
edit('src/lib/components/CartItemCard.svelte', [
    ('''  \$: isJustAdded = \$lastAddedProductId === item.product_id;''',
     '''  // Weight/quantity suggestion chips for SCALE products: learned from
  // real sales (localStorage) + sensible defaults. One tap sets the qty —
  // they live inside the line so scanning flow is never interrupted.
  \$: showQtyChips = item.is_scalable === true;
  \$: qtySuggestions = showQtyChips ? suggestedQuantities(item.product_id) : [];
  function applySuggestion(qty: number) {
    updateItemQuantity(item.product_id, item.is_refund, qty);
  }

  \$: isJustAdded = \$lastAddedProductId === item.product_id;''', 'chip logic'),
    # import suggestedQuantities
    ('''  import {
    updateItemQuantity,''',
     '''  import {
    suggestedQuantities,
    updateItemQuantity,''', 'import suggestions'),
])
# markup: add chips under the qty stepper — find the qty row container
with io.open(path, 'r', encoding='utf-8') as f:
    s = f.read()
marker = "  \$: isJustAdded = \$lastAddedProductId === item.product_id;"
print('marker present:', marker in s)

# find a stable markup anchor: the stepper buttons container
import re
m = re.search(r'(on:click=\{decrement\}.*?on:click=\{increment\}.*?</button>)', s, re.S)
if m:
    print('stepper found')
with io.open(path, 'w', encoding='utf-8', newline='') as f:
    f.write(s)
print('cart item card logic done')
