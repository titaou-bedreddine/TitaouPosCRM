<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import type { DashboardStats } from '../../lib/types';
  import { TrendingUp, ShoppingBag, AlertTriangle, ArrowDownRight, DollarSign, Wallet, Trophy, RefreshCw, Layers, Eye, Pencil, Printer, X } from 'lucide-svelte';
  import DateQuickFilters from '../../lib/components/DateQuickFilters.svelte';
  import { printHtmlSilently, entityQrDataUrl } from '../../lib/utils/printer';
  import { buildUnifiedReceipt } from '../../lib/printing/unifiedReceipt';

  let stats: DashboardStats | null = null;
  let fromDate = new Date().toISOString().split('T')[0];
  let toDate = new Date().toISOString().split('T')[0];
  let selectedTab: 'financial' | 'debts' | 'inventory' | 'expenses' | 'versement' | 'caisse' = 'financial';

  onMount(async () => {
    await loadStats();
    await loadTabData();
  });

  // Per-tab data
  let customersList: any[] = [];
  let suppliersList: any[] = [];
  let productsList: any[] = [];
  let expensesList: any[] = [];
  let movementsList: any[] = [];
  let versementSales: any[] = [];
  // Versement ticket modal: details + re-print. The actions in the versement
  // table (view / load-in-POS / print) mutate this state.
  let versementDetail: any | null = null;
  let versementItems: any[] = [];
  let isPrintingVersement = false;

  // Props from App.svelte: hand the versement's items to the POS cart so the
  // cashier can complete the due payment there.
  export let onEditSaleInPos: (sale: any) => void = () => {};

  $: versementTotalPaid = versementSales.reduce((s2, x) => s2 + (x.paid_amount || 0), 0);
  $: versementTotalRemaining = versementSales.reduce((s2, x) => s2 + Math.max(0, (x.total_amount || 0) - (x.paid_amount || 0)), 0);

  $: totalCustomerDebt = customersList.reduce((s, c) => s + Math.max(0, c.balance || 0), 0);
  $: totalSupplierDue = suppliersList.reduce((s, x) => s + Math.max(0, x.balance || 0), 0);
  $: inventoryValue = productsList.reduce((s, p) => s + (p.purchase_price || 0) * (p.current_stock || 0), 0);
  $: lowStock = productsList.filter((p) => p.current_stock <= (p.min_stock || 0));
  $: expensesFiltered = expensesList.filter((e) => {
    if (fromDate && e.date < fromDate) return false;
    if (toDate && e.date > toDate) return false;
    return true;
  });
  $: expensesTotal = expensesFiltered.reduce((s, e) => s + e.amount, 0);

  async function loadTabData() {
    try {
      const [cs, ss, ps, es] = await Promise.all([
        invoke<any[]>('list_customers'),
        invoke<any[]>('list_suppliers'),
        invoke<any[]>('search_products', { query: '', categoryId: null, searchType: 'all' }),
        invoke<any[]>('list_expenses'),
      ]);
      customersList = cs;
      suppliersList = ss;
      productsList = ps;
      expensesList = es;
    } catch (e) {
      console.warn('Dashboard tab data:', e);
    }
    try {
      const sales = await invoke<any[]>('list_sales', {
        startDate: fromDate || null,
        endDate: toDate || null,
        userId: null,
        limit: 500,
      });
      versementSales = sales.filter((x) => x.payment_method === 'versement');
    } catch {
      versementSales = [];
    }
    try {
      const session = await invoke<any>('get_active_cash_session', { userId: 1 });
      movementsList = session
        ? await invoke<any[]>('list_cash_movements', { sessionId: session.id })
        : [];
    } catch {
      movementsList = [];
    }
  }

  // --- Versement ticket actions (view / load-in-POS / print) ---

  async function openVersementDetails(sale: any) {
    versementDetail = sale;
    versementItems = [];
    try {
      versementItems = await invoke<any[]>('get_sale_items', { saleId: sale.id });
    } catch (e) {
      console.error('Versement items:', e);
    }
  }

  function loadVersementInPos(sale: any) {
    onEditSaleInPos(sale);
    versementDetail = null;
  }

  async function printVersementTicket(sale: any) {
    if (isPrintingVersement) return;
    isPrintingVersement = true;
    try {
      const [items, appSettings] = await Promise.all([
        invoke<any[]>('get_sale_items', { saleId: sale.id }),
        invoke<Record<string, string>>('get_all_settings').catch(() => ({} as Record<string, string>)),
      ]);
      const qrDataUrl = await entityQrDataUrl(`SALE:${sale.sale_number}`, 100).catch(
        () => undefined
      );
      const built = buildUnifiedReceipt({
        qrDataUrl,
        settings: appSettings,
        saleNumber: sale.sale_number,
        saleDate: sale.created_at,
        cashierName: sale.cashier_name || 'Caisse',
        customerName: sale.customer_name || 'Client Comptoir',
        items: items.map((i) => ({
          name: i.name_fr || i.name_ar || `#${i.product_id}`,
          quantity: i.quantity,
          unitPrice: i.unit_price,
          totalPrice: i.total_price,
          discountPerUnit: i.discount_amount || 0,
          isRefund: i.is_refund || false,
        })),
        subtotal: sale.total_amount,
        discount: 0,
        grandTotal: sale.total_amount,
        amountPaid: sale.paid_amount,
        change: 0,
        paymentMethod: 'versement',
        isCredit: true,
        versementPaid: sale.paid_amount,
        versementRemaining: Math.max(0, sale.total_amount - sale.paid_amount),
      });
      await printHtmlSilently(built.html, built.title, { widthMm: built.paperWidthMm });
    } catch (e) {
      console.error('Versement print failed:', e);
    } finally {
      isPrintingVersement = false;
    }
  }

  async function loadStats() {
    try {
      stats = await invoke<DashboardStats>('get_dashboard_stats', {
        startDate: fromDate,
        endDate: toDate,
      });
    } catch (e) {
      console.error(e);
    }
  }
