<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t, currentLocale, setLocale, type Language } from '../i18n';
  import {
    clearCart, isRefundMode, toggleAllCartRefund,
    cartItems, globalDiscountMode, heldSalesList,
    holdCurrentSale, heldNotification, posMode
  } from '../stores/cart';
  import {
    PlusCircle, Trash2, Undo2, Percent, CreditCard,
    PauseCircle, Printer, DollarSign, Languages, Check,
    ShoppingBag, RefreshCw, AlertTriangle, Layers, Banknote, ShieldAlert, Wallet, Coins,
    ShoppingCart, PackagePlus, PackageX, RotateCcw
  } from 'lucide-svelte';

  export let onOpenPayment: () => void;
  export let onOpenCashDrawer: () => void;
  export let onOpenRemise: () => void;
  export let onOpenHeldSales: () => void;
  export let onPrintReceipt: () => void;
  export let onOpenOtherArticle: () => void = () => {};
  export let onCheckout: () => void;
  export let onReopenLastReceipt: () => void = () => {};

  export let selectedPaymentMode: 'cash' | 'tpe' | 'credit' | 'versement' = 'cash';
  export let autoPrintEnabled: boolean = true;
  export let autoDrawerEnabled: boolean = true;

  async function handleNewSale() {
    if ($cartItems.length > 0) {
      await holdCurrentSale();
    } else {
      clearCart();
    }
  }

  async function handleHoldClick() {
    if ($cartItems.length > 0) {
      await holdCurrentSale();
    } else {
      onOpenHeldSales();
    }
  }

  function cyclePaymentMode() {
    if (selectedPaymentMode === 'cash') selectedPaymentMode = 'tpe';
    else if (selectedPaymentMode === 'tpe') selectedPaymentMode = 'credit';
    else if (selectedPaymentMode === 'credit') selectedPaymentMode = 'versement';
    else selectedPaymentMode = 'cash';
  }

  // Sale -> Purchase -> Broken cycle. Sale: everything is sold. Purchase:
  // scanned items are bought from the supplier and add to stock. Broken:
  // items are written off — quantity leaves stock and value becomes an expense.
  // The host parks the cart per-mode before switching (hold sales,
  // hold purchases, hold broken).
  export let onCycleMode: () => void = () => {
    if ($posMode === 'sale') $posMode = 'purchase';
    else if ($posMode === 'purchase') $posMode = 'broken';
    else $posMode = 'sale';
  };

  function cyclePosMode() {
    onCycleMode();
  }

  function handleRefundToggle() {
    $isRefundMode = !$isRefundMode;
    toggleAllCartRefund();
  }

  function handleRemiseToggle() {
    onOpenRemise();
  }

  function switchLanguage(lang: Language) {
    setLocale(lang);
  }

  async function kickDrawer() {
    try {
      const settings = await invoke<Record<string, string>>('get_all_settings');
      const port = parseInt(settings['drawer_com_port'] || '1');
      const baud = parseInt(settings['drawer_baud_rate'] || '9600');
      await invoke('open_serial_cash_drawer', { comPort: port, baudRate: baud });
    } catch (e) {
      console.warn('Native drawer open status:', e);
    }
  }
</script>

