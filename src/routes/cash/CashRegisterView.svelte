<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import type { CashMovement, CashSession, Customer } from '../../lib/types';
  import { activeSession } from '../../lib/stores/session';
  import { currentUser } from '../../lib/stores/auth';
  import { DollarSign, ArrowDownCircle, ArrowUpCircle, Lock, RefreshCw, Plus, CheckCircle, Check, Search, Wallet, TrendingUp, ArrowDownRight, Layers, Banknote, Wallet as WalletIcon, Edit2, Archive, ArchiveRestore, Trash2, AlertTriangle, X } from 'lucide-svelte';
  import DateQuickFilters from '../../lib/components/DateQuickFilters.svelte';
  import { printHtmlSilently } from '../../lib/utils/printer';
  import { sortRows, clickSort } from '../../lib/utils/tableSort';

  let currentTab: 'current' | 'history' = 'current';
  let movements: CashMovement[] = [];
  let historySessions: CashSession[] = [];

  let isDepositOpen = false;
  let isWithdrawOpen = false;
  let isCloseOpen = false;
  let isStartupOpen = false;
  let startupAmount = 0;
  let startupReason = 'Startup Cash / رصيد افتتاحي';

  let amount = 0;
  let reason = '';
  let countedCash = 0;
  let closeNotes = '';

  // History Filter
  let fromDate = '';
  let toDate = '';
  let showArchived = false;

  // Issue 10: Edit Opening Balance (Active Session)
  let isEditOpeningOpen = false;
  let editOpeningAmount = 0;
  let editOpeningReason = '';
  let editOpeningAdminPassword = '';
  let editOpeningError = '';
  let isSubmittingOpening = false;

  // Issue 11: Edit Past Session Details
  let isEditSessionModalOpen = false;
  let sessionToEdit: CashSession | null = null;
  let editSessionOpening = 0;
  let editSessionActual = 0;
  let editSessionNotes = '';
  let editSessionAdminPassword = '';
  let editSessionError = '';
  let isSubmittingEditSession = false;

  // Issue 12: Archive / Restore Past Session
  let isArchiveModalOpen = false;
  let sessionToArchive: CashSession | null = null;
  let archiveAdminPassword = '';
  let archiveError = '';
  let isSubmittingArchive = false;

  // Issue 12: Delete Past Session
  let isDeleteModalOpen = false;
  let sessionToDelete: CashSession | null = null;
  let deleteAdminPassword = '';
  let deleteConfirmText = '';
  let deleteError = '';
  let isSubmittingDelete = false;

  // Shared popup keyboard UX: Enter confirms the focused modal, Esc
  // closes it. Pair with the autofocus action on the first field.
  function modalKey(e: KeyboardEvent, confirm: () => void, close: () => void) {
    if (e.key === 'Enter') {
      e.preventDefault();
      confirm();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      close();
    }
  }
  // Svelte action: focus the first field the moment a modal mounts.
  function autoFocus(node: HTMLInputElement) {
    setTimeout(() => {
      node.focus();
      node.select();
    }, 60);
  }

  // Debt & versement KPIs for the register page
  let totalUnpaidDebt = 0;
  let totalPaidDebt = 0;
  let totalVersement = 0;

  async function loadDebtKpis() {
    try {
      const customers = await invoke<Customer[]>('list_customers');
      // Unpaid debt = sum of what customers still owe.
      totalUnpaidDebt = customers.reduce((s, c) => s + Math.max(0, c.balance || 0), 0);
      // Paid debt: debt repayments recorded in the active session.
      totalPaidDebt = $activeSession
        ? movements
            .filter((m: any) => m.type === 'customer_debt_payment')
            .reduce((s: number, m: any) => s + Math.abs(m.amount || 0), 0)
        : 0;
      // Versement: remaining unpaid layaway balances today.
      try {
        const sales = await invoke<any[]>('list_sales', {
          startDate: new Date().toISOString().split('T')[0],
          endDate: new Date().toISOString().split('T')[0],
          userId: null,
          limit: 500,
        });
        totalVersement = sales
          .filter((s: any) => s.payment_method === 'versement')
          .reduce((sum: number, s: any) => sum + Math.max(0, (s.total_amount || 0) - (s.paid_amount || 0)), 0);
      } catch {
        totalVersement = 0;
      }
    } catch (e) {
      console.warn('Debt KPIs unavailable:', e);
    }
  }

  onMount(async () => {
    await loadData();
    await loadDebtKpis();
  });

  async function loadData() {
    if (!$currentUser) return;
    try {
      const active = await invoke<CashSession | null>('get_active_cash_session', { userId: $currentUser.id });
      $activeSession = active;
      if (active) {
        movements = await invoke<CashMovement[]>('list_cash_movements', { sessionId: active.id });
      }
      await loadHistorySessions();
    } catch (e) {
      console.error(e);
    }
  }

  async function loadHistorySessions() {
    try {
      historySessions = await invoke<CashSession[]>('list_session_history', {
        fromDate: fromDate || null,
        toDate: toDate || null,
        includeArchived: showArchived,
      });
    } catch (e) {
      console.error('Error loading history sessions:', e);
    }
  }

  // Three-state column sort on the session history table (DESC → ASC →
  // default), same cycle as the Stock page.
  let sessSortKey: string | null = null;
  let sessSortDir: 'asc' | 'desc' | null = null;
  function applySessionSort(key: string) {
    const next = clickSort(key, sessSortKey, sessSortDir);
    sessSortKey = next.key;
    sessSortDir = next.dir;
  }
  function sessSortIndicator(key: string): string {
    if (sessSortKey !== key || !sessSortDir) return '';
    return sessSortDir === 'asc' ? '▲' : '▼';
  }
  $: sortedSessions = sortRows(historySessions, sessSortKey, sessSortDir, historySessions);

  function openEditOpeningModal() {
    if (!$activeSession) return;
    editOpeningAmount = $activeSession.opening_amount;
    editOpeningReason = '';
    editOpeningAdminPassword = '';
    editOpeningError = '';
    isEditOpeningOpen = true;
  }

  async function handleEditOpeningBalance() {
    if (!$activeSession) return;
    if (editOpeningAmount < 0) {
      editOpeningError = 'Amount cannot be negative';
      return;
    }
    isSubmittingOpening = true;
    editOpeningError = '';
    try {
      await invoke('edit_opening_balance', {
        sessionId: $activeSession.id,
        newAmount: editOpeningAmount,
        reason: editOpeningReason || 'Correction',
        adminPassword: editOpeningAdminPassword || null,
      });
      isEditOpeningOpen = false;
      await loadData();
    } catch (e: any) {
      editOpeningError = e?.toString() || 'Failed to update opening balance';
    } finally {
      isSubmittingOpening = false;
    }
  }

  function openEditSessionModal(s: CashSession) {
    sessionToEdit = s;
    editSessionOpening = s.opening_amount;
    editSessionActual = s.actual_cash !== null && s.actual_cash !== undefined ? s.actual_cash : s.expected_cash;
    editSessionNotes = s.notes || '';
    editSessionAdminPassword = '';
    editSessionError = '';
    isEditSessionModalOpen = true;
  }

  async function handleEditSession() {
    if (!sessionToEdit) return;
    isSubmittingEditSession = true;
    editSessionError = '';
    try {
      await invoke('edit_cash_session', {
        sessionId: sessionToEdit.id,
        openingAmount: editSessionOpening,
        actualCash: editSessionActual,
        notes: editSessionNotes || null,
        adminPassword: editSessionAdminPassword || null,
      });
      isEditSessionModalOpen = false;
      await loadHistorySessions();
    } catch (e: any) {
      editSessionError = e?.toString() || 'Failed to edit session';
    } finally {
      isSubmittingEditSession = false;
    }
  }

  function openArchiveModal(s: CashSession) {
    sessionToArchive = s;
    archiveAdminPassword = '';
    archiveError = '';
    isArchiveModalOpen = true;
  }

  async function handleToggleArchive() {
    if (!sessionToArchive) return;
    isSubmittingArchive = true;
    archiveError = '';
    try {
      await invoke('archive_cash_session', {
        sessionId: sessionToArchive.id,
        archived: !sessionToArchive.is_archived,
        adminPassword: archiveAdminPassword || null,
      });
      isArchiveModalOpen = false;
      await loadHistorySessions();
    } catch (e: any) {
      archiveError = e?.toString() || 'Failed to archive/restore session';
    } finally {
      isSubmittingArchive = false;
    }
  }

  function openDeleteModal(s: CashSession) {
    sessionToDelete = s;
    deleteAdminPassword = '';
    deleteConfirmText = '';
    deleteError = '';
    isDeleteModalOpen = true;
  }

  async function handleDeleteSession() {
    if (!sessionToDelete) return;
    if (deleteConfirmText.trim().toUpperCase() !== 'DELETE' && deleteConfirmText.trim() !== 'حذف') {
      deleteError = 'Type DELETE to confirm';
      return;
    }
    isSubmittingDelete = true;
    deleteError = '';
    try {
      await invoke('delete_cash_session', {
        sessionId: sessionToDelete.id,
        adminPassword: deleteAdminPassword || null,
      });
      isDeleteModalOpen = false;
      await loadHistorySessions();
    } catch (e: any) {
      deleteError = e?.toString() || 'Failed to delete session';
    } finally {
      isSubmittingDelete = false;
    }
  }

  async function handleDeposit() {
    if (!$activeSession || !$currentUser || amount <= 0) return;
    try {
      await invoke('add_cash_movement', {
        sessionId: $activeSession.id,
        userId: $currentUser.id,
        movementType: 'cash_in',
        amount,
        reason: reason || 'Cash Deposit / إيداع نقدي',
      });
      isDepositOpen = false;
      amount = 0;
      reason = '';
      await loadData();
    } catch (e) {
      console.error(e);
    }
  }

  async function handleWithdraw() {
    if (!$activeSession || !$currentUser || amount <= 0) return;
    try {
      await invoke('add_cash_movement', {
        sessionId: $activeSession.id,
        userId: $currentUser.id,
        movementType: 'cash_out',
        amount,
        reason: reason || 'Cash Withdrawal / سحب نقدي',
      });
      isWithdrawOpen = false;
      amount = 0;
      reason = '';
      await loadData();
    } catch (e) {
      console.error(e);
    }
  }

  // End-of-session Z-report, printed right after a successful close.
  async function printSessionReport(snap: CashSession, counted: number) {
    let shopName = 'TitaouPOS';
    try {
      const settings = await invoke<Record<string, string>>('get_all_settings');
      shopName = settings['shop_name_fr'] || shopName;
    } catch { /* default */ }
    const diff = counted - snap.expected_cash;
    const html = [
      '<div style="width:80mm;font-family:monospace;font-size:11px;padding:4mm;">',
      '<p style="text-align:center;font-size:15px;font-weight:900;margin:0;">' + shopName + '</p>',
      '<p style="text-align:center;font-size:11px;font-weight:900;margin:4px 0;">RAPPORT DE CAISSE / تقرير الصندوق</p>',
      '<hr style="border-top:1px dashed #000;margin:5px 0;" />',
      '<p>Session #' + snap.id + ' — ' + (snap.user_name || 'Caisse') + '</p>',
      '<p>Ouvert: ' + (snap.opened_at || '-') + '</p>',
      '<hr style="border-top:1px dashed #000;margin:5px 0;" />',
      '<table style="width:100%;font-size:11px;">',
      '<tr><td>Solde ouverture:</td><td style="text-align:right;">' + snap.opening_amount.toLocaleString() + ' DZD</td></tr>',
      '<tr><td>Ventes (cash):</td><td style="text-align:right;">' + (snap.total_sales || 0).toLocaleString() + ' DZD</td></tr>',
      '<tr><td>Sorties (cash):</td><td style="text-align:right;">' + (snap.total_expenses || 0).toLocaleString() + ' DZD</td></tr>',
      '<tr><td><b>Attendu:</b></td><td style="text-align:right;"><b>' + snap.expected_cash.toLocaleString() + ' DZD</b></td></tr>',
      '<tr><td><b>Compte:</b></td><td style="text-align:right;"><b>' + counted.toLocaleString() + ' DZD</b></td></tr>',
      '<tr><td><b>Ecart:</b></td><td style="text-align:right;"><b>' + diff.toLocaleString() + ' DZD</b></td></tr>',
      '</table>',
      closeNotes ? '<p style="font-size:10px;margin-top:4px;">Notes: ' + closeNotes + '</p>' : '',
      '<hr style="border-top:1px dashed #000;margin:5px 0;" />',
      '<p style="text-align:center;font-size:9px;">TitaouPOS &bull; ' + new Date().toLocaleString() + '</p>',
      '</div>',
    ].join('');
    const r = await printHtmlSilently(html, 'Session Report #' + snap.id, { widthMm: 80 });
    if (!r.ok) console.error('Session report print failed:', r.message);
  }

  async function handleCloseSession() {
    if (!$activeSession) return;
    try {
      const snap: CashSession = { ...$activeSession };
      await invoke('close_cash_session', {
        sessionId: snap.id,
        actualCash: countedCash,
        notes: closeNotes || null,
      });
      isCloseOpen = false;
      $activeSession = null;
      await loadData();
      // Print the Z-report after the close lands.
      printSessionReport(snap, countedCash);
    } catch (e) {
      console.error(e);
    }
  }

  async function handleStartupSession() {
    if (!$currentUser) return;
    try {
      const session = await invoke<any>('open_cash_session', {
        userId: $currentUser.id,
        registerId: 1,
        openingAmount: startupAmount,
        notes: startupReason || 'Startup Cash / رصيد افتتاحي',
      });
      $activeSession = session;
      isStartupOpen = false;
      await loadData();
    } catch (e: any) {
      console.error(e);
      alert('Failed to open session: ' + (e.message || e));
    }
  }
