<script lang="ts">
  import type { Product } from '../types';
  import { t, tf, currentLocale } from '../i18n';
  import { addToCart, isRefundMode } from '../stores/cart';
  import PrintLabelModal from './PrintLabelModal.svelte';
  import { Package, Plus, Edit2, AlertTriangle, AlertOctagon, QrCode, Tag, Pin, PinOff, ChevronUp, ChevronDown } from 'lucide-svelte';
  import { invoke } from '@tauri-apps/api/core';

  export let product: Product;
  export let categoryColor: string = '#0284c7';
  export let onEditProduct: ((p: Product) => void) | undefined = undefined;
  // Re-fetch the catalog after a pin toggle so the new ordering applies.
  export let onPinned: (() => void) | undefined = undefined;
  // For rearranging pinned products: the full pinned list in display order.
  export let pinnedIds: number[] = [];

  // Move this pinned product one slot up/down among the pinned group.
  async function movePinned(e: MouseEvent, dir: -1 | 1) {
    e.stopPropagation();
    const ids = [...pinnedIds];
    const idx = ids.indexOf(product.id);
    const swapWith = idx + dir;
    if (idx < 0 || swapWith < 0 || swapWith >= ids.length) return;
    [ids[idx], ids[swapWith]] = [ids[swapWith], ids[idx]];
    try {
      await invoke('reorder_pinned_products', { orderedIds: ids });
      onPinned?.();
    } catch (err) {
      console.warn('Reorder failed:', err);
    }
  }

  async function togglePin(e: MouseEvent) {
    e.stopPropagation();
    try {
      await invoke('toggle_product_pin', { productId: product.id, pinned: !product.pinned });
      onPinned?.();
    } catch (err) {
      console.warn('Pin toggle failed:', err);
    }
  }

  let isClicked = false;
  let isPrintLabelOpen = false;
  let printLabelType: 'barcode' | 'etiquette' = 'barcode';
  let printLabelQty = 1;

  // Sticker prints the stock quantity (or 1 when out of stock); shelf tag
  // always prints one.
  function openPrintSticker(e: MouseEvent) {
    e.stopPropagation();
    printLabelType = 'barcode';
    printLabelQty = Math.max(1, Math.floor(product.current_stock || 0));
    isPrintLabelOpen = true;
  }

  function openPrintShelf(e: MouseEvent) {
    e.stopPropagation();
    printLabelType = 'etiquette';
    printLabelQty = 1;
    isPrintLabelOpen = true;
  }

  // In POS purchase mode the host asks for the new supplier cost before
  // the line is added; otherwise the card adds at shelf price directly.
  export let onAddToCart: ((p: Product) => void) | undefined = undefined;

  function handleClick() {
    isClicked = true;
    setTimeout(() => (isClicked = false), 250);
    if (onAddToCart) {
      onAddToCart(product);
      return;
    }
    addToCart(product, 1, $isRefundMode);
  }

  function handleEditClick(e: MouseEvent) {
    e.stopPropagation();
    if (onEditProduct) {
      onEditProduct(product);
    }
  }

  $: displayName =
    $currentLocale === 'ar'
      ? product.name_ar || product.name_fr
      : $currentLocale === 'fr'
      ? product.name_fr || product.name_en
      : product.name_en || product.name_fr || product.name_ar;

  // Clean localized unit
  $: displayUnit = (() => {
    const raw = (product.unit_name || '').toLowerCase();
    if (raw.includes('kg') || raw.includes('kilo') || raw.includes('كغ')) {
      return $currentLocale === 'ar' ? 'كيلوغرام' : $currentLocale === 'fr' ? 'kg' : 'kg';
    }
    if (raw.includes('l') || raw.includes('litre') || raw.includes('لتر')) {
      return $currentLocale === 'ar' ? 'لتر' : $currentLocale === 'fr' ? 'Litre' : 'Liter';
    }
    if (raw.includes('pack') || raw.includes('paquet') || raw.includes('علبة')) {
      return $currentLocale === 'ar' ? 'علبة' : $currentLocale === 'fr' ? 'Paquet' : 'Pack';
    }
    if (raw.includes('box') || raw.includes('carton') || raw.includes('كرتون')) {
      return $currentLocale === 'ar' ? 'كرتون' : $currentLocale === 'fr' ? 'Carton' : 'Box';
    }
    return $currentLocale === 'ar' ? 'قطعة' : $currentLocale === 'fr' ? 'Pièce' : 'Piece';
  })();

  // Expiry check
  $: expiryStatus = (() => {
    if (!product.expiry_date) return 'none';
    const exp = new Date(product.expiry_date).getTime();
    if (isNaN(exp)) return 'none';
    const now = new Date().getTime();
    const diffDays = Math.ceil((exp - now) / (1000 * 60 * 60 * 24));
    if (diffDays <= 0) return 'expired';
    if (diffDays <= 30) return 'near';
    return 'valid';
  })();
