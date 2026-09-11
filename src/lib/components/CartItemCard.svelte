<script lang="ts">
  import type { CartItem } from '../types';
  import { currentLocale } from '../i18n';
  import { t } from '../i18n';
  import {
    suggestedQuantities,
    updateItemQuantity,
    applyItemDiscount,
    toggleItemRefund,
    removeFromCart,
    setLineUnit,
    allPackagings,
    lastAddedProductId,
    qtyEditTarget,
    itemKey,
    stopQtyEdit,
  } from '../stores/cart';
  import { Minus, Plus, Trash2, Undo2, Percent, Package, Eye, Edit2 } from 'lucide-svelte';

  export let item: CartItem;
  // Eye: reveal this item's purchase cost inline; Pen: open its editor.
  export let purchaseCost: number | null = null;
  // Base-unit sale price (for switching the line back to bottles).
  export let baseSalePrice: number = 0;
  export let onEdit: () => void = () => {};
  let showCost = false;

  let showDiscountInput = false;

  function changeUnit(e: Event) {
    const name = (e.currentTarget as HTMLSelectElement).value;
    if (!name || name === (item.sale_unit ?? 'BASE')) return;
    if (name === 'BASE') {
      setLineUnit(item, null, baseSalePrice);
    } else {
      const packaging = linePackagings.find((pp) => pp.name === name);
      if (packaging) setLineUnit(item, packaging, baseSalePrice);
    }
  }
  let lineDiscountValue: number | null = item.discount_amount;
  let discountInputEl: HTMLInputElement;
  let qtyInputEl: HTMLInputElement;

  // Re-seed from the live item each time the popover opens so an
  // already-applied remise shows up ready to edit.
  $: if (showDiscountInput) {
    lineDiscountValue = item.discount_amount;
  }

  $: maxUnitDiscount = item.unit_price;
  $: numericDiscount = lineDiscountValue === null || isNaN(lineDiscountValue as number) ? 0 : (lineDiscountValue as number);
  $: discountInvalid = numericDiscount < 0 || numericDiscount > maxUnitDiscount;

  // F6 quantity-edit mode: when this line is the active target, focus and
  // select the quantity so the user can just type a new value.
  $: myKey = itemKey(item);
  // This product's packagings (largest first) for the unit picker.
  $: linePackagings = ($allPackagings || []).filter((pp: any) => pp.product_id === item.product_id);
  $: hasUnits = (item.units_per_package ?? 1) !== 1 || linePackagings.length > 0;
  $: isQtyEditTarget = $qtyEditTarget === myKey;
  $: if (isQtyEditTarget && qtyInputEl) {
    qtyInputEl.focus();
    qtyInputEl.select();
  }

  function focusDiscountInput(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  function decrement() {
    updateItemQuantity(item.product_id, item.is_refund, item.quantity - 1);
  }

  function increment() {
    updateItemQuantity(item.product_id, item.is_refund, item.quantity + 1);
  }

  function handleQtyChange(e: Event) {
    const val = parseFloat((e.target as HTMLInputElement).value);
    if (!isNaN(val) && val > 0) {
      updateItemQuantity(item.product_id, item.is_refund, val);
    }
  }

  function toggleRefund() {
    toggleItemRefund(item.product_id, item.is_refund);
  }

  function applyDiscount() {
    if (discountInvalid) return;
    // Hard clamp: the per-unit remise can never exceed the unit price.
    const clamped = Math.min(Math.max(0, numericDiscount), maxUnitDiscount);
    applyItemDiscount(item.product_id, item.is_refund, clamped);
    showDiscountInput = false;
  }

  function clearDiscount() {
    applyItemDiscount(item.product_id, item.is_refund, 0);
    showDiscountInput = false;
  }

  // Weight/quantity suggestion chips for SCALE products: learned from
  // real sales (localStorage) + sensible defaults. One tap sets the qty —
  // they live inside the line so the scanning flow is never interrupted.
  $: showQtyChips = item.is_scalable === true && !item.is_refund;
  $: qtySuggestions = showQtyChips ? suggestedQuantities(item.product_id) : [];
  function applySuggestion(qty: number) {
    updateItemQuantity(item.product_id, item.is_refund, qty);
  }

  $: isJustAdded = $lastAddedProductId === item.product_id;
  $: displayName =
    $currentLocale === 'ar'
      ? item.name_ar || item.name_fr
      : $currentLocale === 'fr'
      ? item.name_fr || item.name_en
      : item.name_en || item.name_fr || item.name_ar;

  // Expired item: the cashier must see it the moment it lands in the cart,
  // before checkout — same urgency as the refund badge on the catalog card.
  $: isExpired = (() => {
    if (!item.expiry_date) return false;
    const exp = new Date(item.expiry_date).getTime();
    return !isNaN(exp) && exp < Date.now();
  })();
</script>

<div
  class="relative bg-pos-card border rounded-2xl p-2.5 shadow-xs transition-all duration-200 flex flex-col gap-2 {item.is_refund
    ? 'border-amber-500 bg-amber-500/5'
    : 'border-pos-border'} {isJustAdded
    ? 'ring-2 ring-emerald-500 bg-emerald-50/50 dark:bg-emerald-950/40 scale-[1.01]'
    : ''} {isQtyEditTarget
    ? 'ring-2 ring-sky-500 border-sky-400'
    : ''} {isExpired
    ? 'border-rose-500 bg-rose-500/5'
    : ''}"