</script>

<div class="p-6 space-y-6 overflow-y-auto h-full select-none">
  <!-- Header with Tab Switcher -->
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-2xl font-black text-pos-text">{t('reg_title')}</h1>
      <p class="text-xs text-pos-muted mt-1">{t('reg_subtitle')}</p>
    </div>

    <!-- Toggle Buttons matching screenshot -->
    <div class="flex items-center bg-slate-100 dark:bg-slate-800 p-1 rounded-xl border border-pos-border">
      <button
        on:click={() => currentTab = 'current'}
        class="px-4 py-2 rounded-lg text-xs font-bold transition cursor-pointer {currentTab === 'current' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
      >{t('reg_current_session')}</button>
      <button
        on:click={() => currentTab = 'history'}
        class="px-4 py-2 rounded-lg text-xs font-bold transition cursor-pointer {currentTab === 'history' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
      >{t('reg_session_history')}</button>
    </div>
  </div>

  {#if currentTab === 'current'}
    {#if $activeSession}
      <!-- Active Session Banner -->
      <div class="bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-300 dark:border-emerald-800 rounded-2xl p-4 flex items-center justify-between shadow-xs">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-full bg-emerald-500 text-white flex items-center justify-center font-bold">
            <CheckCircle class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-extrabold text-sm text-emerald-900 dark:text-emerald-200">
              Active Session #{$activeSession.id} — {$activeSession.user_name || 'Cashier'}{$activeSession.terminal_name ? ` @ ${$activeSession.terminal_name}` : ''}
            </h3>
            <p class="text-xs text-emerald-800 dark:text-emerald-300 mt-0.5 font-medium">
              Opened Since {$activeSession.opened_at}
            </p>
          </div>
        </div>

        <button
          on:click={() => { countedCash = $activeSession?.expected_cash || 0; isCloseOpen = true; }}
          class="px-5 py-2.5 bg-rose-600 hover:bg-rose-700 text-white font-extrabold text-xs rounded-xl transition shadow-sm flex items-center gap-2 cursor-pointer"
        >
          <Lock class="w-4 h-4" />
          <span>{t('reg_close_session')}</span>
        </button>
      </div>

      <!-- Metric Cards matching screenshot -->
      <div class="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div class="bg-pos-card border border-pos-border rounded-2xl p-4 shadow-xs relative">
          <div class="flex items-center justify-between mb-2">
            <span class="text-xs font-bold text-pos-muted flex items-center gap-1.5">
              <Wallet class="w-4 h-4 text-sky-500" />
              <span>{t('reg_opening_balance')}</span>
            </span>
            <button
              type="button"
              on:click={openEditOpeningModal}
              class="text-xs font-bold text-sky-700 dark:text-sky-300 hover:text-white hover:bg-sky-600 bg-sky-50 dark:bg-sky-950/80 px-2.5 py-1 rounded-xl border border-sky-300 dark:border-sky-800 transition cursor-pointer flex items-center gap-1.5 shadow-xs active:scale-95"
              title={t('edit_opening_balance')}
            >
              <Edit2 class="w-3.5 h-3.5" />
              <span>{t('edit_opening_balance')}</span>
            </button>
          </div>
          <div class="text-2xl font-black font-mono text-pos-text">
            {$activeSession.opening_amount.toLocaleString()} DZD
          </div>
        </div>

        <div class="bg-pos-card border border-pos-border rounded-2xl p-4 shadow-xs">
          <span class="text-xs font-bold text-pos-muted flex items-center gap-1.5 mb-2">
            <TrendingUp class="w-4 h-4 text-emerald-500" />
            <span>{t('reg_total_sales')}</span>
          </span>
          <div class="text-2xl font-black font-mono text-emerald-600">
            {($activeSession.total_sales || 0).toLocaleString()} DZD
          </div>
        </div>

        <div class="bg-pos-card border border-pos-border rounded-2xl p-4 shadow-xs">
          <span class="text-xs font-bold text-pos-muted flex items-center gap-1.5 mb-2">
            <ArrowDownRight class="w-4 h-4 text-rose-500" />
            <span>{t('reg_total_expenses')}</span>
          </span>
          <div class="text-2xl font-black font-mono text-rose-600">
            {($activeSession.total_expenses || 0).toLocaleString()} DZD
          </div>
        </div>

        <div class="bg-sky-600 text-white rounded-2xl p-4 shadow-md">
          <span class="text-xs font-bold text-sky-100 flex items-center gap-1.5 mb-2">
            <DollarSign class="w-4 h-4 text-white" />
            <span>{t('reg_current_balance')}</span>
          </span>
          <div class="text-2xl font-black font-mono">
            {$activeSession.expected_cash.toLocaleString()} DZD
          </div>
        </div>
      </div>

      <!-- Debt & Versement Cards -->
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div class="bg-pos-card border border-rose-200 dark:border-rose-800/60 rounded-2xl p-4 shadow-xs">
          <span class="text-xs font-bold text-pos-muted flex items-center gap-1.5 mb-2">
            <Layers class="w-4 h-4 text-rose-500" />
            <span>Unpaid Customer Debt (دين غير مسدد)</span>
          </span>
          <div class="text-2xl font-black font-mono text-rose-600">
            {totalUnpaidDebt.toLocaleString()} DZD
          </div>
        </div>

        <div class="bg-pos-card border border-emerald-200 dark:border-emerald-800/60 rounded-2xl p-4 shadow-xs">
          <span class="text-xs font-bold text-pos-muted flex items-center gap-1.5 mb-2">
            <CheckCircle class="w-4 h-4 text-emerald-500" />
            <span>Debt Paid This Session (تسديدات)</span>
          </span>
          <div class="text-2xl font-black font-mono text-emerald-600">
            {totalPaidDebt.toLocaleString()} DZD
          </div>
        </div>

        <div class="bg-pos-card border border-violet-200 dark:border-violet-800/60 rounded-2xl p-4 shadow-xs">
          <span class="text-xs font-bold text-pos-muted flex items-center gap-1.5 mb-2">
            <WalletIcon class="w-4 h-4 text-violet-500" />
            <span>Versement Remaining (تسبقة متبقية)</span>
          </span>
          <div class="text-2xl font-black font-mono text-violet-600">
            {totalVersement.toLocaleString()} DZD
          </div>
        </div>
      </div>

      <!-- Action Buttons Row matching screenshot -->
      <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
        <button
          on:click={() => isDepositOpen = true}
          class="p-3.5 bg-emerald-50 dark:bg-emerald-950/40 hover:bg-emerald-100 border border-emerald-300 dark:border-emerald-800 text-emerald-700 dark:text-emerald-300 rounded-2xl text-xs font-extrabold flex items-center justify-center gap-2 transition cursor-pointer shadow-xs"
        >
          <ArrowDownCircle class="w-4 h-4 text-emerald-600" />
          <span>{t('reg_deposit')}</span>
        </button>

        <button
          on:click={() => isWithdrawOpen = true}
          class="p-3.5 bg-amber-50 dark:bg-amber-950/40 hover:bg-amber-100 border border-amber-300 dark:border-amber-800 text-amber-700 dark:text-amber-300 rounded-2xl text-xs font-extrabold flex items-center justify-center gap-2 transition cursor-pointer shadow-xs"
        >
          <ArrowUpCircle class="w-4 h-4 text-amber-600" />
          <span>{t('reg_withdrawal')}</span>
        </button>

        <button
          on:click={() => isWithdrawOpen = true}
          class="p-3.5 bg-rose-50 dark:bg-rose-950/40 hover:bg-rose-100 border border-rose-300 dark:border-rose-800 text-rose-700 dark:text-rose-300 rounded-2xl text-xs font-extrabold flex items-center justify-center gap-2 transition cursor-pointer shadow-xs"
        >
          <Plus class="w-4 h-4 text-rose-600" />
          <span>{t('reg_add_expense')}</span>
        </button>
      </div>

      <!-- Transaction Log Table matching screenshot -->
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-xs overflow-hidden">
        <div class="p-4 border-b border-pos-border flex items-center justify-between bg-slate-50 dark:bg-slate-800/40">
          <h3 class="font-extrabold text-xs text-pos-text">{t('reg_transaction_log')}</h3>
          <button on:click={loadData} class="p-1 text-pos-muted hover:text-pos-text cursor-pointer">
            <RefreshCw class="w-4 h-4" />
          </button>
        </div>

        <table class="w-full text-start text-xs border-collapse">
          <thead>
            <tr class="border-b border-pos-border text-pos-muted font-bold">
              <th class="p-3 text-start">{t('reg_col_time')}</th>
              <th class="p-3 text-start">{t('reg_col_type')}</th>
              <th class="p-3 text-start">{t('reg_col_description')}</th>
              <th class="p-3 text-start">{t('terminal')}</th>
              <th class="p-3 text-end">{t('reg_col_amount')}</th>
            </tr>
          </thead>
          <tbody>
            {#if movements.length === 0}
              <tr>
                <td colspan="5" class="p-8 text-center text-pos-muted">{t('reg_no_movements')}</td>
              </tr>
            {:else}
              {#each movements as m}
                <tr class="border-b border-pos-border/60 hover:bg-slate-50 dark:hover:bg-slate-800/40">
                  <td class="p-3 text-pos-muted font-mono">{m.created_at}</td>
                  <td class="p-3 font-bold uppercase text-xs">{m.type_name}</td>
                  <td class="p-3 text-pos-text font-semibold">{m.reason || '-'}</td>
                  <td class="p-3">
                    {#if m.terminal_name}
                      <span class="px-2 py-0.5 rounded-md text-[10px] font-black bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300">{m.terminal_name}</span>
                    {:else}
                      <span class="text-pos-muted">—</span>
                    {/if}
                  </td>
                  <td class="p-3 text-end font-mono font-bold text-sm {m.amount < 0 ? 'text-rose-600' : 'text-emerald-600'}">
                    {m.amount.toLocaleString()} DZD
                  </td>
                </tr>
              {/each}
            {/if}
          </tbody>
        </table>
      </div>
    {:else}
      <div class="p-12 text-center bg-pos-card border border-pos-border rounded-2xl shadow-sm max-w-md mx-auto my-8">
        <div class="w-14 h-14 rounded-2xl bg-amber-50 dark:bg-amber-950/40 text-amber-600 flex items-center justify-center mx-auto mb-4 font-bold">
          <Lock class="w-7 h-7" />
        </div>
        <h3 class="text-base font-black text-pos-text mb-1">{t('reg_no_session')}</h3>
        <p class="text-xs font-bold text-pos-muted mb-5">{t('reg_open_hint')}</p>
        <button
          type="button"
          on:click={() => { startupAmount = 0; startupReason = 'Startup Cash / رصيد افتتاحي'; isStartupOpen = true; }}
          class="px-6 py-3 bg-emerald-600 hover:bg-emerald-700 text-white font-black text-xs rounded-xl shadow-md cursor-pointer transition active:scale-95 flex items-center gap-2 mx-auto"
        >
          <Plus class="w-4 h-4" />
          <span>{t('reg_open_new_session')}</span>
        </button>
      </div>
    {/if}
  {:else}
    <!-- Session History matching screenshot -->
    <div class="bg-pos-card border border-pos-border rounded-2xl p-4 shadow-xs space-y-4">
      <DateQuickFilters bind:startDate={fromDate} bind:endDate={toDate} onChange={loadHistorySessions} />
      <div class="grid grid-cols-1 md:grid-cols-4 gap-3 items-end">
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('from_date')}</label>
          <input type="date" bind:value={fromDate} on:change={loadHistorySessions} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-xs text-pos-text font-bold" />
        </div>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('to_date')}</label>
          <input type="date" bind:value={toDate} on:change={loadHistorySessions} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-xs text-pos-text font-bold" />
        </div>
        <div class="flex items-center h-9">
          <label class="flex items-center gap-2 text-xs font-bold text-pos-muted hover:text-pos-text cursor-pointer select-none">
            <input
              type="checkbox"
              bind:checked={showArchived}
              on:change={loadHistorySessions}
              class="w-4 h-4 rounded text-sky-600 focus:ring-sky-500 border-pos-border"
            />
            <span>Show Archived Sessions (عرض المؤرشفة)</span>
          </label>
        </div>
        <button
          type="button"
          on:click={loadHistorySessions}
          class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-bold text-xs rounded-lg flex items-center justify-center gap-1.5 cursor-pointer shadow-xs"
        >
          <Search class="w-4 h-4" />
          <span>{t('btn_search')}</span>
        </button>
      </div>

      <table class="w-full text-start text-xs border-collapse">
        <thead>
          <tr class="border-b border-pos-border text-pos-muted font-bold bg-slate-50 dark:bg-slate-800/40">
            <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('id')}># {sessSortIndicator('id')}</th>
            <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('user_name')}>Employee {sessSortIndicator('user_name')}</th>
            <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('terminal_name')} title="PC / terminal that opened this session">{t('terminal')} {sessSortIndicator('terminal_name')}</th>
            <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('opened_at')}>Opened At {sessSortIndicator('opened_at')}</th>
            <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('closed_at')}>Closed At {sessSortIndicator('closed_at')}</th>
            <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('opening_amount')}>Opening {sessSortIndicator('opening_amount')}</th>
            <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('actual_cash')}>Closing {sessSortIndicator('actual_cash')}</th>
            <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('expected_cash')}>Expected {sessSortIndicator('expected_cash')}</th>
            <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySessionSort('difference')}>Difference {sessSortIndicator('difference')}</th>
            <th class="p-3 text-center">Status</th>
            <th class="p-3 text-center">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#if historySessions.length === 0}
            <tr>
              <td colspan="11" class="p-8 text-center text-pos-muted font-bold">
                No cash sessions found for this period.
              </td>
            </tr>
          {:else}
            {#each sortedSessions as s}
              <tr class="border-b border-pos-border/60 hover:bg-slate-50 dark:hover:bg-slate-800/40 {s.is_archived ? 'opacity-60 bg-slate-50/50 dark:bg-slate-900/20' : ''}">
                <td class="p-3 font-mono font-bold text-pos-muted">{s.id}</td>
                <td class="p-3 font-bold text-pos-text">{s.user_name || 'Admin'}</td>
                <td class="p-3">
                  {#if s.terminal_name}
                    <span class="px-2 py-0.5 rounded-md text-[10px] font-black bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300">{s.terminal_name}</span>
                  {:else}
                    <span class="text-pos-muted">—</span>
                  {/if}
                </td>
                <td class="p-3 font-mono text-pos-muted">{s.opened_at}</td>
                <td class="p-3 font-mono text-pos-muted">{s.closed_at || '-'}</td>
                <td class="p-3 text-end font-mono font-bold">{s.opening_amount.toLocaleString()}</td>
                <td class="p-3 text-end font-mono font-bold">{s.actual_cash !== null && s.actual_cash !== undefined ? s.actual_cash.toLocaleString() : '-'}</td>
                <td class="p-3 text-end font-mono font-bold">{s.expected_cash.toLocaleString()}</td>
                <td class="p-3 text-end font-mono font-bold {s.difference && s.difference < 0 ? 'text-rose-600' : 'text-emerald-600'}">
                  {s.difference !== null && s.difference !== undefined ? s.difference.toLocaleString() : '-'}
                </td>
                <td class="p-3 text-center">
                  {#if s.is_archived}
                    <span class="px-2 py-0.5 rounded-full text-[10px] font-black bg-amber-100 dark:bg-amber-950/60 text-amber-800 dark:text-amber-300">
                      مؤرشفة / Archived
                    </span>
                  {:else if s.status === 'open'}
                    <span class="px-2 py-0.5 rounded-full text-[10px] font-black bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300">
                      مفتوحة
                    </span>
                  {:else}
                    <span class="px-2 py-0.5 rounded-full text-[10px] font-black bg-slate-200 text-slate-800 dark:bg-slate-700 dark:text-slate-200">
                      مغلقة
                    </span>
                  {/if}
                </td>
                <td class="p-3 text-center">
                  <div class="flex items-center justify-center gap-1.5">
                    <button
                      type="button"
                      on:click={() => openEditSessionModal(s)}
                      class="px-2 py-1 rounded-lg bg-sky-50 dark:bg-sky-950/60 hover:bg-sky-100 dark:hover:bg-sky-900 border border-sky-200 dark:border-sky-800 text-sky-700 dark:text-sky-300 text-[11px] font-bold transition cursor-pointer flex items-center gap-1 shadow-2xs"
                      title={t('edit_session')}
                    >
                      <Edit2 class="w-3 h-3" />
                      <span>{t('btn_edit')}</span>
                    </button>
                    <button
                      type="button"
                      on:click={() => openArchiveModal(s)}
                      class="px-2 py-1 rounded-lg bg-amber-50 dark:bg-amber-950/60 hover:bg-amber-100 dark:hover:bg-amber-900 border border-amber-200 dark:border-amber-800 text-amber-700 dark:text-amber-300 text-[11px] font-bold transition cursor-pointer flex items-center gap-1 shadow-2xs"
                      title={s.is_archived ? 'Restore session / إلغاء الأرشفة' : t('archive_session')}
                    >
                      {#if s.is_archived}
                        <ArchiveRestore class="w-3 h-3" />
                        <span>Restore</span>
                      {:else}
                        <Archive class="w-3 h-3" />
                        <span>{t('archive_session')}</span>
                      {/if}
                    </button>
                    <button
                      type="button"
                      on:click={() => openDeleteModal(s)}
                      class="px-2 py-1 rounded-lg bg-rose-50 dark:bg-rose-950/60 hover:bg-rose-100 dark:hover:bg-rose-900 border border-rose-200 dark:border-rose-800 text-rose-700 dark:text-rose-300 text-[11px] font-bold transition cursor-pointer flex items-center gap-1 shadow-2xs"
                      title={t('btn_delete')}
                    >
                      <Trash2 class="w-3 h-3" />
                      <span>{t('btn_delete')}</span>
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>
  {/if}

  <!-- Modals -->
  {#if isDepositOpen}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-xl p-5 w-full max-w-sm space-y-3">
        <h3 class="font-extrabold text-sm text-pos-text">Cash Deposit (إيداع نقدي)</h3>
        <input use:autoFocus type="number" bind:value={amount} on:keydown={(e) => modalKey(e, handleDeposit, () => (isDepositOpen = false))} on:focus={(e) => (e.target as HTMLInputElement).select()} placeholder="Amount DZD" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded text-lg font-mono font-bold text-pos-text" />
        <input type="text" bind:value={reason} on:keydown={(e) => modalKey(e, handleDeposit, () => (isDepositOpen = false))} placeholder="Reason / Notes" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded text-xs text-pos-text" />
        <div class="flex justify-end gap-2 pt-2">
          <button on:click={() => isDepositOpen = false} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded">Cancel</button>
          <button on:click={handleDeposit} class="px-4 py-1.5 bg-emerald-600 text-white text-xs font-bold rounded">Confirm</button>
        </div>
      </div>
    </div>
  {/if}

  {#if isWithdrawOpen}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-xl p-5 w-full max-w-sm space-y-3">
        <h3 class="font-extrabold text-sm text-pos-text">Cash Withdrawal (سحب نقدي / مصروف)</h3>
        <input use:autoFocus type="number" bind:value={amount} on:keydown={(e) => modalKey(e, handleWithdraw, () => (isWithdrawOpen = false))} on:focus={(e) => (e.target as HTMLInputElement).select()} placeholder="Amount DZD" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded text-lg font-mono font-bold text-pos-text" />
        <input type="text" bind:value={reason} on:keydown={(e) => modalKey(e, handleWithdraw, () => (isWithdrawOpen = false))} placeholder="Reason / Notes" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded text-xs text-pos-text" />
        <div class="flex justify-end gap-2 pt-2">
          <button on:click={() => isWithdrawOpen = false} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded">Cancel</button>
          <button on:click={handleWithdraw} class="px-4 py-1.5 bg-amber-600 text-white text-xs font-bold rounded">Confirm</button>
        </div>
      </div>
    </div>
  {/if}

  {#if isCloseOpen}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-xl p-5 w-full max-w-sm space-y-3">
        <h3 class="font-extrabold text-sm text-pos-text">Close Cash Session (إغلاق الصندوق)</h3>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('reg_counted_cash')} (DZD)</label>
          <input use:autoFocus type="number" bind:value={countedCash} on:keydown={(e) => modalKey(e, handleCloseSession, () => (isCloseOpen = false))} on:focus={(e) => (e.target as HTMLInputElement).select()} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded text-xl font-mono font-bold text-pos-text" />
        </div>
        <input type="text" bind:value={closeNotes} placeholder="Closing notes..." class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded text-xs text-pos-text" />
        <div class="flex justify-end gap-2 pt-2">
          <button on:click={() => isCloseOpen = false} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded">Cancel</button>
          <button on:click={handleCloseSession} class="px-4 py-1.5 bg-rose-600 text-white text-xs font-bold rounded">Close Register</button>
        </div>
      </div>
    </div>
  {/if}

  {#if isStartupOpen}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl p-6 w-full max-w-sm space-y-4 shadow-2xl animate-in zoom-in-95 duration-150">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-emerald-100 text-emerald-700 flex items-center justify-center font-bold">
            <DollarSign class="w-6 h-6" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text">Open New Cash Session</h3>
            <p class="text-[11px] text-pos-muted">افتتاح صندوق جديد</p>
          </div>
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Opening Cash Amount (DZD) *</label>
          <input
            use:autoFocus
            type="number"
            bind:value={startupAmount}
            on:keydown={(e) => modalKey(e, handleStartupSession, () => (isStartupOpen = false))}
            on:focus={(e) => (e.target as HTMLInputElement).select()}
            min="0"
            class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-2 border-emerald-500/40 focus:border-emerald-500 rounded-xl text-lg font-mono font-black text-pos-text outline-none"
          />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Notes / Reason (ملاحظات)</label>
          <input
            type="text"
            bind:value={startupReason}
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none"
          />
        </div>

        <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
          <button
            type="button"
            on:click={() => isStartupOpen = false}
            class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer"
          >
            Cancel
          </button>
          <button
            type="button"
            on:click={handleStartupSession}
            class="px-5 py-2 bg-emerald-600 hover:bg-emerald-700 text-white text-xs font-black rounded-xl shadow-md cursor-pointer flex items-center gap-1.5"
          >
            <Check class="w-4 h-4" />
            <span>Open Session (فتح الصندوق)</span>
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Issue 10: Edit Active Session Opening Balance Modal -->
  {#if isEditOpeningOpen && $activeSession}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl p-6 w-full max-w-sm space-y-4 shadow-2xl animate-in zoom-in-95 duration-150">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
            <Edit2 class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text">{t('edit_opening_balance')}</h3>
            <p class="text-[11px] text-pos-muted">Active Session #{$activeSession.id}</p>
          </div>
        </div>

        {#if editOpeningError}
          <div class="p-2.5 rounded-lg bg-rose-50 dark:bg-rose-950/40 text-rose-600 dark:text-rose-400 text-xs font-bold flex items-center gap-2">
            <AlertTriangle class="w-4 h-4 shrink-0" />
            <span>{editOpeningError}</span>
          </div>
        {/if}

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">New Opening Amount (DZD) *</label>
          <input
            use:autoFocus
            type="number"
            bind:value={editOpeningAmount}
            on:keydown={(e) => modalKey(e, handleEditOpeningBalance, () => (isEditOpeningOpen = false))}
            min="0"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-lg font-mono font-black text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Reason / Motif</label>
          <input
            type="text"
            bind:value={editOpeningReason}
            placeholder="Correction solde d'ouverture / تصحيح رصيد البداية"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('admin_password')} *</label>
          <input
            type="password"
            bind:value={editOpeningAdminPassword}
            placeholder="••••••••"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
        </div>

        <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
          <button
            type="button"
            on:click={() => (isEditOpeningOpen = false)}
            class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer"
          >
            Cancel / إلغاء
          </button>
          <button
            type="button"
            on:click={handleEditOpeningBalance}
            disabled={isSubmittingOpening}
            class="px-5 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white text-xs font-black rounded-xl shadow-md cursor-pointer flex items-center gap-1.5"
          >
            {isSubmittingOpening ? 'Saving...' : 'Confirm / حفظ'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Issue 11: Edit Past Session Details Modal -->
  {#if isEditSessionModalOpen && sessionToEdit}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl p-6 w-full max-w-sm space-y-4 shadow-2xl animate-in zoom-in-95 duration-150">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
            <Edit2 class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text">{t('edit_session')}</h3>
            <p class="text-[11px] text-pos-muted">Session #{sessionToEdit.id} ({sessionToEdit.user_name || 'Cashier'})</p>
          </div>
        </div>

        {#if editSessionError}
          <div class="p-2.5 rounded-lg bg-rose-50 dark:bg-rose-950/40 text-rose-600 dark:text-rose-400 text-xs font-bold flex items-center gap-2">
            <AlertTriangle class="w-4 h-4 shrink-0" />
            <span>{editSessionError}</span>
          </div>
        {/if}

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Opening Amount (DZD)</label>
          <input
            type="number"
            bind:value={editSessionOpening}
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-sm font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Counted Closing Cash (DZD)</label>
          <input
            type="number"
            bind:value={editSessionActual}
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-sm font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Notes</label>
          <input
            type="text"
            bind:value={editSessionNotes}
            placeholder="Notes..."
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('admin_password')} *</label>
          <input
            type="password"
            bind:value={editSessionAdminPassword}
            placeholder="••••••••"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
        </div>

        <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
          <button
            type="button"
            on:click={() => (isEditSessionModalOpen = false)}
            class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer"
          >
            Cancel / إلغاء
          </button>
          <button
            type="button"
            on:click={handleEditSession}
            disabled={isSubmittingEditSession}
            class="px-5 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white text-xs font-black rounded-xl shadow-md cursor-pointer flex items-center gap-1.5"
          >
            {isSubmittingEditSession ? 'Saving...' : 'Save / حفظ'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Issue 12: Archive Past Session Modal -->
  {#if isArchiveModalOpen && sessionToArchive}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl p-6 w-full max-w-sm space-y-4 shadow-2xl animate-in zoom-in-95 duration-150">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-amber-100 dark:bg-amber-950 text-amber-600 flex items-center justify-center font-bold">
            <Archive class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text">
              {sessionToArchive.is_archived ? 'Restore Session' : t('archive_session')}
            </h3>
            <p class="text-[11px] text-pos-muted">Session #{sessionToArchive.id}</p>
          </div>
        </div>

        <p class="text-xs text-pos-muted">
          {sessionToArchive.is_archived
            ? 'Restore this session back to the active history list?'
            : 'Archive this session? It will be hidden from normal history views unless "Show Archived" is enabled.'}
        </p>

        {#if archiveError}
          <div class="p-2.5 rounded-lg bg-rose-50 dark:bg-rose-950/40 text-rose-600 dark:text-rose-400 text-xs font-bold flex items-center gap-2">
            <AlertTriangle class="w-4 h-4 shrink-0" />
            <span>{archiveError}</span>
          </div>
        {/if}

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('admin_password')}</label>
          <input
            type="password"
            bind:value={archiveAdminPassword}
            placeholder="••••••••"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono text-pos-text outline-none focus:ring-2 focus:ring-amber-500"
          />
        </div>

        <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
          <button
            type="button"
            on:click={() => (isArchiveModalOpen = false)}
            class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer"
          >
            Cancel / إلغاء
          </button>
          <button
            type="button"
            on:click={handleToggleArchive}
            disabled={isSubmittingArchive}
            class="px-5 py-2 bg-amber-600 hover:bg-amber-700 disabled:opacity-50 text-white text-xs font-black rounded-xl shadow-md cursor-pointer flex items-center gap-1.5"
          >
            {isSubmittingArchive ? 'Saving...' : sessionToArchive.is_archived ? 'Restore / استرجاع' : 'Archive / أرشفة'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Issue 12: Delete Past Session Modal -->
  {#if isDeleteModalOpen && sessionToDelete}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl p-6 w-full max-w-sm space-y-4 shadow-2xl animate-in zoom-in-95 duration-150">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-rose-100 dark:bg-rose-950 text-rose-600 flex items-center justify-center font-bold">
            <Trash2 class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-sm text-rose-600">Delete Session #{sessionToDelete.id}</h3>
            <p class="text-[11px] text-pos-muted">Permanent action / حذف نهائي</p>
          </div>
        </div>

        <p class="text-xs text-pos-muted">
          Are you sure you want to permanently delete Session #{sessionToDelete.id} and all its cash movements? This cannot be undone.
        </p>

        {#if deleteError}
          <div class="p-2.5 rounded-lg bg-rose-50 dark:bg-rose-950/40 text-rose-600 dark:text-rose-400 text-xs font-bold flex items-center gap-2">
            <AlertTriangle class="w-4 h-4 shrink-0" />
            <span>{deleteError}</span>
          </div>
        {/if}

        <div class="space-y-1">
          <label class="block text-xs font-bold text-pos-muted">
            Type <span class="text-rose-600 font-mono font-black">DELETE</span> to confirm:
          </label>
          <input
            type="text"
            bind:value={deleteConfirmText}
            placeholder="DELETE"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono font-black text-rose-600 outline-none focus:ring-2 focus:ring-rose-500"
          />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">{t('admin_password')} *</label>
          <input
            type="password"
            bind:value={deleteAdminPassword}
            placeholder="••••••••"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono text-pos-text outline-none focus:ring-2 focus:ring-rose-500"
          />
        </div>

        <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
          <button
            type="button"
            on:click={() => (isDeleteModalOpen = false)}
            class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer"
          >
            Cancel / إلغاء
          </button>
          <button
            type="button"
            on:click={handleDeleteSession}
            disabled={isSubmittingDelete || (deleteConfirmText.trim().toUpperCase() !== 'DELETE' && deleteConfirmText.trim() !== 'حذف')}
            class="px-5 py-2 bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white text-xs font-black rounded-xl shadow-md cursor-pointer flex items-center gap-1.5"
          >
            {isSubmittingDelete ? 'Deleting...' : 'Delete Session / حذف'}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>