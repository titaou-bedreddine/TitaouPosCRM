<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { cartItems, cartGrandTotal, cartSubtotal, globalDiscountAmount } from '../stores/cart';
  import { currentUser } from '../stores/auth';
  import { networkStatus } from '../stores/network';
  import { entityQrDataUrl, printHtmlSilently, type SilentPrintResult } from '../utils/printer';
  import { buildUnifiedReceipt } from '../printing/unifiedReceipt';
  import { Printer, X, Loader2, CheckCircle2, AlertTriangle } from 'lucide-svelte';

  export let isOpen = false;
  export let onClose: () => void;
  // Optional live-sale context (defaults keep the standalone preview working).
  export let saleNumber = '';
  export let paymentMethod = 'ESPÈCES';
  export let customerName = '';

  let settings: Record<string, string> = {};
  let proQrDataUrl = '';
  let isPrinting = false;
  let printResult: SilentPrintResult | null = null;

  $: effectiveInvoiceNumber = saleNumber || `TCK-${Math.floor(Date.now() / 1000).toString().slice(-6)}`;

  // ONE unified receipt preset system (v0.5.17): the modal preview and the
  // printed job are the exact same HTML built from the same settings.
  $: if (isOpen) {
    printResult = null;
    invoke<Record<string, string>>('get_all_settings')
      .then((fetched) => (settings = { ...settings, ...fetched }))
      .catch(() => {});
    entityQrDataUrl(`SALE:${effectiveInvoiceNumber}`, 240)
      .then((url) => (proQrDataUrl = url))
      .catch(() => (proQrDataUrl = ''));
  }

  $: built = buildUnifiedReceipt({
    saleNumber: effectiveInvoiceNumber,
    saleDate: new Date().toLocaleString('fr-FR'),
    cashierName: $currentUser?.display_name || 'Admin',
    terminalName: $networkStatus?.pc_name || undefined,
    customerName: customerName || settings.default_customer_name || '',
    paymentMethod,
    items: $cartItems.map((i) => ({
      name: i.name_fr || i.name_ar || 'Article',
      quantity: i.quantity,
      unitPrice: i.unit_price,
      totalPrice: i.total_price,
      discountPerUnit: i.discount_amount || 0,
      isRefund: i.is_refund || false,
    })),
    subtotal: $cartSubtotal,
    discount: $globalDiscountAmount,
    grandTotal: $cartGrandTotal,
    amountPaid: $cartGrandTotal,
    change: 0,
    settings,
    qrDataUrl: proQrDataUrl || undefined,
  });

  async function triggerPrint() {
    isPrinting = true;
    printResult = null;
    printResult = await printHtmlSilently(built.html, built.title, {
      widthMm: built.paperWidthMm,
    });
    isPrinting = false;
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-md overflow-hidden animate-in fade-in duration-150">
      <!-- Modal Header -->
      <div class="flex items-center justify-between px-5 py-3.5 border-b border-pos-border bg-slate-50 dark:bg-slate-800/50">
        <h3 class="font-black text-sm text-pos-text flex items-center gap-2">
          <Printer class="w-4 h-4 text-sky-500" />
          <span>Receipt Preview (معاينة الوصل)</span>
        </h3>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1 rounded-lg cursor-pointer">
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Printable Thermal Receipt Preview: the SAME unified builder that prints -->
      <div class="p-6 bg-slate-100 dark:bg-slate-900/60 max-h-[65vh] overflow-y-auto flex justify-center">
        <div class="bg-white shadow-md select-text">
          {@html built.html}
        </div>
      </div>

      {#if printResult}
        <div class="mx-5 mt-3 rounded-xl border p-2.5 text-[11px] font-bold flex items-center gap-2 {printResult.ok ? 'border-emerald-300 bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300' : 'border-rose-300 bg-rose-50 dark:bg-rose-950/40 text-rose-700 dark:text-rose-300'}">
          {#if printResult.ok}
            <CheckCircle2 class="w-4 h-4 shrink-0" />
            <span>Printed silently via the Windows print API.</span>
          {:else}
            <AlertTriangle class="w-4 h-4 shrink-0" />
            <span>Print failed: {printResult.message}</span>
          {/if}
        </div>
      {/if}

      <!-- Action Footer -->
      <div class="px-5 py-3.5 border-t border-pos-border bg-slate-50 dark:bg-slate-800/50 flex justify-end gap-2">
        <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
          Close
        </button>
        <button
          type="button"
          on:click={triggerPrint}
          disabled={isPrinting}
          class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-md transition active:scale-95 disabled:opacity-50"
        >
          {#if isPrinting}
            <Loader2 class="w-4 h-4 animate-spin" />
            <span>Printing…</span>
          {:else}
            <Printer class="w-4 h-4" />
            <span>Print Receipt (طباعة الوصل)</span>
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}
