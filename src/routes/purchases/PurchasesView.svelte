<script lang="ts">
  import QrImage from '../../lib/components/QrImage.svelte';
  import { onMount, tick } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { get } from 'svelte/store';
  import { entityQrPayload } from '../../lib/utils/printer';
  import { sortRows, clickSort } from '../../lib/utils/tableSort';
  import type { Purchase, Supplier, Product } from '../../lib/types';
  import { currentUser } from '../../lib/stores/auth';
  import { printHtmlSilently } from '../../lib/utils/printer';
  import DateQuickFilters from '../../lib/components/DateQuickFilters.svelte';
  import ProductEditModal from '../../lib/components/ProductEditModal.svelte';
  import type { Category, Unit } from '../../lib/types';
  import {
    FileSpreadsheet, Plus, Trash2, CheckCircle2, Search, Package,
    Printer, Eye, Edit3, X, Tag, DollarSign, ArrowRight, Truck,
    TrendingUp, AlertCircle, ShieldAlert
  } from 'lucide-svelte';

  let purchases: Purchase[] = [];
  // Deep-link: a scanned purchase QR (PUR:<invoice>) opens this invoice's
  // preview as soon as the list has loaded.
  export let focusInvoice: string | null = null;
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
    return sortDir === 'asc' ? '▲' : '▼';
  }

  let suppliers: Supplier[] = [];
  let products: Product[] = [];

  let isCreateOpen = false;
  let selectedSupplierId: number | null = null;
  let invoiceNumber = '';
  let invoiceDate = new Date().toISOString().split('T')[0];
  let paidAmount = 0;
  let paymentMethod = 'cash';
  let notes = '';

  // 1-character live search
  let searchQuery = '';
  let liveSearchResults: Product[] = [];
  let isSearchDropdownOpen = false;

  interface ItemRow {
    product_id: number;
    name: string;
    barcode: string;
    quantity: number;
    unit_cost: number;
    sale_price: number;
    total: number;
  }

  let items: ItemRow[] = [];
  let isSaving = false;
  let errorMsg = '';
  let previewPurchase: Purchase | null = null;

  // Editing a saved invoice: loads its items into the create modal so the
  // cashier can fix quantities/prices and re-save as a corrected invoice.
  let editingPurchase: Purchase | null = null;
  let isPrintingPurchase = false;

  async function editPurchaseInvoice(pur: Purchase) {
    try {
      const savedItems = await invoke<any[]>('get_purchase_items', { purchaseId: pur.id });
      const mapped = savedItems.map((it) => ({
        product_id: it.product_id,
        name: it.product_name || it.product_name_ar || '#' + it.product_id,
        barcode: '',
        quantity: it.quantity,
        unit_cost: it.unit_cost,
        sale_price: it.sale_price || it.unit_cost,
        total: it.total,
      }));
      editingPurchase = pur;
      isCreateOpen = true;
      items = mapped;
      invoiceNumber = pur.invoice_number + '-C';
      invoiceDate = pur.date;
      selectedSupplierId = pur.supplier_id;
      paidManuallyEdited = true;
      paidAmount = pur.paid_amount;
    } catch (e: any) {
      alert('Failed to load invoice for editing: ' + (e.message || e));
    }
  }

  // Pay an invoice's due: a dialog asks the amount (prefilled with the
  // remaining due) instead of silently emptying the drawer.
  let payDialogPurchase: Purchase | null = null;
  let payDialogAmount = 0;
  let payDialogErr = '';

  function quickPaySupplierDue(pur: Purchase) {
    payDialogPurchase = pur;
    payDialogAmount = pur.total - pur.paid_amount;
    payDialogErr = '';
  }

  async function confirmPaySupplierDue() {
    if (!payDialogPurchase) return;
    if (payDialogAmount <= 0) {
      payDialogErr = 'Enter a valid amount / المبلغ غير صالح';
      return;
    }
    try {
      const { activeSession } = await import('../../lib/stores/session');
      const { currentUser } = await import('../../lib/stores/auth');
      const me = get(currentUser);
      await invoke('record_supplier_debt_payment', {
        input: {
          supplier_id: payDialogPurchase.supplier_id,
          amount: payDialogAmount,
          payment_method: 'cash',
          reference: payDialogPurchase.invoice_number,
          session_id: get(activeSession)?.id ?? null,
          user_id: me?.id || 1,
          notes: `Payment of invoice ${payDialogPurchase.invoice_number} / تسديد الفاتورة`,
        },
      });
      payDialogPurchase = null;
      await loadData();
    } catch (e: any) {
      payDialogErr = 'Payment failed: ' + (typeof e === 'string' ? e : e.message || e);
    }
  }

  async function printPurchaseInvoice(pur: Purchase) {
    try {
      isPrintingPurchase = true;
      const items = await invoke<any[]>('get_purchase_items', { purchaseId: pur.id });
      const settings = await invoke<Record<string, string>>('get_all_settings');
      const shopName = settings['shop_name_fr'] || 'TitaouPOS';
      const rows = items.map((it) => `
        <tr>
          <td style="padding:3px 4px;border-bottom:1px dashed #000;">${it.product_name || it.product_name_ar || '#' + it.product_id}</td>
          <td style="text-align:center;padding:3px 4px;border-bottom:1px dashed #000;">${it.quantity}</td>
          <td style="text-align:right;padding:3px 4px;border-bottom:1px dashed #000;">${it.unit_cost.toLocaleString()}</td>
          <td style="text-align:right;padding:3px 4px;border-bottom:1px dashed #000;">${it.total.toLocaleString()}</td>
        </tr>`).join('');
      const html = `
        <div style="width:80mm;font-family:monospace;font-size:11px;padding:4mm;">
          <p style="text-align:center;font-weight:900;font-size:15px;margin:0;">${shopName}</p>
          <p style="text-align:center;font-size:10px;margin:2px 0;">BON D'ACHAT / سند شراء</p>
          <hr style="border-top:1px dashed #000;" />
          <div style="display:flex;justify-content:space-between;font-size:10px;">
            <span>N° <strong>${pur.invoice_number}</strong></span>
            <span>${pur.date}</span>
          </div>
          <div style="display:flex;justify-content:space-between;font-size:10px;">
            <span>Fournisseur: <strong>${pur.supplier_name || '-'}</strong></span>
          </div>
          <hr style="border-top:1px dashed #000;" />
          <table style="width:100%;border-collapse:collapse;font-size:10px;">
            <thead>
              <tr><th style="text-align:left;">Article</th><th style="text-align:center;">Qté</th><th style="text-align:right;">P.U</th><th style="text-align:right;">Total</th></tr>
            </thead>
            <tbody>${rows}</tbody>
          </table>
          <div style="display:flex;justify-content:space-between;font-size:13px;font-weight:900;margin-top:6px;">
            <span>TOTAL:</span><span>${pur.total.toLocaleString()} DZD</span>
          </div>
          <div style="display:flex;justify-content:space-between;font-size:10px;">
            <span>Payé:</span><span>${pur.paid_amount.toLocaleString()} DZD</span>
          </div>
          <p style="text-align:center;font-size:8px;margin-top:8px;">TitaouPOS • Titaou Bedreddine</p>
        </div>
      `;
      const { printHtmlSilently } = await import('../../lib/utils/printer');
      printHtmlSilently(html, 'Purchase ' + pur.invoice_number);
    } catch (e: any) {
      alert('Print failed: ' + (e.message || e));
    } finally {
      isPrintingPurchase = false;
    }
  }

  // Inline product creation from the invoice screen.
  let isProductCreateOpen = false;
  let modalCategories: Category[] = [];
  let modalUnits: Unit[] = [];

  // Quick date-range filter for the loaded purchase list.
  let filterStartDate = '';
  let listSearch = '';
  let filterEndDate = '';

  // Invoice details preview: items + delete-with-password.
  let previewItems: any[] = [];
  let isLoadingPreview = false;
  let isDeletePurchaseOpen = false;
  let deletePassword = '';
  let deleteErrorMsg = '';
  let isDeletingPurchase = false;

  // Omni filter: invoice number, supplier, exact amount, or invoice QR.
  $: filteredPurchases = purchases.filter((p) => {
    if (filterStartDate && p.date < filterStartDate) return false;
    if (filterEndDate && p.date > filterEndDate) return false;
    const q = listSearch.trim().toLowerCase();
    if (q) {
      const stripped = q.startsWith('pur:') ? q.slice(4) : q;
      const qr = entityQrPayload('PUR', p.invoice_number).toLowerCase();
      const hit =
        p.invoice_number.toLowerCase().includes(stripped) ||
        (p.supplier_name || '').toLowerCase().includes(stripped) ||
        String(p.total) === stripped ||
        qr === q ||
        qr.includes(stripped);
      if (!hit) return false;
    }
    return true;
  });
  $: sortedPurchases = sortRows(filteredPurchases, sortKey, sortDir, filteredPurchases);

  onMount(async () => {
    await loadData();
  });

  // Deep-link from a scanned purchase QR: once the list lands, open the
  // matching invoice's preview (print / edit / delete live there).
  $: if (focusInvoice && purchases.length > 0) {
    const target = purchases.find((p) => p.invoice_number === focusInvoice);
    if (target) {
      openPreview(target);
      focusInvoice = null;
    } else {
      focusInvoice = null;
    }
  }

  async function loadData() {
    try {
      purchases = await invoke<Purchase[]>('list_purchases');
      suppliers = await invoke<Supplier[]>('list_suppliers');
      if (suppliers.length > 0 && !selectedSupplierId) {
        selectedSupplierId = suppliers[0].id;
      }
      products = await invoke<Product[]>('search_products', { query: '', categoryId: null, searchType: 'all' });
      modalCategories = await invoke<Category[]>('get_categories');
      modalUnits = await invoke<Unit[]>('get_units');
    } catch (e) {
      console.error(e);
    }
  }

  $: if (searchQuery.trim().length > 0) {
    const q = searchQuery.trim().toLowerCase();
    liveSearchResults = products
      .filter(p =>
        (p.name_fr && p.name_fr.toLowerCase().includes(q)) ||
        (p.name_ar && p.name_ar.toLowerCase().includes(q)) ||
        (p.sku && p.sku.toLowerCase().includes(q)) ||
        (p.barcodes && p.barcodes.some(b => b.toLowerCase().includes(q)))
      )
      .slice(0, 8);
    isSearchDropdownOpen = liveSearchResults.length > 0;
  } else {
    liveSearchResults = [];
    isSearchDropdownOpen = false;
  }

  async function selectProductFromSearch(p: Product) {
    searchQuery = '';
    isSearchDropdownOpen = false;
    await addItemAndFocus(p);
  }

  async function addItemAndFocus(p: Product) {
    const existingIndex = items.findIndex(i => i.product_id === p.id);
    let targetIndex = 0;
    if (existingIndex > -1) {
      items[existingIndex].quantity += 1;
      items[existingIndex].total = items[existingIndex].quantity * items[existingIndex].unit_cost;
      items = [...items];
      targetIndex = existingIndex;
    } else {
      items = [...items, {
        product_id: p.id,
        name: p.name_fr || p.name_ar,
        barcode: (p.barcodes && p.barcodes[0]) || p.sku || '',
        quantity: 1,
        unit_cost: p.purchase_price,
        sale_price: p.sale_price,
        total: p.purchase_price,
      }];
      targetIndex = items.length - 1;
    }
    updateTotals();

    await tick();
    const qtyInput = document.getElementById(`qty-${targetIndex}`) as HTMLInputElement;
    if (qtyInput) {
      qtyInput.focus();
      qtyInput.select();
    }
  }

  function handleQtyKeyDown(e: KeyboardEvent, idx: number) {
    if (e.key === 'Enter') {
      e.preventDefault();
      const costInput = document.getElementById(`cost-${idx}`) as HTMLInputElement;
      if (costInput) {
        costInput.focus();
        costInput.select();
      }
    }
  }

  function handleCostKeyDown(e: KeyboardEvent, idx: number) {
    if (e.key === 'Enter') {
      e.preventDefault();
      const saleInput = document.getElementById(`sale-${idx}`) as HTMLInputElement;
      if (saleInput) {
        saleInput.focus();
        saleInput.select();
      }
    }
  }

  function handleSaleKeyDown(e: KeyboardEvent, idx: number) {
    if (e.key === 'Enter') {
      e.preventDefault();
      // After the last column of a row, Enter chains to the NEXT row's
      // quantity — invoice entry flows straight down the lines. On the final
      // row it returns to the product search to scan the next product.
      const nextQty = document.getElementById(`qty-${idx + 1}`) as HTMLInputElement;
      if (nextQty) {
        nextQty.focus();
        nextQty.select();
      } else {
        const searchInput = document.getElementById('purchase-omni-input') as HTMLInputElement;
        if (searchInput) {
          searchInput.focus();
          searchInput.select();
        }
      }
    }
  }

  function removeItem(index: number) {
    items = items.filter((_, i) => i !== index);
    updateTotals();
  }

  // Invoice fields are DZD integers: swallow non-digit keys outright.
  function digitsOnly(e: KeyboardEvent) {
    if (
      e.key.length === 1 &&
      !/[0-9]/.test(e.key) &&
      !e.ctrlKey && !e.metaKey && !e.altKey
    ) {
      e.preventDefault();
    }
  }

  // Keep the bound numeric value clean even on paste/autofill.
  function sanitizeNumber(e: Event) {
    const input = e.target as HTMLInputElement;
    const digits = input.value.replace(/[^0-9]/g, '');
    if (input.value !== digits) {
      input.value = digits;
      input.dispatchEvent(new Event('input'));
    }
  }

  function updateTotals() {
    items = items.map(i => ({ ...i, total: i.quantity * i.unit_cost }));
    // Paid-to-supplier tracks the invoice total until the cashier edits it
    // manually (typing or a preset); after that their choice sticks.
    // Compute the total LOCALLY: the reactive `subtotal` still holds the
    // previous render's value here, which made the field lag one edit
    // behind (e.g. 10000 shown for a 100000 invoice).
    if (!paidManuallyEdited) {
      paidAmount = items.reduce((sum, i) => sum + i.total, 0);
    }
  }

  $: subtotal = items.reduce((sum, i) => sum + i.total, 0);
  $: total = subtotal;
  // Estimated revenue if the whole invoice is sold at the entered sale
  // prices (small digits under the invoice total).
  $: estSaleValue = items.reduce((sum, i) => sum + (i.sale_price || 0) * i.quantity, 0);

  // Cashier overrides the paid default by typing or picking a preset.
  let paidManuallyEdited = false;

  function setPaidPercent(pct: number) {
    paidManuallyEdited = true;
    paidAmount = Math.round((total * pct) / 100);
  }

  function markPaidEdited() {
    paidManuallyEdited = true;
  }

  async function handleCreatePurchase() {
    if (!selectedSupplierId || items.length === 0) {
      errorMsg = 'Please add products and select supplier / الرجاء إضافة منتجات واختيار المورد';
      return;
    }
    try {
      isSaving = true;
      errorMsg = '';
      const invNum = invoiceNumber || `PUR-${Date.now().toString().slice(-6)}`;
      await invoke('create_purchase', {
        input: {
          invoice_number: invNum,
          supplier_id: selectedSupplierId,
          user_id: $currentUser?.id || 1,
          date: invoiceDate,
          subtotal: subtotal,
          discount: 0,
          tax: 0,
          total: total,
          paid_amount: paidAmount,
          payment_method: paymentMethod,
          notes: notes || 'Facture Achat',
          items: items.map(i => ({
            product_id: i.product_id,
            quantity: i.quantity,
            unit_cost: i.unit_cost,
            discount: 0,
            tax: 0,
            total: i.total,
            expiry_date: null,
            batch_number: null,
          })),
        }
      });

      isCreateOpen = false;
      items = [];
      invoiceNumber = '';
      paidAmount = 0;
      paidManuallyEdited = false;
      await loadData();
    } catch (e: any) {
      errorMsg = typeof e === 'string' ? e : e.message || 'Failed to save purchase';
    } finally {
      isSaving = false;
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

  async function openPreview(pur: Purchase) {
    previewPurchase = pur;
    previewItems = [];
    try {
      isLoadingPreview = true;
      previewItems = await invoke<any[]>('get_purchase_items', { purchaseId: pur.id });
    } catch (e) {
      console.error(e);
    } finally {
      isLoadingPreview = false;
    }
  }

  function promptDeletePurchase() {
    deleteErrorMsg = '';
    deletePassword = '';
    isDeletePurchaseOpen = true;
  }

  async function executeDeletePurchase() {
    if (!previewPurchase) return;
    try {
      isDeletingPurchase = true;
      deleteErrorMsg = '';
      // Deleting an invoice reverses stock — admin authorization required.
      const ok = await invoke<boolean>('verify_admin_password', { password: deletePassword });
      if (!ok) {
        deleteErrorMsg = 'Invalid password / كلمة المرور غير صحيحة';
        return;
      }
      await invoke('delete_purchase', { purchaseId: previewPurchase.id, userId: $currentUser?.id });
      isDeletePurchaseOpen = false;
      previewPurchase = null;
      await loadData();
    } catch (e: any) {
      deleteErrorMsg = typeof e === 'string' ? e : e.message || 'Failed to delete invoice';
    } finally {
      isDeletingPurchase = false;
    }
  }

  // Barcode scan into the purchase search: Enter on a matching product adds
  // the line immediately (same behavior as the POS cart).
  async function handleSearchEnter() {
    const code = searchQuery.trim();
    if (!code) return;
    try {
      const list = await invoke<Product[]>('search_products', {
        query: code,
        categoryId: null,
        searchType: 'barcode',
      });
      const matched = list.find((p) => p.barcodes?.includes(code) || p.sku === code);
      if (matched) {
        searchQuery = '';
        isSearchDropdownOpen = false;
        await addItemAndFocus(matched);
      }
    } catch (e) {
      console.error('Purchase scan lookup failed:', e);
    }
  }

  async function printLabelsForPurchase() {
    if (items.length === 0) return;
    const html = items.map(i => `
      <div style="width: 40mm; height: 20mm; padding: 2mm; border: 1px solid #000; text-align: center; page-break-after: always; display: flex; flex-direction: column; justify-content: space-between;">
        <p style="font-size: 8px; font-weight: bold; margin: 0; overflow: hidden; white-space: nowrap;">${i.name}</p>
        <p style="font-size: 11px; font-weight: 900; margin: 0;">${i.sale_price.toLocaleString()} DZD</p>
        <p style="font-size: 7px; font-family: monospace; margin: 0;">${i.barcode}</p>
      </div>
    `).join('');
    const r = await printHtmlSilently(html, 'Price Tags', { widthMm: 40, heightMm: 20 });
    if (!r.ok) alert('Print failed: ' + r.message);
  }
</script>

<div class="h-full flex flex-col bg-pos-bg p-4 overflow-hidden select-none space-y-4">
  <!-- Header -->
  <div class="flex items-center justify-between pb-3 border-b border-pos-border shrink-0">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-2xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <FileSpreadsheet class="w-5 h-5" />
      </div>
      <div>
        <h1 class="text-xl font-black text-pos-text tracking-tight">Purchases & Invoices / المشتريات وفواتير الشراء</h1>
        <p class="text-xs text-pos-muted">Manage supplier purchases, stock replenishment & price tags</p>
      </div>
    </div>

    <button
      type="button"
      on:click={() => {
        isCreateOpen = true;
        errorMsg = '';
        // Each new invoice starts empty — no leftovers from the last one.
        items = [];
        paidManuallyEdited = false;
        paidAmount = 0;
      }}
      class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white rounded-xl text-xs font-black transition shadow-xs flex items-center gap-2 cursor-pointer active:scale-95"
    >
      <Plus class="w-4 h-4" />
      <span>{t('pur_new')}</span>
    </button>
  </div>

  <!-- Quick Date Presets + Omni Search -->
  <div class="flex items-center gap-3 flex-wrap">
    <DateQuickFilters bind:startDate={filterStartDate} bind:endDate={filterEndDate} onChange={() => {}} />
    <input
      type="text"
      bind:value={listSearch}
      placeholder="Search: #, supplier, amount, or scan invoice QR..."
      class="flex-1 min-w-[220px] px-3 py-1.5 bg-pos-card border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none"
    />
  </div>

  <!-- Purchases List Table -->
  <div class="flex-1 overflow-y-auto bg-pos-card border border-pos-border rounded-2xl shadow-xs">
    <table class="w-full text-start text-xs border-collapse">
      <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold sticky top-0 z-10">
        <tr>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('invoice_number')}>{t('pur_invoice_num')} {sortIndicator('invoice_number')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('date')}>{t('pur_col_date')} {sortIndicator('date')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('supplier_name')}>{t('pur_supplier_col')} {sortIndicator('supplier_name')}</th>
          <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('total')}>{t('pur_col_total')} {sortIndicator('total')}</th>
          <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('paid_amount')}>{t('pur_paid_col')} {sortIndicator('paid_amount')}</th>
          <th class="p-3 text-center">{t('pur_status')}</th>
          <th class="p-3 text-end">Actions</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-pos-border/40">
        {#if purchases.length === 0}
          <tr>
            <td colspan="7" class="p-8 text-center text-pos-muted">No purchase invoices recorded yet.</td>
          </tr>
        {:else}
          {#each sortedPurchases as pur}
            <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/40 transition">
              <td class="p-3 font-mono font-bold text-sky-600">#{pur.invoice_number}</td>
              <td class="p-3 font-mono text-pos-muted">{pur.date}</td>
              <td class="p-3 font-bold text-pos-text">{pur.supplier_name || 'Fournisseur Inconnu'}</td>
              <td class="p-3 text-end font-mono font-black text-pos-text">{pur.total.toLocaleString()} DZD</td>
              <td class="p-3 text-end font-mono font-black text-emerald-600">{pur.paid_amount.toLocaleString()} DZD</td>
              <td class="p-3 text-center">
                <span class="px-2 py-0.5 rounded-full text-[10px] font-black uppercase {pur.paid_amount >= pur.total ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' : 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300'}">
                  {pur.paid_amount >= pur.total ? 'Paid' : 'Credit / Dette'}
                </span>
              </td>
              <td class="p-3 text-end">
                <div class="flex items-center justify-end gap-1">
                  <button on:click={() => openPreview(pur)} class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer" title="View Details">
                    <Eye class="w-4 h-4" />
                  </button>
                  {#if pur.paid_amount < pur.total}
                    <button
                      type="button"
                      on:click={() => quickPaySupplierDue(pur)}
                      class="px-2 py-1 bg-emerald-600 hover:bg-emerald-700 text-white text-[10px] font-black rounded-lg cursor-pointer transition active:scale-95"
                      title="Pay the remaining due from the drawer"
                    >
                      {(pur.total - pur.paid_amount).toLocaleString()} DZD Due (تسديد)
                    </button>
                  {/if}
                </div>
              </td>
            </tr>
          {/each}
        {/if}
      </tbody>
    </table>
  </div>
</div>

<!-- Modal: New Purchase Invoice -->
{#if isCreateOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-4xl max-h-[92vh] flex flex-col overflow-hidden animate-in zoom-in-95 duration-150">
      <!-- Modal Header -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold">
            <FileSpreadsheet class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">New Purchase Invoice / فاتورة مشتريات جديدة</h3>
            <p class="text-xs text-pos-muted">Scan products, edit quantities, and auto-print shelf barcode tags</p>
          </div>
        </div>
        <button on:click={() => (isCreateOpen = false)} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      {#if errorMsg}
        <div class="mx-6 mt-4 p-3 bg-rose-100 dark:bg-rose-950 text-rose-800 dark:text-rose-200 text-xs font-bold rounded-xl border border-rose-300">
          {errorMsg}
        </div>
      {/if}

      <!-- Modal Body -->
      <div class="p-6 overflow-y-auto space-y-4 flex-1">
        <!-- Supplier, Invoice # & Date Row -->
        <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Supplier / المورد *</label>
            <select bind:value={selectedSupplierId} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none">
              {#each suppliers as s}
                <option value={s.id}>{s.name} ({s.phone || 'No phone'})</option>
              {/each}
            </select>
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Invoice / Bon Number</label>
            <input data-no-autoselect type="text" bind:value={invoiceNumber} placeholder="Ex: ACH-2026-001" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Date</label>
            <input type="date" bind:value={invoiceDate} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
          </div>
        </div>

        <!-- 1-Character Live Search Bar -->
        <div class="relative">
          <div class="flex items-center gap-2 p-2 bg-slate-100 dark:bg-slate-800 rounded-2xl border border-pos-border">
            <Search class="w-4 h-4 text-pos-muted ml-2" />
            <input
              id="purchase-omni-input"
              type="text"
              data-scanner-input
              bind:value={searchQuery}
              on:keydown={(e) => e.key === 'Enter' && handleSearchEnter()}
              placeholder="{t('pur_search_hint')}"
              class="w-full bg-transparent border-0 text-xs font-bold text-pos-text outline-none"
            />
            {#if searchQuery}
              <button on:click={() => (searchQuery = '')} class="text-pos-muted hover:text-pos-text p-1"><X class="w-3.5 h-3.5" /></button>
            {/if}
            <button
              type="button"
              on:click={() => (isProductCreateOpen = true)}
              class="px-2.5 py-1.5 bg-sky-600 hover:bg-sky-700 text-white text-[11px] font-black rounded-xl shrink-0 flex items-center gap-1 cursor-pointer transition active:scale-95"
              title="Create a new product (إضافة منتج جديد)"
            >
              <Plus class="w-3.5 h-3.5" />
              <span class="hidden sm:inline">New Product</span>
            </button>
          </div>

          <!-- Live Instant Search Dropdown Cards -->
          {#if isSearchDropdownOpen}
            <div class="absolute left-0 right-0 top-full mt-1 bg-pos-card border border-pos-border rounded-2xl shadow-xl z-30 max-h-60 overflow-y-auto p-2 grid grid-cols-1 md:grid-cols-2 gap-2 animate-in fade-in">
              {#each liveSearchResults as p}
                <button
                  type="button"
                  on:click={() => selectProductFromSearch(p)}
                  class="flex items-center justify-between p-2.5 rounded-xl hover:bg-sky-50 dark:hover:bg-sky-950/60 border border-pos-border text-start transition cursor-pointer"
                >
                  <div class="min-w-0">
                    <p class="text-xs font-black text-pos-text truncate">{p.name_fr || p.name_ar}</p>
                    <p class="text-[10px] text-pos-muted font-mono">{p.barcodes?.[0] || p.sku || 'No barcode'} • Stock: {p.current_stock}</p>
                  </div>
                  <div class="text-end shrink-0 ml-2">
                    <p class="text-xs font-black text-sky-600 font-mono">{p.purchase_price} DZD</p>
                    <span class="text-[9px] text-emerald-600 font-bold">Sale: {p.sale_price}</span>
                  </div>
                </button>
              {/each}
            </div>
          {/if}
        </div>

        <!-- Items Table -->
        <div class="bg-pos-card border border-pos-border rounded-2xl overflow-hidden shadow-xs">
          <table class="w-full text-start text-xs border-collapse">
            <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold">
              <tr>
                <th class="p-2.5 text-start">Product</th>
                <th class="p-2.5 text-center w-24">Qty (Qté)</th>
                <th class="p-2.5 text-center w-28">Purchase Cost</th>
                <th class="p-2.5 text-center w-28">Sale Price</th>
                <th class="p-2.5 text-end w-28">Total Cost</th>
                <th class="p-2.5 text-center w-12"></th>
              </tr>
            </thead>
            <tbody class="divide-y divide-pos-border/40">
              {#if items.length === 0}
                <tr>
                  <td colspan="6" class="p-6 text-center text-pos-muted">Scan or type in the search bar above to add products.</td>
                </tr>
              {:else}
                {#each items as item, idx}
                  <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/30 transition">
                    <td class="p-2.5 font-bold text-pos-text">
                      <p>{item.name}</p>
                      <p class="text-[10px] text-pos-muted font-mono">{item.barcode}</p>
                    </td>
                    <td class="p-2.5 text-center">
                      <input
                        id={`qty-${idx}`}
                        type="number"
                        min="1"
                        inputmode="numeric"
                        bind:value={item.quantity}
                        on:input={updateTotals}
                        on:keydown={(e) => { digitsOnly(e); handleQtyKeyDown(e, idx); }}
                        class="w-20 px-2 py-1 text-center bg-slate-100 dark:bg-slate-800 border-0 rounded-lg font-mono font-black text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
                      />
                    </td>
                    <td class="p-2.5 text-center">
                      <input
                        id={`cost-${idx}`}
                        type="number"
                        min="0"
                        inputmode="numeric"
                        bind:value={item.unit_cost}
                        on:input={updateTotals}
                        on:keydown={(e) => { digitsOnly(e); handleCostKeyDown(e, idx); }}
                        class="w-24 px-2 py-1 text-center bg-slate-100 dark:bg-slate-800 border-0 rounded-lg font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
                      />
                    </td>
                    <td class="p-2.5 text-center">
                      <input
                        id={`sale-${idx}`}
                        type="number"
                        min="0"
                        inputmode="numeric"
                        bind:value={item.sale_price}
                        on:keydown={(e) => { digitsOnly(e); handleSaleKeyDown(e, idx); }}
                        class="w-24 px-2 py-1 text-center bg-slate-100 dark:bg-slate-800 border-0 rounded-lg font-mono font-bold text-sky-600 outline-none focus:ring-2 focus:ring-sky-500"
                      />
                    </td>
                    <td class="p-2.5 text-end font-mono font-black text-pos-text">
                      {item.total.toLocaleString()} DZD
                    </td>
                    <td class="p-2.5 text-center">
                      <button on:click={() => removeItem(idx)} class="text-pos-muted hover:text-rose-600 p-1 cursor-pointer">
                        <Trash2 class="w-4 h-4" />
                      </button>
                    </td>
                  </tr>
                {/each}
              {/if}
            </tbody>
          </table>
        </div>

        <!-- Settlement & Totals Grid -->
        <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border flex flex-col md:flex-row items-center justify-between gap-4">
          <div class="space-y-1.5 w-full md:w-auto">
            <label class="block text-xs font-bold text-pos-muted">Paid to Supplier / المبلغ المدفوع للمورد</label>
            <div class="flex items-center gap-2">
              <input
                type="number"
                min="0"
                max={total}
                inputmode="numeric"
                bind:value={paidAmount}
                on:input={markPaidEdited}
                on:keydown={digitsOnly}
                on:blur={sanitizeNumber}
                class="w-36 px-3 py-1.5 bg-pos-card border border-pos-border rounded-xl text-sm font-mono font-black text-emerald-600 outline-none"
              />
              <div class="flex items-center gap-1">
                <button type="button" on:click={() => setPaidPercent(0)} class="px-2 py-1 bg-slate-200 dark:bg-slate-700 text-[10px] font-bold rounded-lg cursor-pointer">0 (دين)</button>
                <button type="button" on:click={() => setPaidPercent(25)} class="px-2 py-1 bg-slate-200 dark:bg-slate-700 text-[10px] font-bold rounded-lg cursor-pointer">25%</button>
                <button type="button" on:click={() => setPaidPercent(50)} class="px-2 py-1 bg-slate-200 dark:bg-slate-700 text-[10px] font-bold rounded-lg cursor-pointer">Half (النصف)</button>
                <button type="button" on:click={() => setPaidPercent(75)} class="px-2 py-1 bg-slate-200 dark:bg-slate-700 text-[10px] font-bold rounded-lg cursor-pointer">75%</button>
                <button type="button" on:click={() => setPaidPercent(100)} class="px-2 py-1 bg-slate-200 dark:bg-slate-700 text-[10px] font-bold rounded-lg cursor-pointer">All (الكل)</button>
              </div>
            </div>
          </div>

          <div class="text-end">
            <p class="text-xs text-pos-muted font-bold">Total Invoice:</p>
            <p class="text-2xl font-black font-mono text-sky-600">{total.toLocaleString()} DZD</p>
            {#if estSaleValue > 0}
              <p class="text-[10px] font-bold text-emerald-600 dark:text-emerald-400 flex items-center justify-end gap-1">
                {t('est_sale_price')}: <span class="font-mono">{estSaleValue.toLocaleString()} DZD</span>
                {#if estSaleValue > total}
                  <span class="font-mono text-emerald-500">(+{(estSaleValue - total).toLocaleString()})</span>
                {/if}
              </p>
            {/if}
          </div>
        </div>
      </div>

      <!-- Modal Footer -->
      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-between">
        <button
          type="button"
          on:click={printLabelsForPurchase}
          class="px-4 py-2 bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 text-pos-text font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-xs"
        >
          <Tag class="w-4 h-4" />
          <span>Auto-Print Shelf Tags (طباعة ملصقات الأسعار)</span>
        </button>

        <div class="flex items-center gap-2">
          <button on:click={() => (isCreateOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">
            Cancel
          </button>
          <button
            on:click={handleCreatePurchase}
            disabled={isSaving}
            class="px-6 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5"
          >
            <CheckCircle2 class="w-4 h-4" />
            <span>{isSaving ? 'Saving...' : 'Save Invoice (حفظ الفاتورة)'}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
<!-- Purchase Invoice Preview Modal -->
{#if previewPurchase}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-2xl max-h-[90vh] flex flex-col overflow-hidden animate-in zoom-in-95 duration-150">
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div>
          <h3 class="font-black text-base text-pos-text flex items-center gap-2">
            <button
              type="button"
              on:click={() => copyInvoiceNumber(previewPurchase!.invoice_number)}
              class="hover:text-sky-600 transition cursor-pointer"
              title={t('invoice_click_to_copy')}
            >Purchase Invoice #{previewPurchase.invoice_number}</button>
            {#if invoiceCopied}<span class="text-[10px] font-black text-emerald-600">{t('invoice_copied')}</span>{/if}
          </h3>
          <p class="text-xs text-pos-muted">{previewPurchase.date} • {previewPurchase.supplier_name || '—'} • Total {previewPurchase.total.toLocaleString()} DZD (Paid {previewPurchase.paid_amount.toLocaleString()})</p>
          <p class="text-[10px] text-pos-muted">{t('invoice_click_to_copy')}</p>
        </div>
        <button on:click={() => (previewPurchase = null)} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="p-6 overflow-y-auto flex-1 space-y-4">
        <div class="flex items-center justify-center gap-4 p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border">
          <QrImage payload={entityQrPayload('PUR', previewPurchase.invoice_number)} size={110} />
          <div class="text-xs text-pos-muted font-bold">
            <p>Invoice QR / رمز الفاتورة</p>
            <p class="font-mono text-pos-text">{entityQrPayload('PUR', previewPurchase.invoice_number)}</p>
          </div>
        </div>
        <table class="w-full text-start text-xs border-collapse">
          <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold">
            <tr>
              <th class="p-2.5 text-start">Product</th>
              <th class="p-2.5 text-center">Qty</th>
              <th class="p-2.5 text-end">Unit Cost</th>
              <th class="p-2.5 text-end">Line Total</th>
            </tr>
          </thead>
          <tbody class="divide-y divide-pos-border/40">
            {#if !isLoadingPreview && previewItems.length > 0}
              <tr><td colspan="4" class="pb-1 text-[10px] font-bold text-pos-muted text-end">{previewItems.length} {t('pos_lines')} · {previewItems.reduce((u, i) => u + (i.quantity || 0), 0)} {t('units_total')}</td></tr>
            {/if}
            {#if isLoadingPreview}
              <tr><td colspan="4" class="p-6 text-center text-pos-muted">Loading items...</td></tr>
            {:else if previewItems.length === 0}
              <tr><td colspan="4" class="p-6 text-center text-pos-muted">No line items recorded.</td></tr>
            {:else}
              {#each previewItems as it}
                <tr>
                  <td class="p-2.5 font-bold text-pos-text">{it.product_name || it.product_name_ar || '#' + it.product_id}</td>
                  <td class="p-2.5 text-center font-mono font-bold">{it.quantity}</td>
                  <td class="p-2.5 text-end font-mono">{it.unit_cost.toLocaleString()} DZD</td>
                  <td class="p-2.5 text-end font-mono font-black">{it.total.toLocaleString()} DZD</td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-between">
        <div class="flex items-center gap-2">
          <button
            type="button"
            on:click={() => { if (previewPurchase) editPurchaseInvoice(previewPurchase); previewPurchase = null; }}
            class="px-4 py-2 bg-amber-100 hover:bg-amber-200 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300 font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer"
          >
            <Edit3 class="w-4 h-4" />
            <span>Load for Correction (تعديل)</span>
          </button>
          <button
            type="button"
            on:click={() => previewPurchase && printPurchaseInvoice(previewPurchase)}
            disabled={isPrintingPurchase}
            class="px-4 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer"
          >
            <Printer class="w-4 h-4" />
            <span>{isPrintingPurchase ? 'Printing...' : 'Print (طباعة)'}</span>
          </button>
          <button
            type="button"
            on:click={promptDeletePurchase}
            class="px-4 py-2 bg-rose-100 hover:bg-rose-200 text-rose-800 dark:bg-rose-950/60 dark:text-rose-300 font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer"
          >
            <Trash2 class="w-4 h-4" />
            <span>Delete (حذف)</span>
          </button>
        </div>
        <button on:click={() => (previewPurchase = null)} class="px-5 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">
          Close
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Protected Purchase Delete Modal (above the preview) -->
{#if isDeletePurchaseOpen && previewPurchase}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-2xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95">
      <div class="flex items-center gap-3 text-rose-600">
        <ShieldAlert class="w-6 h-6 shrink-0" />
        <h3 class="font-black text-sm text-pos-text">Protected Purchase Deletion</h3>
      </div>
      <p class="text-xs text-pos-muted">
        Delete invoice <strong class="text-pos-text">#{previewPurchase.invoice_number}</strong>?
        Its stock is returned and the supplier balance is reversed.
      </p>
      <div>
        <label class="block text-xs font-bold text-pos-muted mb-1">Enter Admin Authorization Password *</label>
        <input
          type="password"
          bind:value={deletePassword}
          placeholder="Password"
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-mono outline-none"
        />
      </div>
      {#if deleteErrorMsg}
        <div class="p-2 bg-rose-100 text-rose-800 text-xs font-bold rounded-lg">{deleteErrorMsg}</div>
      {/if}
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (isDeletePurchaseOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
          Cancel
        </button>
        <button on:click={executeDeletePurchase} disabled={isDeletingPurchase} class="px-4 py-2 bg-rose-600 text-white text-xs font-black rounded-xl cursor-pointer shadow-md">
          {isDeletingPurchase ? 'Deleting...' : 'Confirm Delete'}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Inline Product Creation -->
<ProductEditModal
  isOpen={isProductCreateOpen}
  product={null}
  categories={modalCategories}
  units={modalUnits}
  initialBarcode={''}
  extraBarcode={''}
  onClose={() => (isProductCreateOpen = false)}
  onSaved={async () => {
    products = await invoke<Product[]>('search_products', { query: '', categoryId: null, searchType: 'all' });
  }}
/>

<!-- Pay Supplier Due Dialog -->
{#if payDialogPurchase}
  <div class="fixed inset-0 z-[65] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-sm p-6 space-y-4">
      <h3 class="font-black text-sm text-pos-text">Pay Supplier (تسديد للمورد)</h3>
      <p class="text-xs text-pos-muted">
        Invoice <strong class="text-pos-text">#{payDialogPurchase.invoice_number}</strong> —
        {payDialogPurchase.supplier_name || '—'} • Due: {(payDialogPurchase.total - payDialogPurchase.paid_amount).toLocaleString()} DZD
      </p>
      {#if payDialogErr}
        <div class="p-2 bg-rose-100 text-rose-700 text-xs font-bold rounded-lg">{payDialogErr}</div>
      {/if}
      <div>
        <label class="block text-xs font-bold text-pos-muted mb-1">Amount from drawer (DZD)</label>
        <input
          type="number"
          inputmode="numeric"
          min="0"
          bind:value={payDialogAmount}
          on:focus={(e) => (e.target as HTMLInputElement).select()}
          class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-lg font-mono font-black text-emerald-600 outline-none focus:ring-2 focus:ring-emerald-500"
        />
      </div>
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (payDialogPurchase = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={confirmPaySupplierDue} class="px-5 py-2 bg-emerald-600 hover:bg-emerald-700 text-white text-xs font-black rounded-xl cursor-pointer shadow-md">Confirm Payment</button>
      </div>
    </div>
  </div>
{/if}