<header class="bg-pos-card border-b border-pos-border px-2 py-1.5 flex items-center justify-between shadow-xs select-none relative gap-1.5 shrink-0">
  <div class="flex items-center gap-1 flex-wrap lg:flex-nowrap">
    <!-- Group 1: Sale Lifecycle (F1 New, Del Clear, F3 Held List) -->
    <div class="flex items-center gap-1 bg-slate-50 dark:bg-slate-800/60 p-1 rounded-2xl border border-pos-border shadow-xs">
      <button
        type="button"
        on:click={handleNewSale}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] bg-sky-600 hover:bg-sky-700 text-white rounded-xl text-center transition shadow-xs cursor-pointer active:scale-95 shrink-0"
        title="F1 - New Sale (Auto-holds active cart)"
      >
        <PlusCircle class="w-4 h-4 mb-0.5" />
        <span class="text-xs font-black leading-tight">{t('btn_new_sale')}</span>
        <span class="text-[9px] bg-black/25 px-1 rounded font-mono font-normal mt-0.5">F1</span>
      </button>

      <button
        type="button"
        on:click={clearCart}
        disabled={$cartItems.length === 0}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white rounded-xl text-center transition shadow-xs cursor-pointer active:scale-95 shrink-0"
        title="Del - Clear Cart"
      >
        <Trash2 class="w-4 h-4 mb-0.5" />
        <span class="text-xs font-black leading-tight">{t('btn_delete_cart') || 'Clear'}</span>
        <span class="text-[9px] bg-black/25 px-1 rounded font-mono font-normal mt-0.5">Del</span>
      </button>

      <button
        type="button"
        on:click={handleHoldClick}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 dark:hover:bg-slate-600 text-pos-text rounded-xl text-center transition shadow-xs cursor-pointer active:scale-95 relative shrink-0"
        title="F3 - {$cartItems.length > 0 ? 'Click to Hold Active Cart' : 'Open Held Sales List'}"
      >
        <PauseCircle class="w-4 h-4 text-amber-500 mb-0.5" />
        <span class="text-xs font-black leading-tight">{$cartItems.length > 0 ? 'Hold Cart' : t('btn_held_sales')}</span>
        {#if $heldSalesList.length > 0}
          <span class="absolute top-1 end-1 px-1.5 py-0.2 bg-amber-500 text-white font-mono text-[8px] rounded-full font-black">
            {$heldSalesList.length}
          </span>
        {/if}
        <span class="text-[9px] bg-black/10 dark:bg-white/10 px-1 rounded font-mono font-normal mt-0.5">F3</span>
      </button>
    </div>

    <!-- Vertical Divider -->
    <div class="h-7 w-px bg-pos-border shrink-0 hidden sm:block"></div>

    <!-- Group 2: Cart Modifiers (Refund, Remise, Other Article) -->
    <div class="flex items-center gap-1.5 bg-slate-50 dark:bg-slate-800/60 p-1.5 rounded-2xl border border-pos-border shadow-xs">
      <button
        type="button"
        on:click={handleRefundToggle}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] rounded-xl text-center transition shadow-xs cursor-pointer active:scale-95 shrink-0 {$isRefundMode ? 'bg-amber-600 text-white ring-2 ring-amber-300' : 'bg-slate-200 dark:bg-slate-700 text-pos-text hover:bg-slate-300'}"
        title="Toggle Refund on All Items"
      >
        <Undo2 class="w-4 h-4 mb-0.5" />
        <span class="text-xs font-black leading-tight">{t('btn_refund')}</span>
        <span class="text-[9px] bg-black/10 dark:bg-white/10 px-1 rounded font-mono font-normal mt-0.5">F5</span>
      </button>

      <button
        type="button"
        on:click={handleRemiseToggle}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] rounded-xl text-center transition shadow-xs cursor-pointer active:scale-95 shrink-0 {$globalDiscountMode !== 'none' ? 'bg-purple-600 text-white ring-2 ring-purple-300' : 'bg-slate-200 dark:bg-slate-700 text-pos-text hover:bg-slate-300'}"
        title="F4 - Global Remise (% or DZD)"
      >
        <Percent class="w-4 h-4 mb-0.5" />
        <span class="text-xs font-black leading-tight">{t('btn_remise')}</span>
        <span class="text-[9px] bg-black/10 dark:bg-white/10 px-1 rounded font-mono font-normal mt-0.5">F4</span>
      </button>

      <!-- Other Article (Custom Price / Divers) -->
      <button
        type="button"
        on:click={onOpenOtherArticle}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] bg-amber-50 dark:bg-amber-950/40 border border-amber-300 dark:border-amber-700 hover:bg-amber-100 text-amber-900 dark:text-amber-200 rounded-xl text-center transition shadow-xs cursor-pointer active:scale-95 shrink-0"
        title={t('pos_other_product')}
      >
        <DollarSign class="w-4 h-4 text-amber-600 mb-0.5" />
        <span class="text-xs font-black leading-tight">{t('pos_other_product')}</span>
        <span class="text-[9px] bg-amber-500/20 px-1 rounded font-mono font-normal mt-0.5">+ Price</span>
      </button>
    </div>

    <!-- Vertical Divider -->
    <div class="h-7 w-px bg-pos-border shrink-0 hidden sm:block"></div>

    <!-- Group 3: Payment Mode Single Multi-Toggle Button -->
    <div class="flex items-center bg-slate-50 dark:bg-slate-800/60 p-1.5 rounded-2xl border border-pos-border shadow-xs">
      <button
        type="button"
        on:click={cyclePaymentMode}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] rounded-xl text-center transition cursor-pointer shrink-0 shadow-sm active:scale-95 {selectedPaymentMode === 'cash' ? 'bg-emerald-600 text-white ring-2 ring-emerald-400' : selectedPaymentMode === 'tpe' ? 'bg-sky-600 text-white ring-2 ring-sky-400' : selectedPaymentMode === 'versement' ? 'bg-violet-600 text-white ring-2 ring-violet-400' : 'bg-amber-600 text-white ring-2 ring-amber-400'}"
        title={t('btn_payment_type')}
      >
        {#if selectedPaymentMode === 'cash'}
          <Banknote class="w-4 h-4 mb-0.5" />
          <span class="text-xs font-black leading-tight">{t('pos_payment_cash')}</span>
        {:else if selectedPaymentMode === 'tpe'}
          <CreditCard class="w-4 h-4 mb-0.5" />
          <span class="text-xs font-black leading-tight">{t('pos_payment_tpe')}</span>
        {:else if selectedPaymentMode === 'versement'}
          <Wallet class="w-4 h-4 mb-0.5" />
          <span class="text-xs font-black leading-tight">{t('pos_payment_versement')}</span>
        {:else}
          <Layers class="w-4 h-4 mb-0.5" />
          <span class="text-xs font-black leading-tight">{t('pos_payment_credit')}</span>
        {/if}
        <span class="text-[9px] opacity-80 font-mono mt-0.5">Toggle ↻</span>
      </button>
    </div>

    <!-- Vertical Divider -->
    <div class="h-7 w-px bg-pos-border shrink-0 hidden sm:block"></div>

    <!-- Group 4: Operations (POS Mode, Register) -->
    <div class="flex items-center gap-1.5 bg-slate-50 dark:bg-slate-800/60 p-1.5 rounded-2xl border border-pos-border shadow-xs">
      <button
        type="button"
        on:click={cyclePosMode}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] rounded-xl text-center transition cursor-pointer shrink-0 shadow-sm active:scale-95 {$posMode === 'sale' ? 'bg-emerald-600 text-white ring-2 ring-emerald-400' : $posMode === 'purchase' ? 'bg-sky-600 text-white ring-2 ring-sky-400' : 'bg-rose-600 text-white ring-2 ring-rose-400'}"
        title="Click to Toggle: Sale → Purchase (supplier) → Broken (damaged)"
      >
        {#if $posMode === 'sale'}
          <ShoppingCart class="w-4 h-4 mb-0.5" />
          <span class="text-xs font-black leading-tight">Sale (بيع)</span>
        {:else if $posMode === 'purchase'}
          <PackagePlus class="w-4 h-4 mb-0.5" />
          <span class="text-xs font-black leading-tight">Purchase (شراء)</span>
        {:else}
          <PackageX class="w-4 h-4 mb-0.5" />
          <span class="text-xs font-black leading-tight">Broken (تالف)</span>
        {/if}
        <span class="text-[9px] opacity-80 font-mono mt-0.5">Toggle ↻</span>
      </button>

      <button
        type="button"
        on:click={onOpenCashDrawer}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 dark:hover:bg-slate-600 text-pos-text rounded-xl text-center transition shadow-xs cursor-pointer active:scale-95 shrink-0"
        title="F9 - Cash Register (صندوق Caisse)"
      >
        <DollarSign class="w-4 h-4 text-emerald-500 mb-0.5" />
        <span class="text-xs font-black leading-tight">Register</span>
        <span class="text-[9px] bg-black/10 dark:bg-white/10 px-1 rounded font-mono font-normal mt-0.5">F9</span>
      </button>

      <button
        type="button"
        on:click={onCheckout}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] bg-emerald-600 hover:bg-emerald-700 text-white rounded-xl text-center transition shadow-xs cursor-pointer active:scale-95 shrink-0"
        title="Checkout (F2) — Select customer, enter amount, get change/reste"
      >
        <Coins class="w-4 h-4 mb-0.5" />
        <span class="text-xs font-black leading-tight">{t('bar_checkout')}
        <span class="text-[9px] bg-black/25 px-1 rounded font-mono font-normal mt-0.5">F2</span>
      </button>
    </div>

    <!-- Vertical Divider -->
    <div class="h-7 w-px bg-pos-border shrink-0 hidden sm:block"></div>

    <!-- Group 5: Silent Print Toggle Card + Reopen Last Receipt -->
    <div class="flex items-center gap-1.5 bg-slate-50 dark:bg-slate-800/60 p-1.5 rounded-2xl border border-pos-border shadow-xs">
      <button
        type="button"
        on:click={() => {
          autoPrintEnabled = !autoPrintEnabled;
          invoke('set_setting', { key: 'pos_autoprint', value: autoPrintEnabled ? 'true' : 'false' }).catch(() => {});
        }}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] rounded-xl text-center transition cursor-pointer shrink-0 {autoPrintEnabled ? 'bg-sky-600 text-white' : 'bg-slate-200 dark:bg-slate-700 text-pos-muted'}"
        title="Toggle Auto Print Receipt on Checkout"
      >
        <Printer class="w-4 h-4 mb-0.5" />
        <span class="text-[9px] font-black uppercase">Auto-Print</span>
        <span class="text-[8px] font-mono font-bold mt-0.5">{autoPrintEnabled ? 'ON' : 'OFF'}</span>
      </button>

      <!-- Reopen / Reprint Last Receipt -->
      <button
        type="button"
        on:click={onReopenLastReceipt}
        class="flex flex-col items-center justify-center w-[74px] h-[52px] rounded-xl text-center transition cursor-pointer shrink-0 bg-indigo-600 hover:bg-indigo-700 text-white shadow-sm active:scale-95"
        title="Reopen / Reprint Last Receipt — view, edit in POS or reprint the last sale (أخر وصل)"
      >
        <RotateCcw class="w-4 h-4 mb-0.5" />
        <span class="text-[9px] font-black uppercase leading-tight">Last Receipt</span>
        <span class="text-[8px] font-mono font-bold mt-0.5 opacity-80">Reopen</span>
      </button>
    </div>
  </div>

  <!-- Auto-Hold Notification Banner Popover -->
  {#if $heldNotification}
    <div class="absolute top-14 start-4 z-50 bg-emerald-600 text-white px-4 py-2 rounded-xl shadow-xl flex items-center gap-2 text-xs font-bold animate-in slide-in-from-top-2 duration-150">
      <Check class="w-4 h-4" />
      <span>{$heldNotification}</span>
    </div>
  {/if}
</header>