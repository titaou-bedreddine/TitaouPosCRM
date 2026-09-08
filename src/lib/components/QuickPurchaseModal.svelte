<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { localTodayISO } from '../utils/date';
  import { t, currentLocale } from '../i18n';
  import type { Supplier, Product, CreatePurchaseInput } from '../types';
  import { normalizeBarcode } from '../utils/barcode';
  import { X, Check, ShoppingBag, Plus, Search, DollarSign } from 'lucide-svelte';

  export let isOpen = false;
  export let suppliers: Supplier[] = [];
  export let onClose: () => void;
  export let onPurchaseCompleted: () => void;

  let selectedSupplierId: number | null = 1;
  let barcodeSearch = '';
  let foundProduct: Product | null = null;

  let purchasePrice = 0;
  let salePrice = 0;
  let quantity = 1;
  let errorMsg = '';
  let isSaving = false;
  let barcodeInputEl: HTMLInputElement;

  $: if (isOpen && barcodeInputEl) {
    setTimeout(() => barcodeInputEl?.focus(), 50);
  }

  $: if (isOpen && suppliers.length > 0 && !selectedSupplierId) {
    selectedSupplierId = suppliers[0].id;
  }

  async function handleSearchProduct() {
    if (!barcodeSearch.trim()) return;
    try {
      const res = await invoke<Product[]>('search_products', {
        query: barcodeSearch.trim(),
        categoryId: null,
        searchType: 'barcode',
      });
      if (res.length > 0) {
        foundProduct = res[0];
        purchasePrice = foundProduct.purchase_price;
        salePrice = foundProduct.sale_price;
      }
    } catch (e) {
      console.error(e);
    }
  }

  async function handleConfirmPurchase() {
    if (!foundProduct || !selectedSupplierId || quantity <= 0) {
      errorMsg = 'Please specify valid product, supplier, and quantity';
      return;
    }

    try {
      isSaving = true;
      errorMsg = '';
      const total = purchasePrice * quantity;
      const input = {
        supplier_id: selectedSupplierId,
        user_id: 1,
        invoice_number: 'ACH-' + Date.now().toString().slice(-6),
        date: localTodayISO(),
        subtotal: total,
        discount: 0,
        tax: 0,
        total: total,
        paid_amount: total,
        payment_method: 'cash',
        notes: 'Quick POS Purchase (شراء سريع من نقطة البيع)',
        items: [
          {
            product_id: foundProduct.id,
            quantity,
            unit_cost: purchasePrice,
            discount: 0,
            tax: 0,
            total: total,
            expiry_date: foundProduct.expiry_date || null,
            batch_number: null,
          },
        ],
      };

      await invoke('create_purchase', { input });
      onPurchaseCompleted();
      onClose();
    } catch (e: any) {
      errorMsg = typeof e === 'string' ? e : e.message || 'Failed to complete quick purchase';
    } finally {
      isSaving = false;
    }
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-lg overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col">
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold">
            <ShoppingBag class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">Quick Purchase / شراء سريع (Bon d'Achat)</h3>
            <p class="text-xs text-pos-muted">Scan or search product to instantly record purchase & replenish stock</p>
          </div>
        </div>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      {#if errorMsg}
        <div class="mx-6 mt-4 p-3 bg-rose-100 text-rose-800 text-xs font-bold rounded-xl">{errorMsg}</div>
      {/if}

      <div class="p-6 space-y-4">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Scan or Type Barcode</label>
            <div class="flex items-center gap-2">
              <input
                type="text"
                bind:this={barcodeInputEl}
                bind:value={barcodeSearch}
                on:keydown={(e) => {
                  if (e.key === 'Enter') {
                    barcodeSearch = normalizeBarcode(barcodeSearch);
                    handleSearchProduct();
                  }
                }}
                placeholder="Scan product barcode..."
                class="flex-1 px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none"
              />
              <button on:click={() => { barcodeSearch = normalizeBarcode(barcodeSearch); handleSearchProduct(); }} class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-bold rounded-xl cursor-pointer">
                Find
              </button>
            </div>
          </div>

        {#if foundProduct}
          <div class="p-3 bg-slate-100 dark:bg-slate-800 rounded-xl border border-pos-border text-xs space-y-1">
            <p class="font-black text-pos-text">{foundProduct.name_fr || foundProduct.name_ar}</p>
            <p class="text-pos-muted">Current Stock: <strong class="text-emerald-600">{foundProduct.current_stock} pcs</strong></p>
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Supplier / المورد</label>
            <select bind:value={selectedSupplierId} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text">
              {#each suppliers as s}
                <option value={s.id}>{s.name}</option>
              {/each}
            </select>
          </div>

          <div class="grid grid-cols-3 gap-3">
            <div>
              <label class="block text-[11px] font-bold text-pos-muted mb-1">Qty Purchased</label>
              <input type="number" min="1" bind:value={quantity} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-black text-pos-text" />
            </div>
            <div>
              <label class="block text-[11px] font-bold text-pos-muted mb-1">Cost Price (DZD)</label>
              <input type="number" min="0" bind:value={purchasePrice} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text" />
            </div>
            <div>
              <label class="block text-[11px] font-bold text-pos-muted mb-1">Sale Price (DZD)</label>
              <input type="number" min="0" bind:value={salePrice} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text" />
            </div>
          </div>
        {/if}
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-end gap-3">
        <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Cancel</button>
        <button on:click={handleConfirmPurchase} disabled={!foundProduct || isSaving} class="px-6 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
          <Check class="w-4 h-4" />
          <span>{isSaving ? 'Saving...' : 'Record Quick Purchase'}</span>
        </button>
      </div>
    </div>
  </div>
{/if}