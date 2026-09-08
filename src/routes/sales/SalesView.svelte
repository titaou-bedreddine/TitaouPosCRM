<script lang="ts">
  import QrImage from '../../lib/components/QrImage.svelte';
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { localTodayISO } from '../../lib/utils/date';
  import { invoke } from '@tauri-apps/api/core';
  import { normalizeBarcode } from '../../lib/utils/barcode';
  import { entityQrPayload } from '../../lib/utils/printer';
  import { sortRows, clickSort } from '../../lib/utils/tableSort';
  import type { Sale, User } from '../../lib/types';
  import { currentUser } from '../../lib/stores/auth';
  import { printHtmlSilently } from '../../lib/utils/printer';
  import DateQuickFilters from '../../lib/components/DateQuickFilters.svelte';
  import { originSaleId,  cartItems, clearCart } from '../../lib/stores/cart';
  import { selectedCustomerId } from '../../lib/stores/customers';
  import {
    ShoppingBag, Search, Printer, Calendar, User as UserIcon,
    DollarSign, Eye, Trash2, X, Check, AlertTriangle, Layers,
    CreditCard, Banknote, ShieldAlert, TrendingUp, Pencil, Package, Monitor
  } from 'lucide-svelte';

  let sales: Sale[] = [];
  let users: User[] = [];

  // History defaults to today; the user can widen the range.
  let startDate = localTodayISO();
  let endDate = localTodayISO();
  let selectedCashier: number | null = null;
  let selectedStatus: string = 'all';
  let searchQuery = '';
  // AZERTY-normalized mirror of the search box (scanners may emit & é " ...).
  $: searchQueryN = normalizeBarcode(searchQuery).toLowerCase();

  // Sale Details Modal
  let isDetailModalOpen = false;
  let selectedSale: Sale | null = null;
  let saleItems: any[] = [];
  let isLoadingItems = false;

  // Protected Delete Modal
  let isDeleteModalOpen = false;
  let adminPassword = '';
  let deleteError = '';
  let isDeleting = false;

  onMount(async () => {
    await loadUsers();
    await loadSales();
  });

  async function loadUsers() {
    try {
      users = await invoke<User[]>('get_active_users');
    } catch (e) {
      console.error(e);
    }
  }

  async function loadSales() {
    try {
      sales = await invoke<Sale[]>('list_sales', {
        startDate: startDate || null,
        endDate: endDate || null,
        userId: selectedCashier ? Number(selectedCashier) : null,
        limit: 200,
      });
    } catch (e) {
      console.error(e);
    }
  }

  $: filteredSales = sales.filter(s => {
    // Omni-search: sale number, customer name, exact amount, or the QR
    // payload (scan a receipt QR → 'SALE:POS-...' or plain code).
    const q = searchQueryN;
    const qr = entityQrPayload('SALE', s.sale_number).toLowerCase();
    const stripped = q.startsWith('sale:') ? q.slice(5) : q;
    const matchesSearch =
      !q ||
      s.sale_number.toLowerCase().includes(stripped) ||
      (s.customer_name || '').toLowerCase().includes(stripped) ||
      String(s.total_amount) === stripped ||
      qr === q ||
      qr.includes(stripped);

    if (!matchesSearch) return false;

    if (selectedStatus === 'all') return true;
    if (selectedStatus === 'paid') return s.payment_status === 'paid';
    if (selectedStatus === 'partial') return s.payment_status === 'partial';
    if (selectedStatus === 'unpaid') return s.payment_status === 'unpaid';
    if (selectedStatus === 'cash') return s.payment_method === 'cash';
    if (selectedStatus === 'tpe') return s.payment_method === 'tpe';
    if (selectedStatus === 'credit') return s.payment_method === 'credit';
    if (selectedStatus === 'refunded') return s.status === 'refunded' || s.status === 'partially_refunded';
    return true;
  });

  // Top Real Stats
  // Three-state column sort: asc -> desc -> default.
  let sortKey: string | null = null;
  let sortDir: 'asc' | 'desc' | null = null;
  function applySort(key: string) {
    const next = clickSort(key, sortKey, sortDir);
    sortKey = next.key;
    sortDir = next.dir;
  }
  function sortIndicator(key: string): string {
    if (sortKey !== key || !sortDir) return '';
    // DESC (big→small) first, then ASC — matches the Stock page cycle.
    return sortDir === 'asc' ? '▲' : '▼';
  }
  $: sortedSales = sortRows(filteredSales, sortKey, sortDir, filteredSales);
  $: totalSalesCount = filteredSales.length;
  $: totalGrossRevenue = filteredSales.reduce((sum, s) => sum + s.total_amount, 0);
  $: totalNetPaid = filteredSales.reduce((sum, s) => sum + s.paid_amount, 0);
  $: totalDueCredit = totalGrossRevenue - totalNetPaid;
  // Lines sold = number of distinct sale lines; units sold = Σ quantities.
  // Lines ≠ units: A×2 + B×5 + C×1 → 3 lines, 8 units.
  $: totalLinesSold = filteredSales.reduce((sum, s) => sum + (s.lines_sold || 0), 0);
  $: totalUnitsSold = filteredSales.reduce((sum, s) => sum + (s.units_sold || 0), 0);

  async function openSaleDetails(s: Sale) {
    selectedSale = s;
    isDetailModalOpen = true;
    try {
      isLoadingItems = true;
      saleItems = await invoke<any[]>('get_sale_items', { saleId: s.id });
    } catch (e) {
      console.error('Failed to load sale items:', e);
      saleItems = [];
    } finally {
      isLoadingItems = false;
    }
  }

  async function printReceipt(s: Sale) {
    // ONE unified receipt preset system: reprint from history uses the same
    // settings + template choice as POS auto-print (v0.5.16).
    try {
      const settings = await invoke<Record<string, string>>('get_all_settings');
      const items = await invoke<any[]>('get_sale_items', { saleId: s.id });
      const { buildUnifiedReceipt } = await import('../../lib/printing/unifiedReceipt');
      const { entityQrDataUrl } = await import('../../lib/utils/printer');

      const qr = await entityQrDataUrl(`SALE:${s.sale_number}`, 240).catch(() => undefined);

      const built = buildUnifiedReceipt({
        saleNumber: s.sale_number,
        saleDate: s.created_at,
        cashierName: s.user_name || 'Admin',
        terminalName: s.terminal_name || undefined,
        customerName: s.customer_name || undefined,
        items: items.map((it: any) => ({
          name: it.name_fr || it.name_ar || it.name,
          quantity: it.quantity,
          unitPrice: it.unit_price,
          totalPrice: it.total_price,
          discountPerUnit: it.discount_amount || 0,
          isRefund: !!it.is_refunded,
        })),
        subtotal: (s as any).subtotal ?? s.total_amount,
        discount: (s as any).discount_amount ?? 0,
        grandTotal: s.total_amount,
        amountPaid: s.paid_amount,
        change: s.change_amount,
        paymentMethod: (s.payment_method || 'cash').toUpperCase(),
        settings,
        qrDataUrl: qr,
        copyLabel: 'REPRINT / نسخة',
      });
      const r = await printHtmlSilently(built.html, built.title, { widthMm: built.paperWidthMm });
      if (!r.ok) alert('Print failed: ' + r.message);
    } catch (e: any) {
      alert('Error printing receipt: ' + (e.message || e));
    }
  }

  // Click the invoice number to copy it (hint under the modal header).
  let invoiceCopied = false;
  async function copyInvoiceNumber(num: string) {
    try {
      await navigator.clipboard.writeText(num);
      invoiceCopied = true;
      setTimeout(() => (invoiceCopied = false), 1800);
    } catch {
      console.warn('clipboard write failed');
    }
  }

  function promptProtectedDelete() {
    deleteError = '';
    adminPassword = '';
    isDeleteModalOpen = true;
  }

  // Re-open a completed sale in the POS cart for editing. The caller
  // (App.svelte) switches to the POS route; the sale stays in history until
  // the cashier deletes it via the protected delete.
  async function editSaleInPos(sale: Sale) {
    try {
      const items = await invoke<any[]>('get_sale_items', { saleId: sale.id });
      const mapped = items.map((i) => ({
        product_id: i.product_id,
        sku: i.sku || '',
        barcode: i.barcode || '',
        name_ar: i.name_ar || '',
        name_fr: i.name_fr || '',
        name_en: i.name_en || '',
        image_path: i.image_path,
        unit_price: i.unit_price,
        quantity: i.quantity,
        discount_amount: i.discount_amount || 0,
        tax_amount: i.tax_amount || 0,
        total_price: i.total_price,
        is_refund: i.is_refund || false,
      }));
      clearCart();
      $cartItems = mapped;
      if (sale.customer_id) $selectedCustomerId = sale.customer_id;
      // Editing in place: the checkout updates this sale (tagged MODIFIED)
      // instead of inserting a duplicate row.
      originSaleId.set(sale.id);
      isDetailModalOpen = false;
      onRequestPosRoute?.();
    } catch (e) {
      console.error('Failed to load sale for editing:', e);
    }
  }

  export let onRequestPosRoute: () => void = () => {};

  async function executeProtectedDelete() {
    if (!$currentUser) return;
    if ($currentUser.role_name !== 'admin' && !adminPassword) {
      deleteError = 'Admin password required to delete sale / كلمة المرور مطلوبة';
      return;
    }
    try {
      isDeleting = true;
      deleteError = '';
      if ($currentUser.role_name !== 'admin') {
        const ok = await invoke<boolean>('verify_admin_password', { password: adminPassword });
        if (!ok) {
          deleteError = 'Invalid password / كلمة المرور غير صحيحة';
          isDeleting = false;
          return;
        }
      }

      if (selectedSale) {
        await invoke('delete_sale', { saleId: selectedSale.id, userId: $currentUser?.id });
        isDeleteModalOpen = false;
        isDetailModalOpen = false;
        selectedSale = null;
        await loadSales();
      }
    } catch (e: any) {
      deleteError = typeof e === 'string' ? e : e.message || 'Failed to delete sale';
    } finally {
      isDeleting = false;
    }
  }
