<script lang="ts">
  import { t } from '../i18n';
  import { Search, X, QrCode } from 'lucide-svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { currentUser } from '../stores/auth';
  import type { User } from '../types';

  import { normalizeBarcode } from '../utils/barcode';
  import { onMount, onDestroy } from 'svelte';

  export let query = '';
  export let searchType: 'all' | 'name' | 'barcode' | 'price' | 'qr' = 'all';
  export let onSearch: () => void;
  // Receipt QR scan (`SALE:<number>`): the POS opens the sale for
  // edit/reprint. Return true when the payload was consumed.
  export let onSaleQrScan: ((payload: string) => Promise<boolean>) | null = null;
  // Purchase QR scan (`PUR:<invoice>`): jumps to the purchases page.
  export let onPurchaseQrScan: ((payload: string) => void) | null = null;
  // Employee RFID/QR scan: sets the employee as the cart customer.
  export let onEmployeeScan: ((tag: string) => Promise<boolean>) | null = null;
  // POS rule: when > 0, the search bar auto-refocuses after N idle seconds.
  export let autofocusSeconds = 0;

  let searchInput: HTMLInputElement;
  let idleTimer: any = null;

  export function focusSearch() {
    searchInput?.focus();
    searchInput?.select();
  }

  function resetIdleTimer() {
    clearTimeout(idleTimer);
    if (autofocusSeconds > 0) {
      idleTimer = setTimeout(() => {
        // Only steal focus if no other text field is being used.
        const active = document.activeElement;
        const tag = active?.tagName;
        const otherField =
          tag === 'INPUT' && active !== searchInput && (active as HTMLInputElement).type !== 'checkbox' && (active as HTMLInputElement).type !== 'radio';
        if (!otherField && tag !== 'TEXTAREA' && tag !== 'SELECT') {
          focusSearch();
        }
      }, autofocusSeconds * 1000);
    }
  }

  onMount(() => {
    resetIdleTimer();
  });

  onDestroy(() => {
    clearTimeout(idleTimer);
  });


  const modes: Array<'all' | 'name' | 'barcode' | 'price' | 'qr'> = ['all', 'name', 'barcode', 'price', 'qr'];
  const labels: Record<string, string> = {
    all: 'الكل (All)',
    name: 'الاسم (Name)',
    barcode: 'الباركود (Barcode)',
    price: 'السعر (Price)',
    qr: 'QR Code',
  };

  function cycleSearchType() {
    const currentIndex = modes.indexOf(searchType);
    const nextIndex = (currentIndex + 1) % modes.length;
    searchType = modes[nextIndex];
    onSearch();
  }

  function clearQuery() {
    query = '';
    onSearch();
  }

  async function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      // Receipt / purchase QR payloads route to their own flows — they are
      // NOT product barcodes (scanning a receipt QR opens it for reprint /
      // edit; a purchase QR jumps to the invoice).
      const q = query.trim().toUpperCase();
      if (q.startsWith('SALE:') && onSaleQrScan) {
        const consumed = await onSaleQrScan(query.trim());
        if (consumed) {
          query = '';
          return;
        }
      }
      if (q.startsWith('PUR:') && onPurchaseQrScan) {
        onPurchaseQrScan(query.trim());
        query = '';
        return;
      }
      // Employee tags resolve to the cart customer (RFID badges / EMP-QR).
      if (onEmployeeScan && (q.startsWith('RFID-') || q.startsWith('EMP-QR-') || q.startsWith('EMP_'))) {
        const consumed = await onEmployeeScan(query.trim());
        if (consumed) {
          query = '';
          return;
        }
      }

      // Auto-normalize if scanning numeric barcode with AZERTY/Arabic keyboard active
      if (searchType === 'barcode' || /^[&é"'\(-è_çà0-9١-٩]+$/.test(query.trim())) {
        query = normalizeBarcode(query);
      }

      onSearch();
    }
  }
</script>

<div class="flex items-center gap-2 bg-pos-card border border-pos-border rounded-xl p-1.5 shadow-xs">
  <!-- Sleek Toggle Pill Button instead of select dropdown -->
  <button
    type="button"
    on:click={cycleSearchType}
    class="px-3 py-1.5 bg-sky-50 dark:bg-sky-950/60 border border-sky-200 dark:border-sky-800 text-sky-700 dark:text-sky-300 text-xs font-extrabold rounded-lg shrink-0 transition hover:bg-sky-100 flex items-center gap-1 cursor-pointer"
    title="Click to toggle search mode"
  >
    {#if searchType === 'qr'}
      <QrCode class="w-3.5 h-3.5 text-sky-600" />
    {/if}
    <span>{labels[searchType]}</span>
  </button>

  <div class="relative flex-1 flex items-center">
    <Search class="w-4 h-4 text-pos-muted absolute start-2.5 pointer-events-none" />
    <input
      bind:this={searchInput}
      type="text"
      data-scanner-input
      bind:value={query}
      on:input={() => { resetIdleTimer(); onSearch(); }}
      on:keydown={handleKeyDown}
      on:focus={(e) => { resetIdleTimer(); (e.target as HTMLInputElement).select(); }}
      on:blur={resetIdleTimer}
      placeholder={t('search_placeholder')}
      class="w-full ps-8 pe-8 py-1.5 bg-transparent text-pos-text placeholder-pos-muted text-xs font-semibold outline-none"
    />
    {#if query}
      <button
        type="button"
        on:click={clearQuery}
        class="p-1 text-pos-muted hover:text-rose-500 rounded-full transition absolute end-2 cursor-pointer"
        title="Clear Search"
      >
        <X class="w-3.5 h-3.5" />
      </button>
    {/if}
  </div>
</div>