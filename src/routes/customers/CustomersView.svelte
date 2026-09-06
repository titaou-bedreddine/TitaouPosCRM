<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { Customer } from '../../lib/types';
  import { printHtmlSilently, entityQrPayload, entityQrDataUrl } from '../../lib/utils/printer';
  import { refreshCustomers } from '../../lib/stores/customers';
  import { currentUser } from '../../lib/stores/auth';
  import { sortRows, clickSort } from '../../lib/utils/tableSort';
  import CustomerDebtModal from '../../lib/components/CustomerDebtModal.svelte';
  import UniversalSearchBar from '../../lib/components/UniversalSearchBar.svelte';
  import QrImage from '../../lib/components/QrImage.svelte';
  import {
    Users, Plus, QrCode, DollarSign, Edit2, Trash2, Search, X, Check, Printer, FileText, Phone, MapPin, Building, History,
    Eye, ShieldAlert, Pin, PinOff
  } from 'lucide-svelte';

  let customers: Customer[] = [];
  let searchQuery = '';
  let searchType: 'all' | 'name' | 'barcode' | 'price' | 'qr' = 'all';

  let isModalOpen = false;
  let isDebtModalOpen = false;
  let selectedCustomer: Customer | null = null;
  let previewCustomer: Customer | null = null;
  let customerToDelete: Customer | null = null;
  let deletePassword = '';
  let deleteErrorMsg = '';
  let isDeletingCustomer = false;

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
  async function printCustomerCard(c: Customer) {
    let shopName = 'TitaouPOS';
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
    parts.push('<p style="font-size:9px;margin-top:6px;">TitaouPOS &bull; ' + shopName + '</p>');
    parts.push('</div>');
    const r = await printHtmlSilently(parts.join('\n'), 'Client Card ' + c.name, { widthMm: 80 });
    if (!r.ok) console.error('Client card print failed:', r.message);
  }

  let customerHistory: any[] = [];

  // Sales history for the previewed customer.
  async function loadCustomerHistory(c: Customer) {
    try {
      const sales = await invoke<any[]>('list_sales', {
        startDate: null,
        endDate: null,
        userId: null,
        limit: 500,
      });
      customerHistory = sales.filter((s) => s.customer_id === c.id).slice(0, 50);
    } catch {
      customerHistory = [];
    }
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
    const q = searchQuery.trim().toLowerCase();
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
    // Offline-first: the QR is generated locally (no api.qrserver.com).
    const qrUrl = await entityQrDataUrl(entityQrPayload('CUST', c.qr_code || 'CUST-' + c.id), 200);

    const html = `
      <div class="text-center pb-2 border-b-dashed">
        <h2 class="font-black text-sm uppercase">TitaouPOS Superette</h2>
        <p class="text-xxs">Rue Principale, Alger • Tél: 0553444057</p>
        <p class="font-black text-xs mt-1 bg-black text-white px-1">RECAPITULATIF DE DETTE / كشف حساب دين</p>
        <p class="text-xxs mt-0.5">${new Date().toLocaleString()}</p>
      </div>

      <div class="py-2 border-b-dashed text-xxs space-y-1">
        <div class="flex justify-between"><span>Client:</span><strong>${c.name}</strong></div>
        <div class="flex justify-between"><span>Téléphone:</span><strong>${c.phone || 'N/A'}</strong></div>
        ${c.rc ? `<div class="flex justify-between"><span>RC:</span><strong>${c.rc}</strong></div>` : ''}
        ${c.nif ? `<div class="flex justify-between"><span>NIF:</span><strong>${c.nif}</strong></div>` : ''}
      </div>

      <div class="py-3 border-b-dashed text-center">
        <p class="text-xs text-gray-600 font-bold">SOLDE ACTUEL DU (دين مستحق):</p>
        <p class="text-2xl font-black font-mono text-black mt-1">${c.balance.toLocaleString()} DZD</p>
      </div>

      <div class="text-center pt-2">
        <img src="${qrUrl}" alt="QR" class="qr-box" />
        <p class="text-xxs text-gray-500 mt-1">Code Client: #CUST-${c.id}</p>
        <p class="text-[8px] text-gray-400 mt-1">TitaouPOS • Created by Titaou Bedreddine 0553444057</p>
      </div>
    `;

    const r = await printHtmlSilently(html, `Recap Dette - ${c.name}`, { widthMm: 80 });
    if (!r.ok) console.error('Recap print failed:', r.message);
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
              <td class="p-3 font-bold text-pos-text cursor-pointer" on:click={() => { previewCustomer = c; loadCustomerHistory(c); }}>
                {c.name}
              </td>
              <td class="p-3 font-mono text-pos-muted">{c.phone || '—'}</td>
              <td class="p-3 font-mono text-pos-muted">{c.rc ? `${c.rc} / ${c.nif || ''}` : '—'}</td>
              <td class="p-3 text-end font-mono font-black {c.balance > 0 ? 'text-rose-600' : 'text-emerald-600'}">
                {c.balance.toLocaleString()} DZD
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
                    on:click={() => { previewCustomer = c; loadCustomerHistory(c); }}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title="View details"
                  >
                    <Eye class="w-3.5 h-3.5" />
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

<!-- Modal: Customer Profile & Real QR Code -->
{#if previewCustomer}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-lg overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col">
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold">
            <Users class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">{previewCustomer.name}</h3>
            <p class="text-xs text-pos-muted">Customer Code: #CUST-{previewCustomer.id}</p>
          </div>
        </div>
        <button on:click={() => (previewCustomer = null)} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="p-6 space-y-4 max-h-[70vh] overflow-y-auto">
        <!-- Balance Summary Pill -->
        <div class="p-4 rounded-2xl border flex items-center justify-between {previewCustomer.balance > 0 ? 'bg-rose-50 dark:bg-rose-950/30 border-rose-200 dark:border-rose-900 text-rose-800 dark:text-rose-200' : 'bg-emerald-50 dark:bg-emerald-950/30 border-emerald-200 dark:border-emerald-900 text-emerald-800 dark:text-emerald-200'}">
          <div>
            <p class="text-[10px] uppercase font-black tracking-wider">Outstanding Debt Balance (الرصيد المستحق)</p>
            <p class="text-xl font-black font-mono">{previewCustomer.balance.toLocaleString()} DZD</p>
          </div>
          {#if previewCustomer.balance > 0}
            <button
              type="button"
              on:click={() => { selectedCustomer = previewCustomer; isDebtModalOpen = true; previewCustomer = null; }}
              class="px-3.5 py-1.5 bg-rose-600 hover:bg-rose-700 text-white font-black text-xs rounded-xl shadow-xs cursor-pointer"
            >
              Record Payment
            </button>
          {:else}
            <span class="px-2.5 py-1 bg-emerald-600 text-white font-black text-[10px] rounded-lg">SETTLED (خالص)</span>
          {/if}
        </div>

        <!-- Sales History -->
        <div class="bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border p-3">
          <h4 class="font-black text-xs text-pos-text mb-2">Sales History (سجل المبيعات) — {customerHistory.length}</h4>
          <div class="max-h-48 overflow-y-auto space-y-1">
            {#each customerHistory as sale}
              <div class="flex items-center justify-between p-2 bg-pos-card rounded-lg text-xs border border-pos-border/60">
                <span class="font-mono font-bold text-sky-600 truncate">#{sale.sale_number}</span>
                <span class="text-pos-muted font-mono">{sale.created_at}</span>
                <span class="font-mono font-black text-pos-text">{sale.total_amount.toLocaleString()} DZD</span>
                <span class="px-2 py-0.5 rounded-full text-[10px] font-black {sale.payment_status === 'paid' ? 'bg-emerald-100 text-emerald-700' : 'bg-amber-100 text-amber-700'}">
                  {sale.payment_status}
                </span>
              </div>
            {/each}
            {#if customerHistory.length === 0}
              <p class="text-xs text-pos-muted text-center py-3">No sales recorded for this customer.</p>
            {/if}
          </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <!-- QR Card -->
          <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border text-center space-y-2 flex flex-col items-center justify-center">
            <div class="w-28 h-28 bg-white p-1.5 rounded-xl shadow-sm border border-pos-border flex items-center justify-center">
              <QrImage payload={entityQrPayload('CUST', previewCustomer.qr_code || 'CUST-' + previewCustomer.id)} size={104} alt="Customer QR" />
            </div>
            <p class="text-[10px] text-pos-muted font-bold">Scan at POS for Instant Account Lookup</p>
            <button
              type="button"
              on:click={() => printCustomerCard(previewCustomer)}
              class="px-3 py-1.5 bg-sky-600 hover:bg-sky-700 text-white text-[10px] font-black rounded-xl cursor-pointer shadow-xs"
            >
              Print QR Card (طباعة)
            </button>
          </div>

          <!-- Contact & Registration -->
          <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border text-xs space-y-2">
            <div class="flex justify-between pb-1 border-b border-pos-border/50">
              <span class="text-pos-muted font-bold">Phone:</span>
              <span class="font-mono font-bold text-pos-text">{previewCustomer.phone || '—'}</span>
            </div>
            <div class="flex justify-between pb-1 border-b border-pos-border/50">
              <span class="text-pos-muted font-bold">Email:</span>
              <span class="text-pos-text truncate max-w-[130px]">{previewCustomer.email || '—'}</span>
            </div>
            <div class="flex justify-between pb-1 border-b border-pos-border/50">
              <span class="text-pos-muted font-bold">City / Address:</span>
              <span class="text-pos-text">{previewCustomer.address || 'Alger'}</span>
            </div>
            <div class="flex justify-between">
              <span class="text-pos-muted font-bold">RC / NIF:</span>
              <span class="font-mono text-pos-text">{previewCustomer.rc || '—'} / {previewCustomer.nif || '—'}</span>
            </div>
          </div>
        </div>
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-between">
        <button on:click={() => printCustomerDebtRecap(previewCustomer)} class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-xs">
          <Printer class="w-4 h-4" />
          <span>Print Statement (طباعة كشف)</span>
        </button>
        <button on:click={() => (previewCustomer = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Close</button>
      </div>
    </div>
  </div>
{/if}

<CustomerDebtModal
  isOpen={isDebtModalOpen}
  customer={selectedCustomer}
  onClose={() => (isDebtModalOpen = false)}
  onPaymentRecorded={() => { loadCustomers(); refreshCustomers(); }}
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