</script>

<div class="p-6 space-y-4 overflow-y-auto h-full select-none flex flex-col bg-pos-bg">
  <div class="flex items-center justify-between shrink-0">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-2xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <ShoppingBag class="w-5 h-5" />
      </div>
      <div>
        <h1 class="text-xl font-black text-pos-text tracking-tight">{t('sales_title')}</h1>
        <p class="text-xs text-pos-muted">{t('sales_subtitle')}</p>
      </div>
    </div>
  </div>

  <!-- Top Statistics Cards -->
  <div class="grid grid-cols-2 md:grid-cols-4 gap-3 shrink-0">
    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-sky-50 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <ShoppingBag class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">{t('sales_transactions')}</p>
        <p class="text-base font-black font-mono text-pos-text">{totalSalesCount.toLocaleString()}</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-emerald-50 dark:bg-emerald-950 text-emerald-600 flex items-center justify-center font-bold">
        <DollarSign class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">{t('sales_total_revenue')}</p>
        <p class="text-base font-black font-mono text-emerald-600">{totalGrossRevenue.toLocaleString()} DZD</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-blue-50 dark:bg-blue-950 text-blue-600 flex items-center justify-center font-bold">
        <CreditCard class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">{t('sales_net_paid')}</p>
        <p class="text-base font-black font-mono text-blue-600">{totalNetPaid.toLocaleString()} DZD</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-amber-50 dark:bg-amber-950 text-amber-600 flex items-center justify-center font-bold">
        <Layers class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">{t('sales_credit_due')}</p>
        <p class="text-base font-black font-mono text-amber-600">{totalDueCredit.toLocaleString()} DZD</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-purple-50 dark:bg-purple-950 text-purple-600 flex items-center justify-center font-bold">
        <Layers class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">{t('sales_lines_sold') || 'Lines Sold'}</p>
        <p class="text-base font-black font-mono text-purple-600">{totalLinesSold.toLocaleString()}</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-indigo-50 dark:bg-indigo-950 text-indigo-600 flex items-center justify-center font-bold">
        <Package class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">{t('sales_units_sold') || 'Units Sold'}</p>
        <p class="text-base font-black font-mono text-indigo-600">{totalUnitsSold.toLocaleString()}</p>
      </div>
    </div>
  </div>

  <!-- Quick Date Presets -->
  <div class="flex items-center justify-between shrink-0">
    <DateQuickFilters bind:startDate bind:endDate onChange={loadSales} />
  </div>

  <!-- Filter Bar -->
  <div class="bg-pos-card border border-pos-border rounded-2xl p-3 shadow-xs grid grid-cols-1 md:grid-cols-5 gap-2.5 items-end shrink-0">
    <div>
      <label class="block text-[10px] font-bold text-pos-muted mb-1">Search Sale # or Customer</label>
      <input
        type="text"
        bind:value={searchQuery}
        placeholder={t('sales_search')}
        class="w-full px-3 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none"
      />
    </div>

    <div>
      <label class="block text-[10px] font-bold text-pos-muted mb-1">{t('from_date')}</label>
      <input type="date" bind:value={startDate} on:change={loadSales} class="w-full px-3 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
    </div>

    <div>
      <label class="block text-[10px] font-bold text-pos-muted mb-1">{t('to_date')}</label>
      <input type="date" bind:value={endDate} on:change={loadSales} class="w-full px-3 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
    </div>

    <div>
      <label class="block text-[10px] font-bold text-pos-muted mb-1">Status Filter</label>
      <select bind:value={selectedStatus} class="w-full px-3 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none">
        <option value="all">{t('all')} ({t('filter_all')})</option>
        <option value="paid">Fully Paid (مدفوع بالكامل)</option>
        <option value="partial">Partial / Credit (غير مكتمل / دين)</option>
        <option value="cash">Cash Only (نقد)</option>
        <option value="tpe">TPE Card (بطاقة)</option>
        <option value="credit">Credit Only (دين)</option>
        <option value="refunded">Refunded (مسترجع)</option>
      </select>
    </div>

    <div>
      <label class="block text-[10px] font-bold text-pos-muted mb-1">Cashier</label>
      <select bind:value={selectedCashier} on:change={loadSales} class="w-full px-3 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none">
        <option value={null}>{t('exp_all_users')}</option>
        {#each users as u}
          <option value={u.id}>{u.display_name || u.username}</option>
        {/each}
      </select>
    </div>
  </div>

  <!-- Sales Table -->
  <div class="bg-pos-card border border-pos-border rounded-2xl shadow-xs overflow-hidden flex-1 overflow-y-auto">
    <table class="w-full text-start text-xs border-collapse">
      <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold sticky top-0 z-10">
        <tr>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('sale_number')}>{t('sales_sale_num')} {sortIndicator('sale_number')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('created_at')}>{t('sales_date_time')} {sortIndicator('created_at')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('user_name')}>{t('sales_cashier')} {sortIndicator('user_name')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('terminal_name')} title="PC / terminal that recorded the sale">{t('terminal')} {sortIndicator('terminal_name')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('customer_name')}>{t('customer')} {sortIndicator('customer_name')}</th>
          <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('total_amount')}>{t('sales_total_amount')} {sortIndicator('total_amount')}</th>
          <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('paid_amount')}>{t('sales_paid_amount')} {sortIndicator('paid_amount')}</th>
          <th class="p-3 text-center cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('lines_sold')} title="Distinct sale lines">{t('sales_lines_sold') || 'Lines'} {sortIndicator('lines_sold')}</th>
          <th class="p-3 text-center cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('units_sold')} title="Total quantities (Σ per-line qty)">{t('sales_units_sold') || 'Units'} {sortIndicator('units_sold')}</th>
          <th class="p-3 text-center">{t('sales_payment')}</th>
          <th class="p-3 text-center">{t('status')}</th>
          <th class="p-3 text-end">{t('actions')}</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-pos-border/40">
        {#if filteredSales.length === 0}
          <tr>
            <td colspan="12" class="p-8 text-center text-pos-muted">{t('no_data')}</td>
          </tr>
        {:else}
          {#each sortedSales as s}
            <tr
              on:click={() => openSaleDetails(s)}
              class="hover:bg-slate-50 dark:hover:bg-slate-800/40 transition cursor-pointer"
            >
              <td class="p-3 font-mono font-bold text-sky-600">
                #{s.sale_number}
                {#if s.is_edited}
                  <span class="ms-1 px-1.5 py-0.5 rounded-full text-[9px] font-black uppercase bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300">{t('sale_edited_tag')}</span>
                {/if}
              </td>
              <td class="p-3 font-mono text-pos-muted">{s.created_at}</td>
              <td class="p-3 font-bold text-pos-text">{s.user_name || 'Admin'}</td>
              <td class="p-3">
                {#if s.terminal_name}
                  <span class="px-2 py-0.5 rounded-md text-[10px] font-black bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300" title={t('terminal')}><Monitor class="w-3 h-3 inline -mt-0.5 me-0.5" />{s.terminal_name}</span>
                {:else}
                  <span class="text-pos-muted">—</span>
                {/if}
              </td>
              <td class="p-3 text-pos-muted">{s.customer_name || 'Client Comptoir'}</td>
              <td class="p-3 text-end font-mono font-black text-pos-text">{s.total_amount.toLocaleString()} DZD</td>
              <td class="p-3 text-end font-mono font-black text-emerald-600">{s.paid_amount.toLocaleString()} DZD</td>
              <td class="p-3 text-center font-mono font-bold text-purple-600">{s.lines_sold ?? '—'}</td>
              <td class="p-3 text-center font-mono font-bold text-indigo-600">{s.units_sold ?? '—'}</td>
              <td class="p-3 text-center">
                <span class="px-2 py-0.5 rounded-md text-[10px] font-bold uppercase {(s.payment_method || 'cash') === 'cash' ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' : (s.payment_method || '') === 'tpe' ? 'bg-sky-100 text-sky-800' : 'bg-amber-100 text-amber-800'}">
                  {s.payment_method || 'cash'}
                </span>
              </td>
              <td class="p-3 text-center">
                <span class="px-2 py-0.5 rounded-full text-[10px] font-black uppercase {s.payment_status === 'paid' ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' : 'bg-amber-100 text-amber-800'}">
                  {s.payment_status}
                </span>
              </td>
              <td class="p-3 text-end">
                <div class="flex items-center justify-end gap-1">
                  <button
                    type="button"
                    on:click={(e) => { e.stopPropagation(); printReceipt(s); }}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title={t('sales_reprint')}
                  >
                    <Printer class="w-4 h-4" />
                  </button>
                  <button
                    type="button"
                    on:click={(e) => { e.stopPropagation(); openSaleDetails(s); }}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title={t('sales_view_details')}
                  >
                    <Eye class="w-4 h-4" />
                  </button>
                </div>
              </td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>
  </div>
</div>

<!-- Sale Details Modal -->
{#if isDetailModalOpen && selectedSale}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-2xl max-h-[90vh] flex flex-col overflow-hidden animate-in zoom-in-95 duration-150">
      <!-- Modal Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold">
            <ShoppingBag class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text flex items-center gap-2">
              <button
                type="button"
                on:click={() => copyInvoiceNumber(selectedSale!.sale_number)}
                class="hover:text-sky-600 transition cursor-pointer"
                title={t('invoice_click_to_copy')}
              >Sale Invoice #{selectedSale.sale_number}</button>
              {#if invoiceCopied}<span class="text-[10px] font-black text-emerald-600">{t('invoice_copied')}</span>{/if}
              {#if selectedSale.is_edited}
                <span class="px-2 py-0.5 rounded-full text-[10px] font-black uppercase bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300">{t('sale_edited_tag')}</span>
              {/if}
            </h3>
            <p class="text-xs text-pos-muted">{selectedSale.created_at} • Cashier: {selectedSale.user_name || 'Admin'}{selectedSale.terminal_name ? ` • ${t('terminal')}: ${selectedSale.terminal_name}` : ''}</p>
            <p class="text-[10px] text-pos-muted">{t('invoice_click_to_copy')}</p>
          </div>
        </div>
        <button on:click={() => (isDetailModalOpen = false)} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Modal Body -->
      <div class="p-6 overflow-y-auto space-y-4 flex-1">
        <!-- Receipt QR (scan to find this sale) -->
        <div class="flex items-center justify-center gap-4 p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border">
          <QrImage payload={entityQrPayload('SALE', selectedSale.sale_number)} size={110} />
          <div class="text-xs text-pos-muted font-bold">
            <p>Receipt QR / رمز الوصل</p>
            <p class="font-mono text-pos-text">{entityQrPayload('SALE', selectedSale.sale_number)}</p>
          </div>
        </div>

        <!-- Customer & Payment Summary -->
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3 p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border text-xs">
          <div>
            <span class="text-pos-muted font-bold block mb-0.5">Customer:</span>
            <span class="font-black text-pos-text">{selectedSale.customer_name || 'Client Comptoir'}</span>
          </div>
          <div>
            <span class="text-pos-muted font-bold block mb-0.5">Payment Method:</span>
            <span class="font-black capitalize text-sky-600">{selectedSale.payment_method || 'cash'}</span>
          </div>
          <div>
            <span class="text-pos-muted font-bold block mb-0.5">Status:</span>
            <span class="font-black capitalize text-emerald-600">{selectedSale.payment_status}</span>
          </div>
          <div>
            <span class="text-pos-muted font-bold block mb-0.5">Total Amount:</span>
            <span class="font-black font-mono text-pos-text">{selectedSale.total_amount.toLocaleString()} DZD</span>
          </div>
        </div>

        <!-- Items Table -->
        <div class="bg-pos-card border border-pos-border rounded-2xl overflow-hidden shadow-xs">
          <table class="w-full text-start text-xs border-collapse">
            <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold">
              <tr>
                <th class="p-2.5 text-start">Item</th>
                <th class="p-2.5 text-center">Qty</th>
                <th class="p-2.5 text-end">Unit Price</th>
                <th class="p-2.5 text-end">Line Total</th>
              </tr>
            </thead>
            <tbody class="divide-y divide-pos-border/40">
              {#if !isLoadingItems && saleItems.length > 0}
                <tr><td colspan="4" class="pb-1 text-[10px] font-bold text-pos-muted text-end">{saleItems.length} {t('pos_lines')} · {saleItems.reduce((u, i) => u + (i.quantity || 0), 0)} {t('units_total')}</td></tr>
              {/if}
              {#if isLoadingItems}
                <tr>
                  <td colspan="4" class="p-6 text-center text-pos-muted">Loading item details...</td>
                </tr>
              {:else if saleItems.length === 0}
                <tr>
                  <td colspan="4" class="p-6 text-center text-pos-muted">No line items recorded for this sale.</td>
                </tr>
              {:else}
                {#each saleItems as item}
                  <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/30">
                    <td class="p-2.5 font-bold text-pos-text">
                      <p>{item.name_fr || item.name_ar}</p>
                      <p class="text-[10px] text-pos-muted font-mono">{item.barcode || item.sku || '—'}</p>
                    </td>
                    <td class="p-2.5 text-center font-mono font-bold">{item.quantity}</td>
                    <td class="p-2.5 text-end font-mono text-pos-muted">{item.unit_price} DZD</td>
                    <td class="p-2.5 text-end font-mono font-black text-pos-text">{item.total_price} DZD</td>
                  </tr>
                {/each}
              {/if}
            </tbody>
          </table>
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-between">
        <button
          type="button"
          on:click={promptProtectedDelete}
          class="px-4 py-2 bg-rose-100 hover:bg-rose-200 text-rose-800 dark:bg-rose-950/60 dark:text-rose-300 font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer"
        >
          <Trash2 class="w-4 h-4" />
          <span>{t('btn_delete')}</span>
        </button>

        <div class="flex items-center gap-2">
          <button
            type="button"
            on:click={() => editSaleInPos(selectedSale)}
            class="px-4 py-2 bg-amber-100 hover:bg-amber-200 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300 font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer"
            title="Re-open this sale in the POS cart"
          >
            <Pencil class="w-4 h-4" />
            <span>{t('sales_edit_in_pos')}</span>
          </button>
          <button on:click={() => (isDetailModalOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">
            Close
          </button>
          <button
            on:click={() => printReceipt(selectedSale)}
            class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-md"
          >
            <Printer class="w-4 h-4" />
            <span>{t('print_receipt')}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- Protected Delete Modal (z-index above the sale-detail modal it opens from) -->
{#if isDeleteModalOpen && selectedSale}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-2xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95">
      <div class="flex items-center gap-3 text-rose-600">
        <ShieldAlert class="w-6 h-6 shrink-0" />
        <h3 class="font-black text-sm text-pos-text">Protected Sale Deletion</h3>
      </div>
      <p class="text-xs text-pos-muted">
        Are you sure you want to delete invoice <strong class="text-pos-text">#{selectedSale.sale_number}</strong>? This action updates cash drawer totals and returns stock.
      </p>

      {#if $currentUser?.role_name !== 'admin'}
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Enter Admin Authorization Password *</label>
          <input
            type="password"
            bind:value={adminPassword}
            placeholder="Password"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-mono outline-none"
          />
        </div>
      {/if}

      {#if deleteError}
        <div class="p-2 bg-rose-100 text-rose-800 text-xs font-bold rounded-lg">{deleteError}</div>
      {/if}

      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (isDeleteModalOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
          Cancel
        </button>
        <button on:click={executeProtectedDelete} disabled={isDeleting} class="px-4 py-2 bg-rose-600 text-white text-xs font-black rounded-xl cursor-pointer shadow-md">
          {isDeleting ? 'Deleting...' : 'Confirm Delete'}
        </button>
      </div>
    </div>
  </div>
{/if}
