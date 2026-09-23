<script lang="ts">
  import { t } from '../i18n';
  import { X, Check, Truck } from 'lucide-svelte';

  export let isOpen = false;
  export let productName = '';
  export let quantity = 1;
  // Pre-fill: the product's configured per-unit fee (DZD), or the line's
  // current fee when reopening for an edit.
  export let defaultFee = 0;
  export let isEdit = false;

  export let onConfirm: (feePerUnit: number) => void = () => {};
  export let onSkip: () => void = () => {};
  export let onClose: () => void = () => {};

  let feeInput: number = 0;

  // Re-seed from the default each time the modal opens.
  $: if (isOpen) {
    feeInput = defaultFee;
  }

  $: feeValue = Math.max(0, Math.round(Number(feeInput) || 0));
  $: feeTotal = Math.round(feeValue * (Number(quantity) || 0));

  function confirm() {
    onConfirm(Math.max(0, Math.round(Number(feeInput) || 0)));
  }

  function skip() {
    onSkip();
  }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      confirm();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }

  function autofocusSelect(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-[70] bg-black/50 backdrop-blur-[2px] flex items-center justify-center p-4" on:mousedown|self={onClose}>
    <div
      class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-[360px] p-5 space-y-4"
      on:keydown={handleKey}
      role="dialog"
      tabindex="-1"
    >
      <div class="flex items-center justify-between">
        <div class="flex items-center gap-2">
          <div class="w-8 h-8 rounded-xl bg-amber-100 dark:bg-amber-950/60 flex items-center justify-center">
            <Truck class="w-4 h-4 text-amber-600" />
          </div>
          <h3 class="font-black text-sm text-pos-text">{t('pos_unloading_title')}</h3>
        </div>
        <button
          type="button"
          on:click={onClose}
          class="p-1.5 rounded-lg bg-slate-100 dark:bg-slate-800 text-pos-muted hover:text-pos-text cursor-pointer transition"
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <div class="text-xs font-bold text-pos-muted truncate">
        {productName}{quantity !== 1 ? ` × ${quantity}` : ''}
      </div>

      <div>
        <label class="block text-xs font-bold text-pos-muted mb-1">
          {t('pos_unloading_fee_per_unit')}
        </label>
        <input
          use:autofocusSelect
          type="number"
          min="0"
          step="1"
          bind:value={feeInput}
          on:keydown={handleKey}
          placeholder="0"
          class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-lg font-black font-mono text-amber-600 dark:text-amber-400 outline-none focus:ring-2 focus:ring-amber-500"
        />
      </div>

      <div class="flex items-center justify-between p-2.5 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800/60 rounded-xl">
        <span class="text-[10px] font-black text-pos-muted uppercase tracking-wider">
          {t('pos_unloading_total')}
        </span>
        <span class="font-mono font-black text-sm text-amber-600 dark:text-amber-400">
          {feeValue.toLocaleString()} × {quantity} = {feeTotal.toLocaleString()} DZD
        </span>
      </div>

      <div class="flex gap-2">
        {#if !isEdit}
          <button
            type="button"
            on:click={skip}
            class="flex-1 py-2.5 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700 text-pos-muted text-xs font-black rounded-xl cursor-pointer transition"
          >
            {t('pos_unloading_skip')}
          </button>
        {/if}
        <button
          type="button"
          on:click={confirm}
          class="flex-1 py-2 bg-amber-600 hover:bg-amber-700 text-white text-xs font-black rounded-xl cursor-pointer flex items-center justify-center gap-1 transition active:scale-95"
        >
          <Check class="w-3.5 h-3.5" /> {t('pos_unloading_apply')}
        </button>
      </div>
    </div>
  </div>
{/if}
