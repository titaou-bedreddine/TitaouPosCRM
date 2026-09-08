<script lang="ts">
  import { t } from '../../lib/i18n';
  import QrImage from '../../lib/components/QrImage.svelte';
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { normalizeBarcode } from '../../lib/utils/barcode';
  import { sortRows, clickSort } from '../../lib/utils/tableSort';
  import SupplierDebtModal from '../../lib/components/SupplierDebtModal.svelte';
  import { entityQrPayload, entityQrDataUrl, printHtmlSilently } from '../../lib/utils/printer';
  import type { Supplier } from '../../lib/types';
  import {
    Truck, Plus, QrCode, Edit2, Trash2, Search, X, Check, DollarSign, Eye, ShieldAlert,
    Phone, Mail, MapPin, Building, FileSpreadsheet, Pin, PinOff
  } from 'lucide-svelte';

  let suppliers: Supplier[] = [];
  let searchQuery = '';
  // AZERTY-normalized mirror of the search box (scanners may emit & é " ...).
  $: searchQueryN = normalizeBarcode(searchQuery).toLowerCase();
  let isModalOpen = false;
  let previewSupplier: Supplier | null = null;
  let supplierHistory: any[] = [];

  async function toggleSupplierPin(s: Supplier) {
    try {
      await invoke('toggle_supplier_pin', { supplierId: s.id, pinned: !(s as any).pinned });
      await loadSuppliers();
    } catch (e) {
      console.warn('Pin failed:', e);
    }
  }

  // Full supplier card: shop header, details, dues and QR.
  async function printSupplierCard(x: Supplier) {
    let shopName = 'TitaouPosCRM';
    let shopPhone = '';
    let shopAddress = '';
    try {
      const settings = await invoke<Record<string, string>>('get_all_settings');
      shopName = settings['shop_name_fr'] || shopName;
      shopPhone = settings['shop_phone'] || '';
      shopAddress = settings['shop_address'] || '';
    } catch { /* defaults */ }
    const code = x.qr_code || 'SUP-' + x.id;
    const html = `
      <div style="width:80mm;font-family:monospace;font-size:11px;padding:4mm;text-align:center;">
        <p style="font-size:15px;font-weight:900;margin:0;">${shopName}</p>
        <p style="font-size:9px;margin:2px 0;">${shopAddress} • ${shopPhone}</p>
        <hr style="border-top:1px dashed #000;margin:5px 0;" />
        <p style="font-size:13px;font-weight:900;margin:4px 0;">FOURNISSEUR CARD / بطاقة مورد</p>
        <p style="font-weight:900;font-size:12px;margin:3px 0;">${x.name}</p>
        ${x.contact_person ? `<p style="margin:2px 0;">Contact: ${x.contact_person}</p>` : ''}
        ${x.phone ? `<p style="margin:2px 0;">Tel: ${x.phone}</p>` : ''}
        ${x.rc ? `<p style="margin:2px 0;font-size:10px;">RC: ${x.rc} ${x.nif ? '| NIF: ' + x.nif : ''}</p>` : ''}
        <img src="${await entityQrDataUrl(entityQrPayload('SUP', code), 300)}" alt="QR" style="width:38mm;height:38mm;margin:5px auto;" />
        <p style="font-size:10px;font-family:monospace;">${code}</p>
        <hr style="border-top:1px dashed #000;margin:5px 0;" />
        <p style="font-size:13px;font-weight:900;margin:2px 0;">DUES: ${(x.balance || 0).toLocaleString()} DZD</p>
        <p style="font-size:9px;margin-top:6px;">TitaouPosCRM • ${shopName}</p>
      </div>
    `;
    const r = await printHtmlSilently(html, 'Fournisseur Card ' + x.name, { widthMm: 80 });
    if (!r.ok) console.error('Supplier card print failed:', r.message);
  }

  async function confirmDeleteSupplier() {
    if (!supplierToDelete) return;
    try {
      isDeletingSupplier = true;
      deleteErrorMsg = '';
      const ok = await invoke<boolean>('verify_admin_password', { password: deletePassword });
      if (!ok) {
        deleteErrorMsg = 'Invalid password / كلمة المرور غير صحيحة';
        return;
      }
      await invoke('delete_supplier', { supplierId: supplierToDelete.id });
      supplierToDelete = null;
      await loadSuppliers();
    } catch (e: any) {
      deleteErrorMsg = typeof e === 'string' ? e : e.message || 'Delete failed';
    } finally {
      isDeletingSupplier = false;
    }
  }

  async function loadSupplierHistory(x: Supplier) {
    try {
      const purchases = await invoke<any[]>('list_purchases');
      supplierHistory = purchases.filter((pur) => pur.supplier_id === x.id).slice(0, 50);
    } catch {
      supplierHistory = [];
    }
  }
  let isDebtModalOpen = false;
  let supplierToDelete: Supplier | null = null;
  let deletePassword = '';
  let deleteErrorMsg = '';
  let isDeletingSupplier = false;
  let payingSupplier: Supplier | null = null;

  let name = '';
  let contactPerson = '';
  let phone = '';
  let email = '';
  let address = '';
  let rc = '';
  let nif = '';
  let nis = '';
  let ai = '';
  let notes = '';
  let editingId: number | null = null;
  let isSaving = false;
  let errorMsg = '';

  onMount(async () => {
    await loadSuppliers();
  });

  async function loadSuppliers() {
    try {
      suppliers = await invoke<Supplier[]>('list_suppliers');
    } catch (e) {
      console.error(e);
    }
  }

  // Omni search: name, contact, phone, email, ids, exact balance, QR.
  $: filteredSuppliers = suppliers.filter((x) => {
    const q = searchQueryN;
    if (!q) return true;
    const hit = ['name', 'contact_person', 'phone', 'email', 'rc', 'nif', 'code', 'qr_code'].some(
      (f) => String((x as any)[f] || '').toLowerCase().includes(q)
    );
    if (hit) return true;
    if (String(Math.max(0, x.balance || 0)) === q) return true;
    const qr = ('SUP:' + (x.qr_code || 'SUP-' + x.id)).toLowerCase();
    return qr === q || qr.includes(q);
  });
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
  $: sortedSuppliers = sortRows(filteredSuppliers, sortKey, sortDir, filteredSuppliers);

  function openAddModal() {
    editingId = null;
    name = '';
    contactPerson = '';
    phone = '';
    email = '';
    address = '';
    rc = '';
    nif = '';
    nis = '';
    ai = '';
    notes = '';
    errorMsg = '';
    isModalOpen = true;
  }

  function openEditModal(s: Supplier) {
    editingId = s.id;
    name = s.name;
    contactPerson = s.contact_person || '';
    phone = s.phone || '';
    email = s.email || '';
    address = s.address || '';
    rc = s.rc || '';
    nif = s.nif || '';
    nis = s.nis || '';
    ai = s.ai || '';
    notes = s.notes || '';
    errorMsg = '';
    isModalOpen = true;
  }

  async function handleSave() {
    if (!name.trim()) {
      errorMsg = 'Supplier name is required / اسم المورد مطلوب';
      return;
    }
    try {
      isSaving = true;
      errorMsg = '';
      await invoke('save_supplier', {
        name,
        contactPerson: contactPerson || null,
        phone: phone || null,
        email: email || null,
        address: address || null,
        rc: rc || null,
        nif: nif || null,
        nis: nis || null,
        ai: ai || null,
        notes: notes || null,
        supplierId: editingId,
      });
      isModalOpen = false;
      await loadSuppliers();
    } catch (e: any) {
      errorMsg = typeof e === 'string' ? e : e.message || 'Failed to save supplier';
    } finally {
      isSaving = false;
    }
  }
