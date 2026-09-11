<script lang="ts">
  import { t } from '../i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { cartItems, cartSubtotal, cartGrandTotal, globalDiscountAmount, globalDiscountPercent, selectedCustomerId, clearCart } from '../stores/cart';
  import { currentUser } from '../stores/auth';
  import { activeSession } from '../stores/session';
  import { Banknote, CreditCard, UserCheck, X, Check } from 'lucide-svelte';

  export let isOpen = false;
  export let onClose: () => void;
  export let onSaleSuccess: (saleNumber: string) => void;

  let paymentMethod: 'cash' | 'tpe' | 'credit' = 'cash';
  let tenderedAmount = 0;
  let tpeReference = '';
  let isSubmitting = false;
  let errorMsg = '';

  // TVA (TTC pricing): the VAT lives INSIDE the grand total. Extracted for
  // the ledger; the client pays the TTC total either way. Default from
  // Settings, editable right here, 0 = exempt.
  let tvaRate = 19;
  let tvaEditable = false;
  onMount(async () => {
    try {
      const st = await invoke<Record<string, string>>('get_all_settings');
      const r = parseFloat(st?.default_tva_sale ?? '19');
      if (!isNaN(r)) tvaRate = r;
    } catch {}
    tvaEditable = true;
  });
  $: tvaAmount = Math.round((total * Math.max(0, tvaRate)) / (100 + Math.max(0, tvaRate)));
  $: total = $cartGrandTotal;
  $: if (isOpen && tenderedAmount === 0 && paymentMethod === 'cash') {
    tenderedAmount = total;
  }
  $: change = Math.max(0, tenderedAmount - total);

  function setTender(amount: number) {
    tenderedAmount = amount;
  }

  function addTender(amount: number) {
    tenderedAmount += amount;
  }

  async function handleCheckout() {
    if (!$activeSession) {
      errorMsg = 'Please open a Cash Register session first before completing sales.';
      return;
    }
    if (!$currentUser) {
      errorMsg = 'User session expired. Please log in.';
      return;
    }
    if (paymentMethod === 'cash' && tenderedAmount < total) {
      errorMsg = 'Tendered amount is less than grand total.';
      return;
    }

    try {
      isSubmitting = true;
      errorMsg = '';

      const payments = [
        {
          payment_method: paymentMethod,
          amount: paymentMethod === 'cash' ? total : total,
          reference_code: paymentMethod === 'tpe' ? tpeReference : null,
        }
      ];

      const saleNumber = await invoke<string>('process_sale', {
        input: {
          session_id: $activeSession.id,
          user_id: $currentUser.id,
          customer_id: $selectedCustomerId,
          items: $cartItems,
          subtotal: total - tvaAmount,
          discount_amount: $globalDiscountAmount,
          discount_percentage: $globalDiscountPercent,
          discount_reason: null,
          tax_amount: tvaAmount,
          total_amount: total,
          paid_amount: paymentMethod === 'cash' ? tenderedAmount : total,
          change_amount: change,
          payments,
          notes: null,
        }
      });

      clearCart();
      onSaleSuccess(saleNumber);
      onClose();
    } catch (err: any) {
      errorMsg = typeof err === 'string' ? err : err.message || 'Checkout failed';
    } finally {
      isSubmitting = false;
    }
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-xl shadow-2xl w-full max-w-lg overflow-hidden animate-in fade-in zoom-in-95 duration-150">
      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-3.5 border-b border-pos-border bg-slate-50 dark:bg-slate-800/50">
        <h3 class="font-extrabold text-base text-pos-text flex items-center gap-2">
          <CreditCard class="w-5 h-5 text-sky-500" />
          <span>{t('complete_sale')}</span>
        </h3>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1 rounded">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Body -->
      <div class="p-5 space-y-4">
        {#if errorMsg}
          <div class="p-3 bg-rose-100 dark:bg-rose-950/60 border border-rose-300 dark:border-rose-800 text-rose-700 dark:text-rose-300 text-xs font-bold rounded">
            {errorMsg}
          </div>
        {/if}

        <!-- Total Display -->
        <div class="bg-slate-100 dark:bg-slate-800 rounded-lg p-3 text-center border border-pos-border">
          <div class="flex items-center justify-between mb-1">
            <span class="text-[10px] font-semibold text-pos-muted">TVA %</span>
            <input type="number" min="0" max="100" bind:value={tvaRate} disabled={!tvaEditable}
              class="w-16 px-2 py-0.5 text-center bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-[11px] font-black font-mono text-pos-text outline-none" />
          </div>
          {#if tvaRate > 0}
            <div class="flex items-center justify-between mb-1">
              <span class="text-[10px] font-semibold text-pos-muted">dont TVA / VAT</span>
              <span class="text-[10px] font-mono font-bold text-pos-muted">{tvaAmount.toLocaleString()} DZD</span>
            </div>
          {/if}
          <span class="text-xs font-semibold text-pos-muted">{t('grand_total')}</span>
          {#if $globalDiscountAmount > 0}
            <div class="text-xs font-bold text-purple-600 dark:text-purple-400 flex items-center justify-center gap-1">
              Remise: -{$globalDiscountAmount.toLocaleString()} DZD
              <span class="text-pos-muted font-mono">({$cartSubtotal.toLocaleString()} DZD)</span>
            </div>
          {/if}
          <div class="text-3xl font-black text-sky-600 dark:text-sky-400 font-mono mt-0.5">
            {total.toLocaleString()} DZD
          </div>
        </div>

        <!-- Payment Method Tabs -->
        <div class="grid grid-cols-3 gap-2">
          <button
            type="button"
            on:click={() => { paymentMethod = 'cash'; tenderedAmount = total; }}
            class="flex flex-col items-center justify-center gap-1.5 p-3 rounded-lg border font-bold text-xs transition cursor-pointer {paymentMethod === 'cash' ? 'border-sky-500 bg-sky-50 dark:bg-sky-950/40 text-sky-600 dark:text-sky-400 shadow-xs' : 'border-pos-border text-pos-muted hover:border-pos-muted'}"
          >
            <Banknote class="w-5 h-5" />
            <span>{t('pay_cash')}</span>
          </button>

          <button
            type="button"
            on:click={() => { paymentMethod = 'tpe'; tenderedAmount = total; }}
            class="flex flex-col items-center justify-center gap-1.5 p-3 rounded-lg border font-bold text-xs transition cursor-pointer {paymentMethod === 'tpe' ? 'border-sky-500 bg-sky-50 dark:bg-sky-950/40 text-sky-600 dark:text-sky-400 shadow-xs' : 'border-pos-border text-pos-muted hover:border-pos-muted'}"
          >
            <CreditCard class="w-5 h-5" />
            <span>{t('pay_tpe')}</span>
          </button>

          <button
            type="button"
            on:click={() => { paymentMethod = 'credit'; tenderedAmount = 0; }}
            class="flex flex-col items-center justify-center gap-1.5 p-3 rounded-lg border font-bold text-xs transition cursor-pointer {paymentMethod === 'credit' ? 'border-sky-500 bg-sky-50 dark:bg-sky-950/40 text-sky-600 dark:text-sky-400 shadow-xs' : 'border-pos-border text-pos-muted hover:border-pos-muted'}"
          >
            <UserCheck class="w-5 h-5" />
            <span>{t('pay_credit')}</span>
          </button>
        </div>

        <!-- Cash Details -->
        {#if paymentMethod === 'cash'}
          <div class="space-y-3">
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">{t('received_amount')}</label>
              <input
                type="number"
                bind:value={tenderedAmount}
                class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded text-xl font-bold font-mono text-pos-text focus:ring-2 focus:ring-sky-500 outline-none"
              />
            </div>

            <!-- Quick Cash Preset Buttons -->
            <div class="grid grid-cols-4 gap-1.5">
              <button type="button" on:click={() => setTender(total)} class="py-1.5 px-2 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 text-xs font-bold font-mono rounded">Exact</button>
              <button type="button" on:click={() => addTender(500)} class="py-1.5 px-2 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 text-xs font-bold font-mono rounded">+500</button>
              <button type="button" on:click={() => addTender(1000)} class="py-1.5 px-2 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 text-xs font-bold font-mono rounded">+1000</button>
              <button type="button" on:click={() => addTender(2000)} class="py-1.5 px-2 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 text-xs font-bold font-mono rounded">+2000</button>
            </div>

            <!-- Change Return Calculation -->
            <div class="flex items-center justify-between p-3 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-300 dark:border-emerald-800 rounded-lg">
              <span class="text-sm font-bold text-emerald-800 dark:text-emerald-300">{t('change_amount')}</span>
              <span class="text-2xl font-black font-mono text-emerald-600 dark:text-emerald-400">
                {change.toLocaleString()} DZD
              </span>
            </div>
          </div>
        {:else if paymentMethod === 'tpe'}
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">TPE Transaction Authorization / Reference Code</label>
            <input
              type="text"
              bind:value={tpeReference}
              placeholder="e.g. AUTH-98234"
              class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded text-sm font-mono text-pos-text focus:ring-2 focus:ring-sky-500 outline-none"
            />
          </div>
        {:else}
          <div class="p-3 bg-amber-50 dark:bg-amber-950/40 border border-amber-300 dark:border-amber-800 rounded text-xs text-amber-800 dark:text-amber-300">
            Recorded as Customer Debt (آجل). Customer balance ledger will be updated automatically.
          </div>
        {/if}
      </div>

      <!-- Footer Buttons -->
      <div class="px-5 py-3.5 border-t border-pos-border bg-slate-50 dark:bg-slate-800/50 flex justify-end gap-2">
        <button
          type="button"
          on:click={onClose}
          class="px-4 py-2 bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 text-pos-text font-bold text-sm rounded transition"
        >
          Cancel
        </button>
        <button
          type="button"
          on:click={handleCheckout}
          disabled={isSubmitting}
          class="px-6 py-2 bg-emerald-600 hover:bg-emerald-700 disabled:opacity-50 text-white font-extrabold text-sm rounded transition shadow-md flex items-center gap-1.5 cursor-pointer"
        >
          {#if isSubmitting}
            <span class="animate-spin">⌛</span>
          {:else}
            <Check class="w-4 h-4" />
          {/if}
          <span>{t('complete_sale')}</span>
        </button>
      </div>
    </div>
  </div>
{/if}