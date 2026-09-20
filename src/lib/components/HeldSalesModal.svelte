<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import {
    cartItems, heldSalesList, refreshHeldSales, clearCart, holdCurrentSale,
    globalDiscountMode, globalDiscountValue, computeCartDiscount, parseHeldCart, posMode,
  } from '../stores/cart';
  import { currentUser } from '../stores/auth';
  import type { HeldSale } from '../types';
  import { PauseCircle, Play, Trash2, X, ShoppingBag, Clock, Percent } from 'lucide-svelte';

  export let isOpen = false;
  export let onClose: () => void;

  let holdNote = '';
  let isSaving = false;

  $: if (isOpen) {
    refreshHeldSales();
  }

  function getSaleDetails(sale: HeldSale): { total: number; count: number; preview: string; discount: number } {
    try {
      const { items, discountMode, discountValue } = parseHeldCart(sale.cart_json);
      // Raw parse again for the mode field (parseHeldCart doesn't return it).
      let heldMode: string | undefined;
      try {
        const raw = JSON.parse(sale.cart_json);
        heldMode = (raw && raw.mode) as string | undefined;
      } catch {
        heldMode = undefined;
      }
      const subtotal = items.reduce((sum, i) => {
        const val = i.total_price || (i.quantity * (i.unit_price - (i.discount_amount || 0)));
        return i.is_refund ? sum - val : sum + val;
      }, 0);
      const discount = computeCartDiscount(subtotal, discountMode, discountValue);
      const count = items.reduce((c, i) => c + i.quantity, 0);
      const preview = items.slice(0, 2).map(i => `${i.name_fr || i.name_ar} (x${i.quantity})`).join(', ') + (items.length > 2 ? '...' : '');
      return { total: subtotal - discount, count, preview, discount };
    } catch {
      return { total: 0, count: 0, preview: '', discount: 0 };
    }
  }

  async function handleHoldCurrentCart() {
    if (!$currentUser || $cartItems.length === 0) return;
    try {
      isSaving = true;
      await holdCurrentSale(holdNote || undefined);
      holdNote = '';
      await refreshHeldSales();
      onClose();
    } catch (e) {
      console.error(e);
    } finally {
      isSaving = false;
    }
  }

  async function handleResume(sale: HeldSale) {
    try {
      // Restore items AND the saved cart-level remise; a fresh sale (nothing held)
      // starts with no discount because resume/hold flows reset the stores.
      const { items, discountMode, discountValue } = parseHeldCart(sale.cart_json);
      // Raw parse for the mode field (parseHeldCart doesn't return it).
      let heldMode: string | undefined;
      try {
        const raw = JSON.parse(sale.cart_json);
        heldMode = (raw && raw.mode) as string | undefined;
      } catch {
        heldMode = undefined;
      }
      if ($cartItems.length > 0) {
        await holdCurrentSale();
      }
      // items came from parseHeldCart — same-key lines already merged.
      $cartItems = items;
      $globalDiscountMode = discountMode;
      $globalDiscountValue = discountValue;

      // A cart held from a specific mode returns in that mode. Priority:
      // the JSON's own `mode` field (held since v0.5.9), else the legacy
      // [SALE MODE] note tag, else sale.
      if (heldMode === 'purchase' || heldMode === 'broken' || heldMode === 'sale') {
        posMode.set(heldMode);
      } else {
        const note = (sale.note || '').toUpperCase();
        if (note.includes('[PURCHASE MODE]')) {
          posMode.set('purchase');
        } else if (note.includes('[BROKEN MODE]')) {
          posMode.set('broken');
        } else {
          posMode.set('sale');
        }
      }

      await handleDelete(sale.id);
      onClose();
    } catch (e) {
      console.error(e);
    }
  }

  async function handleDelete(id: number) {
    try {
      await invoke('delete_held_sale', { heldId: id });
      await refreshHeldSales();
    } catch (e) {
      console.error(e);
    }
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-lg overflow-hidden animate-in fade-in duration-150 flex flex-col max-h-[85vh]">
      <div class="flex items-center justify-between px-5 py-3.5 border-b border-pos-border bg-slate-50 dark:bg-slate-800/50">
        <h3 class="font-extrabold text-base text-pos-text flex items-center gap-2">
          <PauseCircle class="w-5 h-5 text-indigo-500" />
          <span>Held Sales / المبيعات المعلقة ({$heldSalesList.length})</span>
        </h3>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1 rounded-lg cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="p-5 space-y-4 overflow-y-auto flex-1">
        {#if $cartItems.length > 0}
          <div class="p-3.5 bg-indigo-50 dark:bg-indigo-950/50 border border-indigo-200 dark:border-indigo-800 rounded-xl space-y-2">
            <span class="text-xs font-bold text-indigo-900 dark:text-indigo-200">Hold Active Cart ({$cartItems.length} items):</span>
            <div class="flex gap-2">
              <input
                type="text"
                bind:value={holdNote}
                placeholder="Optional customer name or table note..."
                class="flex-1 px-3 py-2 bg-pos-card border border-pos-border rounded-xl text-xs text-pos-text outline-none focus:ring-2 focus:ring-indigo-500"
              />
              <button
                type="button"
                on:click={handleHoldCurrentCart}
                disabled={isSaving}
                class="px-4 py-2 bg-indigo-600 hover:bg-indigo-700 text-white font-bold text-xs rounded-xl transition flex items-center gap-1 cursor-pointer shadow-sm"
              >
                <span>Hold Now</span>
              </button>
            </div>
          </div>
        {/if}

        <!-- List of Held Sales -->
        <div class="space-y-2.5">
          {#if $heldSalesList.length === 0}
            <div class="p-10 text-center text-pos-muted text-xs flex flex-col items-center justify-center gap-2">
              <ShoppingBag class="w-8 h-8 opacity-40" />
              <span>No held sales currently in queue / لا توجد مبيعات معلقة حالياً</span>
            </div>
          {:else}
            {#each $heldSalesList as sale}
              {@const details = getSaleDetails(sale)}
              <div class="p-3.5 bg-pos-card border border-pos-border rounded-xl hover:border-indigo-400 transition shadow-xs flex items-center justify-between gap-3">
                <div class="min-w-0 flex-1 space-y-1">
                  <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-sm font-black text-emerald-600 dark:text-emerald-400 font-mono">
                      {details.total.toLocaleString()} DZD
                    </span>
                    <span class="text-[10px] font-bold bg-slate-100 dark:bg-slate-800 text-pos-muted px-2 py-0.5 rounded-full font-mono">
                      {details.count} items
                    </span>
                    {#if details.discount > 0}
                      <span class="text-[10px] font-bold bg-purple-100 dark:bg-purple-950 text-purple-700 dark:text-purple-300 px-2 py-0.5 rounded-full flex items-center gap-0.5">
                        <Percent class="w-2.5 h-2.5" />
                        Remise -{details.discount.toLocaleString()} DZD
                      </span>
                    {/if}
                    <span class="text-[10px] text-pos-muted font-mono flex items-center gap-1">
                      <Clock class="w-3 h-3" />
                      {sale.created_at}
                    </span>
                  </div>

                  {#if sale.note}
                    <p class="text-xs font-semibold text-pos-text truncate">
                      {sale.note}
                    </p>
                  {:else if details.preview}
                    <p class="text-[11px] text-pos-muted truncate font-mono">
                      {details.preview}
                    </p>
                  {/if}
                </div>

                <div class="flex items-center gap-2 shrink-0">
                  <button
                    type="button"
                    on:click={() => handleResume(sale)}
                    class="px-3.5 py-2 bg-emerald-600 hover:bg-emerald-700 text-white text-xs font-black rounded-xl flex items-center gap-1.5 cursor-pointer shadow-sm active:scale-95 transition"
                  >
                    <Play class="w-3.5 h-3.5 fill-current" />
                    <span>Resume / استرجاع</span>
                  </button>
                  <button
                    type="button"
                    on:click={() => handleDelete(sale.id)}
                    class="p-2 text-rose-500 hover:bg-rose-50 dark:hover:bg-rose-950/60 rounded-xl cursor-pointer transition"
                    title="Delete Held Sale"
                  >
                    <Trash2 class="w-4 h-4" />
                  </button>
                </div>
              </div>
            {/each}
          {/if}
        </div>
      </div>

      <div class="px-5 py-3 border-t border-pos-border bg-slate-50 dark:bg-slate-800/50 flex justify-end">
        <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
          Close / إغلاق
        </button>
      </div>
    </div>
  </div>
{/if}