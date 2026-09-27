<script lang="ts">
  import { addToCart, isRefundMode } from '../stores/cart';
  import type { Product } from '../types';
  import { X, Tag, Plus, DollarSign } from 'lucide-svelte';

  export let isOpen = false;
  export let onClose: () => void;

  let articleName = '';
  let articlePrice: number | null = null;
  let articleQty = 1;
  let priceInputEl: HTMLInputElement;

  $: if (isOpen && priceInputEl) {
    articleName = '';
    articlePrice = null;
    articleQty = 1;
    setTimeout(() => {
      priceInputEl?.focus();
      priceInputEl?.select();
    }, 50);
  }

  function handleAddCustomArticle() {
    if (!articlePrice || articlePrice <= 0) return;

    const customName = articleName.trim() || `Article Divers (${articlePrice} DZD)`;
    const dummyProduct: Product = {
      id: -Date.now(), // Negative ID for transient/custom item
      sku: 'DIV-' + Date.now().toString().slice(-4),
      barcodes: [],
      name_fr: customName,
      name_ar: customName,
      name_en: customName,
      category_id: undefined,
      purchase_price: Math.round(articlePrice * 0.8),
      sale_price: articlePrice,
      min_sale_price: 0,
      tax_rate: 0,
      is_bundle: false,
      min_stock: 0,
      current_stock: 999,
      unit_name: 'Pièce',
      is_active: true,
      is_scalable: false,
    };

    addToCart(dummyProduct, articleQty || 1, $isRefundMode);
    onClose();
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-sm overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col">
      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-2.5">
          <div class="w-9 h-9 rounded-xl bg-amber-500/10 text-amber-600 flex items-center justify-center font-bold">
            <Tag class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text">Other Product / منتج حر خارج قائمة المبيعات (Article Hors Liste)</h3>
            <p class="text-[11px] text-pos-muted">Quick-sell item without barcode</p>
          </div>
        </div>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1 rounded-lg cursor-pointer">
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Body -->
      <form on:submit|preventDefault={handleAddCustomArticle} class="p-5 space-y-3.5">
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Sale Price (Prix de Vente DZD) *</label>
          <div class="relative">
            <input
              type="number"
              bind:this={priceInputEl}
              bind:value={articlePrice}
              min="1"
              placeholder="e.g. 500"
              required
              class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-2 border-amber-500/40 focus:border-amber-500 rounded-xl text-base font-black font-mono text-pos-text outline-none"
            />
            <span class="absolute end-3 top-2.5 text-xs font-bold text-pos-muted">DZD</span>
          </div>
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Article Name / Description (Optional)</label>
          <input
            type="text"
            bind:value={articleName}
            placeholder="e.g. Pain, Café, Service..."
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none"
          />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Quantity (الكمية)</label>
          <input
            type="number"
            bind:value={articleQty}
            min="1"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none"
          />
        </div>

        <div class="flex items-center justify-end gap-2 pt-2 border-t border-pos-border">
          <button
            type="button"
            on:click={onClose}
            class="px-4 py-2 bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 text-pos-text text-xs font-bold rounded-xl cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={!articlePrice || articlePrice <= 0}
            class="px-5 py-2 bg-amber-600 hover:bg-amber-700 disabled:opacity-40 text-white text-xs font-black rounded-xl cursor-pointer shadow-md flex items-center gap-1.5 active:scale-95"
          >
            <Plus class="w-4 h-4" />
            <span>Add to Cart</span>
          </button>
        </div>
      </form>
    </div>
  </div>
{/if}