</script>

<div class="p-6 space-y-6 overflow-y-auto h-full select-none">
  <!-- Header with Date Filters matching screenshot -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-black text-pos-text">{t('dash_title')}</h1>
      <p class="text-xs text-pos-muted mt-1">Real-time overview of revenue, profits, margins, and inventory performance</p>
    </div>

    <div class="flex items-center gap-2 bg-pos-card border border-pos-border p-1.5 rounded-xl shadow-xs">
      <input type="date" bind:value={fromDate} on:change={loadStats} class="px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-xs font-bold text-pos-text" />
      <span class="text-xs text-pos-muted">to</span>
      <input type="date" bind:value={toDate} on:change={loadStats} class="px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-xs font-bold text-pos-text" />
      <button on:click={loadStats} class="p-1.5 bg-sky-600 text-white rounded-lg hover:bg-sky-700 cursor-pointer">
        <RefreshCw class="w-4 h-4" />
      </button>
    </div>
  </div>

  <!-- Quick Date Presets -->
  <DateQuickFilters bind:startDate={fromDate} bind:endDate={toDate} onChange={loadStats} />

  {#if stats}
    <!-- Top 6 Metric Cards matching photo_2026-08-27_18-52-00.jpg -->
    <div class="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-6 gap-3">
      <div class="bg-pos-card border border-pos-border rounded-xl p-3.5 shadow-xs">
        <span class="text-[11px] font-bold text-pos-muted">{t('dash_metric_sales')}</span>
        <div class="text-lg font-black font-mono text-sky-600 mt-1">{stats.today_sales.toLocaleString()} DZD</div>
      </div>

      <div class="bg-pos-card border border-pos-border rounded-xl p-3.5 shadow-xs">
        <span class="text-[11px] font-bold text-pos-muted">{t('dash_metric_returns')}</span>
        <div class="text-lg font-black font-mono text-amber-600 mt-1">{stats.returns_amount.toLocaleString()} DZD</div>
      </div>

      <div class="bg-pos-card border border-pos-border rounded-xl p-3.5 shadow-xs">
        <span class="text-[11px] font-bold text-pos-muted">{t('dash_metric_net')}</span>
        <div class="text-lg font-black font-mono text-pos-text mt-1">{stats.net_revenue.toLocaleString()} DZD</div>
      </div>

      <div class="bg-pos-card border border-pos-border rounded-xl p-3.5 shadow-xs">
        <span class="text-[11px] font-bold text-pos-muted">{t('dash_metric_cogs')}</span>
        <div class="text-lg font-black font-mono text-slate-500 mt-1">{stats.cost_of_goods.toLocaleString()} DZD</div>
      </div>

      <div class="bg-pos-card border border-pos-border rounded-xl p-3.5 shadow-xs">
        <span class="text-[11px] font-bold text-pos-muted">{t('dash_metric_gross')}</span>
        <div class="text-lg font-black font-mono text-emerald-600 mt-1">{stats.gross_profit.toLocaleString()} DZD</div>
      </div>

      <div class="bg-pos-card border border-pos-border rounded-xl p-3.5 shadow-xs">
        <span class="text-[11px] font-bold text-pos-muted">{t('dash_metric_basket')}</span>
        <div class="text-lg font-black font-mono text-indigo-600 mt-1">{stats.average_basket.toLocaleString()} DZD</div>
      </div>
    </div>
    {/if}

    <!-- Category Tabs matching screenshot -->
    <div class="flex items-center gap-2 border-b border-pos-border pb-2 overflow-x-auto">
      <button
        on:click={() => selectedTab = 'financial'}
        class="px-4 py-2 rounded-xl text-xs font-bold transition cursor-pointer {selectedTab === 'financial' ? 'bg-sky-600 text-white shadow-xs' : 'bg-pos-card border border-pos-border text-pos-muted'}"
      >{t('dash_tab_financial')}</button>
      <button
        on:click={() => selectedTab = 'debts'}
        class="px-4 py-2 rounded-xl text-xs font-bold transition cursor-pointer {selectedTab === 'debts' ? 'bg-sky-600 text-white shadow-xs' : 'bg-pos-card border border-pos-border text-pos-muted'}"
      >{t('dash_tab_debts')}</button>
      <button
        on:click={() => selectedTab = 'inventory'}
        class="px-4 py-2 rounded-xl text-xs font-bold transition cursor-pointer {selectedTab === 'inventory' ? 'bg-sky-600 text-white shadow-xs' : 'bg-pos-card border border-pos-border text-pos-muted'}"
      >{t('dash_tab_inventory')}</button>
      <button
        on:click={() => selectedTab = 'versement'}
        class="px-4 py-2 rounded-xl text-xs font-bold transition cursor-pointer {selectedTab === 'versement' ? 'bg-sky-600 text-white shadow-xs' : 'bg-pos-card border border-pos-border text-pos-muted'}"
      >{t('dash_tab_versement')}</button>
      <button
        on:click={() => selectedTab = 'expenses'}
        class="px-4 py-2 rounded-xl text-xs font-bold transition cursor-pointer {selectedTab === 'expenses' ? 'bg-sky-600 text-white shadow-xs' : 'bg-pos-card border border-pos-border text-pos-muted'}"
      >{t('dash_tab_expenses')}</button>
      <button
        on:click={() => selectedTab = 'caisse'}
        class="px-4 py-2 rounded-xl text-xs font-bold transition cursor-pointer {selectedTab === 'caisse' ? 'bg-sky-600 text-white shadow-xs' : 'bg-pos-card border border-pos-border text-pos-muted'}"
      >{t('dash_tab_caisse')}</button>
    </div>

    {#if selectedTab === 'financial' && stats}
    <!-- Main Analytics Content matching photo_2026-08-27_18-52-04.jpg -->
    <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
      <!-- Left Column: {t('dash_tab_financial')} -->
      <div class="space-y-4">
        <h3 class="font-extrabold text-sm text-pos-text">{t('dash_tab_financial')}</h3>

        <div class="bg-pos-card border-2 border-sky-500/40 rounded-2xl p-5 shadow-xs space-y-2">
          <span class="text-xs font-extrabold text-sky-600 uppercase tracking-wider">{t('dash_net_profit')}</span>
          <div class="text-3xl font-black font-mono text-sky-600">{stats.today_profit.toLocaleString()} DZD</div>
          <span class="inline-block text-[11px] font-bold text-sky-700 bg-sky-50 dark:bg-sky-950 px-2 py-0.5 rounded">
            {t('dash_profit_margin')}: {stats.today_sales > 0 ? Math.round((stats.today_profit / stats.today_sales) * 100) : 0}%
          </span>
        </div>

        <div class="bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-300 dark:border-emerald-800 rounded-2xl p-4 flex items-center justify-between">
          <div>
            <span class="text-[11px] font-black text-emerald-800 uppercase">{t('dash_cash_in')}</span>
            <div class="text-xl font-black font-mono text-emerald-700 mt-1">{stats.today_sales.toLocaleString()} DZD</div>
          </div>
          <div class="w-9 h-9 rounded-full bg-emerald-500 text-white flex items-center justify-center">
            <TrendingUp class="w-4 h-4" />
          </div>
        </div>

        <div class="bg-rose-50 dark:bg-rose-950/40 border border-rose-300 dark:border-rose-800 rounded-2xl p-4 flex items-center justify-between">
          <div>
            <span class="text-[11px] font-black text-rose-800 uppercase">{t('dash_cash_out')}</span>
            <div class="text-xl font-black font-mono text-rose-700 mt-1">{stats.today_expenses.toLocaleString()} DZD</div>
          </div>
          <div class="w-9 h-9 rounded-full bg-rose-500 text-white flex items-center justify-center">
            <ArrowDownRight class="w-4 h-4" />
          </div>
        </div>
      </div>

      <!-- Right Column: Top Profitable Products matching screenshot -->
      <div class="lg:col-span-2 bg-pos-card border border-pos-border rounded-2xl shadow-xs overflow-hidden flex flex-col">
        <div class="p-4 border-b border-pos-border flex items-center justify-between bg-slate-50 dark:bg-slate-800/40">
          <h3 class="font-extrabold text-xs text-pos-text flex items-center gap-2">
            <Trophy class="w-4 h-4 text-amber-500" />
            <span>{t('dash_top_products')}</span>
          </h3>
          <span class="text-[11px] font-bold text-amber-600 bg-amber-50 dark:bg-amber-950 px-2 py-0.5 rounded">
            Top 15 Products
          </span>
        </div>

        <table class="w-full text-start text-xs border-collapse">
          <thead>
            <tr class="border-b border-pos-border text-pos-muted font-bold">
              <th class="p-3 text-start">Product</th>
              <th class="p-3 text-start">Category</th>
              <th class="p-3 text-center">Sold Qty</th>
              <th class="p-3 text-end">Revenue</th>
              <th class="p-3 text-end">Cost</th>
              <th class="p-3 text-end">Profit</th>
            </tr>
          </thead>
          <tbody>
            {#if stats.top_products.length === 0}
              <tr>
                <td colspan="6" class="p-12 text-center text-pos-muted font-semibold">
                  {t('dash_no_sales_period')}
                </td>
              </tr>
            {:else}
              {#each stats.top_products as tp}
                <tr class="border-b border-pos-border/60 hover:bg-slate-50 dark:hover:bg-slate-800/40">
                  <td class="p-3 font-bold text-pos-text">{tp.product_name}</td>
                  <td class="p-3 text-pos-muted">{tp.category_name}</td>
                  <td class="p-3 text-center font-mono font-bold">{tp.sold_qty}</td>
                  <td class="p-3 text-end font-mono font-bold text-sky-600">{tp.revenue.toLocaleString()} DZD</td>
                  <td class="p-3 text-end font-mono text-pos-muted">{tp.cost.toLocaleString()} DZD</td>
                  <td class="p-3 text-end font-mono font-black text-emerald-600">{tp.profit.toLocaleString()} DZD</td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>
    </div>
  {:else if selectedTab === 'debts'}
    <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
      <div class="bg-pos-card border border-rose-200 dark:border-rose-800/60 rounded-2xl p-4 shadow-xs">
        <h3 class="font-black text-xs text-pos-text mb-3">Customer Debts (ديون الزبائن) — {totalCustomerDebt.toLocaleString()} DZD</h3>
        <div class="max-h-80 overflow-y-auto space-y-1.5">
          {#each customersList.filter(c => (c.balance || 0) > 0).slice(0, 30) as c}
            <div class="flex items-center justify-between p-2 bg-rose-50/60 dark:bg-rose-950/20 rounded-lg text-xs">
              <span class="font-bold text-pos-text truncate">{c.name}</span>
              <span class="font-mono font-black text-rose-600">{(c.balance || 0).toLocaleString()} DZD</span>
            </div>
          {/each}
          {#if customersList.filter(c => (c.balance || 0) > 0).length === 0}
            <p class="text-xs text-pos-muted text-center py-4">No customer debts.</p>
          {/if}
        </div>
      </div>
      <div class="bg-pos-card border border-amber-200 dark:border-amber-800/60 rounded-2xl p-4 shadow-xs">
        <h3 class="font-black text-xs text-pos-text mb-3">Supplier Dues (ديون الموردين) — {totalSupplierDue.toLocaleString()} DZD</h3>
        <div class="max-h-80 overflow-y-auto space-y-1.5">
          {#each suppliersList.filter(x => (x.balance || 0) > 0).slice(0, 30) as x}
            <div class="flex items-center justify-between p-2 bg-amber-50/60 dark:bg-amber-950/20 rounded-lg text-xs">
              <span class="font-bold text-pos-text truncate">{x.name}</span>
              <span class="font-mono font-black text-amber-600">{(x.balance || 0).toLocaleString()} DZD</span>
            </div>
          {/each}
          {#if suppliersList.filter(x => (x.balance || 0) > 0).length === 0}
            <p class="text-xs text-pos-muted text-center py-4">No supplier dues.</p>
          {/if}
        </div>
      </div>
    </div>
  {:else if selectedTab === 'inventory'}
    <div class="space-y-4">
      <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
        <div class="bg-pos-card border border-pos-border rounded-2xl p-4">
          <span class="text-[10px] font-bold text-pos-muted uppercase">Inventory Value (achat)</span>
          <div class="text-xl font-black font-mono text-sky-600">{inventoryValue.toLocaleString()} DZD</div>
        </div>
        <div class="bg-pos-card border border-pos-border rounded-2xl p-4">
          <span class="text-[10px] font-bold text-pos-muted uppercase">Distinct Products</span>
          <div class="text-xl font-black font-mono text-pos-text">{productsList.length}</div>
        </div>
        <div class="bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-800/60 rounded-2xl p-4">
          <span class="text-[10px] font-bold text-amber-700 uppercase">Low Stock</span>
          <div class="text-xl font-black font-mono text-amber-600">{lowStock.length}</div>
        </div>
        <div class="bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-800/60 rounded-2xl p-4">
          <span class="text-[10px] font-bold text-rose-700 uppercase">Out of Stock</span>
          <div class="text-xl font-black font-mono text-rose-600">{productsList.filter(p => (p.current_stock || 0) <= 0).length}</div>
        </div>
      </div>
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-xs overflow-hidden">
        <div class="p-3 border-b border-pos-border font-black text-xs text-pos-text bg-slate-50 dark:bg-slate-800/40">
          Products to Restock (منتجات تحتاج تعبئة) — Top 20
        </div>
        <table class="w-full text-xs">
          <thead class="bg-slate-50 dark:bg-slate-800/60 text-pos-muted font-bold">
            <tr>
              <th class="p-2.5 text-start">Product</th>
              <th class="p-2.5 text-center">Stock</th>
              <th class="p-2.5 text-center">Min</th>
              <th class="p-2.5 text-end">Status</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-pos-border/40">
            {#each lowStock.slice(0, 20) as p}
              <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/40">
                <td class="p-2.5 font-bold text-pos-text">{p.name_fr || p.name_ar}</td>
                <td class="p-2.5 text-center font-mono font-black {(p.current_stock || 0) <= 0 ? 'text-rose-600' : 'text-amber-600'}">{p.current_stock}</td>
                <td class="p-2.5 text-center font-mono text-pos-muted">{p.min_stock}</td>
                <td class="p-2.5 text-end">
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-black {(p.current_stock || 0) <= 0 ? 'bg-rose-100 text-rose-700' : 'bg-amber-100 text-amber-700'}">
                    {(p.current_stock || 0) <= 0 ? 'OUT' : 'LOW'}
                  </span>
                </td>
              </tr>
            {/each}
            {#if lowStock.length === 0}
              <tr><td colspan="4" class="p-6 text-center text-pos-muted">All products are above their minimum stock.</td></tr>
            {/if}
          </tbody>
        </table>
      </div>
    </div>
  {:else if selectedTab === 'expenses'}
    <div class="space-y-4">
      <div class="grid grid-cols-2 md:grid-cols-3 gap-3">
        <div class="bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-800/60 rounded-2xl p-4">
          <span class="text-[10px] font-bold text-rose-700 uppercase">Expenses in Range</span>
          <div class="text-xl font-black font-mono text-rose-600">{expensesTotal.toLocaleString()} DZD</div>
        </div>
        <div class="bg-pos-card border border-pos-border rounded-2xl p-4">
          <span class="text-[10px] font-bold text-pos-muted uppercase">Vouchers</span>
          <div class="text-xl font-black font-mono text-pos-text">{expensesFiltered.length}</div>
        </div>
        <div class="bg-pos-card border border-pos-border rounded-2xl p-4">
          <span class="text-[10px] font-bold text-pos-muted uppercase">Average Voucher</span>
          <div class="text-xl font-black font-mono text-pos-text">{(expensesFiltered.length > 0 ? Math.round(expensesTotal / expensesFiltered.length) : 0).toLocaleString()} DZD</div>
        </div>
      </div>
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-xs overflow-hidden">
        <table class="w-full text-xs">
          <thead class="bg-slate-50 dark:bg-slate-800/60 text-pos-muted font-bold">
            <tr>
              <th class="p-2.5 text-start">Voucher</th>
              <th class="p-2.5 text-start">Date</th>
              <th class="p-2.5 text-start">Category</th>
              <th class="p-2.5 text-start">User</th>
              <th class="p-2.5 text-end">Amount</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-pos-border/40">
            {#each expensesFiltered.slice(0, 30) as e}
              <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/40">
                <td class="p-2.5 font-mono font-bold text-rose-600">#{e.expense_number}</td>
                <td class="p-2.5 font-mono text-pos-muted">{e.date}</td>
                <td class="p-2.5 font-bold text-pos-text">{e.category_name || 'Général'}</td>
                <td class="p-2.5 text-pos-muted">{e.user_name || 'User #' + e.user_id}</td>
                <td class="p-2.5 text-end font-mono font-black text-rose-600">{e.amount.toLocaleString()} DZD</td>
              </tr>
            {/each}
            {#if expensesFiltered.length === 0}
              <tr><td colspan="5" class="p-6 text-center text-pos-muted">No expenses in the selected range.</td></tr>
            {/if}
          </tbody>
        </table>
      </div>
    </div>
  {:else if selectedTab === 'versement'}
    <div class="space-y-4">
      <div class="grid grid-cols-2 md:grid-cols-3 gap-3">
        <div class="bg-violet-50 dark:bg-violet-950/30 border border-violet-200 dark:border-violet-800/60 rounded-2xl p-4">
          <span class="text-[10px] font-bold text-violet-700 uppercase">Versement Sales</span>
          <div class="text-xl font-black font-mono text-violet-600">{versementSales.length}</div>
        </div>
        <div class="bg-emerald-50 dark:bg-emerald-950/30 border border-emerald-200 dark:border-emerald-800/60 rounded-2xl p-4">
          <span class="text-[10px] font-bold text-emerald-700 uppercase">Deposits Collected</span>
          <div class="text-xl font-black font-mono text-emerald-600">{versementTotalPaid.toLocaleString()} DZD</div>
        </div>
        <div class="bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-800/60 rounded-2xl p-4">
          <span class="text-[10px] font-bold text-amber-700 uppercase">Still Owed (goods at shop)</span>
          <div class="text-xl font-black font-mono text-amber-600">{versementTotalRemaining.toLocaleString()} DZD</div>
        </div>
      </div>
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-xs overflow-hidden">
        <table class="w-full text-xs">
          <thead class="bg-slate-50 dark:bg-slate-800/60 text-pos-muted font-bold">
            <tr>
              <th class="p-2.5 text-start">Ticket</th>
              <th class="p-2.5 text-start">Date</th>
              <th class="p-2.5 text-start">Customer</th>
              <th class="p-2.5 text-end">Total</th>
              <th class="p-2.5 text-end">Paid</th>
              <th class="p-2.5 text-end">Remaining</th>
              <th class="p-2.5 text-end">{t('actions')}</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-pos-border/40">
            {#each versementSales.slice(0, 30) as x}
              <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/40">
                <td class="p-2.5 font-mono font-bold text-violet-600">#{x.sale_number}</td>
                <td class="p-2.5 font-mono text-pos-muted">{x.created_at}</td>
                <td class="p-2.5 font-bold text-pos-text">{x.customer_name || 'Client Comptoir'}</td>
                <td class="p-2.5 text-end font-mono font-bold">{x.total_amount.toLocaleString()}</td>
                <td class="p-2.5 text-end font-mono font-black text-emerald-600">{x.paid_amount.toLocaleString()}</td>
                <td class="p-2.5 text-end font-mono font-black text-amber-600">{Math.max(0, x.total_amount - x.paid_amount).toLocaleString()}</td>
                <td class="p-2.5 text-end">
                  <div class="flex items-center justify-end gap-1">
                    <button
                      type="button"
                      on:click={() => openVersementDetails(x)}
                      class="p-1 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                      title="View details"
                    >
                      <Eye class="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      on:click={() => loadVersementInPos(x)}
                      class="p-1 text-pos-muted hover:text-amber-600 rounded-lg cursor-pointer"
                      title="Load in POS to complete payment / sell / cancel"
                    >
                      <Pencil class="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      on:click={() => printVersementTicket(x)}
                      class="p-1 text-pos-muted hover:text-emerald-600 rounded-lg cursor-pointer"
                      title="Print ticket"
                    >
                      <Printer class="w-3.5 h-3.5" />
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
            {#if versementSales.length === 0}
              <tr><td colspan="7" class="p-6 text-center text-pos-muted">{t('dash_no_versements')}</td></tr>
            {/if}
          </tbody>
        </table>
      </div>
    </div>
  {:else if selectedTab === 'caisse'}
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-xs overflow-hidden">
      <div class="p-3 border-b border-pos-border font-black text-xs text-pos-text bg-slate-50 dark:bg-slate-800/40">
        Register Movements — Active Session (حركات الصندوق)
      </div>
      <table class="w-full text-xs">
        <thead class="bg-slate-50 dark:bg-slate-800/60 text-pos-muted font-bold">
          <tr>
            <th class="p-2.5 text-start">Time</th>
            <th class="p-2.5 text-start">Type</th>
            <th class="p-2.5 text-start">Reason</th>
            <th class="p-2.5 text-end">Amount</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-pos-border/40">
          {#each movementsList.slice(0, 50) as m}
            <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/40">
              <td class="p-2.5 font-mono text-pos-muted">{m.created_at}</td>
              <td class="p-2.5 font-bold uppercase">{m.type_name || m.type}</td>
              <td class="p-2.5 text-pos-text truncate">{m.reason || '-'}</td>
              <td class="p-2.5 text-end font-mono font-black {m.amount < 0 ? 'text-rose-600' : 'text-emerald-600'}">
                {m.amount.toLocaleString()} DZD
              </td>
            </tr>
          {/each}
          {#if movementsList.length === 0}
            <tr><td colspan="4" class="p-6 text-center text-pos-muted">No active session movements.</td></tr>
          {/if}
        </tbody>
      </table>
    </div>
  {/if}
</div>
<!-- Versement Ticket Details -->
{#if versementDetail}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-md p-6 space-y-4">
      <div class="flex items-start justify-between">
        <div>
          <h3 class="font-black text-sm text-pos-text">Versement #{versementDetail.sale_number}</h3>
          <p class="text-xs text-pos-muted">{versementDetail.created_at} • {versementDetail.customer_name || 'Client Comptoir'}</p>
        </div>
        <button on:click={() => (versementDetail = null)} class="p-1.5 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>
      <div class="grid grid-cols-3 gap-2 text-xs">
        <div class="p-2.5 bg-violet-50 dark:bg-violet-950/30 rounded-xl text-center border border-violet-200 dark:border-violet-800/60">
          <span class="text-[9px] font-bold text-violet-700 uppercase block">Total</span>
          <span class="font-black font-mono text-violet-600">{versementDetail.total_amount.toLocaleString()}</span>
        </div>
        <div class="p-2.5 bg-emerald-50 dark:bg-emerald-950/30 rounded-xl text-center border border-emerald-200 dark:border-emerald-800/60">
          <span class="text-[9px] font-bold text-emerald-700 uppercase block">Paid</span>
          <span class="font-black font-mono text-emerald-600">{versementDetail.paid_amount.toLocaleString()}</span>
        </div>
        <div class="p-2.5 bg-amber-50 dark:bg-amber-950/30 rounded-xl text-center border border-amber-200 dark:border-amber-800/60">
          <span class="text-[9px] font-bold text-amber-700 uppercase block">Remaining</span>
          <span class="font-black font-mono text-amber-600">{Math.max(0, versementDetail.total_amount - versementDetail.paid_amount).toLocaleString()}</span>
        </div>
      </div>
      <div class="max-h-48 overflow-y-auto space-y-1">
        {#each versementItems as it}
          <div class="flex items-center justify-between p-2 bg-slate-50 dark:bg-slate-800/40 rounded-lg text-xs">
            <span class="font-bold text-pos-text truncate">{it.name_fr || it.name_ar || '#' + it.product_id}</span>
            <span class="font-mono text-pos-muted">x{it.quantity}</span>
            <span class="font-mono font-black text-pos-text">{it.total_price.toLocaleString()} DZD</span>
          </div>
        {/each}
        {#if versementItems.length === 0}
          <p class="text-xs text-pos-muted text-center py-3">{t('no_data')}</p>
        {/if}
      </div>
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button
          type="button"
          on:click={() => { printVersementTicket(versementDetail); }}
          disabled={isPrintingVersement}
          class="px-4 py-2 bg-emerald-600 hover:bg-emerald-700 disabled:opacity-50 text-white text-xs font-black rounded-xl cursor-pointer"
        >
          {t('print')}
        </button>
        <button on:click={() => (versementDetail = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">
          {t('btn_close')}
        </button>
      </div>
    </div>
  </div>
{/if}
