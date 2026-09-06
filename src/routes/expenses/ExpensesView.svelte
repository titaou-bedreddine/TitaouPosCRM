<script lang="ts">
  import QrImage from '../../lib/components/QrImage.svelte';
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import type { Expense } from '../../lib/types';
  import { currentUser } from '../../lib/stores/auth';
  import { activeSession } from '../../lib/stores/session';
  import { printHtmlSilently } from '../../lib/utils/printer';
  import { entityQrPayload } from '../../lib/utils/printer';
  import DateQuickFilters from '../../lib/components/DateQuickFilters.svelte';
  import { sortRows, clickSort } from '../../lib/utils/tableSort';
  import {
    Plus, DollarSign, Trash2, Eye, Printer, X, Check, Pencil,
    TrendingDown, Calendar, User, Receipt, AlertTriangle,
    CreditCard, Banknote, Building
  } from 'lucide-svelte';

  let expenses: Expense[] = [];
  let isAddOpen = false;
  let editingExpense: Expense | null = null;
  let previewExpense: Expense | null = null;

  // Date-range filter for the loaded expense list.
  let filterStartDate = '';
  let selectedUserFilter = ''; // '' = all users
  let filterEndDate = '';
  let expenseDate = new Date().toISOString().split('T')[0];

  $: usersWithExpenses = Array.from(
    new Set(expenses.map((e) => e.user_name || `User #${e.user_id}`))
  ).sort();

  $: filteredExpenses = expenses.filter((e) => {
    if (filterStartDate && e.date < filterStartDate) return false;
    if (filterEndDate && e.date > filterEndDate) return false;
    if (selectedUserFilter) {
      const who = e.user_name || `User #${e.user_id}`;
      if (who !== selectedUserFilter) return false;
    }
    return true;
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
  $: sortedExpenses = sortRows(filteredExpenses, sortKey, sortDir, filteredExpenses);

  let amount = 0;
  let categoryId = 1;
  let paymentMethod = 'cash';
  let recipient = '';
  let receiptRef = '';
  let notes = '';

  let isSaving = false;
  let errorMsg = '';

  // Custom Delete Modal
  let isDeleteModalOpen = false;
  let expenseToDelete: Expense | null = null;
  let isDeleting = false;

  onMount(async () => {
    await loadExpenses();
  });

  async function loadExpenses() {
    try {
      expenses = await invoke<Expense[]>('list_expenses');
    } catch (e) {
      console.error(e);
    }
  }

  // Real Top Statistics
  // Statistics reflect the active date filter.
  $: totalExpensesCount = filteredExpenses.length;
  $: totalAmountSpent = filteredExpenses.reduce((sum, e) => sum + e.amount, 0);
  $: totalCashDeducted = filteredExpenses.filter(e => e.payment_method === 'cash').reduce((sum, e) => sum + e.amount, 0);
  $: totalOtherPayments = totalAmountSpent - totalCashDeducted;

  async function handleAddExpense() {
    if (amount <= 0) {
      errorMsg = 'Please enter a valid expense amount / الرجاء إدخال مبلغ صحيح';
      return;
    }

    try {
      isSaving = true;
      errorMsg = '';
      const userId = $currentUser?.id || 1;
      const sessionId = paymentMethod === 'cash' ? ($activeSession?.id || null) : null;

      await invoke('add_expense', {
        categoryId,
        amount,
        paymentMethod,
        sessionId,
        userId,
        date: expenseDate,
        recipient: recipient || null,
        receiptReference: receiptRef || null,
        notes: notes || null,
      });

      isAddOpen = false;
      amount = 0;
      recipient = '';
      receiptRef = '';
      notes = '';
      await loadExpenses();
    } catch (e: any) {
      errorMsg = typeof e === 'string' ? e : e.message || 'Failed to record expense';
    } finally {
      isSaving = false;
    }
  }

  function promptDelete(exp: Expense, e?: Event) {
    if (e) e.stopPropagation();
    expenseToDelete = exp;
    isDeleteModalOpen = true;
  }

  async function confirmDeleteExpense() {
    if (!expenseToDelete) return;
    try {
      isDeleting = true;
      await invoke('delete_expense', { expenseId: expenseToDelete.id });
      isDeleteModalOpen = false;
      expenseToDelete = null;
      await loadExpenses();
    } catch (e: any) {
      alert('Failed to delete expense: ' + (e.message || e));
    } finally {
      isDeleting = false;
    }
  }

  async function printExpenseVoucher(exp: Expense) {
    try {
      const settings = await invoke<Record<string, string>>('get_all_settings');
      const shopName = settings['shop_name_fr'] || 'TitaouPOS';
      const shopPhone = settings['shop_phone'] || '0553444057';
      const shopAddress = settings['shop_address'] || 'Alger Centre';

      const html = `
        <div style="width: 72mm; font-family: monospace; font-size: 10px; text-align: center; margin: 0 auto; padding: 2mm;">
          <p style="font-size: 14px; font-weight: 900; margin: 0; text-transform: uppercase;">${shopName}</p>
          <p style="font-size: 8px; margin: 2px 0;">${shopAddress} • Tél: ${shopPhone}</p>
          <hr style="border-top: 1px dashed #000; margin: 4px 0;" />
          <p style="font-size: 11px; font-weight: 900; background: #000; color: #fff; padding: 2px 0; margin: 2px 0;">BON DE DÉCAISSEMENT / سند صرف</p>
          <div style="display: flex; justify-content: space-between; font-size: 9px; font-weight: bold; margin-top: 4px;">
            <span>BON #${exp.expense_number}</span>
            <span>${exp.date}</span>
          </div>
          <div style="display: flex; justify-content: space-between; font-size: 8px;">
            <span>Bénéficiaire: ${exp.recipient || 'Divers'}</span>
            <span>Catégorie: ${exp.category_name || 'Général'}</span>
          </div>
          <hr style="border-top: 1px dashed #000; margin: 4px 0;" />
          <div style="display: flex; justify-content: space-between; font-size: 12px; font-weight: 900;">
            <span>MONTANT SORTI:</span>
            <span>${exp.amount.toLocaleString()} DZD</span>
          </div>
          <div style="display: flex; justify-content: space-between; font-size: 9px;">
            <span>Mode de règlement:</span>
            <span style="font-weight: bold; text-transform: uppercase;">${exp.payment_method}</span>
          </div>
          ${exp.notes ? `
            <div style="text-align: left; font-size: 8px; margin-top: 4px;">
              <span>Motif: ${exp.notes}</span>
            </div>
          ` : ''}
          <hr style="border-top: 1px dashed #000; margin: 4px 0;" />
          <div style="display: flex; justify-content: space-between; font-size: 8px; margin-top: 8px;">
            <span>Signature Caissier</span>
            <span>Signature Bénéficiaire</span>
          </div>
        </div>
      `;
      const r = await printHtmlSilently(html, 'Voucher #' + exp.expense_number, { widthMm: 72 });
      if (!r.ok) alert('Print failed: ' + r.message);
    } catch (e: any) {
      alert('Error printing voucher: ' + (e.message || e));
    }
  }
</script>

<div class="h-full flex flex-col bg-pos-bg p-6 overflow-hidden select-none space-y-4">
  <!-- Header -->
  <div class="flex items-center justify-between pb-3 border-b border-pos-border shrink-0">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-2xl bg-rose-100 dark:bg-rose-950 text-rose-600 flex items-center justify-center font-bold">
        <TrendingDown class="w-5 h-5" />
      </div>
      <div>
        <h1 class="text-xl font-black text-pos-text tracking-tight">{t('exp_title')}</h1>
        <p class="text-xs text-pos-muted">Track store overhead and auto-deduct cash directly from active cash register session</p>
      </div>
    </div>

    <button
      type="button"
      on:click={() => {
        editingExpense = null;
        amount = 0;
        categoryId = 1;
        paymentMethod = 'cash';
        recipient = '';
        receiptRef = '';
        notes = '';
        expenseDate = new Date().toISOString().split('T')[0];
        isAddOpen = true;
        errorMsg = '';
      }}
      class="px-4 py-2 bg-rose-600 hover:bg-rose-700 text-white rounded-xl text-xs font-black transition shadow-xs flex items-center gap-2 cursor-pointer active:scale-95"
    >
      <Plus class="w-4 h-4" />
      <span>{t('exp_new')}</span>
    </button>
  </div>

  <!-- Quick Date Presets -->
  <div class="flex items-center gap-3 flex-wrap shrink-0">
    <DateQuickFilters bind:startDate={filterStartDate} bind:endDate={filterEndDate} onChange={() => {}} />
    <div class="flex items-center gap-1.5">
      <input
        type="date"
        bind:value={filterStartDate}
        title="From date"
        class="px-2.5 py-1.5 bg-pos-card border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none"
      />
      <span class="text-xs text-pos-muted font-bold">→</span>
      <input
        type="date"
        bind:value={filterEndDate}
        title="To date"
        class="px-2.5 py-1.5 bg-pos-card border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none"
      />
    </div>
    <select
      bind:value={selectedUserFilter}
      class="px-2.5 py-1.5 bg-pos-card border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none cursor-pointer"
    >
      <option value="">{t('exp_all_users')}</option>
      {#each usersWithExpenses as u}
        <option value={u}>{u}</option>
      {/each}
    </select>
  </div>

  <!-- Real Statistics Cards -->
  <div class="grid grid-cols-2 md:grid-cols-4 gap-3 shrink-0">
    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-rose-50 dark:bg-rose-950 text-rose-600 flex items-center justify-center font-bold">
        <Receipt class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">{t('exp_vouchers')}</p>
        <p class="text-base font-black font-mono text-pos-text">{totalExpensesCount.toLocaleString()}</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-purple-50 dark:bg-purple-950 text-purple-600 flex items-center justify-center font-bold">
        <DollarSign class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">{t('exp_total_spent')}</p>
        <p class="text-base font-black font-mono text-rose-600">{totalAmountSpent.toLocaleString()} DZD</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-emerald-50 dark:bg-emerald-950 text-emerald-600 flex items-center justify-center font-bold">
        <Banknote class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">Cash Drawer Paid</p>
        <p class="text-base font-black font-mono text-emerald-600">{totalCashDeducted.toLocaleString()} DZD</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-sky-50 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <CreditCard class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">Bank / Other</p>
        <p class="text-base font-black font-mono text-sky-600">{totalOtherPayments.toLocaleString()} DZD</p>
      </div>
    </div>
  </div>

  <!-- Expenses Table -->
  <div class="flex-1 overflow-y-auto bg-pos-card border border-pos-border rounded-2xl shadow-xs">
    <table class="w-full text-start text-xs border-collapse">
      <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold sticky top-0 z-10">
        <tr>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('expense_number')}>{t('exp_voucher_num')} {sortIndicator('expense_number')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('date')}>{t('date')} {sortIndicator('date')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('category_name')}>{t('exp_category')} {sortIndicator('category_name')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('recipient')}>{t('exp_beneficiary')} {sortIndicator('recipient')}</th>
          <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('user_name')}>{t('exp_user_col')} {sortIndicator('user_name')}</th>
          <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('amount')}>{t('exp_amount_col')} {sortIndicator('amount')}</th>
          <th class="p-3 text-center">Payment</th>
          <th class="p-3 text-end">Actions</th>
        </tr>
      </thead>
      <tbody class="divide-y divide-pos-border/40">
        {#if filteredExpenses.length === 0}
          <tr>
            <td colspan="8" class="p-8 text-center text-pos-muted">No expense records recorded yet.</td>
          </tr>
        {:else}
          {#each sortedExpenses as exp}
            <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/40 transition">
              <td class="p-3 font-mono font-bold text-rose-600">#{exp.expense_number}</td>
              <td class="p-3 font-mono text-pos-muted">{exp.date}</td>
              <td class="p-3 font-bold text-pos-text">{exp.category_name || 'Général'}</td>
              <td class="p-3 text-pos-muted">{exp.recipient || 'Divers'}</td>
              <td class="p-3 text-pos-muted font-semibold">{exp.user_name || `User #${exp.user_id}`}</td>
              <td class="p-3 text-end font-mono font-black text-rose-600">{exp.amount.toLocaleString()} DZD</td>
              <td class="p-3 text-center">
                <span class="px-2 py-0.5 rounded-full text-[10px] font-bold uppercase {exp.payment_method === 'cash' ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' : 'bg-sky-100 text-sky-800'}">
                  {exp.payment_method}
                </span>
              </td>
              <td class="p-3 text-end">
                <div class="flex items-center justify-end gap-1">
                  <button
                    type="button"
                    on:click={() => printExpenseVoucher(exp)}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title="Print Voucher"
                  >
                    <Printer class="w-4 h-4" />
                  </button>
                  <button
                    type="button"
                    on:click={() => (previewExpense = exp)}
                    class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer"
                    title={t('exp_voucher')}
                  >
                    <Eye class="w-4 h-4" />
                  </button>
                  <button
                    type="button"
                    on:click={() => {
                      editingExpense = exp;
                      amount = exp.amount;
                      categoryId = exp.category_id;
                      paymentMethod = exp.payment_method;
                      recipient = exp.recipient || '';
                      receiptRef = exp.receipt_reference || '';
                      notes = exp.notes || '';
                      expenseDate = exp.date || new Date().toISOString().split('T')[0];
                      isAddOpen = true;
                      errorMsg = '';
                    }}
                    class="p-1.5 text-pos-muted hover:text-amber-600 rounded-lg cursor-pointer"
                    title={t('exp_edit')}
                  >
                    <Pencil class="w-4 h-4" />
                  </button>
                  <button
                    type="button"
                    on:click={(e) => promptDelete(exp, e)}
                    class="p-1.5 text-pos-muted hover:text-rose-600 rounded-lg cursor-pointer"
                    title={t('exp_delete_title')}
                  >
                    <Trash2 class="w-4 h-4" />
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

<!-- Modal: New Expense Voucher -->
{#if isAddOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-lg overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col">
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-rose-600/10 text-rose-600 flex items-center justify-center font-bold">
            <TrendingDown class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">{t('exp_new')}</h3>
            <p class="text-xs text-pos-muted">Deducts automatically from active cash drawer</p>
          </div>
        </div>
        <button on:click={() => (isAddOpen = false)} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      {#if errorMsg}
        <div class="mx-6 mt-4 p-3 bg-rose-100 dark:bg-rose-950 text-rose-800 dark:text-rose-200 text-xs font-bold rounded-xl border border-rose-300">
          {errorMsg}
        </div>
      {/if}

      <div class="p-6 space-y-3">
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('exp_amount')} *</label>
          <input type="number" min="1" bind:value={amount} placeholder="Ex: 5000" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-base font-mono font-black text-rose-600 outline-none focus:ring-2 focus:ring-rose-500" />
          <label class="block text-xs font-bold text-pos-muted mb-1 mt-2">{t('exp_expense_date')}</label>
          <input type="date" bind:value={expenseDate} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
        </div>

        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Category / الفئة</label>
            <select bind:value={categoryId} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none">
              <option value={1}>Loyer / Rent (إيجار)</option>
              <option value={2}>Électricité & Eau / Utilities (كهرباء وغاز ومياه)</option>
              <option value={3}>Transport & Livraison / Delivery (نقل وتوصيل)</option>
              <option value={4}>Maintenance & Réparation (صيانة وإصلاح)</option>
              <option value={5}>Fournitures & Emballage / Packaging (مستلزمات وتغليف)</option>
              <option value={6}>Divers / General Expenses (مصاريف عامة)</option>
            </select>
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Payment Method</label>
            <select bind:value={paymentMethod} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none">
              <option value="cash">Cash (الصندوق)</option>
              <option value="tpe">TPE Card</option>
              <option value="bank_transfer">Bank Transfer</option>
              <option value="other">Other</option>
            </select>
          </div>
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('exp_beneficiary_label')}</label>
          <input type="text" bind:value={recipient} placeholder="Ex: Sonelgaz, Propriétaire, Chauffeur" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none" />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Notes / Description (ملاحظات)</label>
          <input type="text" bind:value={notes} placeholder="Ex: Facture électricité Janvier" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none" />
        </div>
      </div>

      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-between">
        <button on:click={() => (isAddOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">
          Cancel
        </button>
        <button on:click={handleAddExpense} disabled={isSaving} class="px-6 py-2 bg-rose-600 hover:bg-rose-700 disabled:opacity-50 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
          <Check class="w-4 h-4" />
          <span>{isSaving ? 'Recording...' : 'Record Voucher (تسجيل السند)'}</span>
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Delete Confirmation Modal -->
{#if isDeleteModalOpen && expenseToDelete}
  <div class="fixed inset-0 z-60 bg-black/60 backdrop-blur-2xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95">
      <div class="flex items-center gap-3 text-rose-600">
        <AlertTriangle class="w-6 h-6 shrink-0" />
        <h3 class="font-black text-sm text-pos-text">Confirm Expense Deletion</h3>
      </div>
      <p class="text-xs text-pos-muted">
        Are you sure you want to delete expense voucher <strong class="text-pos-text">#{expenseToDelete.expense_number}</strong> ({expenseToDelete.amount.toLocaleString()} DZD)?
      </p>
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (isDeleteModalOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
          Cancel
        </button>
        <button on:click={confirmDeleteExpense} disabled={isDeleting} class="px-4 py-2 bg-rose-600 text-white text-xs font-black rounded-xl cursor-pointer shadow-md">
          {isDeleting ? 'Deleting...' : 'Delete Voucher'}
        </button>
      </div>
    </div>
  </div>
{/if}

<!-- Expense Preview Modal (View) -->
{#if previewExpense}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-md p-6 space-y-4 animate-in zoom-in-95">
      <div class="flex items-start justify-between">
        <div>
          <h3 class="font-black text-sm text-pos-text">Voucher #{previewExpense.expense_number}</h3>
          <p class="text-xs text-pos-muted">{previewExpense.date} • by {previewExpense.user_name || 'User #' + previewExpense.user_id}</p>
        </div>
        <button on:click={() => (previewExpense = null)} class="p-1.5 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="flex items-center justify-center gap-4 p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border">
        <QrImage payload={entityQrPayload('EXP', previewExpense.expense_number)} size={100} />
        <div class="text-xs text-pos-muted font-bold">
          <p>Voucher QR / رمز السند</p>
          <p class="font-mono text-pos-text">{entityQrPayload('EXP', previewExpense.expense_number)}</p>
        </div>
      </div>

      <div class="grid grid-cols-2 gap-3 text-xs">
        <div class="p-3 bg-slate-50 dark:bg-slate-800/40 rounded-xl border border-pos-border">
          <span class="text-pos-muted font-bold block text-[10px] uppercase">Category</span>
          <span class="font-black text-pos-text">{previewExpense.category_name || 'Général'}</span>
        </div>
        <div class="p-3 bg-slate-50 dark:bg-slate-800/40 rounded-xl border border-pos-border">
          <span class="text-pos-muted font-bold block text-[10px] uppercase">Payment</span>
          <span class="font-black text-pos-text uppercase">{previewExpense.payment_method}</span>
        </div>
        <div class="p-3 bg-slate-50 dark:bg-slate-800/40 rounded-xl border border-pos-border">
          <span class="text-pos-muted font-bold block text-[10px] uppercase">{t('exp_beneficiary_label')}</span>
          <span class="font-black text-pos-text">{previewExpense.recipient || 'Divers'}</span>
        </div>
        <div class="p-3 bg-rose-50 dark:bg-rose-950/30 rounded-xl border border-rose-200 dark:border-rose-800/60">
          <span class="text-rose-600 font-bold block text-[10px] uppercase">Amount</span>
          <span class="font-black text-rose-600 font-mono">{previewExpense.amount.toLocaleString()} DZD</span>
        </div>
      </div>

      {#if previewExpense.notes}
        <div class="p-3 bg-slate-50 dark:bg-slate-800/40 rounded-xl border border-pos-border text-xs">
          <span class="text-pos-muted font-bold block text-[10px] uppercase mb-1">Notes</span>
          <span class="text-pos-text">{previewExpense.notes}</span>
        </div>
      {/if}

      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button
          type="button"
          on:click={() => printExpenseVoucher(previewExpense)}
          class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-bold rounded-xl cursor-pointer flex items-center gap-1.5"
        >
          <Printer class="w-4 h-4" />
          <span>Print</span>
        </button>
        <button on:click={() => (previewExpense = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Close</button>
      </div>
    </div>
  </div>
{/if}
