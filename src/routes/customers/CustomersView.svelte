<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { normalizeBarcode } from '../../lib/utils/barcode';
  import type { Customer, CustomerStats } from '../../lib/types';
  import { printHtmlSilently, entityQrPayload, entityQrDataUrl } from '../../lib/utils/printer';
  import { printService } from '../../lib/services/printService';
  import { refreshCustomers } from '../../lib/stores/customers';
  import { currentUser } from '../../lib/stores/auth';
  import { sortRows, clickSort } from '../../lib/utils/tableSort';
  import CustomerDebtModal from '../../lib/components/CustomerDebtModal.svelte';
  import UniversalSearchBar from '../../lib/components/UniversalSearchBar.svelte';
  import QrImage from '../../lib/components/QrImage.svelte';
  import DeletionRequestsView from '../deletionrequests/DeletionRequestsView.svelte';
  import {
    Users, Plus, QrCode, DollarSign, Edit2, Trash2, Search, X, Check, Printer, FileText, Phone, MapPin, Building, History,
    Eye, ShieldAlert, Pin, PinOff, Mail, BadgeCheck, Undo2, TrendingUp, ShoppingBag, FileWarning
  } from 'lucide-svelte';

  let customers: Customer[] = [];
  let searchQuery = '';
  // AZERTY-normalized mirror of the search box (scanners may emit & é " ...).
  $: searchQueryN = normalizeBarcode(searchQuery).toLowerCase();
  let searchType: 'all' | 'name' | 'barcode' | 'price' | 'qr' = 'all';

  // Deletion requests (field client-delete requests) live on this page now
  // — admin-only, opened as an overlay.
  let isDeletionRequestsOpen = false;
  // Eye-row map button: resolve this local customer's CRM id, then jump to
  // the Live Map focused on its position + detail card.
  export let onShowClientOnMap: (crmClientId: string) => void = () => {};

  // Page summary tiles.
  $: clientsTotalDebt = customers.reduce((s, c) => s + Math.max(0, c.balance || 0), 0);
  $: clientsTotalSales = customers.reduce((s, c) => s + (c.total_purchases || 0), 0);
  $: clientsWithDebt = customers.filter((c) => (c.balance || 0) > 0).length;

  // SETTLE (clear to zero) — for debts AND negative credits (Fev-type
  // anomalies): admin-gated forgiveness via clear_customer_debt, archived.
  let settleCustomer: Customer | null = null;
  let settlePassword = '';
  let settleReason = '';
  let settleError = '';
  let settleBusy = false;

  function openSettle(c: Customer) {
    settleCustomer = c;
    settlePassword = '';
    settleReason = '';
    settleError = '';
  }

  async function confirmSettle() {
    if (!settleCustomer || !$currentUser) return;
    settleBusy = true;
    settleError = '';
    try {
      await invoke('clear_customer_debt', {
        customerId: settleCustomer.id,
        reason: settleReason || null,
        adminPassword: settlePassword,
        userId: $currentUser.id,
      });
      settleCustomer = null;
      await loadCustomers();
      if (previewCustomer) {
        const fresh = customers.find((c) => c.id === previewCustomer!.id);
        if (fresh) previewCustomer = fresh;
      }
    } catch (e: any) {
      settleError = typeof e === 'string' ? e : e?.message || 'Settle failed';
    } finally {
      settleBusy = false;
    }
  }

  async function showClientOnMap(c: Customer) {
    try {
      const res = await invoke<any>('cloud_client_id_for_customer', { customerId: c.id });
      if (res?.crm_id) {
        onShowClientOnMap(res.crm_id);
      } else {
        console.warn('not linked to CRM yet');
      }
    } catch (e) {
      console.warn('map link failed:', e);
    }
  }

  let isModalOpen = false;
  let isDebtModalOpen = false;
  let selectedCustomer: Customer | null = null;
  let previewCustomer: Customer | null = null;
  let previewCustomerStats: CustomerStats | null = null;
  let customerToDelete: Customer | null = null;
  let deletePassword = '';
  let deleteErrorMsg = '';
  let isDeletingCustomer = false;
  // Clicking a sale in the customer's history opens it on the Sales page.
  export let onOpenSale: (sale: any) => void = () => {};

  async function toggleCustomerPin(c: Customer) {
    try {
      await invoke('toggle_customer_pin', { customerId: c.id, pinned: !(c as any).pinned });
      await loadCustomers();
    } catch (e) {
      console.warn('Pin failed:', e);
    }
  }

  async function confirmDeleteCustomer() {
    if (!customerToDelete || !$currentUser) return;
    try {
      isDeletingCustomer = true;
      deleteErrorMsg = '';
      const ok = await invoke<boolean>('verify_admin_password', { password: deletePassword });
      if (!ok) {
        deleteErrorMsg = 'Invalid password / كلمة المرور غير صحيحة';
        return;
      }
      await invoke('delete_customer', { customerId: customerToDelete.id });
      customerToDelete = null;
      await loadCustomers();
    } catch (e: any) {
      deleteErrorMsg = typeof e === 'string' ? e : e.message || 'Delete failed';
    } finally {
      isDeletingCustomer = false;
    }
  }

  // Full customer card: shop header, details, debts and QR.
  // Uses the existing silent-print infrastructure.
  async function printCustomerCard(c: Customer) {
    let shopName = 'Titaou One';
    let shopPhone = '';
    let shopAddress = '';
    try {
      const settings = await invoke<Record<string, string>>('get_all_settings');
      shopName = settings['shop_name_fr'] || shopName;
      shopPhone = settings['shop_phone'] || '';
      shopAddress = settings['shop_address'] || '';
    } catch {
      // defaults stand
    }
    const code = c.qr_code || 'CUST-' + c.id;
    const balance = (c.balance || 0).toLocaleString();
    const parts = [];
    parts.push('<div style="width:80mm;font-family:monospace;font-size:11px;padding:4mm;text-align:center;">');
    parts.push('<p style="font-size:15px;font-weight:900;margin:0;">' + shopName + '</p>');
    parts.push('<p style="font-size:9px;margin:2px 0;">' + shopAddress + ' &bull; ' + shopPhone + '</p>');
    parts.push('<hr style="border-top:1px dashed #000;margin:5px 0;" />');
    parts.push('<p style="font-size:13px;font-weight:900;margin:4px 0;">CLIENT CARD / بطاقة زبون</p>');
    parts.push('<p style="font-weight:900;font-size:12px;margin:3px 0;">' + c.name + '</p>');
    if (c.phone) parts.push('<p style="margin:2px 0;">Tel: ' + c.phone + '</p>');
    if (c.rc) parts.push('<p style="margin:2px 0;font-size:10px;">RC: ' + c.rc + (c.nif ? ' | NIF: ' + c.nif : '') + '</p>');
    const qrDataUrl = await entityQrDataUrl(entityQrPayload('CUST', code), 300);
    parts.push('<img src="' + qrDataUrl + '" alt="QR" style="width:38mm;height:38mm;margin:5px auto;" />');
    parts.push('<p style="font-size:10px;font-family:monospace;">' + code + '</p>');
    parts.push('<hr style="border-top:1px dashed #000;margin:5px 0;" />');
    parts.push('<p style="font-size:13px;font-weight:900;margin:2px 0;">DETTES: ' + balance + ' DZD</p>');
    parts.push('<p style="font-size:9px;margin-top:6px;">Titaou One &bull; ' + shopName + '</p>');
    parts.push('</div>');
    const r = await printHtmlSilently(parts.join('\n'), 'Client Card ' + c.name, { widthMm: 80 });
    if (!r.ok) console.error('Client card print failed:', r.message);
  }

  let customerHistory: any[] = [];
  // Lifetime aggregates for the profile popup's stat tiles.
  let customerStats: CustomerStats | null = null;

  /// Opens the profile popup: identity, stat tiles, history, QR.
  async function openCustomerPreview(c: Customer) {
    previewCustomer = c;
    customerStats = null;
    loadCustomerHistory(c);
    try {
      customerStats = await invoke<CustomerStats>('get_customer_stats', { customerId: c.id });
    } catch {
      customerStats = null; // tiles fall back to "—"
    }
  }

  function closeCustomerPreview() {
    previewCustomer = null;
    customerStats = null;
  }

  // Sales history for the previewed customer: LOCAL sales (POS counter)
  // PLUS the sales made on the FIELD app, fetched from the CRM by the
  // customer's mapped id. POS-pushed CRM rows carry pos_ref — those are
  // skipped (the local rows above already show them; no double listing).
  async function loadCustomerHistory(c: Customer) {
    let local: any[] = [];
    try {
      const sales = await invoke<any[]>('list_sales', {
        startDate: null,
        endDate: null,
        userId: null,
        limit: 500,
      });
      local = sales.filter((s) => s.customer_id === c.id).slice(0, 50);
    } catch {
      local = [];
    }
    let cloud: any[] = [];
    try {
      const res = await invoke<any>('cloud_client_id_for_customer', { customerId: c.id });
      if (res?.crm_id) {
        const rows = await invoke<any[]>('cloud_client_orders', { crmClientId: res.crm_id });
        cloud = (rows ?? [])
          .filter((o) => !o.pos_ref)
          .map((o) => ({
            id: 'crm-' + o.id,
            sale_number: o.order_number ?? String(o.id).slice(0, 8),
            created_at: o.created_at,
            total_amount: o.total_amount ?? 0,
            payment_status: o.amount_due > 0 && (o.payment_status || '') !== 'paid' ? 'credit' : (o.payment_status || 'paid'),
          }));
      }
    } catch {
      cloud = [];
    }
    const seen = new Set(local.map((s) => String(s.sale_number)));
    customerHistory = [...local, ...cloud]
      .sort((a, b) => String(b.created_at || '').localeCompare(String(a.created_at || '')))
      .slice(0, 60);
  }

  let name = '';
  let phone = '';
  let email = '';
  let address = '';
  let rc = '';
  let nif = '';
  let nis = '';
  let ai = '';
  let initialDebt = 0;
  let notes = '';
  let editingId: number | null = null;

  let isSaving = false;
  let errorMsg = '';

  onMount(async () => {
    await loadCustomers();
  });

  async function loadCustomers() {
    try {
      customers = await invoke<Customer[]>('list_customers');
    } catch (e) {
      console.error(e);
    }
  }

    $: filteredCustomers = customers.filter((x) => {
    // Omni search: name, phone, email, ids, exact balance, or QR payload.
    const q = searchQueryN;
    if (!q) return true;
    const hit = ['name', 'phone', 'email', 'rc', 'nif', 'code', 'qr_code', ].some(
      (f) => String((x as any)[f] || '').toLowerCase().includes(q)
    );
    if (hit) return true;
    if (String(Math.max(0, x.balance || 0)) === q) return true;
    const qr = ('CUST:' + (x.qr_code || 'CUST-' + x.id)).toLowerCase();
    return qr === q || qr.includes(q);
  });

  // Three-state column sort (DESC → ASC → default), like the Stock page.
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
  $: sortedCustomers = sortRows(filteredCustomers, sortKey, sortDir, filteredCustomers);

  function openAddModal() {
    editingId = null;
    name = '';
    phone = '';
    email = '';
    address = '';
    rc = '';
    nif = '';
    nis = '';
    ai = '';
    initialDebt = 0;
    notes = '';
    errorMsg = '';
    isModalOpen = true;
  }

  function openEditModal(c: Customer) {
    editingId = c.id;
    name = c.name;
    phone = c.phone || '';
    email = c.email || '';
    address = c.address || '';
    rc = c.rc || '';
    nif = c.nif || '';
    nis = c.nis || '';
    ai = c.ai || '';
    initialDebt = 0;
    notes = c.notes || '';
    errorMsg = '';
    isModalOpen = true;
  }

  async function handleSave() {
    if (!name.trim()) {
      errorMsg = 'Customer name is required / اسم العميل مطلوب';
      return;
    }
    try {
      isSaving = true;
      errorMsg = '';
      await invoke('save_customer', {
        name,
        phone: phone || null,
        email: email || null,
        address: address || null,
        rc: rc || null,
        nif: nif || null,
        nis: nis || null,
        ai: ai || null,
        initialDebt,
        notes: notes || null,
        customerId: editingId,
      });

      isModalOpen = false;
      await loadCustomers();
    } catch (e: any) {
      errorMsg = typeof e === 'string' ? e : e.message || 'Failed to save customer';
    } finally {
      isSaving = false;
    }
  }

  async function printCustomerDebtRecap(c: Customer) {
    try {
      const debts: any[] = (await invoke('get_customer_debts', { customerId: c.id }).catch(() => [])) as any[];
      const r = await printService.printCustomerDebtRecap(c, debts);
      if (!r.ok && r.mode !== 'disabled') {
        console.warn('Customer debt recap print result:', r.message);
      }
    } catch (e: any) {
      console.error('Customer debt recap print failed:', e);
    }
  }

  // After a debt payment the popup stays open, so every surface inside it
  // must agree: the balance card (fresh customer row) and the Debts tile.
  async function refreshAfterCustomerPayment() {
    await loadCustomers();
    refreshCustomers();
    if (previewCustomer) {
      const fresh = customers.find((c) => c.id === previewCustomer!.id);
      if (fresh) previewCustomer = fresh;
      if (previewCustomer) {
        customerStats = null;
        try {
          customerStats = await invoke<CustomerStats>('get_customer_stats', { customerId: previewCustomer.id });
        } catch {
          customerStats = null;
        }
      }
    }
  }
