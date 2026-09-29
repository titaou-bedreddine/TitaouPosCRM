<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import {
    Truck, Plus, RefreshCw, X, Pencil, Play, ClipboardList, Lock, Wallet,
    Package, Search, CircleDollarSign, TrendingDown, CheckCircle2, Fuel,
  } from 'lucide-svelte';
  import { printService } from '../../lib/services/printService';
  import { currentUser } from '../../lib/stores/auth';
  import { activeSession } from '../../lib/stores/session';

  let trucks: any[] = [];
  let staff: any[] = [];
  let products: any[] = [];
  let trips: any[] = []; // stats_truck_trips rows (canonical totals)
  let loads: any[] = []; // truck_loads with items (the trips)
  let loading = true;
  let error = '';
  let msg = '';
  let busy = false;
  let selectedId = '';

  $: selected = trucks.find((x) => x.id === selectedId) ?? null;
  $: sellerName = (id: string | null) => staff.find((s) => s.id === id)?.full_name ?? '—';
  $: truckTrips = trips.filter((tp) => tp.out_truck_id === selectedId);
  $: activeTrip = loads.find((l) => l.truck_id === selectedId && ['loaded', 'in_progress', 'reconciling'].includes(l.status)) ?? null;
  $: activeStats = activeTrip ? (trips.find((tp) => tp.out_load_id === activeTrip.id) ?? null) : null;

  // CRM money is integer centimes everywhere.
  const fmt = (v: number | null | undefined) =>
    (((v ?? 0) as number) / 100).toLocaleString('fr-DZ', { maximumFractionDigits: 2 });

  async function load() {
    loading = true;
    error = '';
    try {
      [trucks, staff, products, trips, loads] = await Promise.all([
        invoke<any[]>('cloud_list_trucks').catch(() => []),
        invoke<any[]>('cloud_field_staff').catch(() => []),
        invoke<any[]>('cloud_products_for_promos').catch(() => []),
        invoke<any[]>('cloud_stats_truck_trips', { fromDate: null, toDate: null }).catch(() => []),
        invoke<any[]>('cloud_truck_loads').catch(() => []),
      ]);
      if (!selectedId && trucks.length > 0) selectedId = trucks[0].id;
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      loading = false;
    }
  }

  // ── Truck create / edit ────────────────────────────────────────────────────
  let showTruckForm = false;
  let editingTruck: any = null;
  let truckName = '';
  let truckPlate = '';
  let truckDriver = '';
  let truckSeller = '';
  let truckActive = true;
  let truckNotes = '';

  function openCreateTruck() {
    editingTruck = null;
    truckName = ''; truckPlate = ''; truckDriver = ''; truckSeller = ''; truckActive = true; truckNotes = '';
    showTruckForm = true;
  }

  function openEditTruck(tr: any) {
    editingTruck = tr;
    truckName = tr.name ?? '';
    truckPlate = tr.plate ?? '';
    truckDriver = tr.driver_name ?? '';
    truckSeller = tr.seller_id ?? '';
    truckActive = tr.is_active !== false;
    truckNotes = tr.notes ?? '';
    showTruckForm = true;
  }

  async function saveTruck() {
    busy = true; error = '';
    try {
      await invoke('cloud_save_truck', {
        id: editingTruck?.id ?? null,
        name: truckName.trim(),
        plate: truckPlate.trim(),
        driverName: truckDriver.trim(),
        sellerId: truckSeller || null,
        isActive: truckActive,
        notes: truckNotes.trim() || null,
      });
      showTruckForm = false;
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  // ── New trip (load) for the selected truck ─────────────────────────────────
  let showTripForm = false;
  let tripSeller = '';
  let tripDate = new Date().toISOString().slice(0, 10);
  let tripName = '';
  let tripItems: { product_id: string; name: string; unit: string; unitsPerPackage: number; quantity: number }[] = [];
  let tripProductSearch = '';

  $: tripProductMatches = tripProductSearch.trim()
    ? products.filter((p) => p.name.toLowerCase().includes(tripProductSearch.toLowerCase())).slice(0, 6)
    : [];

  function packTypesFor(productId: string) {
    return (products.find((x) => x.id === productId)?.packagings ?? []);
  }

  $: tripBaseTotal = tripItems.reduce((sum, i) => sum + i.quantity * (i.unitsPerPackage || 1), 0);

  

  async function printTrip(copies: number) {
    if (!activeTrip || !stockRows.length) return;
    for (let c = 0; c < copies; c++) {
      const result = await printService.printDocument({
        id: activeTrip.id,
        documentNumber: 'TRIP-' + String(activeTrip.id).slice(0, 8),
        documentType: 'stock_operation',
        title: 'BON DE CHARGEMENT - TOURNEE',
        date: String(activeTrip.route_date || new Date().toISOString()).slice(0, 10),
        party: { name: (selected?.name || '') + ' - ' + (activeTrip.driver_name || ''), type: 'employee' },
        items: stockRows.map((r) => ({
          name: r.name,
          quantity: r.loaded,
          unitPrice: 0,
          totalPrice: 0,
          notes: 'Charge: ' + r.loaded + ' (base)',
        })),
        subtotal: 0, discountTotal: 0, taxTotal: 0, grandTotal: 0,
        notes: 'Vendeur: ' + sellerName(selected?.seller_id) + ' - Chauffeur: ' + (activeTrip.driver_name || '-'),
        footerNote: c === 0 ? 'Exemplaire depot / Warehouse copy' : 'Exemplaire camion / Truck copy',
      });
      if (!result.ok && result.mode !== 'disabled') error = result.message;
    }
  }

  function addTripProduct(p: any) {
    if (tripItems.some((i) => i.product_id === p.id)) return;
    tripItems = [...tripItems, { product_id: p.id, name: p.name, unit: 'Base', unitsPerPackage: 1, quantity: 1 }];
    tripProductSearch = '';
  }

  async function saveTrip() {
    if (!selected) return;
    busy = true; error = '';
    try {
      await invoke('cloud_create_truck_load', {
        sellerId: tripSeller,
        routeId: null,
        routeDate: tripDate,
        items: tripItems.map((i) => ({
          product_id: i.product_id,
          quantity: i.quantity * (i.unitsPerPackage || 1),
        })),
        notes: null,
        name: tripName.trim() || null,
        truckId: selected.id,
        driverName: selected.driver_name || null,
      });
      showTripForm = false;
      tripItems = []; tripName = ''; tripProductSearch = '';
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  // ── Active-trip stock rows: loaded / sold / returned / expected (I7 math) ──
  let stockRows: { product_id: string; name: string; loaded: number; sold: number; returned: number; expected: number }[] = [];

  $: refreshStock(activeTrip?.id);
  async function refreshStock(loadId: string | undefined) {
    if (!loadId) { stockRows = []; return; }
    try {
      const orders = await invoke<any[]>('cloud_trip_orders', { loadId }).catch(() => []);
      const soldBy: Record<string, number> = {};
      for (const o of orders ?? []) {
        for (const ln of o.items ?? []) {
          soldBy[ln.product_id] = (soldBy[ln.product_id] ?? 0) + Number(ln.base_quantity ?? ln.quantity ?? 0);
        }
      }
      stockRows = (activeTrip.items ?? []).map((it: any) => {
        const loaded = Number(it.quantity ?? 0);
        const returned = Number(it.returned_quantity ?? 0);
        const sold = soldBy[it.product_id] ?? 0;
        return { product_id: it.product_id, name: it.product?.name ?? '—', loaded, sold, returned, expected: loaded - sold - returned };
      });
    } catch {
      stockRows = [];
    }
  }

  // ── State machine actions (server enforces; UI only exposes the next step) ─
  async function startTrip() {
    if (!activeTrip) return;
    busy = true; error = '';
    try { await invoke('cloud_start_truck_trip', { loadId: activeTrip.id }); await load(); msg = '✅'; }
    catch (e: any) { error = typeof e === 'string' ? e : e?.message || 'Failed'; }
    finally { busy = false; }
  }

  async function openReconciliation() {
    if (!activeTrip) return;
    busy = true; error = '';
    try { await invoke('cloud_open_truck_reconciliation', { loadId: activeTrip.id }); await load(); msg = '✅'; }
    catch (e: any) { error = typeof e === 'string' ? e : e?.message || 'Failed'; }
    finally { busy = false; }
  }

  // ── CLOSE & SETTLE wizard: STOCK RECONCILIATION + FINANCIAL/CASH SETTLEMENT ─
  let showClose = false;
  let physical: Record<string, number> = {};
  let reasons: Record<string, string> = {};
  let actualCash = 0;
  let cashReason = '';

  $: previewExpectedCash = (activeStats?.out_cash_value ?? 0) - (activeStats?.out_expenses ?? 0);
  $: allDifferencesJustified = stockRows.every(
    (r) => Number(physical[r.product_id] ?? r.expected) - r.expected === 0 || (reasons[r.product_id] ?? '') !== '',
  );

  function openClose() {
    physical = {};
    reasons = {};
    for (const r of stockRows) physical[r.product_id] = r.expected;
    actualCash = Math.round(previewExpectedCash / 100) * 100 > 0 ? Math.round(previewExpectedCash) : 0;
    cashReason = '';
    showClose = true;
  }

  async function confirmClose() {
    if (!activeTrip) return;
    busy = true; error = '';
    try {
      const counts = stockRows.map((r) => ({
        product_id: r.product_id,
        quantity: Number(physical[r.product_id] ?? r.expected),
      }));
      const reasonMap: Record<string, string> = {};
      for (const r of stockRows) {
        const diff = Number(physical[r.product_id] ?? r.expected) - r.expected;
        if (diff !== 0) reasonMap[r.product_id] = reasons[r.product_id] ?? 'other';
      }
      await invoke('cloud_close_truck_trip', {
        loadId: activeTrip.id,
        counts,
        reasons: reasonMap,
        actualCash: Math.round(actualCash),
        cashReason: cashReason.trim() || null,
      });
      showClose = false;
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  // ── Trip expenses (explicit amounts only) ──────────────────────────────────
  let showExpense = false;
  let expCategory = 'fuel';
  let expAmount = 0;
  let expNotes = '';

  async function addExpense() {
    if (!activeTrip) return;
    busy = true; error = '';
    try {
      await invoke('cloud_add_truck_trip_expense', {
        loadId: activeTrip.id,
        category: expCategory,
        amount: Math.round(expAmount * 100),
        notes: expNotes.trim() || null,
      });
      showExpense = false;
      expAmount = 0; expNotes = '';
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  // ── Closed-trip audit (immutable) + deposit ────────────────────────────────
  let auditTrip: any = null;
  let auditData: any = null;

  async function viewAudit(tp: any) {
    auditTrip = tp;
    auditData = await invoke<any>('cloud_trip_settlement', { loadId: tp.out_load_id }).catch(() => null);
  }

  let depositBusy = '';
  async function deposit(tp: any) {
    if (!$activeSession || !$currentUser) { error = t('trucks_need_session'); return; }
    depositBusy = tp.out_load_id;
    try {
      const res = await invoke<any>('deposit_truck_settlement', {
        sessionId: $activeSession.id,
        userId: $currentUser.id,
        loadId: tp.out_load_id,
        amount: Math.round(tp.out_actual_cash ?? 0),
      });
      msg = res?.already_deposited ? t('trucks_already_deposited') : '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      depositBusy = '';
    }
  }

  const statusBadge: Record<string, string> = {
    loaded: 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-200',
    in_progress: 'bg-sky-100 text-sky-700 dark:bg-sky-950/60 dark:text-sky-300',
    reconciling: 'bg-amber-100 text-amber-700 dark:bg-amber-950/60 dark:text-amber-300',
    returned: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300',
    closed: 'bg-slate-100 text-pos-muted dark:bg-slate-800',
  };

  onMount(load);
</script>

<div class="p-4 md:p-6 space-y-4">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <Truck class="w-5 h-5 text-sky-500" />
        {t('trucks_title')}
      </h1>
      <p class="text-[10px] text-pos-muted">{t('trucks_subtitle')}</p>
    </div>
    <div class="flex gap-2">
      <button type="button" on:click={load} disabled={loading}
        class="p-2 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer">
        <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      </button>
      <button type="button" on:click={openCreateTruck}
        class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-sky-600 hover:bg-sky-700 text-white rounded-xl cursor-pointer">
        <Plus class="w-3.5 h-3.5" />{t('trucks_new')}
      </button>
    </div>
  </div>

  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">❌ {error}</p>
  {/if}
  {#if msg}
    <p class="text-[11px] font-bold text-emerald-600 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 rounded-xl px-3 py-2">{msg}</p>
  {/if}

  <div class="grid gap-4 lg:grid-cols-[280px_1fr]">
    <!-- Trucks list -->
    <div class="space-y-2">
      {#each trucks as tr (tr.id)}
        {@const at = loads.find((l) => l.truck_id === tr.id && ['loaded', 'in_progress', 'reconciling'].includes(l.status))}
        <button type="button" on:click={() => (selectedId = tr.id)}
          class="w-full text-start p-3 rounded-2xl border cursor-pointer transition {selectedId === tr.id ? 'border-sky-500 bg-sky-50 dark:bg-sky-950/30' : 'border-pos-border bg-white dark:bg-slate-900 hover:border-pos-muted'} {tr.is_active === false ? 'opacity-50' : ''}">
          <div class="flex items-center justify-between">
            <p class="text-sm font-black text-pos-text">{tr.name}</p>
            {#if at}
              <span class="text-[9px] font-black px-2 py-0.5 rounded-full {statusBadge[at.status]}">{at.status}</span>
            {/if}
          </div>
          <p class="text-[10px] text-pos-muted font-mono">{tr.plate || '—'}</p>
          <p class="text-[10px] text-pos-muted">👤 {tr.driver_name || '—'} · 🛒 {sellerName(tr.seller_id)}</p>
        </button>
      {/each}
      {#if !loading && trucks.length === 0}
        <p class="text-center text-pos-muted text-xs py-8">🚚 {t('trucks_empty')}</p>
      {/if}
    </div>

    <!-- Truck detail -->
    {#if selected}
      <div class="space-y-3">
        <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border flex items-start justify-between">
          <div>
            <p class="text-sm font-black text-pos-text">{selected.name} <span class="font-mono text-[10px] text-pos-muted">{selected.plate}</span></p>
            <p class="text-[10px] text-pos-muted">👤 {selected.driver_name || '—'} · 🛒 {sellerName(selected.seller_id)} · {selected.is_active === false ? '📦 ARCHIVED / ARCHIVÉ' : '✓ Active'}</p>
          </div>
          <div class="flex gap-1.5">
            <button type="button" on:click={() => openEditTruck(selected)} class="p-1.5 text-amber-600 hover:bg-amber-50 rounded-lg cursor-pointer" title="Modifier"><Pencil class="w-3.5 h-3.5" /></button>
            {#if !activeTrip && selected.is_active !== false}
              <button type="button" on:click={() => { tripSeller = selected.seller_id ?? ''; tripDate = new Date().toISOString().slice(0, 10); tripItems = []; tripName = ''; showTripForm = true; }}
                class="flex items-center gap-1 px-2.5 py-1.5 text-[10px] font-black bg-sky-600 hover:bg-sky-700 text-white rounded-xl cursor-pointer">
                <Plus class="w-3 h-3" />{t('trucks_new_trip')}
              </button>
            {/if}
          </div>
        </div>

        <!-- Active trip dashboard -->
        {#if activeTrip}
          <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border space-y-3">
            <div class="flex items-center justify-between">
              <div>
                <p class="text-xs font-black text-pos-text">{t('trucks_trip')} — {activeTrip.name || String(activeTrip.route_date).slice(0, 10)}</p>
                <p class="text-[10px] text-pos-muted">
                  👤 {activeTrip.driver_name || '—'} · {String(activeTrip.route_date).slice(0, 10)}
                  {#if activeTrip.departed_at}· 🕐 {String(activeTrip.departed_at).slice(0, 16).replace('T', ' ')}{/if}
                </p>
              </div>
              <span class="text-[9px] font-black px-2 py-0.5 rounded-full {statusBadge[activeTrip.status]}">{activeTrip.status}</span>
            </div>

            <!-- Stock table -->
            <div class="overflow-x-auto">
              <table class="w-full text-[10px]">
                <thead><tr class="text-pos-muted font-black uppercase">
                  <th class="p-1.5 text-start">{t('trucks_product')}</th>
                  <th class="p-1.5 text-end">{t('trucks_loaded')}</th>
                  <th class="p-1.5 text-end">{t('trucks_sold')}</th>
                  <th class="p-1.5 text-end">{t('trucks_returned')}</th>
                  <th class="p-1.5 text-end">{t('trucks_expected')}</th>
                </tr></thead>
                <tbody>
                  {#each stockRows as r (r.product_id)}
                    <tr class="border-t border-pos-border">
                      <td class="p-1.5 font-bold text-pos-text">{r.name}</td>
                      <td class="p-1.5 text-end font-mono">{r.loaded}</td>
                      <td class="p-1.5 text-end font-mono text-sky-600">{r.sold}</td>
                      <td class="p-1.5 text-end font-mono text-emerald-600">{r.returned}</td>
                      <td class="p-1.5 text-end font-mono font-black">{r.expected}</td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>

            <!-- Financials (canonical values from the DB) -->
            <div class="grid grid-cols-2 md:grid-cols-4 gap-2 text-center">
              <div class="p-2 bg-slate-50 dark:bg-slate-800 rounded-xl"><p class="text-[9px] font-black text-pos-muted uppercase">{t('trucks_gross')}</p><p class="text-xs font-black font-mono text-pos-text">{fmt(activeStats?.out_sales_value)} DA</p></div>
              <div class="p-2 bg-sky-50 dark:bg-sky-950/40 rounded-xl"><p class="text-[9px] font-black text-sky-600 uppercase">{t('trucks_cash')}</p><p class="text-xs font-black font-mono text-sky-600">{fmt(activeStats?.out_cash_value)} DA</p></div>
              <div class="p-2 bg-amber-50 dark:bg-amber-950/40 rounded-xl"><p class="text-[9px] font-black text-amber-600 uppercase">{t('trucks_credit')}</p><p class="text-xs font-black font-mono text-amber-600">{fmt(activeStats?.out_outstanding)} DA</p></div>
              <div class="p-2 bg-rose-50 dark:bg-rose-950/40 rounded-xl"><p class="text-[9px] font-black text-rose-600 uppercase">{t('trucks_expenses')}</p><p class="text-xs font-black font-mono text-rose-600">{fmt(activeStats?.out_expenses)} DA</p></div>
            </div>

            <div class="flex flex-wrap justify-end gap-1.5">
              <button type="button" on:click={() => printTrip(1)}
                class="flex items-center gap-1 px-2.5 py-1.5 text-[10px] font-black text-slate-600 hover:bg-slate-100 rounded-xl cursor-pointer" title="Print x1">🖨 x1</button>
              <button type="button" on:click={() => printTrip(2)}
                class="flex items-center gap-1 px-2.5 py-1.5 text-[10px] font-black text-slate-600 hover:bg-slate-100 rounded-xl cursor-pointer" title="Print x2">🖨 x2</button>
              <button type="button" on:click={() => (showExpense = true)}
                class="flex items-center gap-1 px-2.5 py-1.5 text-[10px] font-black text-rose-600 hover:bg-rose-50 rounded-xl cursor-pointer">
                <Fuel class="w-3 h-3" />{t('trucks_add_expense')}
              </button>
              {#if activeTrip.status === 'loaded'}
                <button type="button" on:click={startTrip} disabled={busy}
                  class="flex items-center gap-1 px-3 py-1.5 text-[10px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
                  <Play class="w-3 h-3" />{t('trucks_start')}
                </button>
              {:else if activeTrip.status === 'in_progress'}
                <button type="button" on:click={openReconciliation} disabled={busy}
                  class="flex items-center gap-1 px-3 py-1.5 text-[10px] font-black bg-amber-600 hover:bg-amber-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
                  <ClipboardList class="w-3 h-3" />{t('trucks_open_reconcile')}
                </button>
              {:else if activeTrip.status === 'reconciling'}
                <button type="button" on:click={openClose} disabled={busy}
                  class="flex items-center gap-1 px-3 py-1.5 text-[10px] font-black bg-emerald-600 hover:bg-emerald-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
                  <CheckCircle2 class="w-3 h-3" />{t('trucks_close_settle')}
                </button>
              {/if}
            </div>
          </div>
        {/if}

        <!-- Trip history -->
        <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border">
          <p class="text-[10px] font-black text-pos-muted uppercase mb-2">{t('trucks_history')}</p>
          {#if truckTrips.length === 0}
            <p class="text-[11px] text-pos-muted">—</p>
          {:else}
            <div class="overflow-x-auto">
              <table class="w-full text-[10px]">
                <thead><tr class="text-pos-muted font-black uppercase">
                  <th class="p-1.5 text-start">{t('trucks_date')}</th>
                  <th class="p-1.5 text-start">{t('trucks_status')}</th>
                  <th class="p-1.5 text-end">{t('trucks_gross')}</th>
                  <th class="p-1.5 text-end">{t('trucks_cash')}</th>
                  <th class="p-1.5 text-end">{t('trucks_expenses')}</th>
                  <th class="p-1.5 text-end">{t('trucks_expected')}</th>
                  <th class="p-1.5 text-end">{t('trucks_actual')}</th>
                  <th class="p-1.5 text-end">{t('trucks_diff')}</th>
                  <th class="p-1.5"></th>
                </tr></thead>
                <tbody>
                  {#each truckTrips as tp (tp.out_load_id)}
                    <tr class="border-t border-pos-border">
                      <td class="p-1.5 font-bold">{String(tp.out_route_date).slice(0, 10)}</td>
                      <td class="p-1.5"><span class="font-black px-1.5 py-0.5 rounded-full {statusBadge[tp.out_status] ?? ''}">{tp.out_status}</span></td>
                      <td class="p-1.5 text-end font-mono">{fmt(tp.out_sales_value)}</td>
                      <td class="p-1.5 text-end font-mono">{fmt(tp.out_cash_value)}</td>
                      <td class="p-1.5 text-end font-mono text-rose-600">{fmt(tp.out_expenses)}</td>
                      <td class="p-1.5 text-end font-mono">{fmt(tp.out_expected_cash)}</td>
                      <td class="p-1.5 text-end font-mono">{tp.out_actual_cash === null ? '—' : fmt(tp.out_actual_cash)}</td>
                      <td class="p-1.5 text-end font-mono font-black {tp.out_cash_difference === null ? '' : tp.out_cash_difference < 0 ? 'text-rose-600' : 'text-emerald-600'}">{tp.out_cash_difference === null ? '—' : fmt(tp.out_cash_difference)}</td>
                      <td class="p-1.5 text-end space-x-1">
                        {#if tp.out_status === 'closed'}
                          <button type="button" on:click={() => viewAudit(tp)} class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" title="Audit"><Lock class="w-3 h-3" /></button>
                          {#if (tp.out_actual_cash ?? 0) > 0}
                            <button type="button" on:click={() => deposit(tp)} disabled={depositBusy === tp.out_load_id || !$activeSession}
                              class="p-1 text-emerald-600 hover:bg-emerald-50 cursor-pointer disabled:opacity-40" title={t('trucks_deposit')}><Wallet class="w-3 h-3" /></button>
                          {/if}
                        {/if}
                      </td>
                    </tr>
                  {/each}
                </tbody>
              </table>
            </div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="flex items-center justify-center text-pos-muted text-xs py-16">← {t('trucks_pick')}</div>
    {/if}
  </div>
</div>

<!-- Truck create/edit modal -->
{#if showTruckForm}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showTruckForm = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-md max-h-[85vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between">
        <h3 class="text-sm font-black text-pos-text flex items-center gap-2"><Truck class="w-4 h-4 text-sky-500" />{editingTruck ? t('trucks_edit') : t('trucks_new')}</h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (showTruckForm = false)}><X class="w-4 h-4" /></button>
      </div>
      <div class="p-4 overflow-y-auto space-y-3">
        <div>
          <label class="block text-[10px] font-black text-pos-muted mb-1">{t('trucks_name')}</label>
          <input type="text" bind:value={truckName} placeholder="Truck 01"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
        </div>
        <div>
          <label class="block text-[10px] font-black text-pos-muted mb-1">{t('trucks_plate')}</label>
          <input type="text" bind:value={truckPlate} placeholder="12345-116-05"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold font-mono outline-none" />
        </div>
        <div>
          <label class="block text-[10px] font-black text-pos-muted mb-1">{t('trucks_driver')}</label>
          <input type="text" bind:value={truckDriver} placeholder="Ahmed"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
        </div>
        <div>
          <label class="block text-[10px] font-black text-pos-muted mb-1">{t('tl_seller')}</label>
          <select bind:value={truckSeller}
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
            <option value="">—</option>
            {#each staff as s (s.id)}<option value={s.id}>{s.full_name} ({s.role})</option>{/each}
          </select>
        </div>
        <label class="flex items-center gap-2 text-[11px] font-bold text-pos-text cursor-pointer">
          <input type="checkbox" bind:checked={truckActive} class="accent-sky-600" />{t('trucks_active')}
        </label>
        <div>
          <label class="block text-[10px] font-black text-pos-muted mb-1">{t('trucks_notes')}</label>
          <textarea bind:value={truckNotes} rows="2"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text outline-none"></textarea>
        </div>
        <div class="flex justify-between items-center pt-1">
          {#if editingTruck && !(loads.some((l) => l.truck_id === editingTruck.id))}
            <button type="button" on:click={async () => {
              busy = true; error = '';
              try {
                await invoke('cloud_delete_truck', { id: editingTruck.id });
                showTruckForm = false; selectedId = '';
                await load();
                msg = '✅';
              } catch (e: any) {
                error = typeof e === 'string' ? e : e?.message || 'Failed';
              } finally { busy = false; }
            }} class="px-3 py-2 text-[11px] font-black bg-rose-600 hover:bg-rose-700 text-white rounded-xl cursor-pointer">🗑 {t('promo_confirm_delete')}</button>
          {/if}
          <div class="flex justify-end gap-2 ms-auto">
            <button type="button" on:click={() => (showTruckForm = false)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">✕</button>
            <button type="button" on:click={saveTruck} disabled={busy || !truckName.trim()}
              class="px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">{t('trucks_save')}</button>
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- New trip (load) modal -->
{#if showTripForm && selected}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showTripForm = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-xl max-h-[85vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between">
        <h3 class="text-sm font-black text-pos-text flex items-center gap-2"><Truck class="w-4 h-4 text-sky-500" />{t('trucks_new_trip')} — {selected.name}</h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (showTripForm = false)}><X class="w-4 h-4" /></button>
      </div>
      <div class="p-4 overflow-y-auto space-y-3">
        <p class="text-[10px] text-pos-muted">👤 {t('trucks_driver')}: <b>{selected.driver_name || '—'}</b></p>
        <div class="grid grid-cols-2 gap-2.5">
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('tl_seller')}</label>
            <select bind:value={tripSeller}
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
              <option value="">—</option>
              {#each staff as s (s.id)}<option value={s.id}>{s.full_name} ({s.role})</option>{/each}
            </select>
          </div>
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('tl_date')}</label>
            <input type="date" bind:value={tripDate}
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
          </div>
          <div class="col-span-2">
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('routes_name')}</label>
            <input type="text" bind:value={tripName} placeholder="Tournée {selected.name} — {tripDate}"
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
          </div>
        </div>

        <div>
          <p class="text-[10px] font-black text-pos-muted uppercase mb-1.5">{t('tl_products')}</p>
          <div class="relative mb-1.5">
            <Search class="w-3.5 h-3.5 absolute start-2.5 top-2.5 text-pos-muted" />
            <input type="text" bind:value={tripProductSearch} placeholder={t('trucks_add_product')}
              class="w-full ps-8 pe-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
            {#if tripProductMatches.length > 0}
              <div class="absolute z-10 w-full mt-1 bg-white dark:bg-slate-900 border border-pos-border rounded-xl shadow-lg overflow-hidden">
                {#each tripProductMatches as p (p.id)}
                  <button type="button" on:click={() => addTripProduct(p)}
                    class="w-full text-start px-3 py-2 text-xs font-bold text-pos-text hover:bg-sky-50 dark:hover:bg-sky-950/40 cursor-pointer">{p.name}</button>
                {/each}
              </div>
            {/if}
          </div>
          {#each tripItems as it (it.product_id)}
            <div class="flex items-center gap-2 mb-1.5">
              <Package class="w-3.5 h-3.5 text-pos-muted shrink-0" />
              <span class="text-xs font-bold text-pos-text flex-1 truncate">{it.name}</span>
              <select bind:value={it.unit}
                on:change={() => { const pk = (packTypesFor(it.product_id) || []).find((pk2: any) => pk2.name === it.unit); it.unitsPerPackage = pk ? Number(pk.units_per_package) : 1; }}
                class="w-28 px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs text-pos-text font-bold outline-none">
                <option value="Base">Base</option>
                {#each (packTypesFor(it.product_id) || []) as pk (pk.name)}
                  <option value={pk.name}>{pk.name} x{pk.units_per_package}</option>
                {/each}
              </select>
              <input type="number" step="1" min="0" bind:value={it.quantity}
                class="w-20 px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs text-pos-text font-mono outline-none" />
              <span class="text-[9px] text-pos-muted font-mono whitespace-nowrap">= {it.quantity * (it.unitsPerPackage || 1)} base</span>
            </div>
          {/each}
          {#if tripItems.length > 0}
            <p class="text-[10px] font-black text-sky-600">{t('trucks_loaded')}: {tripBaseTotal} base units / unités de base</p>
          {/if}
        </div>

        <div class="flex justify-end gap-2 pt-2">
          <button type="button" on:click={() => (showTripForm = false)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">✕</button>
          <button type="button" on:click={saveTrip} disabled={busy || !tripSeller || tripItems.length === 0}
            class="flex items-center gap-1.5 px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
            <Truck class="w-3.5 h-3.5" />{t('tl_save')}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- CLOSE & SETTLE wizard -->
{#if showClose && activeTrip}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showClose = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-2xl max-h-[90vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between">
        <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
          <ClipboardList class="w-4 h-4 text-emerald-500" />
          {t('trucks_close_settle')} — {selected?.name} · {String(activeTrip.route_date).slice(0, 10)}
        </h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (showClose = false)}><X class="w-4 h-4" /></button>
      </div>
      <div class="p-4 overflow-y-auto space-y-4">
        <!-- STOCK RECONCILIATION -->
        <div>
          <p class="text-[10px] font-black text-pos-muted uppercase mb-1.5">{t('trucks_stock_reconciliation')}</p>
          <table class="w-full text-[10px]">
            <thead><tr class="text-pos-muted font-black uppercase">
              <th class="p-1.5 text-start">{t('trucks_product')}</th>
              <th class="p-1.5 text-end">{t('trucks_loaded')}</th>
              <th class="p-1.5 text-end">{t('trucks_sold')}</th>
              <th class="p-1.5 text-end">{t('trucks_returned')}</th>
              <th class="p-1.5 text-end">{t('trucks_expected')}</th>
              <th class="p-1.5 text-end">{t('trucks_physical')}</th>
              <th class="p-1.5 text-end">{t('trucks_diff')}</th>
              <th class="p-1.5 text-start">{t('trucks_reason')}</th>
            </tr></thead>
            <tbody>
              {#each stockRows as r (r.product_id)}
                {@const phys = Number(physical[r.product_id] ?? r.expected)}
                {@const diff = phys - r.expected}
                <tr class="border-t border-pos-border">
                  <td class="p-1.5 font-bold text-pos-text">{r.name}</td>
                  <td class="p-1.5 text-end font-mono">{r.loaded}</td>
                  <td class="p-1.5 text-end font-mono">{r.sold}</td>
                  <td class="p-1.5 text-end font-mono">{r.returned}</td>
                  <td class="p-1.5 text-end font-mono font-black">{r.expected}</td>
                  <td class="p-1.5 text-end">
                    <input type="number" step="1" bind:value={physical[r.product_id]}
                      class="w-20 px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs text-pos-text font-mono outline-none" />
                  </td>
                  <td class="p-1.5 text-end font-mono font-black {diff < 0 ? 'text-rose-600' : diff > 0 ? 'text-amber-600' : 'text-emerald-600'}">{diff}</td>
                  <td class="p-1.5">
                    <select bind:value={reasons[r.product_id]} disabled={diff === 0}
                      class="w-28 px-1.5 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-[10px] font-bold text-pos-text outline-none disabled:opacity-40">
                      <option value="">—</option>
                      <option value="damaged">{t('trucks_r_damaged')}</option>
                      <option value="lost">{t('trucks_r_lost')}</option>
                      <option value="counting_error">{t('trucks_r_counting')}</option>
                      <option value="other">{t('trucks_r_other')}</option>
                    </select>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>

        <!-- FINANCIAL / CASH SETTLEMENT -->
        <div>
          <p class="text-[10px] font-black text-pos-muted uppercase mb-1.5">{t('trucks_cash_settlement')}</p>
          <div class="grid grid-cols-2 gap-2">
            <div class="p-2.5 bg-sky-50 dark:bg-sky-950/40 rounded-xl"><p class="text-[9px] font-black text-sky-600 uppercase">{t('trucks_cash_sales')}</p><p class="text-sm font-black font-mono text-sky-600">{fmt(activeStats?.out_cash_value)} DA</p></div>
            <div class="p-2.5 bg-rose-50 dark:bg-rose-950/40 rounded-xl"><p class="text-[9px] font-black text-rose-600 uppercase">{t('trucks_expenses')}</p><p class="text-sm font-black font-mono text-rose-600">{fmt(activeStats?.out_expenses)} DA</p></div>
            <div class="p-2.5 bg-slate-50 dark:bg-slate-800 rounded-xl"><p class="text-[9px] font-black text-pos-muted uppercase">{t('trucks_expected_return')}</p><p class="text-sm font-black font-mono text-pos-text">{fmt(previewExpectedCash)} DA</p></div>
            <div class="p-2.5 bg-emerald-50 dark:bg-emerald-950/40 rounded-xl">
              <p class="text-[9px] font-black text-emerald-600 uppercase">{t('trucks_actual_return')}</p>
              <input type="number" bind:value={actualCash} step="1"
                class="w-full mt-0.5 px-2 py-1 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-sm font-black font-mono text-pos-text outline-none" />
            </div>
          </div>
          <div class="mt-2 flex items-center justify-between p-2.5 rounded-xl border {actualCash - previewExpectedCash < 0 ? 'border-rose-300 bg-rose-50 dark:bg-rose-950/40' : 'border-emerald-300 bg-emerald-50 dark:bg-emerald-950/40'}">
            <span class="text-[10px] font-black text-pos-muted uppercase">{t('trucks_cash_diff')}</span>
            <span class="text-sm font-black font-mono {actualCash - previewExpectedCash < 0 ? 'text-rose-600' : 'text-emerald-600'}">
              {fmt(actualCash - previewExpectedCash)} DA
            </span>
          </div>
          <input type="text" bind:value={cashReason} placeholder={t('trucks_cash_reason')}
            class="w-full mt-2 px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
        </div>

        <div class="flex justify-end gap-2 pt-1">
          <button type="button" on:click={() => (showClose = false)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">✕</button>
          <button type="button" on:click={confirmClose} disabled={busy || !allDifferencesJustified}
            class="flex items-center gap-1.5 px-4 py-2 text-[11px] font-black bg-emerald-600 hover:bg-emerald-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
            <CheckCircle2 class="w-3.5 h-3.5" />{t('trucks_confirm_close')}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- Trip expense modal -->
{#if showExpense && activeTrip}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showExpense = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-xs p-5 space-y-3" on:click|stopPropagation>
      <h3 class="text-sm font-black text-pos-text flex items-center gap-2"><TrendingDown class="w-4 h-4 text-rose-500" />{t('trucks_add_expense')}</h3>
      <div>
        <label class="block text-[10px] font-black text-pos-muted mb-1">{t('trucks_category')}</label>
        <select bind:value={expCategory}
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
          <option value="loading_fee">{t('trucks_c_loading')}</option>
          <option value="unloading_fee">{t('trucks_c_unloading')}</option>
          <option value="fuel">{t('trucks_c_fuel')}</option>
          <option value="driver_fee">{t('trucks_c_driver')}</option>
          <option value="other">{t('trucks_r_other')}</option>
        </select>
      </div>
      <div>
        <label class="block text-[10px] font-black text-pos-muted mb-1">{t('trucks_amount')}</label>
        <input type="number" bind:value={expAmount} min="0" step="1"
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-sm font-black font-mono text-pos-text outline-none" />
      </div>
      <input type="text" bind:value={expNotes} placeholder={t('trucks_notes')}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text outline-none" />
      <div class="flex justify-end gap-2">
        <button type="button" on:click={() => (showExpense = false)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">✕</button>
        <button type="button" on:click={addExpense} disabled={busy || expAmount <= 0}
          class="px-4 py-2 text-[11px] font-black bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">OK</button>
      </div>
    </div>
  </div>
{/if}

<!-- Closed-trip immutable audit view -->
{#if auditTrip}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (auditTrip = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-xl max-h-[85vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between">
        <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
          <Lock class="w-4 h-4 text-slate-500" />{t('trucks_audit')} — {String(auditTrip.out_route_date).slice(0, 10)}
        </h3>
        <span class="text-[9px] font-black px-2 py-1 rounded-full bg-slate-100 text-pos-muted dark:bg-slate-800">CLOSED — READ ONLY</span>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (auditTrip = null)}><X class="w-4 h-4" /></button>
      </div>
      <div class="p-4 overflow-y-auto space-y-3 text-[11px]">
        {#if auditData?.settlement}
          <div class="grid grid-cols-2 gap-2">
            <div class="p-2 bg-slate-50 dark:bg-slate-800 rounded-xl"><p class="text-[9px] font-black text-pos-muted uppercase">{t('trucks_expected_return')}</p><p class="font-black font-mono">{fmt(auditData.settlement.expected_cash)} DA</p></div>
            <div class="p-2 bg-slate-50 dark:bg-slate-800 rounded-xl"><p class="text-[9px] font-black text-pos-muted uppercase">{t('trucks_actual_return')}</p><p class="font-black font-mono">{fmt(auditData.settlement.actual_cash)} DA</p></div>
            <div class="p-2 bg-slate-50 dark:bg-slate-800 rounded-xl"><p class="text-[9px] font-black text-pos-muted uppercase">{t('trucks_cash_diff')}</p><p class="font-black font-mono {(auditData.settlement.cash_difference ?? 0) < 0 ? 'text-rose-600' : 'text-emerald-600'}">{fmt(auditData.settlement.cash_difference)} DA</p></div>
            <div class="p-2 bg-slate-50 dark:bg-slate-800 rounded-xl"><p class="text-[9px] font-black text-pos-muted uppercase">{t('trucks_reason')}</p><p class="font-bold">{auditData.settlement.reason || '—'}</p></div>
          </div>
        {/if}
        {#if (auditData?.reconciliations ?? []).length > 0}
          <div>
            <p class="text-[10px] font-black text-pos-muted uppercase mb-1">{t('trucks_stock_reconciliation')}</p>
            <table class="w-full text-[10px]">
              <thead><tr class="text-pos-muted font-black uppercase">
                <th class="p-1.5 text-start">{t('trucks_product')}</th>
                <th class="p-1.5 text-end">{t('trucks_expected')}</th>
                <th class="p-1.5 text-end">{t('trucks_physical')}</th>
                <th class="p-1.5 text-end">{t('trucks_diff')}</th>
                <th class="p-1.5 text-start">{t('trucks_reason')}</th>
              </tr></thead>
              <tbody>
                {#each auditData.reconciliations as r (r.id)}
                  <tr class="border-t border-pos-border">
                    <td class="p-1.5 font-bold">{r.product?.name ?? '—'}</td>
                    <td class="p-1.5 text-end font-mono">{r.expected_quantity}</td>
                    <td class="p-1.5 text-end font-mono">{r.physical_quantity}</td>
                    <td class="p-1.5 text-end font-mono font-black {r.difference < 0 ? 'text-rose-600' : r.difference > 0 ? 'text-amber-600' : ''}">{r.difference}</td>
                    <td class="p-1.5">{r.reason}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}
        {#if (auditData?.expenses ?? []).length > 0}
          <div>
            <p class="text-[10px] font-black text-pos-muted uppercase mb-1">{t('trucks_expenses')}</p>
            {#each auditData.expenses as e (e.id)}
              <div class="flex justify-between py-1 border-t border-pos-border">
                <span class="font-bold">{e.category}{e.notes ? ' — ' + e.notes : ''}</span>
                <span class="font-mono text-rose-600">{fmt(e.amount)} DA</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
