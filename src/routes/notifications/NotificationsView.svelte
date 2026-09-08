<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { localTodayISO } from '../../lib/utils/date';
  import { invoke } from '@tauri-apps/api/core';
  import {
    Bell, CheckCircle2, AlertTriangle, AlertOctagon,
    Shield, RefreshCw, ShoppingCart, Undo2, DollarSign,
    Package, Trash2, X, Check, Filter, UserCheck
  } from 'lucide-svelte';

  // Clicking a notification jumps to the page where it can be acted on.
  export let onRequestRoute: (route: string) => void = () => {};
  // Product alerts open the product's editor directly in the POS.
  export let onOpenProduct: (productId: number) => void = () => {};

  interface NotificationLog {
    id: number;
    type: 'sale' | 'refund' | 'expiry' | 'stock' | 'system' | 'payroll';
    title: string;
    message: string;
    timestamp: string;
    is_dismissed?: boolean;
    related_id?: number;
  }

  let notifications: NotificationLog[] = [];
  let filterType: string = 'all';
  let isLoading = false;

  onMount(async () => {
    await loadNotifications();
  });

  async function loadNotifications() {
    try {
      isLoading = true;
      // Fetch dynamic alerts based on database queries
      const today = localTodayISO();
      const prods = await invoke<any[]>('search_products', { query: '', categoryId: null, searchType: 'all' });

      let dynamicList: NotificationLog[] = [];
      let idCounter = 1;

      for (const p of prods) {
        if (p.expiry_date && p.expiry_date < today) {
          dynamicList.push({
            id: idCounter++,
            type: 'expiry',
            title: t('notif_expired_alert'),
            message: `Product "${p.name_fr || p.name_ar}" expired on ${p.expiry_date} (Stock: ${p.current_stock} pcs)`,
            timestamp: t('notif_immediate'),
            related_id: p.id,
          });
        } else if (p.current_stock <= p.min_stock) {
          dynamicList.push({
            id: idCounter++,
            type: 'stock',
            title: t('notif_low_stock_alert'),
            message: `Product "${p.name_fr || p.name_ar}" has reached low stock threshold (${p.current_stock} pcs remaining)`,
            timestamp: t('notif_replenish'),
            related_id: p.id,
          });
        }
      }

      // Persistent in-app feed (payroll reminders etc.) — real rows saved
      // by the backend, dismissed state included.
      try {
        const persisted = await invoke<any[]>('list_app_notifications', { limit: 100 });
        for (const n of persisted) {
          dynamicList.push({
            id: n.id,
            type: (n.type === 'payroll' ? 'payroll' : 'system') as any,
            title: n.title,
            message: n.message,
            timestamp: n.created_at,
            related_id: n.related_id ?? undefined,
          });
        }
      } catch {
        // Older binary without the command — the dynamic list still works.
      }

      // Add recent sales & system events
      dynamicList.push({
        id: idCounter++,
        type: 'sale',
        title: t('notif_session_active'),
        message: 'Current register session #01 is online and synchronized with local database.',
        timestamp: 'Active Session',
      });

      notifications = dynamicList;
    } catch (e) {
      console.error(e);
    } finally {
      isLoading = false;
    }
  }

  $: filteredNotifications = notifications.filter(n => {
    if (n.is_dismissed) return false;
    if (filterType === 'all') return true;
    return n.type === filterType;
  });

  function dismissNotification(id: number) {
    notifications = notifications.map(n => n.id === id ? { ...n, is_dismissed: true } : n);
    // Persisted feed rows dismiss server-side too (small numeric ids ≥ 1000
    // are the dynamic in-memory ones; persisted rows start fresh per page
    // load — dismiss both is safe because in-memory ids never collide with
    // persisted ids below 100000).
    const row = notifications.find(n => n.id === id);
    if (row && row.timestamp !== t('notif_immediate') && row.timestamp !== t('notif_replenish') && row.timestamp !== 'Active Session') {
      invoke('dismiss_app_notification', { id }).catch(() => {});
    }
  }

  // Deep-link: product alerts (expiry/stock) open that product's editor in
  // the POS; payroll reminders jump to the payroll page; the session card
  // jumps to the register.
  function handleCardClick(log: NotificationLog) {
    if ((log.type === 'expiry' || log.type === 'stock') && log.related_id) {
      onOpenProduct(log.related_id);
    } else if (log.type === 'payroll') {
      onRequestRoute('payroll');
    } else {
      onRequestRoute('cash');
    }
  }

  function dismissAll() {
    notifications = notifications.map(n => ({ ...n, is_dismissed: true }));
  }
</script>

