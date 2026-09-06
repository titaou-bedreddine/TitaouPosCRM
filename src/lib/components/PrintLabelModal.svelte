<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { Product } from '../types';
  import { printLabelSilently, type LabelPrintOutcome } from '../utils/printer';
  import {
    LABEL_PRESETS,
    LABEL_PRESET_IDS,
    buildLabelPresetHtml,
    toLabelCurrency,
    type LabelPresetId,
  } from '../printing/labelPresets';
  import { Printer, QrCode, X, RefreshCw, ZoomIn, ZoomOut, Loader2, CheckCircle2, AlertTriangle } from 'lucide-svelte';

  export let isOpen = false;
  export let product: Product | null = null;
  export let onClose: () => void;
  export let initialType: 'barcode' | 'etiquette' = 'barcode';
  export let initialQty: number = 1;

  let labelType: 'barcode' | 'etiquette' = 'barcode';
  let copies = 1;
  // ONE unified preset system: the built-in mm-true thermal presets. The
  // legacy px-based 'custom' sticker/shelf modes are removed (v0.5.17).
  let presetId: LabelPresetId = 'vprice40x20';
  let zoom = 1;
  // Print job state (exact-media silent pipeline).
  let isPrinting = false;
  let printOutcome: LabelPrintOutcome | null = null;

  // Settings loaded from DB (shop name, currency, printer, DPI, current preset)
  let settings: Record<string, string> = {};
  let settingsLoaded = false;

  $: shopName = settings.shop_name_fr || 'TitaouPOS';
  $: barcode = product?.barcodes?.[0] || product?.sku || '';

  // Default preset resolved WITHOUT a reactive statement (the old $:if
  // created a widthMm → labelType → settings → labelType cycle): computed
  // when the modal opens, from the settings already loaded on mount.
  function resolveDefaultPreset(): LabelPresetId {
    const configured = settings.label_preset_id;
    if (configured === 'shelf40x20') return 'shelf40x20';
    return 'vprice40x20';
  }

  $: if (isOpen) {
    labelType = initialType;
    copies = initialQty;
    // Shelf etiquette opened as such prefers the shelf preset; otherwise
    // the configured default from Settings wins.
    presetId = initialType === 'etiquette' ? 'shelf40x20' : resolveDefaultPreset();
    zoom = 1;
  }

  // ---- Built-in thermal preset (mm-true) ----
  $: presetDef = LABEL_PRESETS[presetId];
  $: presetData = {
    shopName: settings.shop_name_fr || 'TITAOU POS',
    productName: product?.name_fr || product?.name_ar || '',
    barcode: product?.barcodes?.[0] || product?.sku || '',
    price: product?.sale_price ?? 0,
    currency: toLabelCurrency(settings.default_currency),
  };
  $: presetHtml = presetDef && product ? presetDef.build(presetData) : '';

  onMount(async () => {
    try {
      const fetched = await invoke<Record<string, string>>('get_all_settings');
      settings = { ...settings, ...fetched };
      settingsLoaded = true;
    } catch (e) {
      settingsLoaded = true;
    }
  });

  // Re-fetch settings every time the modal opens so recent changes made in
  // Settings (font sizes, bold, alignment...) are reflected without remount.
  $: if (isOpen) {
    invoke<Record<string, string>>('get_all_settings')
      .then((fetched) => {
        settings = { ...settings, ...fetched };
        settingsLoaded = true;
      })
      .catch(() => {});
  }

  async function triggerPrint() {
    isPrinting = true;
    printOutcome = null;
    try {
      printOutcome = await printLabelSilently({
        html: presetHtml,
        label: presetDef.name,
        widthMm: presetDef.widthMm,
        heightMm: presetDef.heightMm,
        copies,
        printer: settings.label_printer || undefined,
        dpi: parseInt(settings.label_printer_dpi || '203', 10) || 203,
      });
    } catch (e: any) {
      printOutcome = {
        ok: false,
        message: typeof e === 'string' ? e : e?.message || String(e),
        diagnostics: null,
      } as any;
    } finally {
      isPrinting = false;
    }
  }
</script>

