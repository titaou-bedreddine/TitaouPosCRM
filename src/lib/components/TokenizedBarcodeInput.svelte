<script lang="ts">
  import { X, Plus } from 'lucide-svelte';
  import { normalizeBarcode } from '../utils/barcode';

  export let barcodes: string[] = [];

  let inputValue = '';

  function addBarcode() {
    const val = normalizeBarcode(inputValue).trim();
    if (val && !barcodes.includes(val)) {
      barcodes = [...barcodes, val];
      inputValue = '';
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      addBarcode();
    }
  }

  function removeBarcode(index: number) {
    barcodes = barcodes.filter((_, i) => i !== index);
  }
</script>

<div class="space-y-1.5">
  <div class="flex flex-wrap gap-1.5 p-2 bg-pos-card border border-pos-border rounded min-h-[42px] items-center">
    {#each barcodes as code, i}
      <span class="inline-flex items-center gap-1 px-2 py-1 rounded bg-sky-100 dark:bg-sky-950/60 text-sky-800 dark:text-sky-300 font-mono text-xs font-bold border border-sky-300 dark:border-sky-800">
        <span>{code}</span>
        {#if i === 0}
          <span class="text-[10px] bg-sky-600 text-white px-1 rounded">Primary</span>
        {/if}
        <button type="button" on:click={() => removeBarcode(i)} class="hover:text-rose-500 cursor-pointer">
          <X class="w-3 h-3" />
        </button>
      </span>
    {/each}
    <input
      type="text"
      bind:value={inputValue}
      on:keydown={handleKeyDown}
      placeholder="Type or scan barcode and press Enter..."
      class="flex-1 min-w-[180px] bg-transparent border-0 outline-none text-xs text-pos-text font-mono"
    />
  </div>
</div>