</script>

<div class="h-full flex flex-col bg-pos-bg p-4 overflow-hidden select-none">
  <!-- Header -->
  <div class="flex items-center justify-between pb-4 border-b border-pos-border shrink-0">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-2xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <Truck class="w-5 h-5" />
      </div>
      <div>
        <h1 class="text-xl font-black text-pos-text tracking-tight">Suppliers / الموردون والشركات</h1>
        <p class="text-xs text-pos-muted">Manage supplier contacts, purchases history, and company details</p>
      </div>
    </div>

    <div class="flex items-center gap-2">
      <button
        type="button"
        on:click={openAddModal}
        class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white rounded-xl text-xs font-black transition shadow-xs flex items-center gap-2 cursor-pointer active:scale-95"
      >
        <Plus class="w-4 h-4" />
        <span>Add Supplier (إضافة مورد جديد)</span>
      </button>
    </div>
  </div>

  <!-- Search Filter -->
  <div class="mt-4 mb-2">
    <div class="relative">
      <Search class="w-4 h-4 text-pos-muted absolute start-3 top-2.5" />
      <input
        type="text"
        bind:value={searchQuery}
        placeholder="Search by supplier name, contact person, or phone..."
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

  <!-- Table -->
  <div class="flex-1 overflow-y-auto bg-pos-card border border-pos-border rounded-2xl shadow-xs">
    <table class="w-full text-start text-xs border-collapse">
      <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold sticky top-0 z-10">
        <tr>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('name_fr')}>Supplier Name / الشركة {sortIndicator('name_fr')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('contact_person')}>Contact Person {sortIndicator('contact_person')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('phone')}>Phone {sortIndicator('phone')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('address')}>Address {sortIndicator('address')}</th>
          <th class="p-3 text-end">Actions</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-pos-border/40">
        {#if filteredSuppliers.length === 0}
          <tr>
            <td colspan="5" class="p-8 text-center text-pos-muted">No suppliers found.</td>
          </tr>
        {:else}
          {#each sortedSuppliers as s}
            <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/40 transition">
              <td class="p-3 font-bold text-pos-text cursor-pointer" on:click={() => { previewSupplier = s; loadSupplierHistory(s); }}>
                {s.name}
              </td>
              <td class="p-3 text-pos-muted">{s.contact_person || '—'}</td>
              <td class="p-3 font-mono text-pos-muted">{s.phone || '—'}</td>
              <td class="p-3 text-pos-muted">{s.address || '—'}</td>
              <td class="p-3 text-end">
                <div class="flex items-center justify-end gap-1">
                                      <button
                      type="button"
                      on:click={(ev) => { ev.stopPropagation(); printSupplierCard(s); }}
                      class="p-1.5 rounded-lg cursor-pointer transition text-pos-muted hover:text-sky-600"
                      title={t('supplier_print_qr')}
                    >
                      <QrCode class="w-3.5 h-3.5" />
                    </button>
<button
                    type="button"
                    on:click={() => toggleSupplierPin(s)}
                    class="p-1.5 rounded-lg cursor-pointer transition {(s as any).pinned ? 'bg-amber-500 text-white' : 'text-pos-muted hover:text-amber-500'}"
                    title="Pin to top (تثبيت)"
                  >
                    {#if (s as any).pinned}
                      <PinOff class="w-3.5 h-3.5" />
                    {:else}
                      <Pin class="w-3.5 h-3.5" />
                    {/if}
                  </button>
                  <button
                    type="button"
                    on:click={() => { previewSupplier = s; loadSupplierHistory(s); }}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title="View details (عرض)"
                  >
                    <Eye class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    on:click={() => openEditModal(s)}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title="Edit"
                  >
                    <Edit2 class="w-3.5 h-3.5" />
                  </button>
                  {#if s.id !== 1}
                    <button
                      type="button"
                      on:click={() => { supplierToDelete = s; deletePassword = ''; deleteErrorMsg = ''; }}
                      class="p-1.5 text-pos-muted hover:text-rose-600 rounded-lg cursor-pointer"
                      title="Delete (requires admin password)"
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

<!-- Modal: Add / Edit Supplier -->
{#if isModalOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-xl overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col">
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold">
            <Truck class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">
              {editingId ? 'Edit Supplier / تعديل مورد' : 'New Supplier / إضافة مورد جديد'}
            </h3>
            <p class="text-xs text-pos-muted">Enter contact and commercial registry details</p>
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
            <label class="block text-xs font-bold text-pos-muted mb-1">Company / Supplier Name *</label>
            <input type="text" bind:value={name} placeholder="Ex: Sarl Agro Food" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500" />
          </div>
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Contact Person / المسؤول</label>
            <input type="text" bind:value={contactPerson} placeholder="Ex: Karim Benali" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500" />
          </div>
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Phone / الهاتف</label>
            <input type="text" bind:value={phone} placeholder="Ex: 0550 99 88 77" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
          </div>
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Email / البريد</label>
            <input type="email" bind:value={email} placeholder="Ex: contact@agrofood.dz" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none" />
          </div>
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Address / العنوان</label>
          <input type="text" bind:value={address} placeholder="Ex: Zone Industrielle Rouiba, Alger" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none" />
        </div>

        <div class="p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-2">
          <span class="text-xs font-black text-pos-text block">Legal & Fiscal Details (السجل التجاري والجبائي)</span>
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
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-end gap-2">
        <button on:click={() => (isModalOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Cancel</button>
        <button on:click={handleSave} disabled={isSaving} class="px-6 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
          <Check class="w-4 h-4" />
          <span>{isSaving ? 'Saving...' : 'Save Supplier (حفظ)'}</span>
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Modal: Supplier Profile Details Popup -->
{#if previewSupplier}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-lg overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col">
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold">
            <Truck class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">{previewSupplier.name}</h3>
            <p class="text-xs text-pos-muted">{previewSupplier.contact_person || 'Principal Commercial Contact'}</p>
          </div>
        </div>
        <button on:click={() => { previewSupplier = null; loadSupplierHistory(null); }} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="p-6 space-y-4 max-h-[70vh] overflow-y-auto">
        <!-- Balance / Credit Summary Banner -->
        <div class="p-4 bg-purple-50 dark:bg-purple-950/30 border border-purple-200 dark:border-purple-900 rounded-2xl flex items-center justify-between text-purple-900 dark:text-purple-200">
          <div>
            <p class="text-[10px] uppercase font-black tracking-wider">Supplier Account Balance (حساب المورد)</p>
            <p class="text-xl font-black font-mono">{previewSupplier.balance.toLocaleString()} DZD</p>
          </div>
        <div class="flex items-center justify-between gap-4 p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border">
          <div class="flex items-center gap-3">
            <QrImage payload={entityQrPayload('SUP', previewSupplier.qr_code || 'SUP-' + previewSupplier.id)} size={90} />
            <div class="text-xs text-pos-muted font-bold">
              <p>Supplier QR / رمز المورد</p>
              <p class="font-mono text-pos-text">{previewSupplier.qr_code || 'SUP-' + previewSupplier.id}</p>
            </div>
          </div>
          <button
            type="button"
            on:click={() => printSupplierCard(previewSupplier)}
            class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl cursor-pointer shadow-md"
          >
            Print QR Card (طباعة البطاقة)
          </button>
        </div>

        <div class="bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border p-3">
          <h4 class="font-black text-xs text-pos-text mb-2">Purchase History (سجل الفواتير) — {supplierHistory.length}</h4>
          <div class="max-h-56 overflow-y-auto space-y-1">
            {#each supplierHistory as pur}
              <div class="flex items-center justify-between p-2 bg-pos-card rounded-lg text-xs border border-pos-border/60">
                <span class="font-mono font-bold text-sky-600 truncate">#{pur.invoice_number}</span>
                <span class="text-pos-muted font-mono">{pur.date}</span>
                <span class="font-mono font-black text-pos-text">{pur.total.toLocaleString()} DZD</span>
                <span class="px-2 py-0.5 rounded-full text-[10px] font-black {pur.paid_amount >= pur.total ? 'bg-emerald-100 text-emerald-700' : 'bg-amber-100 text-amber-700'}">
                  {pur.paid_amount >= pur.total ? 'PAID' : 'DUE ' + (pur.total - pur.paid_amount).toLocaleString()}
                </span>
              </div>
            {/each}
            {#if supplierHistory.length === 0}
              <p class="text-xs text-pos-muted text-center py-3">No purchase invoices for this supplier.</p>
            {/if}
          </div>
        </div>

          <div class="flex items-center gap-2">
            {#if previewSupplier.balance > 0}
              <button
                type="button"
                on:click={() => { payingSupplier = previewSupplier; isDebtModalOpen = true; }}
                class="px-3 py-1.5 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl shadow-xs cursor-pointer flex items-center gap-1.5"
              >
                <DollarSign class="w-3.5 h-3.5" />
                <span>Pay Debt (تسديد)</span>
              </button>
            {/if}
            <span class="px-3 py-1 bg-purple-600 text-white text-xs font-black rounded-xl shadow-xs">
              {previewSupplier.balance > 0 ? 'PAYABLE DUE' : 'CLEAR'}
            </span>
          </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
          <div class="p-4 bg-slate-100 dark:bg-slate-800 rounded-2xl space-y-2.5">
            <h4 class="font-black text-pos-text pb-1 border-b border-pos-border/60">Direct Contacts</h4>
            <div class="flex items-center gap-2">
              <Phone class="w-3.5 h-3.5 text-sky-500 shrink-0" />
              <span class="font-mono font-bold text-pos-text">{previewSupplier.phone || 'No phone recorded'}</span>
            </div>
            <div class="flex items-center gap-2">
              <Mail class="w-3.5 h-3.5 text-sky-500 shrink-0" />
              <span class="text-pos-text truncate">{previewSupplier.email || 'No email recorded'}</span>
            </div>
            <div class="flex items-center gap-2">
              <MapPin class="w-3.5 h-3.5 text-sky-500 shrink-0" />
              <span class="text-pos-text">{previewSupplier.address || 'Alger, Algérie'}</span>
            </div>
          </div>

          <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-1.5">
            <h4 class="font-black text-pos-text pb-1 border-b border-pos-border/60">Commercial & Tax Info</h4>
            <div class="flex justify-between"><span class="text-pos-muted font-bold">RC:</span><span class="font-mono font-bold text-pos-text">{previewSupplier.rc || '—'}</span></div>
            <div class="flex justify-between"><span class="text-pos-muted font-bold">NIF:</span><span class="font-mono font-bold text-pos-text">{previewSupplier.nif || '—'}</span></div>
            <div class="flex justify-between"><span class="text-pos-muted font-bold">NIS:</span><span class="font-mono font-bold text-pos-text">{previewSupplier.nis || '—'}</span></div>
            <div class="flex justify-between"><span class="text-pos-muted font-bold">AI:</span><span class="font-mono font-bold text-pos-text">{previewSupplier.ai || '—'}</span></div>
          </div>
        </div>

        {#if previewSupplier.notes}
          <div class="p-3 bg-slate-50 dark:bg-slate-800/50 rounded-xl border border-pos-border text-xs">
            <span class="text-pos-muted font-bold block mb-0.5">Supplier Notes:</span>
            <p class="text-pos-text">{previewSupplier.notes}</p>
          </div>
        {/if}
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-end">
        <button on:click={() => { previewSupplier = null; loadSupplierHistory(null); }} class="px-5 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Close</button>
      </div>
    </div>
  </div>
{/if}
<SupplierDebtModal
  isOpen={isDebtModalOpen}
  supplier={payingSupplier}
  onClose={() => (isDebtModalOpen = false)}
  onPaymentRecorded={loadSuppliers}
/>

<!-- Protected Supplier Delete -->
{#if supplierToDelete}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4">
      <div class="flex items-center gap-3 text-rose-600">
        <ShieldAlert class="w-6 h-6 shrink-0" />
        <h3 class="font-black text-sm text-pos-text">Delete Supplier (حذف مورد)</h3>
      </div>
      <p class="text-xs text-pos-muted">Delete <strong class="text-pos-text">{supplierToDelete.name}</strong>? Admin password required.</p>
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
        <button on:click={() => (supplierToDelete = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={confirmDeleteSupplier} disabled={isDeletingSupplier} class="px-4 py-2 bg-rose-600 text-white text-xs font-black rounded-xl cursor-pointer shadow-md">
          {isDeletingSupplier ? 'Deleting...' : 'Confirm Delete'}
        </button>
      </div>
    </div>
  </div>
{/if}