{#if isOpen && product}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-lg overflow-hidden animate-in fade-in duration-150">

      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-3.5 border-b border-pos-border bg-slate-50 dark:bg-slate-800/50">
        <h3 class="font-black text-sm text-pos-text flex items-center gap-2">
          <QrCode class="w-4 h-4 text-sky-500" />
          <span>Print Label / طباعة الملصق</span>
        </h3>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1 rounded-lg cursor-pointer">
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="p-5 space-y-4">
        <!-- Print Preset Tabs (ONE unified preset system) -->
        <div>
          <label class="block text-[11px] font-bold text-pos-muted mb-1.5">Preset / نوع الملصق</label>
          <div class="grid grid-cols-2 gap-2">
            <button
              type="button"
              on:click={() => { presetId = 'vprice40x20'; }}
              class="px-2.5 py-2 rounded-xl border text-xs font-bold transition-all cursor-pointer flex flex-col items-center justify-center text-center gap-0.5 {presetId === 'vprice40x20' ? 'border-sky-500 bg-sky-50 dark:bg-sky-950/60 text-sky-600 dark:text-sky-400 shadow-xs' : 'border-pos-border bg-slate-50 dark:bg-slate-800/40 text-pos-muted hover:text-pos-text'}"
            >
              <span class="leading-tight">Vertical Price</span>
              <span class="text-[10px] opacity-75 font-mono">40×20 mm</span>
            </button>
            <button
              type="button"
              on:click={() => { presetId = 'shelf40x20'; }}
              class="px-2.5 py-2 rounded-xl border text-xs font-bold transition-all cursor-pointer flex flex-col items-center justify-center text-center gap-0.5 {presetId === 'shelf40x20' ? 'border-emerald-500 bg-emerald-50 dark:bg-emerald-950/60 text-emerald-600 dark:text-emerald-400 shadow-xs' : 'border-pos-border bg-slate-50 dark:bg-slate-800/40 text-pos-muted hover:text-pos-text'}"
            >
              <span class="leading-tight">Shelf Price</span>
              <span class="text-[10px] opacity-75 font-mono">40×20 mm</span>
            </button>
          </div>
        </div>

        {#if presetDef}
          <!-- Exact-media preview: every copy stacked consecutively, exactly
               as the print job emits them (N × 20mm, zero gaps). -->
          <div class="space-y-2">
            <div class="flex items-center justify-between">
              <span class="text-[11px] font-bold text-pos-muted">
                Print preview — {copies} × {presetDef.widthMm}×{presetDef.heightMm}mm ({copies * presetDef.heightMm}mm total)
              </span>
              <div class="flex items-center gap-1">
                <button
                  type="button"
                  on:click={() => (zoom = Math.max(1, zoom - 1))}
                  class="p-1.5 rounded-lg bg-slate-100 dark:bg-slate-800 text-pos-text cursor-pointer"
                  title="Zoom out"
                >
                  <ZoomOut class="w-3.5 h-3.5" />
                </button>
                <span class="text-[11px] font-black font-mono text-pos-text w-8 text-center">{zoom}×</span>
                <button
                  type="button"
                  on:click={() => (zoom = Math.min(8, zoom + 1))}
                  class="p-1.5 rounded-lg bg-slate-100 dark:bg-slate-800 text-pos-text cursor-pointer"
                  title="Zoom in"
                >
                  <ZoomIn class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>
            <div class="bg-white border-2 border-dashed border-slate-300 rounded-xl p-3 overflow-auto flex flex-col items-center gap-[2px] max-h-64">
              {#each Array(Math.min(copies, 12)) as _, i}
                <div
                  dir="ltr"
                  class="label-strip-item"
                  style="width: calc({presetDef.widthMm}mm * {zoom}); height: calc({presetDef.heightMm}mm * {zoom}); flex: 0 0 auto;"
                >
                  <div class="label-strip-inner" style="width: {presetDef.widthMm}mm; height: {presetDef.heightMm}mm; transform: scale({zoom}); transform-origin: top left;">
                    {@html presetHtml}
                  </div>
                </div>
              {/each}
              {#if copies > 12}
                <span class="text-[10px] text-pos-muted font-bold py-1">+ {copies - 12} more…</span>
              {/if}
            </div>
            <p class="text-[10px] text-pos-muted text-center">
              Physical Size: <span dir="ltr" class="font-bold">{presetDef.widthMm} × {presetDef.heightMm} mm</span> •
              Orientation: <span class="font-bold">Landscape</span> •
              Media locked per label — no gaps, {copies} page(s)
            </p>
            <style>
              .label-strip-item { position: relative; overflow: hidden; }
              .label-strip-inner { position: absolute; top: 0; left: 0; }
            </style>
          </div>

          <!-- Print job diagnostics (exact-media pipeline) -->
          {#if printOutcome}
            <div class="rounded-xl border p-2.5 text-[10px] font-mono {printOutcome.ok ? 'border-emerald-300 bg-emerald-50 dark:bg-emerald-950/40 text-emerald-800 dark:text-emerald-300' : 'border-amber-300 bg-amber-50 dark:bg-amber-950/40 text-amber-800 dark:text-amber-300'}">
              <div class="flex items-start gap-1.5">
                {#if printOutcome.ok}
                  <CheckCircle2 class="w-3.5 h-3.5 shrink-0 mt-0.5" />
                {:else}
                  <AlertTriangle class="w-3.5 h-3.5 shrink-0 mt-0.5" />
                {/if}
                <div class="space-y-0.5 leading-relaxed">
                  <div class="font-bold">{printOutcome.message}</div>
                  <div>Printer: {printOutcome.diagnostics.printer} • DPI: {printOutcome.diagnostics.dpi}</div>
                  <div>Media: {printOutcome.diagnostics.media_width_mm}×{printOutcome.diagnostics.media_height_mm}mm • Pages: {printOutcome.diagnostics.page_count} • Raster: {printOutcome.diagnostics.raster_width_px}×{printOutcome.diagnostics.raster_height_px}px</div>
                  <div>Generated print: {printOutcome.diagnostics.print_width_mm}mm × {printOutcome.diagnostics.print_height_mm}mm total</div>
                </div>
              </div>
            </div>
          {/if}
        {/if}

        <!-- Print Parameters: Copies + Dimensions (from the selected preset) -->
        <div class="grid grid-cols-3 gap-2">
          <div>
            <label class="block text-[11px] font-bold text-pos-muted mb-1">Copies (Labels)</label>
            <input
              type="number"
              bind:value={copies}
              min="1"
              class="w-full px-2 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-bold font-mono outline-none"
            />
          </div>
          <div>
            <label class="block text-[11px] font-bold text-pos-muted mb-1">Width (mm)</label>
            <div class="w-full px-2 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-bold font-mono text-center opacity-70">
              {presetDef.widthMm}mm
            </div>
          </div>
          <div>
            <label class="block text-[11px] font-bold text-pos-muted mb-1">Height (mm)</label>
            <div class="w-full px-2 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-bold font-mono text-center opacity-70">
              {presetDef.heightMm}mm
            </div>
          </div>
        </div>

        {#if !settingsLoaded}
          <p class="text-[11px] text-pos-muted flex items-center gap-1">
            <RefreshCw class="w-3 h-3 animate-spin" /> Loading label settings...
          </p>
        {:else}
          <p class="text-[10px] text-pos-muted">
            Built-in preset with fixed mm geometry — real barcode, auto-fit text, dynamic product data.
          </p>
        {/if}
      </div>

      <!-- Action Footer -->
      <div class="px-5 py-3.5 border-t border-pos-border bg-slate-50 dark:bg-slate-800/50 flex justify-end gap-2">
        <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
          Cancel
        </button>
        <button
          type="button"
          on:click={triggerPrint}
          disabled={isPrinting}
          class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-md transition active:scale-95 disabled:opacity-50 disabled:cursor-wait"
        >
          {#if isPrinting}
            <Loader2 class="w-4 h-4 animate-spin" />
            <span>Printing…</span>
          {:else}
            <Printer class="w-4 h-4" />
            <span>Print {copies}× Label(s)</span>
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}