</script>

<div
  role="button"
  tabindex="0"
  on:click={handleClick}
  on:keydown={(e) => { if (e.key === 'Enter') handleClick(); }}
  class="flex flex-col text-start bg-pos-card border {product.pinned ? 'border-amber-400 ring-1 ring-amber-400/60' : 'border-pos-border hover:border-slate-400 dark:hover:border-slate-600'} rounded-2xl p-2.5 transition-all duration-150 shadow-xs hover:shadow-md cursor-pointer group relative overflow-hidden focus:outline-none focus:ring-2 focus:ring-sky-500 active:scale-95 {isClicked ? 'ring-2 ring-emerald-500 bg-emerald-50/40 dark:bg-emerald-950/30' : ''} {expiryStatus === 'expired' ? 'bg-rose-500/10' : expiryStatus === 'near' ? 'bg-amber-500/5' : ''}"
>
  <!-- Edit Pen + Pin (Top Start) -->
  <div class="absolute top-2 start-2 z-20 flex items-center gap-1">
    <button
      type="button"
      on:click={handleEditClick}
      class="w-6 h-6 rounded-lg bg-white/90 dark:bg-slate-800/90 text-pos-muted hover:text-sky-600 hover:scale-110 shadow-xs flex items-center justify-center cursor-pointer transition"
      title="Edit Product Details"
    >
      <Edit2 class="w-3.5 h-3.5" />
    </button>
    <button
      type="button"
      on:click={togglePin}
      class="w-6 h-6 rounded-lg shadow-xs flex items-center justify-center cursor-pointer transition hover:scale-110 {product.pinned ? 'bg-amber-500 text-white' : 'bg-white/90 dark:bg-slate-800/90 text-pos-muted hover:text-amber-500'}"
      title={product.pinned ? 'Unpin (إلغاء التثبيت)' : 'Pin to top (تثبيت في الأعلى)'}
    >
      {#if product.pinned}
        <PinOff class="w-3.5 h-3.5" />
      {:else}
        <Pin class="w-3.5 h-3.5" />
      {/if}
    </button>
    {#if product.pinned}
      <button
        type="button"
        on:click={(e) => movePinned(e, -1)}
        class="w-6 h-6 rounded-lg bg-amber-100 dark:bg-amber-950/60 text-amber-600 hover:scale-110 shadow-xs flex items-center justify-center cursor-pointer transition opacity-0 group-hover:opacity-100 focus:opacity-100"
        title="Move pinned up (تقديم)"
      >
        <ChevronUp class="w-3.5 h-3.5" />
      </button>
      <button
        type="button"
        on:click={(e) => movePinned(e, 1)}
        class="w-6 h-6 rounded-lg bg-amber-100 dark:bg-amber-950/60 text-amber-600 hover:scale-110 shadow-xs flex items-center justify-center cursor-pointer transition opacity-0 group-hover:opacity-100 focus:opacity-100"
        title="Move pinned down (تأخير)"
      >
        <ChevronDown class="w-3.5 h-3.5" />
      </button>
    {/if}
  </div>

  <!-- Stock & Expiry Status Badges (Top End) -->
  <div class="absolute top-2 end-2 z-10 flex flex-col items-end gap-1">
    <!-- Expiry Pill -->
    {#if expiryStatus === 'expired'}
      <span class="inline-flex items-center gap-0.5 text-[9px] font-black px-1.5 py-0.2 rounded-full bg-rose-600 text-white font-mono shadow-xs animate-pulse">
        <AlertOctagon class="w-2.5 h-2.5" />
        EXPIRED
      </span>
    {:else if expiryStatus === 'near'}
      <span class="inline-flex items-center gap-0.5 text-[9px] font-black px-1.5 py-0.2 rounded-full bg-amber-500 text-white font-mono shadow-xs">
        <AlertTriangle class="w-2.5 h-2.5" />
        NEAR EXP
      </span>
    {/if}

    <!-- Stock Status Pill -->
    {#if product.current_stock > product.min_stock}
      <span class="inline-flex items-center gap-1 text-[10px] font-black px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300">
        <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
        {product.current_stock}
      </span>
    {:else if product.current_stock > 0}
      <span class="inline-flex items-center gap-1 text-[10px] font-black px-2 py-0.5 rounded-full bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300">
        <span class="w-1.5 h-1.5 rounded-full bg-amber-500"></span>
        {product.current_stock}
      </span>
    {:else}
      <span class="inline-flex items-center gap-1 text-[10px] font-black px-2 py-0.5 rounded-full bg-rose-100 text-rose-800 dark:bg-rose-950 dark:text-rose-300">
        <span class="w-1.5 h-1.5 rounded-full bg-rose-500"></span>
        {t('stock_out')}
      </span>
    {/if}
  </div>

  <!-- Product Image or Placeholder -->
  <div class="w-full h-22 bg-slate-100 dark:bg-slate-800/80 rounded-xl flex items-center justify-center mb-2 overflow-hidden relative">
    {#if product.image_path}
      <img src={product.image_path} alt={displayName} class="w-full h-full object-cover group-hover:scale-105 transition duration-200" />
    {:else}
      <Package class="w-7 h-7 text-pos-muted/40 group-hover:text-sky-500 transition duration-200" />
    {/if}

    <!-- Click Overlay Animation -->
    {#if isClicked}
      <div class="absolute inset-0 bg-emerald-500/20 backdrop-blur-[1px] flex items-center justify-center animate-in zoom-in-50 duration-150">
        <div class="w-7 h-7 rounded-full bg-emerald-500 text-white flex items-center justify-center font-black text-xs shadow-md">
          +1
        </div>
      </div>
    {/if}
  </div>

  <!-- Product Title & Primary Barcode -->
  <div class="flex-1 min-h-[38px]">
    <h3 class="font-bold text-xs text-pos-text line-clamp-2 leading-tight group-hover:text-sky-600 transition">
      {displayName}
    </h3>
    {#if product.barcodes && product.barcodes.length > 0}
      <span class="text-[10px] text-pos-muted font-mono block mt-0.5">{product.barcodes[0]}</span>
    {/if}
  </div>

  <!-- Price Footer -->
  <div class="mt-2 pt-1 border-t border-pos-border/60 flex items-center justify-between w-full">
    <span class="text-[10px] text-pos-muted font-bold">{displayUnit}</span>
    <div class="flex items-center gap-1.5">
      <button
        type="button"
        on:click={openPrintSticker}
        class="w-6 h-6 rounded-lg bg-white/90 dark:bg-slate-800/90 text-sky-600 hover:scale-110 shadow-xs flex items-center justify-center cursor-pointer transition"
        title={tf('product_print_barcode_hint', { n: String(Math.max(1, Math.floor(product.current_stock || 0))) })}
      >
        <QrCode class="w-3.5 h-3.5" />
      </button>
      <button
        type="button"
        on:click={openPrintShelf}
        class="w-6 h-6 rounded-lg bg-white/90 dark:bg-slate-800/90 text-emerald-600 hover:scale-110 shadow-xs flex items-center justify-center cursor-pointer transition"
        title={t('product_print_shelf_hint')}
      >
        <Tag class="w-3.5 h-3.5" />
      </button>
      <span class="text-sm font-black text-sky-600 dark:text-sky-400 font-mono">
        {product.sale_price.toLocaleString()} DZD
      </span>
    </div>
  </div>
</div>
<!-- Label Print Modal (sticker = stock qty, shelf = 1) -->
<PrintLabelModal
  isOpen={isPrintLabelOpen}
  product={product}
  initialType={printLabelType}
  initialQty={printLabelQty}
  onClose={() => (isPrintLabelOpen = false)}
/>