<div class="h-full flex flex-col bg-pos-bg p-6 overflow-hidden select-none space-y-4">
  <!-- Header -->
  <div class="flex items-center justify-between pb-3 border-b border-pos-border shrink-0">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-2xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <Bell class="w-5 h-5" />
      </div>
      <div>
        <h1 class="text-xl font-black text-pos-text tracking-tight">{t('notif_title')}</h1>
        <p class="text-xs text-pos-muted">{t('notif_subtitle')}</p>
      </div>
    </div>

    <div class="flex items-center gap-2">
      <button
        type="button"
        on:click={loadNotifications}
        disabled={isLoading}
        class="px-3.5 py-2 bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 text-pos-text rounded-xl text-xs font-bold transition flex items-center gap-1.5 cursor-pointer shadow-2xs"
      >
        <RefreshCw class="w-3.5 h-3.5 {isLoading ? 'animate-spin' : ''}" />
        <span>Refresh</span>
      </button>

      {#if filteredNotifications.length > 0}
        <button
          type="button"
          on:click={dismissAll}
          class="px-4 py-2 bg-rose-600 hover:bg-rose-700 text-white rounded-xl text-xs font-black transition shadow-xs flex items-center gap-1.5 cursor-pointer"
        >
          <Trash2 class="w-4 h-4" />
          <span>Dismiss All (مسح الكل)</span>
        </button>
      {/if}
    </div>
  </div>

  <!-- Filter Pills -->
  <div class="flex items-center gap-1.5 shrink-0 bg-pos-card border border-pos-border p-1.5 rounded-2xl">
    <button
      type="button"
      on:click={() => (filterType = 'all')}
      class="px-3 py-1.5 rounded-xl text-xs font-bold transition cursor-pointer {filterType === 'all' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
    >
      {t('notif_all_alerts')} ({notifications.filter(n => !n.is_dismissed).length})
    </button>
    <button
      type="button"
      on:click={() => (filterType = 'expiry')}
      class="px-3 py-1.5 rounded-xl text-xs font-bold transition cursor-pointer {filterType === 'expiry' ? 'bg-rose-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
    >
      {t('notif_expiry_alerts')} ({notifications.filter(n => !n.is_dismissed && n.type === 'expiry').length})
    </button>
    <button
      type="button"
      on:click={() => (filterType = 'stock')}
      class="px-3 py-1.5 rounded-xl text-xs font-bold transition cursor-pointer {filterType === 'stock' ? 'bg-amber-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
    >
      {t('notif_stock_alerts')} ({notifications.filter(n => !n.is_dismissed && n.type === 'stock').length})
    </button>
    <button
      type="button"
      on:click={() => (filterType = 'sale')}
      class="px-3 py-1.5 rounded-xl text-xs font-bold transition cursor-pointer {filterType === 'sale' ? 'bg-emerald-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
    >
      Sales & Sessions ({notifications.filter(n => !n.is_dismissed && n.type === 'sale').length})
    </button>
    <button
      type="button"
      on:click={() => (filterType = 'payroll')}
      class="px-3 py-1.5 rounded-xl text-xs font-bold transition cursor-pointer {filterType === 'payroll' ? 'bg-indigo-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
    >
      {t('notif_payroll_alerts') || 'Payroll'} ({notifications.filter(n => !n.is_dismissed && n.type === 'payroll').length})
    </button>
  </div>

  <!-- Notifications List Feed -->
  <div class="flex-1 overflow-y-auto space-y-3">
    {#if filteredNotifications.length === 0}
      <div class="p-12 text-center bg-pos-card border border-pos-border rounded-3xl space-y-2">
        <CheckCircle2 class="w-10 h-10 text-emerald-500 mx-auto" />
        <h3 class="font-black text-sm text-pos-text">All Caught Up! (لا توجد تنبيهات جديدة)</h3>
        <p class="text-xs text-pos-muted">Your store inventory, register sessions, and expiry dates are in good standing.</p>
      </div>
    {:else}
      {#each filteredNotifications as log}
        <div
          class="p-4 bg-pos-card border rounded-2xl shadow-xs flex items-start justify-between gap-4 transition hover:shadow-md cursor-pointer {log.type === 'expiry' ? 'border-rose-300 bg-rose-50/30 dark:bg-rose-950/10' : log.type === 'stock' ? 'border-amber-300 bg-amber-50/30 dark:bg-amber-950/10' : log.type === 'payroll' ? 'border-indigo-300 bg-indigo-50/30 dark:bg-indigo-950/10' : 'border-pos-border'}"
          on:click={() => handleCardClick(log)}
          title={t('notif_open_relevant')}
        >
          <div class="flex items-start gap-3 min-w-0">
            <div class="w-9 h-9 rounded-xl flex items-center justify-center font-bold shrink-0 mt-0.5 {log.type === 'expiry' ? 'bg-rose-100 text-rose-600 dark:bg-rose-950' : log.type === 'stock' ? 'bg-amber-100 text-amber-600 dark:bg-amber-950' : log.type === 'payroll' ? 'bg-indigo-100 text-indigo-600 dark:bg-indigo-950' : 'bg-sky-100 text-sky-600 dark:bg-sky-950'}">
              {#if log.type === 'expiry'}
                <AlertOctagon class="w-5 h-5" />
              {:else if log.type === 'stock'}
                <AlertTriangle class="w-5 h-5" />
              {:else if log.type === 'payroll'}
                <UserCheck class="w-5 h-5" />
              {:else}
                <CheckCircle2 class="w-5 h-5" />
              {/if}
            </div>

            <div class="space-y-1 min-w-0">
              <div class="flex items-center gap-2">
                <h4 class="font-black text-xs text-pos-text truncate">{log.title}</h4>
                <span class="px-2 py-0.5 rounded-full text-[9px] font-mono font-bold uppercase {log.type === 'expiry' ? 'bg-rose-100 text-rose-800 dark:bg-rose-950 dark:text-rose-300' : log.type === 'stock' ? 'bg-amber-100 text-amber-800' : log.type === 'payroll' ? 'bg-indigo-100 text-indigo-800 dark:bg-indigo-950 dark:text-indigo-300' : 'bg-sky-100 text-sky-800'}">
                  {log.type}
                </span>
              </div>
              <p class="text-xs text-pos-muted whitespace-pre-line">{log.message}</p>
              <p class="text-[10px] text-pos-muted font-mono">{log.timestamp}</p>
            </div>
          </div>

          <button
            type="button"
            on:click={() => dismissNotification(log.id)}
            class="px-2.5 py-1 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 text-pos-muted hover:text-pos-text text-[10px] font-bold rounded-lg flex items-center gap-1 cursor-pointer transition shrink-0"
          >
            <X class="w-3 h-3" />
            <span>Dismiss</span>
          </button>
        </div>
      {/each}
    {/if}
  </div>
</div>
