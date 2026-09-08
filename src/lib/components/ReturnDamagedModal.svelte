<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { localTodayISO } from '../utils/date';
  import { currentUser } from '../stores/auth';
  import { t } from '../i18n';
  import type { Product, Supplier } from '../types';
  import { X, Check, AlertTriangle, Package, Calendar } from 'lucide-svelte';

  export let isOpen = false;
  export let suppliers: Supplier[] = [];
  export let onClose: () => void;
  export let onReturnCompleted: () => void;

  let barcodeSearch = '';
  let foundProduct: Product | null = null;
  let returnQty = 1;
  let returnReason = 'damaged';
  let returnDate = localTodayISO();
  let isSaving = false;
  let errorMsg = '';

  async function handleSearch() {
    if (!barcodeSearch.trim()) return;
    try {
      const res = await invoke<Product[]>('search_products', {
        query: barcodeSearch.trim(),
        categoryId: null,
        searchType: 'barcode',
      });
      if (res.length > 0) {
        foundProduct = res[0];
      }
    } catch (e) {
      console.error(e);
    }
  }

  async function handleSaveReturn() {
    if (!foundProduct || returnQty <= 0) {
      errorMsg = 'Please select a product and valid return quantity';
      return;
    }

    try {
      isSaving = true;
      // Deduct from stock
      const newStock = Math.max(0, foundProduct.current_stock - returnQty);
      await invoke('save_product', {
        input: {
          sku: foundProduct.sku,
          name_ar: foundProduct.name_ar,
          name_fr: foundProduct.name_fr,
          name_en: foundProduct.name_en,
          category_id: foundProduct.category_id,
          unit_id: foundProduct.unit_id,
          purchase_price: foundProduct.purchase_price,
          sale_price: foundProduct.sale_price,
          min_sale_price: foundProduct.min_sale_price,
          tax_rate: foundProduct.tax_rate,
          current_stock: newStock,
          min_stock: foundProduct.min_stock,
          image_path: foundProduct.image_path,
          expiry_date: foundProduct.expiry_date,
          is_bundle: foundProduct.is_bundle,
          barcodes: foundProduct.barcodes,
        },
        productId: foundProduct.id,
        userId: $currentUser?.id,
      });

      onReturnCompleted();
      onClose();
    } catch (e: any) {
      errorMsg = typeof e === 'string' ? e : e.message || 'Failed to record return';
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
          <div class="w-10 h-10 rounded-2xl bg-rose-600/10 text-rose-600 flex items-center justify-center font-bold">
            <AlertTriangle class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">Damaged / Return Goods (إرجاع أو إتلاف بضاعة)</h3>
            <p class="text-xs text-pos-muted">Deducts damaged/expired goods from inventory and logs return history</p>
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
          <label class="block text-xs font-bold text-pos-muted mb-1">Scan or Search Barcode</label>
          <div class="flex items-center gap-2">
            <input
              type="text"
              bind:value={barcodeSearch}
              on:keydown={(e) => { if (e.key === 'Enter') handleSearch(); }}
              placeholder="Scan damaged item barcode..."
              class="flex-1 px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none"
            />
            <button on:click={handleSearch} class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-bold rounded-xl cursor-pointer">
              Find
            </button>
          </div>
        </div>

        {#if foundProduct}
          <div class="p-3 bg-slate-100 dark:bg-slate-800 rounded-xl border border-pos-border text-xs space-y-1">
            <p class="font-black text-pos-text">{foundProduct.name_fr || foundProduct.name_ar}</p>
            <p class="text-pos-muted">Current Stock: <strong class="text-emerald-600">{foundProduct.current_stock} pcs</strong></p>
          </div>

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Quantity Returned / Broken</label>
              <input type="number" min="1" max={foundProduct.current_stock} bind:value={returnQty} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-black text-rose-600" />
            </div>

            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Reason / السبب</label>
              <select bind:value={returnReason} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text">
                <option value="damaged">Damaged / تالف مكسور</option>
                <option value="expired">Expired / منتهي الصلاحية</option>
                <option value="lost">Lost / ضائع</option>
                <option value="supplier_return">Return to Supplier / إرجاع للمورد</option>
              </select>
            </div>
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Supplier Return Notice Date (Optional)</label>
            <input type="date" bind:value={returnDate} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text" />
          </div>
        {/if}
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-end gap-3">
        <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Cancel</button>
        <button on:click={handleSaveReturn} disabled={!foundProduct || isSaving} class="px-6 py-2 bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
          <Check class="w-4 h-4" />
          <span>{isSaving ? 'Saving...' : 'Confirm Return / Loss (تأكيد الإتلاف)'}</span>
        </button>
      </div>
    </div>
  </div>
{/if}