>
  {#if isExpired}
    <div class="absolute inset-0 rounded-2xl pointer-events-none flex items-center justify-center overflow-hidden z-10">
      <span class="text-rose-600/25 font-black text-lg tracking-widest uppercase select-none" style="transform: rotate(-12deg);">
        EXPIRED / منتهي
      </span>
    </div>
    <span class="absolute top-1 end-1 z-20 inline-flex items-center gap-0.5 text-[9px] font-black px-1.5 py-0.2 rounded-full bg-rose-600 text-white font-mono shadow-xs">
      EXPIRED
    </span>
  {/if}
  <div class="flex items-center gap-3">
    <!-- Thumbnail Image -->
    <div
      class="w-11 h-11 rounded-xl bg-slate-100 dark:bg-slate-800 flex items-center justify-center shrink-0 overflow-hidden border border-pos-border/40"
    >
      {#if item.image_path}
        <img src={item.image_path} alt={displayName} class="w-full h-full object-cover" />
      {:else}
        <Package class="w-5 h-5 text-pos-muted/40" />
      {/if}
    </div>

    <!-- Title & Barcode -->
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-1.5">
        <h4 class="font-black text-xs text-pos-text truncate">
          {displayName}
        </h4>
      </div>

      <div class="flex items-center gap-2 text-[10px] text-pos-muted mt-0.5 font-mono">
        {#if item.barcode}
          <span>{item.barcode}</span>
        {/if}
        <span>•</span>
        <span class="font-bold text-pos-text">{item.unit_price.toLocaleString()} DZD</span>
        {#if item.discount_amount > 0}
          <span class="text-purple-600 font-bold">(-{item.discount_amount} DZD)</span>
        {/if}
      </div>
      {#if hasUnits}
        <div class="flex items-center gap-1.5 mt-0.5">
          <select
            class="text-[10px] font-black bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg px-1.5 py-0.5 text-pos-text outline-none cursor-pointer"
            value={item.sale_unit ?? 'BASE'}
            on:change={changeUnit}
          >
            <option value="BASE">Bouteille / زجاجة</option>
            {#each linePackagings as pp (pp.id)}
              <option value={pp.name}>{pp.name} ({pp.units_per_package})</option>
            {/each}
          </select>
          {#if (item.units_per_package ?? 1) !== 1}
            <span class="text-[9px] font-mono text-pos-muted">
              = {item.base_quantity?.toLocaleString()} u
            </span>
          {/if}
        </div>
      {/if}
      {#if showCost}
        <div class="text-[10px] font-mono font-bold text-emerald-600 dark:text-emerald-400 mt-0.5">
          {t('pem_purchase_cost')}: {(purchaseCost ?? item.purchase_price ?? item.unit_price).toLocaleString()} DZD
        </div>
      {/if}
    </div>

    <!-- Total Line Price -->
    <div class="text-end shrink-0">
      <span class="font-mono font-black text-sm {item.is_refund ? 'text-amber-600' : 'text-pos-text'}">
        {item.is_refund ? '-' : ''}{item.total_price.toLocaleString()} DZD
      </span>
    </div>
  </div>

  <!-- Bottom Row: Stepper + Remise + Refund + Delete -->
  <div class="flex items-center justify-between gap-2 pt-1 border-t border-pos-border/40">
    <!-- Stepper (- [Qty] +) -->
    <div class="flex items-center bg-slate-100 dark:bg-slate-800 rounded-xl p-0.5 border border-pos-border/60">
      <button
        type="button"
        on:click={decrement}
        class="w-6 h-6 flex items-center justify-center rounded-lg bg-pos-card hover:bg-slate-200 dark:hover:bg-slate-700 text-pos-text cursor-pointer transition active:scale-90"
      >
        <Minus class="w-3 h-3" />
      </button>

      <input
        bind:this={qtyInputEl}
        type="number"
        min="1"
        value={item.quantity}
        on:input={handleQtyChange}
        on:focus={(e) => (e.target as HTMLInputElement).select()}
        on:keydown={(e) => {
          if (e.key === 'Enter') {
            // PosView's global handler advances to the next cart line.
            e.preventDefault();
          } else if (e.key === 'Escape') {
            stopQtyEdit();
          }
        }}
        class="w-10 text-center bg-transparent border-0 font-mono font-black text-xs text-pos-text outline-none p-0 {isQtyEditTarget ? 'ring-2 ring-sky-400 rounded-md' : ''}"
      />

      <button
        type="button"
        on:click={increment}
        class="w-6 h-6 flex items-center justify-center rounded-lg bg-pos-card hover:bg-slate-200 dark:hover:bg-slate-700 text-pos-text cursor-pointer transition active:scale-90"
      >
        <Plus class="w-3 h-3" />
      </button>
    </div>

    <!-- Weight/qty suggestion chips (scale products only) -->
    {#if showQtyChips && qtySuggestions.length > 0}
      <div class="flex items-center gap-1 flex-wrap -mt-1">
        {#each qtySuggestions as sq}
          <button
            type="button"
            on:click={() => applySuggestion(sq)}
            class="px-1.5 py-0.5 rounded-md bg-sky-50 dark:bg-sky-950/60 border border-sky-200 dark:border-sky-800 text-sky-700 dark:text-sky-300 text-[10px] font-black font-mono cursor-pointer hover:bg-sky-100 dark:hover:bg-sky-900 active:scale-90 transition"
            title={String(sq)}
          >
            {sq % 1 === 0 ? sq : sq.toFixed(2)}
          </button>
        {/each}
      </div>
    {/if}

    <!-- Action Buttons -->
    <div class="flex items-center gap-1">
      <!-- View Cost (eye): reveals the purchase cost inline -->
      <button
        type="button"
        on:click={() => (showCost = !showCost)}
        class="p-1 rounded-lg bg-slate-100 dark:bg-slate-800 text-pos-muted hover:text-sky-600 hover:bg-sky-50 dark:hover:bg-sky-950/40 cursor-pointer transition {showCost ? 'text-sky-600 bg-sky-50 dark:bg-sky-950/40' : ''}"
        title="Purchase cost / سعر الشراء"
      >
        <Eye class="w-3.5 h-3.5" />
      </button>

      <!-- Edit (pen) -->
      <button
        type="button"
        on:click={onEdit}
        class="p-1 rounded-lg bg-slate-100 dark:bg-slate-800 text-pos-muted hover:text-emerald-600 hover:bg-emerald-50 dark:hover:bg-emerald-950/40 cursor-pointer transition"
        title="Edit product / تعديل المنتج"
      >
        <Edit2 class="w-3.5 h-3.5" />
      </button>

      <!-- Item Discount Toggle Button -->
      <button
        type="button"
        on:click={() => (showDiscountInput = !showDiscountInput)}
        class="px-2 py-1 rounded-lg text-[10px] font-bold flex items-center gap-1 transition cursor-pointer {item.discount_amount > 0 ? 'bg-purple-600 text-white' : 'bg-slate-100 dark:bg-slate-800 text-pos-muted hover:text-pos-text'}"
        title={t('pos_item_discount')}
      >
        <Percent class="w-3 h-3" />
        <span>{item.discount_amount > 0 ? '-' + item.discount_amount.toLocaleString() : t('pos_remise')}</span>
      </button>

      <!-- Item Refund Toggle Button -->
      <button
        type="button"
        on:click={toggleRefund}
        class="px-2 py-1 rounded-lg text-[10px] font-bold flex items-center gap-1 transition cursor-pointer {item.is_refund ? 'bg-amber-600 text-white' : 'bg-slate-100 dark:bg-slate-800 text-pos-muted hover:text-pos-text'}"
        title="Item Refund Toggle"
      >
        <Undo2 class="w-3 h-3" />
        <span>{item.is_refund ? t('pos_refund_badge') : t('pos_normal')}</span>
      </button>

      <!-- Delete Item Button -->
      <button
        type="button"
        on:click={() => removeFromCart(item.product_id, item.is_refund)}
        class="p-1 text-rose-500 hover:text-rose-700 hover:bg-rose-50 dark:hover:bg-rose-950/40 rounded-lg cursor-pointer transition"
        title={t('pos_remove_item')}
      >
        <Trash2 class="w-3.5 h-3.5" />
      </button>
    </div>
  </div>

  <!-- Inline Discount Input Popover -->
  {#if showDiscountInput}
    <div class="p-2 bg-slate-50 dark:bg-slate-800/80 rounded-xl border border-pos-border flex items-center gap-2 animate-in fade-in duration-100 flex-wrap">
      <span class="text-[10px] font-bold text-pos-muted">{t('pos_item_discount')} DZD/u:</span>
      <input
        bind:this={discountInputEl}
        use:focusDiscountInput
        type="number"
        bind:value={lineDiscountValue}
        min="0"
        max={maxUnitDiscount}
        step="any"
        on:keydown={(e) => e.key === 'Enter' && applyDiscount()}
        class="w-20 px-2 py-1 bg-white dark:bg-slate-900 border rounded-lg text-xs font-mono font-bold text-pos-text {discountInvalid ? 'border-rose-500' : 'border-pos-border'}"
      />
      <span class="text-[9px] font-bold {discountInvalid ? 'text-rose-600' : 'text-pos-muted'}">
        {discountInvalid ? `Max ${maxUnitDiscount.toLocaleString()} DZD` : `Max ${maxUnitDiscount.toLocaleString()}`}
      </span>
      <button
        type="button"
        on:click={applyDiscount}
        disabled={discountInvalid}
        class="px-2 py-1 bg-purple-600 hover:bg-purple-700 disabled:opacity-40 disabled:cursor-not-allowed text-white text-[10px] font-black rounded-lg cursor-pointer"
      >
        {t('pos_apply_discount')}
      </button>
      {#if item.discount_amount > 0}
        <button
          type="button"
          on:click={clearDiscount}
          class="px-2 py-1 text-[10px] font-bold text-rose-500 hover:text-rose-700 rounded-lg cursor-pointer"
        >
          {t('pos_clear_discount')}
        </button>
      {/if}
      <button
        type="button"
        on:click={() => (showDiscountInput = false)}
        class="text-[10px] text-pos-muted hover:text-pos-text px-1 cursor-pointer"
      >
        ✕
      </button>
    </div>
  {/if}
</div>