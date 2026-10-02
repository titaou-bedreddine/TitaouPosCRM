<script lang="ts">
  // Presentation picker (pricing refinement §14): a product with configured
  // packaging sale prices opens this card picker instead of adding base
  // units directly. The cashier picks a presentation and sees the real
  // package total — never a manual calculation:
  //   Unité            200 DZD / unit        → 200 DZD
  //   Palette PL112    112 units @ 190 DZD/u → 21 280 DZD / palette
  // The cart prices packaging lines from the packaging row (per-unit ×
  // contains is stored derived as sale_price), so the picked card IS the
  // price that lands on the receipt (historical, per sold unit).
  import { X, Package } from 'lucide-svelte';
  import { t } from '../i18n';

  export let isOpen = false;
  export let product: any = null;
  /** Cached per-product packaging loader (PosView owns the cache). */
  export let loadPackagings: (productId: number) => Promise<any[]> = async () => [];
  export let onPick: (packaging: any | null) => void = () => {};
  export let onClose: () => void = () => {};

  let packs: any[] = [];
  let isLoading = false;

  $: if (isOpen && product) {
    load(product.id);
  }

  async function load(id: number) {
    isLoading = true;
    packs = await loadPackagings(id);
    isLoading = false;
  }

  function money(v: number): string {
    return (Number.isFinite(v) ? v : 0).toLocaleString('fr-DZ');
  }

  /** Per-unit price of a packaging row (authoritative per-unit, or derived
   * from the legacy per-package total for rows saved by older versions). */
  function perUnitOf(pk: any): number {
    if ((pk.sale_price_per_unit ?? 0) > 0) return pk.sale_price_per_unit;
    const units = Number(pk.units_per_package) || 1;
    return Math.round(Number(pk.sale_price ?? 0) / units);
  }

  function packageTotal(pk: any): number {
    return Math.round(perUnitOf(pk) * (Number(pk.units_per_package) || 0));
  }
</script>

{#if isOpen && product}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <!-- |self: only a click on the BACKDROP closes — without it a mousedown on
       a card closed the modal before onPick ran and nothing was added to
       the cart (field-reported bug). -->
  <div class="fixed inset-0 z-[60] bg-black/50 backdrop-blur-[2px] flex items-center justify-center p-4" on:mousedown|self={onClose}>
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-lg max-h-[85vh] flex flex-col overflow-hidden animate-in zoom-in-95 duration-150" role="dialog" tabindex="-1">
      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-3.5 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60 shrink-0">
        <div class="flex items-center gap-2.5">
          <div class="w-8 h-8 rounded-xl bg-sky-600/10 text-sky-600 flex items-center justify-center">
            <Package class="w-4 h-4" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text truncate max-w-[340px]">{product.name_fr || product.name_ar || product.name_en}</h3>
            <p class="text-[10px] text-pos-muted font-bold">{t('pos_presentation_title')}</p>
          </div>
        </div>
        <button type="button" on:click={onClose} class="p-1.5 rounded-lg bg-slate-100 dark:bg-slate-800 text-pos-muted hover:text-pos-text cursor-pointer transition">
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Presentation cards -->
      <div class="p-4 overflow-y-auto grid grid-cols-1 sm:grid-cols-2 gap-2.5 flex-1">
        <!-- Base unit card (always present — Unité cannot be removed) -->
        <button
          type="button"
          on:click={() => onPick(null)}
          class="text-start p-3 rounded-2xl border-2 border-sky-500/70 bg-sky-50/60 dark:bg-sky-950/30 hover:border-sky-600 hover:shadow-md transition cursor-pointer"
        >
          <div class="flex items-center justify-between mb-1">
            <span class="text-xs font-black text-pos-text">{t('sales_unit')}</span>
            <span class="text-[10px] font-bold text-pos-muted">1 = 1</span>
          </div>
          <p class="text-lg font-black font-mono text-sky-600 dark:text-sky-400">{money(product.sale_price)} DZD</p>
        </button>

        {#if isLoading}
          <div class="p-3 rounded-2xl border border-pos-border text-[10px] text-pos-muted font-bold flex items-center justify-center">…</div>
        {/if}

        {#each packs as pk (pk.id)}
          <button
            type="button"
            on:click={() => onPick(pk)}
            class="text-start p-3 rounded-2xl border-2 border-emerald-500/60 bg-emerald-50/50 dark:bg-emerald-950/25 hover:border-emerald-600 hover:shadow-md transition cursor-pointer"
          >
            <div class="flex items-center justify-between mb-1">
              <span class="text-xs font-black text-pos-text">{pk.name}</span>
              <span class="text-[10px] font-bold text-pos-muted">{pk.units_per_package} {t('pem_units')}</span>
            </div>
            <p class="text-[11px] font-bold text-emerald-700 dark:text-emerald-400 font-mono">{money(perUnitOf(pk))} DZD / {t('sales_unit')}</p>
            <p class="text-lg font-black font-mono text-pos-text">{money(packageTotal(pk))} <span class="text-[10px] font-bold text-pos-muted">DZD / {pk.name}</span></p>
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}
