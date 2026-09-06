<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { printHtmlSilently, entityQrDataUrl } from '../utils/printer';
  import { buildUnifiedReceipt } from '../printing/unifiedReceipt';
  import type { Sale } from '../types';
  import { Printer, X, RefreshCw, RotateCcw, Loader2, FileText, User, Clock, Banknote, Package, Pencil } from 'lucide-svelte';

  export let isOpen = false;
  export let onClose: () => void;
  // Reopen-in-POS hands the sale to the existing "edit in POS" flow.
  export let onEditInPos: (sale: Sale) => void = () => {};

  interface LastSalePayload {
    sale: Sale;
    items: any[];
  }

  let payload: LastSalePayload | null = null;
  let isLoading = false;
  let error = '';
  let isReprinting = false;
  let printMsg = '';

  $: if (isOpen) {
    error = '';
    printMsg = '';
    loadLastSale();
  }

  async function loadLastSale() {
    isLoading = true;
    payload = null;
    try {
      const res = await invoke<LastSalePayload | null>('get_last_sale');
      payload = res;
      if (!res) error = 'No sale has been recorded yet / لا توجد عمليات بيع بعد';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed to load last sale';
    } finally {
      isLoading = false;
    }
  }

  async function reprint() {
    if (!payload) return;
    isReprinting = true;
    printMsg = '';
    try {
      const settings = await invoke<Record<string, string>>('get_all_settings');
      const s = payload.sale;
      // Same receipt number as the original — a reprint never creates a new
      // sale row or receipt number.
      const qr = await entityQrDataUrl(`SALE:${s.sale_number}`, 240).catch(() => undefined);
      const built = buildUnifiedReceipt({
        saleNumber: s.sale_number,
        saleDate: s.created_at,
        cashierName: s.user_name || 'Admin',
        customerName: s.customer_name || undefined,
        items: (payload.items || []).map((it: any) => ({
          name: it.name_fr || it.name_ar || 'Article',
          quantity: it.quantity,
          unitPrice: it.unit_price,
          totalPrice: it.total_price,
          discountPerUnit: it.discount_amount || 0,
          isRefund: !!it.is_refund,
        })),
        subtotal: s.subtotal ?? s.total_amount,
        discount: s.discount_amount ?? 0,
        grandTotal: s.total_amount,
        amountPaid: s.paid_amount,
        change: s.change_amount,
        paymentMethod: (s.payment_method || 'cash').toUpperCase(),
        settings,
        qrDataUrl: qr,
        copyLabel: 'REPRINT / نسخة',
      });
      const result = await printHtmlSilently(built.html, built.title, { widthMm: built.paperWidthMm });
      printMsg = result.ok
        ? '✅ Reprint sent to printer / تمت إعادة الطباعة'
        : '❌ ' + result.message;
    } catch (e: any) {
      printMsg = '❌ ' + (typeof e === 'string' ? e : e?.message || 'Reprint failed');
    } finally {
      isReprinting = false;
    }
  }

  function reopenInPos() {
    if (!payload) return;
    onEditInPos(payload.sale);
    onClose();
  }

  function t_lines() { return 'lines'; }
  function t_units() { return 'units'; }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-md overflow-hidden animate-in fade-in zoom-in-95 duration-150">
      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-3.5 border-b border-pos-border bg-slate-50 dark:bg-slate-800/50">
        <h3 class="font-black text-sm text-pos-text flex items-center gap-2">
          <RotateCcw class="w-4 h-4 text-sky-500" />
          <span>Reopen Last Receipt / آخر وصل</span>
        </h3>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1 rounded-lg cursor-pointer">
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Body -->
      <div class="p-5 space-y-4 max-h-[70vh] overflow-y-auto">
        {#if isLoading}
          <p class="text-xs text-pos-muted flex items-center gap-2 py-6 justify-center">
            <Loader2 class="w-4 h-4 animate-spin" /> Loading last receipt…
          </p>
        {:else if error}
          <p class="text-xs text-pos-muted text-center py-6">{error}</p>
        {:else if payload}
          {@const s = payload.sale}
          <!-- Receipt summary card -->
          <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-2.5">
            <div class="flex items-center justify-between">
              <span class="font-mono font-black text-sky-600 text-sm">#{s.sale_number}</span>
              <span class="px-2 py-0.5 rounded-full text-[10px] font-black uppercase bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300">
                {s.payment_status}
              </span>
            </div>
            <div class="grid grid-cols-2 gap-2 text-[11px] font-bold text-pos-muted">
              <span class="flex items-center gap-1.5"><Clock class="w-3.5 h-3.5" /> {s.created_at}</span>
              <span class="flex items-center gap-1.5"><User class="w-3.5 h-3.5" /> {s.user_name || 'Admin'}</span>
              <span class="flex items-center gap-1.5"><FileText class="w-3.5 h-3.5" /> {s.customer_name || 'Client Comptoir'}</span>
              <span class="flex items-center gap-1.5"><Banknote class="w-3.5 h-3.5" /> {(s.payment_method || 'cash').toUpperCase()}</span>
              <span class="flex items-center gap-1.5"><Package class="w-3.5 h-3.5" /> {payload.items.length} {t_lines()} · {s.units_sold} {t_units()}</span>
            </div>
            <div class="pt-2 border-t border-pos-border flex items-center justify-between">
              <span class="text-[11px] font-black text-pos-muted uppercase">TOTAL</span>
              <span class="font-mono font-black text-base text-emerald-600">{s.total_amount.toLocaleString()} DZD</span>
            </div>
          </div>

          <!-- Items list -->
          <div class="rounded-2xl border border-pos-border overflow-hidden">
            <table class="w-full text-start text-xs">
              <thead class="bg-slate-50 dark:bg-slate-800/60 text-pos-muted font-bold">
                <tr>
                  <th class="p-2.5 text-start">Article</th>
                  <th class="p-2.5 text-center">Qté</th>
                  <th class="p-2.5 text-end">Total</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-pos-border/40">
                {#each payload.items as it}
                  <tr>
                    <td class="p-2.5 font-bold text-pos-text">
                      {it.name_fr || it.name_ar}
                      {#if it.is_refund}<span class="text-[8px] font-black text-rose-600">[RETOUR]</span>{/if}
                    </td>
                    <td class="p-2.5 text-center font-mono">{it.quantity}</td>
                    <td class="p-2.5 text-end font-mono font-bold">{it.total_price.toLocaleString()}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>

          {#if printMsg}
            <p class="text-[11px] font-bold rounded-lg p-2 {printMsg.startsWith('✅') ? 'text-emerald-600 bg-emerald-50 dark:bg-emerald-950/40' : 'text-rose-600 bg-rose-50 dark:bg-rose-950/40'}">{printMsg}</p>
          {/if}
        {/if}
      </div>

      <!-- Footer -->
      <div class="px-5 py-3.5 border-t border-pos-border bg-slate-50 dark:bg-slate-800/50 flex items-center justify-between gap-2">
        <button
          type="button"
          on:click={loadLastSale}
          disabled={isLoading}
          class="p-2 rounded-xl bg-slate-200 dark:bg-slate-700 text-pos-text cursor-pointer"
          title="Refresh"
        >
          <RefreshCw class="w-4 h-4 {isLoading ? 'animate-spin' : ''}" />
        </button>
        <div class="flex gap-2">
          <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
            Close
          </button>
          {#if payload}
            <button
              type="button"
              on:click={reopenInPos}
              class="px-4 py-2 bg-amber-100 hover:bg-amber-200 dark:bg-amber-950/60 dark:hover:bg-amber-900 text-amber-800 dark:text-amber-300 text-xs font-black rounded-xl cursor-pointer flex items-center gap-1.5"
              title="Reopen this sale in the POS cart for editing (updates in place, no duplicate)"
            >
              <Pencil class="w-4 h-4" />
              <span>Edit in POS</span>
            </button>
            <button
              type="button"
              on:click={reprint}
              disabled={isReprinting}
              class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-md active:scale-95 disabled:opacity-50"
            >
              {#if isReprinting}
                <Loader2 class="w-4 h-4 animate-spin" /> Printing…
              {:else}
                <Printer class="w-4 h-4" /> Reprint
              {/if}
            </button>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}
