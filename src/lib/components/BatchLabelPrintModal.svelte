<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { Product } from '../types';
  import { LABEL_PRESETS, buildLabelPresetHtml, toLabelCurrency, type LabelPresetId } from '../printing/labelPresets';
  import { printLabelSilently } from '../utils/printer';
  import { t } from '../i18n';
  import { Printer, X, ScanLine, Trash2, Loader2, CheckCircle2, AlertTriangle } from 'lucide-svelte';

  export let isOpen = false;
  export let onClose: () => void;
  export let products: Product[] = [];
  export let settings: Record<string, string> = {};

  // Queue of products to label (each row = product + copies).
  interface QueueRow {
    product: Product;
    copies: number;
  }
  let queue: QueueRow[] = [];
  let scanInput = '';
  let scanInputEl: HTMLInputElement | null = null;
  let presetId: LabelPresetId = 'vprice40x20';
  let isPrinting = false;
  let outcomeMsg = '';
  let outcomeOk = false;
  let presets: LabelPresetId[] = ['vprice40x20', 'shelf40x20'];

  $: shopName = settings.shop_name_fr || 'TITAOU POS';

  // Live preview of BOTH presets: uses the last queued product (or a demo
  // row) so the operator sees exactly what will print before printing.
  $: previewProduct = queue.length > 0
    ? queue[queue.length - 1].product
    : ({ name_fr: 'Produit Démo / Demo', name_ar: 'منتج تجريبي', barcodes: ['6130000000018'], sale_price: 120 } as any);
  $: previewHtmls = {
    vprice40x20: buildLabelPresetHtml('vprice40x20', {
      shopName,
      productName: previewProduct.name_fr || previewProduct.name_ar || '',
      barcode: previewProduct.barcodes?.[0] || previewProduct.sku || '',
      price: previewProduct.sale_price ?? 0,
      currency: toLabelCurrency(settings.default_currency),
    }),
    shelf40x20: buildLabelPresetHtml('shelf40x20', {
      shopName,
      productName: previewProduct.name_fr || previewProduct.name_ar || '',
      barcode: previewProduct.barcodes?.[0] || previewProduct.sku || '',
      price: previewProduct.sale_price ?? 0,
      currency: toLabelCurrency(settings.default_currency),
    }),
  };

  // Respect the configured default preset (Settings → Barcode Labels).
  $: if (isOpen && settings.label_preset_id === 'shelf40x20') {
    presetId = 'shelf40x20';
  }

  async function handleScan(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    const code = scanInput.trim();
    if (!code) return;
    scanInput = '';

    // Find by barcode or SKU; unknown scans are ignored with a shake hint.
    const found = products.find(
      (p) => p.barcodes?.includes(code) || p.sku === code || String(p.id) === code
    );
    if (!found) {
      outcomeMsg = `Not found: ${code}`;
      outcomeOk = false;
      return;
    }
    const existing = queue.find((r) => r.product.id === found.id);
    if (existing) {
      existing.copies += 1;
    } else {
      queue = [...queue, { product: found, copies: 1 }];
    }
    outcomeMsg = '';
    await tick();
    scanInputEl?.focus();
  }

  function setCopies(row: QueueRow, n: number) {
    row.copies = Math.max(1, Math.min(999, n));
    queue = [...queue];
  }

  function removeRow(row: QueueRow) {
    queue = queue.filter((r) => r.product.id !== row.product.id);
  }

  $: totalLabels = queue.reduce((sum, r) => sum + r.copies, 0);

  async function printQueue() {
    if (queue.length === 0 || isPrinting) return;
    isPrinting = true;
    outcomeMsg = '';
    try {
      // One silent job per queued row (product): N pages of one label each.
      let allOk = true;
      let lastMsg = '';
      for (const row of queue) {
        const html = buildLabelPresetHtml(presetId, {
          shopName,
          productName: row.product.name_fr || row.product.name_ar || '',
          barcode: row.product.barcodes?.[0] || row.product.sku || '',
          price: row.product.sale_price ?? 0,
          currency: toLabelCurrency(settings.default_currency),
        });
        const res = await printLabelSilently({
          html,
          label: `Batch ${presetId} — ${row.product.name_fr || row.product.name_ar}`,
          widthMm: 40,
          heightMm: 20,
          copies: row.copies,
          printer: settings.label_printer || undefined,
          dpi: parseInt(settings.label_printer_dpi || '203', 10) || 203,
        });
        allOk = allOk && res.ok;
        lastMsg = res.message;
      }
      outcomeOk = allOk;
      outcomeMsg = allOk
        ? `Printed ${totalLabels} label(s) (${queue.length} product(s)) — one 40×20mm page each, no gaps`
        : lastMsg;
      // Keep the queue after printing so labels can be re-printed or
      // adjusted; a Clear button empties it explicitly.
    } catch (e: any) {
      // The native pipeline is the only path — surface the real error.
      outcomeMsg = 'Print failed: ' + (typeof e === 'string' ? e : e?.message || String(e));
      outcomeOk = false;
    } finally {
      isPrinting = false;
    }
  }

  $: if (isOpen) {
    scanInput = '';
    outcomeMsg = '';
    setTimeout(() => scanInputEl?.focus(), 100);
  }

  function clearQueue() {
    queue = [];
    outcomeMsg = '';
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-xl overflow-hidden animate-in fade-in duration-150">
      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-3.5 border-b border-pos-border bg-slate-50 dark:bg-slate-800/50">
        <h3 class="font-black text-sm text-pos-text flex items-center gap-2">
          <ScanLine class="w-4 h-4 text-amber-500" />
          <span>Batch Label Printing (طباعة ملصقات متعددة)</span>
        </h3>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1 rounded-lg cursor-pointer">
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="p-5 space-y-4">
        <!-- Preset picker -->
        <div class="grid grid-cols-2 gap-2">
          <button
            type="button"
            on:click={() => (presetId = 'vprice40x20')}
            class="p-2.5 rounded-xl border font-bold text-xs transition cursor-pointer {presetId === 'vprice40x20' ? 'border-sky-500 bg-sky-50 dark:bg-sky-950 text-sky-600' : 'border-pos-border text-pos-muted'}"
          >
            {t('label_preset_vprice')}
          </button>
          <button
            type="button"
            on:click={() => (presetId = 'shelf40x20')}
            class="p-2.5 rounded-xl border font-bold text-xs transition cursor-pointer {presetId === 'shelf40x20' ? 'border-emerald-500 bg-emerald-50 dark:bg-emerald-950 text-emerald-600' : 'border-pos-border text-pos-muted'}"
          >
            {t('label_preset_shelf')}
          </button>
        </div>

        <!-- Live previews: both presets, first queued product -->
        <div class="grid grid-cols-2 gap-2">
          {#each Object.keys(previewHtmls) as pid}
            <div class="space-y-1">
              <div dir="ltr" class="bg-white border border-slate-300 rounded-lg p-2 flex justify-center overflow-hidden">
                <div style="width: calc(40mm * 1.6); height: calc(20mm * 1.6); position: relative; overflow: hidden;">
                  <div style="width: 40mm; height: 20mm; transform: scale(1.6); transform-origin: top left;">
                    {@html previewHtmls[pid as LabelPresetId]}
                  </div>
                </div>
              </div>
              <p class="text-[9px] text-pos-muted text-center font-bold">
                {t(pid === 'vprice40x20' ? 'label_preset_vprice' : 'label_preset_shelf')}
                {#if presetId === pid}<span class="text-sky-600"> ●</span>{/if}
              </p>
            </div>
          {/each}
        </div>

        <!-- Scan input -->
        <div>
          <label class="block text-[11px] font-bold text-pos-muted mb-1">
            Scan products to queue (امسح المنتجات لإضافتها)
          </label>
          <input
            type="text"
            bind:this={scanInputEl}
            bind:value={scanInput}
            on:keydown={handleScan}
            placeholder="Scan barcode / SKU…"
            class="w-full px-4 py-3 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-amber-500"
          />
        </div>

        <!-- Queue -->
        <div class="max-h-56 overflow-y-auto rounded-xl border border-pos-border divide-y divide-pos-border">
          {#if queue.length === 0}
            <p class="p-4 text-center text-xs text-pos-muted">
              Queue is empty — scanned products appear here with their copy counts.
            </p>
          {:else}
            {#each queue as row (row.product.id)}
              <div class="flex items-center gap-3 p-2.5">
                <div class="flex-1 min-w-0">
                  <p class="text-xs font-black text-pos-text truncate">{row.product.name_fr || row.product.name_ar}</p>
                  <p class="text-[10px] font-mono text-pos-muted truncate">{row.product.barcodes?.[0] || row.product.sku || '—'}</p>
                </div>
                <div class="flex items-center gap-1">
                  <button on:click={() => setCopies(row, row.copies - 1)} class="w-6 h-6 rounded-lg bg-slate-100 dark:bg-slate-800 font-black text-pos-text cursor-pointer">−</button>
                  <input
                    type="number"
                    min="1"
                    max="999"
                    value={row.copies}
                    on:change={(e) => setCopies(row, parseInt((e.target as HTMLInputElement).value) || 1)}
                    class="w-14 px-1 py-0.5 text-center bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-xs font-mono font-bold text-pos-text outline-none"
                  />
                  <button on:click={() => setCopies(row, row.copies + 1)} class="w-6 h-6 rounded-lg bg-slate-100 dark:bg-slate-800 font-black text-pos-text cursor-pointer">+</button>
                </div>
                <button on:click={() => removeRow(row)} class="p-1.5 text-rose-500 hover:bg-rose-50 dark:hover:bg-rose-950/40 rounded-lg cursor-pointer" title="Remove">
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </div>
            {/each}
          {/if}
        </div>

        {#if outcomeMsg}
          <div class="rounded-xl border p-2.5 text-[11px] font-mono {outcomeOk ? 'border-emerald-300 bg-emerald-50 dark:bg-emerald-950/40 text-emerald-800 dark:text-emerald-300' : 'border-amber-300 bg-amber-50 dark:bg-amber-950/40 text-amber-800 dark:text-amber-300'}">
            {outcomeMsg}
          </div>
        {/if}
      </div>

      <!-- Footer -->
      <div class="px-5 py-3.5 border-t border-pos-border bg-slate-50 dark:bg-slate-800/50 flex items-center justify-between gap-3">
        <span class="text-[11px] font-bold text-pos-muted">
          {queue.length} product(s) • {totalLabels} label(s) total
        </span>
        <div class="flex gap-2">
          <button
            type="button"
            on:click={clearQueue}
            disabled={queue.length === 0}
            class="px-4 py-2 bg-rose-100 hover:bg-rose-200 dark:bg-rose-950/60 disabled:opacity-40 dark:bg-rose-950/60 text-rose-700 dark:text-rose-300 text-xs font-bold rounded-xl cursor-pointer flex items-center gap-1.5"
          >
            <Trash2 class="w-3.5 h-3.5" /> Clear
          </button>
          <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
          <button
            type="button"
            on:click={printQueue}
            disabled={queue.length === 0 || isPrinting}
            class="px-5 py-2 bg-amber-600 hover:bg-amber-700 disabled:opacity-40 text-white font-black text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-md transition active:scale-95"
          >
            {#if isPrinting}
              <Loader2 class="w-4 h-4 animate-spin" /> Printing…
            {:else}
              <Printer class="w-4 h-4" /> Print {totalLabels} Label(s)
            {/if}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