</script>

<div class="h-full flex flex-col bg-pos-bg p-4 overflow-hidden select-none">
  <!-- Header -->
  <div class="flex items-center justify-between pb-4 border-b border-pos-border shrink-0">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-2xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <Users class="w-5 h-5" />
      </div>
      <div>
        <h1 class="text-xl font-black text-pos-text tracking-tight">Customers & Debts / العملاء والديون</h1>
        <p class="text-xs text-pos-muted">Manage customer records, credit balances, and debt payment receipts</p>
      </div>
    </div>

    <div class="flex items-center gap-2">
      {#if $currentUser && (($currentUser as any).role_id === 1 || ($currentUser as any).role_id === 4 || ($currentUser as any).role_name === 'Administrator' || ($currentUser as any).role_name === 'Owner')}
        <button
          type="button"
          on:click={() => (isDeletionRequestsOpen = true)}
          class="px-4 py-2 bg-amber-600 hover:bg-amber-700 text-white rounded-xl text-xs font-black transition shadow-xs flex items-center gap-2 cursor-pointer active:scale-95"
        >
          <FileWarning class="w-4 h-4" />
          <span>Deletion Requests (طلبات الحذف)</span>
        </button>
      {/if}
      <button
        type="button"
        on:click={openAddModal}
        class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white rounded-xl text-xs font-black transition shadow-xs flex items-center gap-2 cursor-pointer active:scale-95"
      >
        <Plus class="w-4 h-4" />
        <span>Add Customer (إضافة عميل جديد)</span>
      </button>
    </div>
  </div>

  <!-- Search Filter Bar -->
  <div class="mt-4 mb-2">
    <div class="relative">
      <Search class="w-4 h-4 text-pos-muted absolute start-3 top-2.5" />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder="Search by customer name, phone, RC, or NIF..."
        class="w-full ps-9 pe-8 py-2 bg-pos-card border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none focus:border-sky-500 shadow-xs"
      />
      {#if searchQuery}
        <button
          type="button"
          on:click={() => (searchQuery = '')}
          class="absolute end-2.5 top-2 text-pos-muted hover:text-rose-500 rounded-full p-0.5 cursor-pointer"
          title="Clear"
        ><X class="w-4 h-4" /></button>
      {/if}
    </div>
  </div>

  <!-- Page summary tiles -->
  <div class="grid grid-cols-2 lg:grid-cols-4 gap-3 mb-3 shrink-0">
    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs">
      <p class="text-[10px] font-black text-pos-muted uppercase">Clients / العملاء</p>
      <p class="text-base font-black font-mono text-pos-text">{customers.length}</p>
    </div>
    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs">
      <p class="text-[10px] font-black text-pos-muted uppercase">Clients with debt</p>
      <p class="text-base font-black font-mono text-amber-600">{clientsWithDebt}</p>
    </div>
    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs">
      <p class="text-[10px] font-black text-pos-muted uppercase">Total Debts</p>
      <p class="text-base font-black font-mono text-rose-600">{clientsTotalDebt.toLocaleString()} DZD</p>
    </div>
    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs">
      <p class="text-[10px] font-black text-pos-muted uppercase">Total Sales</p>
      <p class="text-base font-black font-mono text-sky-600">{clientsTotalSales.toLocaleString()} DZD</p>
    </div>
  </div>

  <!-- Customers Table -->
  <div class="flex-1 overflow-y-auto bg-pos-card border border-pos-border rounded-2xl shadow-xs">
    <table class="w-full text-start text-xs border-collapse">
      <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold sticky top-0 z-10">
        <tr>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('name')}>Customer Name / الاسم {sortIndicator('name')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('phone')}>Phone {sortIndicator('phone')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('rc')}>RC / NIF {sortIndicator('rc')}</th>
          <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('balance')}>Current Debt (الديون) {sortIndicator('balance')}</th>
          <th class="p-3 text-center">QR Code</th>
          <th class="p-3 text-end">Actions</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-pos-border/40">
        {#if filteredCustomers.length === 0}
          <tr>
            <td colspan="6" class="p-8 text-center text-pos-muted">No customers found.</td>
          </tr>
        {:else}
          {#each sortedCustomers as c}
            <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/40 transition">
              <td class="p-3 font-bold text-pos-text cursor-pointer" on:click={() => openCustomerPreview(c)}>
                {c.name}
              </td>
              <td class="p-3 font-mono text-pos-muted">{c.phone || '—'}</td>
              <td class="p-3 font-mono text-pos-muted">{c.rc ? `${c.rc} / ${c.nif || ''}` : '—'}</td>
              <td class="p-3 text-end font-mono font-black {c.balance > 0 ? 'text-rose-600' : c.balance < 0 ? 'text-violet-600' : 'text-emerald-600'}" title={c.balance < 0 ? 'Credit — the shop owes this client (anomaly: paid then sale deleted/refunded)' : ''}>
                {c.balance.toLocaleString()} DZD{#if c.balance < 0}<span class="ms-1 text-[9px] font-black uppercase">credit</span>{/if}
              </td>
              <td class="p-3 text-center">
                <button
                  type="button"
                  on:click={() => printCustomerDebtRecap(c)}
                  class="p-1 text-sky-600 hover:bg-sky-50 dark:hover:bg-sky-950 rounded-lg cursor-pointer transition"
                  title="Print Debt QR Recap"
                >
                  <QrCode class="w-4 h-4 mx-auto" />
                </button>
              </td>
              <td class="p-3 text-end">
                <div class="flex items-center justify-end gap-1">
                  {#if c.balance > 0}
                    <button
                      type="button"
                      on:click={() => { selectedCustomer = c; isDebtModalOpen = true; }}
                      class="px-2.5 py-1 bg-amber-600 hover:bg-amber-700 text-white rounded-lg text-[11px] font-bold cursor-pointer transition shadow-xs"
                    >
                      Regler Dette
                    </button>
                  {:else if c.balance < 0}
                    <button
                      type="button"
                      on:click={() => openSettle(c)}
                      class="px-2.5 py-1 bg-violet-600 hover:bg-violet-700 text-white rounded-lg text-[11px] font-bold cursor-pointer transition shadow-xs"
                    >
                      Settle (تصفير)
                    </button>
                  {/if}
                  <button
                    type="button"
                    on:click={() => toggleCustomerPin(c)}
                    class="p-1.5 rounded-lg cursor-pointer transition {(c as any).pinned ? 'bg-amber-500 text-white' : 'text-pos-muted hover:text-amber-500'}"
                    title="Pin to top (تثبيت)"
                  >
                    {#if (c as any).pinned}
                      <PinOff class="w-3.5 h-3.5" />
                    {:else}
                      <Pin class="w-3.5 h-3.5" />
                    {/if}
                  </button>
                  <button
                    type="button"
                    on:click={() => openCustomerPreview(c)}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title="View details"
                  >
                    <Eye class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    on:click={() => showClientOnMap(c)}
                    class="p-1.5 text-pos-muted hover:text-emerald-600 rounded-lg cursor-pointer"
                    title="Show on the Live Map (الموقع على الخريطة)"
                  >
                    <MapPin class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    on:click={() => openEditModal(c)}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title="Edit"
                  >
                    <Edit2 class="w-3.5 h-3.5" />
                  </button>
                  {#if c.id !== 1}
                    <button
                      type="button"
                      on:click={() => { customerToDelete = c; deletePassword = ''; deleteErrorMsg = ''; }}
                      class="p-1.5 text-pos-muted hover:text-rose-600 rounded-lg cursor-pointer"
                      title="Delete (admin password)"
                    >
                      <Trash2 class="w-3.5 h-3.5" />
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

<!-- Modal: Add / Edit Customer -->
{#if isModalOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-xl overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col">
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold">
            <Users class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">
              {editingId ? 'Edit Customer / تعديل عميل' : 'New Customer / إضافة عميل جديد'}
            </h3>
            <p class="text-xs text-pos-muted">Enter personal, business & legal details</p>
          </div>
        </div>
        <button on:click={() => (isModalOpen = false)} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      {#if errorMsg}
        <div class="mx-6 mt-4 p-3 bg-rose-100 text-rose-800 text-xs font-bold rounded-xl">{errorMsg}</div>
      {/if}

      <div class="p-6 space-y-3 overflow-y-auto max-h-[70vh]">
        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Full Name / الاسم الكامل *</label>
            <input type="text" bind:value={name} placeholder="Ex: Mohamed Amine" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500" />
          </div>
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Phone / الهاتف</label>
            <input type="text" bind:value={phone} placeholder="Ex: 0550 12 34 56" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500" />
          </div>
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Email / البريد الإلكتروني</label>
            <input type="email" bind:value={email} placeholder="Ex: client@email.com" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none" />
          </div>
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Address / العنوان</label>
            <input type="text" bind:value={address} placeholder="Ex: Alger Centre" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none" />
          </div>
        </div>

        <div class="p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-2">
          <span class="text-xs font-black text-pos-text block">Legal & Fiscal Details (بيانات السجل الجبائي)</span>
          <div class="grid grid-cols-2 md:grid-cols-4 gap-2">
            <div>
              <label class="block text-[10px] font-bold text-pos-muted mb-0.5">RC</label>
              <input type="text" bind:value={rc} class="w-full px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-xs font-mono" />
            </div>
            <div>
              <label class="block text-[10px] font-bold text-pos-muted mb-0.5">NIF</label>
              <input type="text" bind:value={nif} class="w-full px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-xs font-mono" />
            </div>
            <div>
              <label class="block text-[10px] font-bold text-pos-muted mb-0.5">NIS</label>
              <input type="text" bind:value={nis} class="w-full px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-xs font-mono" />
            </div>
            <div>
              <label class="block text-[10px] font-bold text-pos-muted mb-0.5">AI</label>
              <input type="text" bind:value={ai} class="w-full px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-xs font-mono" />
            </div>
          </div>
        </div>

        {#if !editingId}
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Initial Starting Debt / الرصيد الافتتاحي السابق (DZD)</label>
            <input type="number" min="0" bind:value={initialDebt} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
          </div>
        {/if}
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-end gap-2">
        <button on:click={() => (isModalOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Cancel</button>
        <button on:click={handleSave} disabled={isSaving} class="px-6 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
          <Check class="w-4 h-4" />
          <span>{isSaving ? 'Saving...' : 'Save Customer (حفظ)'}</span>
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Modal: Customer Profile Details Popup (mirrors the supplier popup) -->
{#if previewCustomer}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-3xl overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col">
      <!-- Header: identity + permanent data (contact & tax info) -->
      <div class="px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-start justify-between gap-3">
          <div class="flex items-center gap-3 min-w-0">
            <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold shrink-0">
              <Users class="w-5 h-5" />
            </div>
            <div class="min-w-0">
              <h3 class="font-black text-base text-pos-text truncate">{previewCustomer.name}</h3>
              <p class="text-xs text-pos-muted">Customer Code: #CUST-{previewCustomer.id}</p>
            </div>
          </div>
          <button on:click={closeCustomerPreview} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer shrink-0">
            <X class="w-5 h-5" />
          </button>
        </div>
        <!-- Permanent data strip: contacts + commercial & tax info -->
        <div class="mt-3 grid grid-cols-2 md:grid-cols-3 gap-x-4 gap-y-1 text-[11px]">
          <div class="flex items-center gap-1.5 min-w-0">
            <Phone class="w-3 h-3 text-sky-500 shrink-0" />
            <span class="font-mono font-bold text-pos-text truncate">{previewCustomer.phone || '—'}</span>
          </div>
          <div class="flex items-center gap-1.5 min-w-0">
            <Mail class="w-3 h-3 text-sky-500 shrink-0" />
            <span class="text-pos-text truncate">{previewCustomer.email || '—'}</span>
          </div>
          <div class="flex items-center gap-1.5 min-w-0">
            <MapPin class="w-3 h-3 text-sky-500 shrink-0" />
            <span class="text-pos-text truncate">{previewCustomer.address || 'Alger'}</span>
          </div>
          <span class="font-mono text-pos-muted">RC: <b class="text-pos-text">{previewCustomer.rc || '—'}</b></span>
          <span class="font-mono text-pos-muted">NIF: <b class="text-pos-text">{previewCustomer.nif || '—'}</b></span>
          <span class="font-mono text-pos-muted">NIS: <b class="text-pos-text">{previewCustomer.nis || '—'}</b> · AI: <b class="text-pos-text">{previewCustomer.ai || '—'}</b></span>
        </div>
      </div>

      <div class="p-6 space-y-4 max-h-[70vh] overflow-y-auto">
        <!-- Balance: FULL WIDTH (owner: the red card was too small) -->
        <div class="space-y-4">
          <div class="p-4 border rounded-2xl flex flex-col justify-between space-y-3
            {previewCustomer.balance > 0 ? 'bg-rose-50 dark:bg-rose-950/30 border-rose-200 dark:border-rose-900 text-rose-900 dark:text-rose-200'
              : previewCustomer.balance < 0 ? 'bg-violet-50 dark:bg-violet-950/30 border-violet-200 dark:border-violet-900 text-violet-900 dark:text-violet-200'
              : 'bg-emerald-50 dark:bg-emerald-950/30 border-emerald-200 dark:border-emerald-900 text-emerald-900 dark:text-emerald-200'}">
            <div>
              <p class="text-[10px] uppercase font-black tracking-wider">Customer Account Balance (حساب الزبون)</p>
              <p class="text-2xl font-black font-mono">{previewCustomer.balance.toLocaleString()} DZD</p>
              {#if previewCustomer.balance < 0}
                <p class="text-[10px] font-bold mt-0.5">CREDIT — the shop owes this client (usually a paid sale that was later deleted/refunded; clear the debt to reset to 0).</p>
              {/if}
            </div>
            <div class="flex items-center gap-2 flex-wrap">
              {#if previewCustomer.balance > 0}
                <button
                  type="button"
                  on:click={() => { selectedCustomer = previewCustomer; isDebtModalOpen = true; }}
                  class="px-3 py-1.5 bg-rose-600 hover:bg-rose-700 text-white text-xs font-black rounded-xl shadow-xs cursor-pointer flex items-center gap-1.5"
                >
                  <DollarSign class="w-3.5 h-3.5" />
                  <span>Record Payment (تسديد)</span>
                </button>
              {/if}
              {#if previewCustomer.balance !== 0}
                <button
                  type="button"
                  on:click={() => openSettle(previewCustomer!)}
                  class="px-3 py-1.5 bg-slate-700 hover:bg-slate-800 text-white text-xs font-black rounded-xl shadow-xs cursor-pointer flex items-center gap-1.5"
                  title="Settle to 0 (تصفير) — admin password, archived, no cash movement"
                >
                  <Check class="w-3.5 h-3.5" />
                  <span>Settle / تصفير (0)</span>
                </button>
              {/if}
              <span class="px-3 py-1 text-white text-xs font-black rounded-xl shadow-xs
                {previewCustomer.balance > 0 ? 'bg-rose-600' : previewCustomer.balance < 0 ? 'bg-violet-600' : 'bg-emerald-600'}">
                {previewCustomer.balance > 0 ? 'DEBT DUE (دين)' : previewCustomer.balance < 0 ? 'CREDIT (رصيد له)' : 'SETTLED (خالص)'}
              </span>
            </div>
          </div>
          <div class="flex items-center justify-between gap-4 p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border">
            <div class="flex items-center gap-3">
              <QrImage payload={entityQrPayload('CUST', previewCustomer.qr_code || 'CUST-' + previewCustomer.id)} size={90} />
              <div class="text-xs text-pos-muted font-bold">
                <p>Customer QR / رمز الزبون</p>
                <p class="font-mono text-pos-text">{previewCustomer.qr_code || 'CUST-' + previewCustomer.id}</p>
              </div>
            </div>
          </div>
        </div>

        <!-- Lifetime stat tiles: Total Sales / Total Profit / Debts / Returns -->
        <div class="grid grid-cols-2 md:grid-cols-4 gap-3">
          <div class="p-3 bg-sky-50 dark:bg-sky-950/30 border border-sky-200 dark:border-sky-900 rounded-2xl" title="Gross sales booked to this customer (refunds excluded)">
            <div class="flex items-center gap-1.5 text-[10px] uppercase font-black tracking-wider text-sky-700 dark:text-sky-400">
              <ShoppingBag class="w-3.5 h-3.5" />
              <span>Total Sales (المبيعات)</span>
            </div>
            <p class="text-lg font-black font-mono text-pos-text mt-1">
              {customerStats ? customerStats.total_sales.toLocaleString() + ' DZD' : '—'}
            </p>
          </div>
          <div class="p-3 bg-emerald-50 dark:bg-emerald-950/30 border border-emerald-200 dark:border-emerald-900 rounded-2xl" title="Gross margin: sales minus the goods' purchase cost, refunds reversed">
            <div class="flex items-center gap-1.5 text-[10px] uppercase font-black tracking-wider text-emerald-700 dark:text-emerald-400">
              <TrendingUp class="w-3.5 h-3.5" />
              <span>Total Profit (الربح)</span>
            </div>
            <p class="text-lg font-black font-mono text-pos-text mt-1">
              {customerStats ? customerStats.total_profit.toLocaleString() + ' DZD' : '—'}
            </p>
          </div>
          <div class="p-3 bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900 rounded-2xl" title="Outstanding balance still owed by this customer">
            <div class="flex items-center gap-1.5 text-[10px] uppercase font-black tracking-wider text-rose-700 dark:text-rose-400">
              <BadgeCheck class="w-3.5 h-3.5" />
              <span>Debts (الديون)</span>
            </div>
            <p class="text-lg font-black font-mono text-pos-text mt-1">{previewCustomer.balance.toLocaleString()} DZD</p>
          </div>
          <div class="p-3 bg-amber-50 dark:bg-amber-950/30 border border-amber-200 dark:border-amber-900 rounded-2xl" title="Returned/refunded goods value on this customer's sales">
            <div class="flex items-center gap-1.5 text-[10px] uppercase font-black tracking-wider text-amber-700 dark:text-amber-400">
              <Undo2 class="w-3.5 h-3.5" />
              <span>Returns (المرتجعات)</span>
            </div>
            <p class="text-lg font-black font-mono text-pos-text mt-1">
              {customerStats ? customerStats.returns_amount.toLocaleString() + ' DZD' : '—'}
            </p>
          </div>
        </div>

        <!-- Purchase history: real table; a row opens the sale on the Sales page -->
        <div class="bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border p-3">
          <h4 class="font-black text-xs text-pos-text mb-2">Purchase History (سجل المشتريات) — {customerHistory.length}</h4>
          <div class="max-h-64 overflow-y-auto rounded-xl border border-pos-border/60">
            <table class="w-full text-[11px] border-collapse">
              <thead class="bg-slate-100 dark:bg-slate-800 text-pos-muted font-black uppercase sticky top-0">
                <tr>
                  <th class="p-2 text-start">Invoice #</th>
                  <th class="p-2 text-start">Date & Time</th>
                  <th class="p-2 text-end">Total</th>
                  <th class="p-2 text-end">Status</th>
                  <th class="p-2 text-end"></th>
                </tr>
              </thead>
              <tbody class="divide-y divide-pos-border/40">
                {#each customerHistory as sale (sale.id)}
                  <tr class="bg-pos-card hover:bg-sky-50 dark:hover:bg-sky-950/30 cursor-pointer transition"
                    on:click={() => { closeCustomerPreview(); onOpenSale(sale); }}
                    title="Open this invoice on the Sales page">
                    <td class="p-2 font-mono font-bold text-sky-600">#{sale.sale_number}</td>
                    <td class="p-2 font-mono text-pos-muted">{String(sale.created_at || '').slice(0, 16).replace('T', ' ')}</td>
                    <td class="p-2 text-end font-mono font-black text-pos-text">{sale.total_amount.toLocaleString()} DZD</td>
                    <td class="p-2 text-end">
                      <span class="px-2 py-0.5 rounded-full text-[10px] font-black {sale.payment_status === 'paid' ? 'bg-emerald-100 text-emerald-700' : 'bg-amber-100 text-amber-700'}">
                        {sale.payment_status}
                      </span>
                    </td>
                    <td class="p-2 text-end">
                      {#if !String(sale.id || '').startsWith('crm-')}
                        <button
                          type="button"
                          on:click|stopPropagation={() => { closeCustomerPreview(); onOpenSale(sale); }}
                          class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                          title="View this sale (عرض الفاتورة)"
                        >
                          <Eye class="w-4 h-4" />
                        </button>
                      {/if}
                    </td>
                  </tr>
                {/each}
                {#if customerHistory.length === 0}
                  <tr><td colspan="5" class="p-4 text-xs text-pos-muted text-center">No purchase invoices for this customer.</td></tr>
                {/if}
              </tbody>
            </table>
          </div>
        </div>

        {#if previewCustomer.notes}
          <div class="p-3 bg-slate-50 dark:bg-slate-800/50 rounded-xl border border-pos-border text-xs">
            <span class="text-pos-muted font-bold block mb-0.5">Customer Notes:</span>
            <p class="text-pos-text">{previewCustomer.notes}</p>
          </div>
        {/if}
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-between">
        <div class="flex items-center gap-2">
          <button on:click={() => printCustomerDebtRecap(previewCustomer!)} class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-xs">
            <Printer class="w-4 h-4" />
            <span>Print Statement (طباعة كشف)</span>
          </button>
          <button on:click={() => printCustomerCard(previewCustomer!)} class="px-4 py-2 bg-slate-700 hover:bg-slate-800 text-white font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-xs">
            <QrCode class="w-4 h-4" />
            <span>Print QR Card (طباعة البطاقة)</span>
          </button>
        </div>
        <button on:click={closeCustomerPreview} class="px-5 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Close</button>
      </div>
    </div>
  </div>
{/if}

<!-- Settle (clear to zero) -->
{#if settleCustomer}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95 duration-150">
      <div class="flex items-center gap-3 text-violet-600">
        <Check class="w-6 h-6 shrink-0" />
        <h3 class="font-black text-sm text-pos-text">Settle {settleCustomer.name} to 0</h3>
      </div>
      <p class="text-xs text-pos-muted leading-relaxed">
        Balance <strong class="text-pos-text">{settleCustomer.balance.toLocaleString()} DZD</strong> is cleared to
        <strong class="text-pos-text">0</strong>. No cash movement — archived in the debt log. Admin password required.
        <br/>الرصيد يُصفّر إلى 0 بدون حركة نقدية — يتطلب كلمة مرور المسؤول.
      </p>
      <input type="text" bind:value={settleReason} placeholder="Reason / السبب (ex: correction)"
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none" />
      <input type="password" bind:value={settlePassword} placeholder="Admin password / كلمة مرور المسؤول"
        on:keydown={(e) => e.key === 'Enter' && confirmSettle()}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono text-pos-text outline-none" />
      {#if settleError}
        <p class="text-[11px] font-bold text-rose-600">{settleError}</p>
      {/if}
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (settleCustomer = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={confirmSettle} disabled={settleBusy || !settlePassword.trim()}
          class="px-4 py-2 bg-violet-600 hover:bg-violet-700 disabled:opacity-40 text-white text-xs font-black rounded-xl cursor-pointer shadow-md">
          {settleBusy ? 'Settling…' : 'Settle to 0'}
        </button>
      </div>
    </div>
  </div>
{/if}

<CustomerDebtModal
  isOpen={isDebtModalOpen}
  customer={selectedCustomer}
  onClose={() => (isDebtModalOpen = false)}
  onPaymentRecorded={refreshAfterCustomerPayment}
/>
<!-- Protected Customer Delete -->
{#if customerToDelete}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4">
      <div class="flex items-center gap-3 text-rose-600">
        <ShieldAlert class="w-6 h-6 shrink-0" />
        <h3 class="font-black text-sm text-pos-text">Delete Customer</h3>
      </div>
      <p class="text-xs text-pos-muted">Delete <strong class="text-pos-text">{customerToDelete.name}</strong>? Admin password required.</p>
      {#if deleteErrorMsg}
        <div class="p-2 bg-rose-100 text-rose-700 text-xs font-bold rounded-lg">{deleteErrorMsg}</div>
      {/if}
      <input
        type="password"
        bind:value={deletePassword}
        placeholder="Admin password"
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-mono outline-none"
      />
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (customerToDelete = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={confirmDeleteCustomer} disabled={isDeletingCustomer} class="px-4 py-2 bg-rose-600 text-white text-xs font-black rounded-xl cursor-pointer shadow-md">
          {isDeletingCustomer ? 'Deleting...' : 'Confirm Delete'}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Deletion requests (field client-delete approvals) — admins only -->
{#if isDeletionRequestsOpen}
  <div class="fixed inset-0 z-[70] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-bg border border-pos-border rounded-3xl shadow-2xl w-full max-w-4xl h-[85vh] overflow-hidden flex flex-col">
      <div class="flex items-center justify-between px-5 py-3 border-b border-pos-border bg-pos-card shrink-0">
        <div class="flex items-center gap-2">
          <div class="w-8 h-8 rounded-xl bg-amber-100 dark:bg-amber-950 text-amber-600 flex items-center justify-center">
            <FileWarning class="w-4 h-4" />
          </div>
          <h3 class="font-black text-sm text-pos-text">Deletion Requests (طلبات الحذف)</h3>
        </div>
        <button
          type="button"
          on:click={() => (isDeletionRequestsOpen = false)}
          class="p-1.5 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer"
        >
          <X class="w-5 h-5" />
        </button>
      </div>
      <div class="flex-1 overflow-y-auto">
        <DeletionRequestsView />
      </div>
    </div>
  </div>
{/if}
