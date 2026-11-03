<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { printHtmlSilently } from '../../lib/utils/printer';
  import { printService } from '../../lib/services/printService';
  import type { PrintableDocument, PrintableItem } from '../../lib/printing/printableDocument';
  import { pickReportFormat } from '../../lib/stores/reportFormat';
  import { Truck, Plus, RefreshCw, Printer, Package, X, Undo2, Eye, Pencil, Trash2, AlertTriangle } from 'lucide-svelte';

  let loads: any[] = [];

  // Deep-link from the Trucks page ("Modifier" on the active trip panel):
  // once the load list has landed, open the matching load in the edit form.
  export let focusLoadId: string | null = null;
  $: if (focusLoadId && loads.length > 0) {
    const target = loads.find((l) => l.id === focusLoadId);
    focusLoadId = null;
    if (target && loadEditable(target)) startEdit(target);
  }
  let staff: any[] = [];
  let routes: any[] = [];
  let trucksList: any[] = [];
  let loading = true;
  let error = '';
  let msg = '';

  // New load form
  let showForm = false;
  let loadName = '';
  let sellerId = '';
  let routeId = '';
  let routeDate = new Date().toISOString().slice(0, 10);
  let truckId = '';
  let driverName = '';
  let items: { product_id: string; product_name: string; quantity: number; unit_price: number; carry: number }[] = [];
  let busy = false;
  // Carry-over pool of the selected truck (0059): product_id -> on-board qty.
  let truckPool: Record<string, number> = {};

  // Returns modal
  let returnsLoad: any = null;
  let returnQty: Record<string, number> = {};

  async function load() {
    loading = true;
    error = '';
    try {
      [loads, staff, routes, trucksList] = await Promise.all([
        invoke<any[]>('cloud_truck_loads').catch(() => []),
        invoke<any[]>('cloud_field_staff').catch(() => []),
        invoke<any[]>('cloud_recent_routes', { limit: 30 }).catch(() => []),
        invoke<any[]>('cloud_list_trucks').catch(() => []),
      ]);
      const nameById: Record<string, string> = {};
      for (const m of staff) nameById[m.id] = m.full_name;
      // Enrich each route with seller name + stop count for the dropdown so
      // the user can't pick the wrong one blind.
      for (const r of routes) {
        r.seller_name = nameById[r.seller_id] ?? '—';
        try {
          const detail = await invoke<any>('cloud_route_detail', { routeId: r.id }).catch(() => null);
          r.stop_count = detail?.stops?.length ?? 0;
          r.stop_preview = (detail?.stops ?? [])
            .slice(0, 3)
            .map((st: any) => st.client?.name)
            .filter(Boolean)
            .join(', ');
        } catch { r.stop_count = 0; }
      }
      routes = [...routes];
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      loading = false;
    }
  }

  let viewingRoute: any = null;
  async function viewRoute(id: string) {
    viewingRoute = await invoke<any>('cloud_route_detail', { routeId: id });
  }

  async function fillFromRoute() {
    if (!routeId) {
      items = [];
      return;
    }
    try {
      const payload = await invoke<any>('cloud_route_order_items', { routeId });
      const lines = payload?.lines ?? [];
      // Aggregate per product (routes can serve the same product to several stops).
      const agg = new Map<string, { product_id: string; product_name: string; quantity: number; unit_price: number; carry: number }>();
      for (const ln of lines) {
        const pid = ln.product_id as string;
        const prev = agg.get(pid);
        if (prev) {
          prev.quantity += Number(ln.quantity ?? 0);
        } else {
          agg.set(pid, {
            product_id: pid,
            product_name: ln.product?.name || '—',
            quantity: Number(ln.quantity ?? 0),
            unit_price: Number(ln.unit_price ?? 0),
            carry: 0,
          });
        }
      }
      items = [...agg.values()];
      const r = routes.find((x) => x.id === routeId);
      if (r?.route_date) routeDate = String(r.route_date).slice(0, 10);
      if (r?.seller_id) sellerId = r.seller_id;
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
      items = [];
    }
  }

  // 0059 — pull the selected truck's carry-over pool and prefill the form:
  // pool products become (or top up) item lines with their on-board quantity
  // marked as carry. Carry consumes NO warehouse stock server-side.
  async function applyPool() {
    truckPool = {};
    if (editing || !truckId) return;
    let rows: any[] = [];
    try {
      rows = (await invoke<any[]>('cloud_truck_stock', { truckId }).catch(() => [])) ?? [];
    } catch { rows = []; }
    if (rows.length === 0) return;
    const byId = new Map(items.map((it) => [it.product_id, it]));
    for (const r of rows) {
      const pid = r.product_id as string;
      const qty = Number(r.quantity ?? 0);
      truckPool[pid] = qty;
      const it = byId.get(pid);
      if (it) {
        it.quantity += qty;
        it.carry = qty;
      } else {
        items = [...items, { product_id: pid, product_name: r.products?.name ?? '—', quantity: qty, unit_price: 0, carry: qty }];
      }
    }
    items = [...items];
  }

  // When set, the form edits this load instead of creating a new one.
  let editing: any = null;

  function openCreate() {
    editing = null;
    loadName = '';
    sellerId = '';
    routeId = '';
    truckId = '';
    driverName = '';
    items = [];
    truckPool = {};
    showForm = true;
  }

  function startEdit(l: any) {
    editing = l;
    loadName = l.name ?? '';
    sellerId = l.seller_id ?? '';
    routeId = l.route_id ?? '';
    routeDate = String(l.route_date).slice(0, 10);
    truckId = l.truck_id ?? '';
    driverName = l.driver_name ?? '';
    items = (l.items ?? []).map((it: any) => ({
      product_id: it.product_id as string,
      product_name: it.product?.name ?? '—',
      quantity: Number(it.quantity ?? 0),
      unit_price: 0,
      carry: 0,
    }));
    truckPool = {};
    showForm = true;
  }

  // Owner: statuses showed as raw enum text — translate (same map as Trucks).
  function statusLabel(s?: string | null): string {
    switch (s) {
      case 'loaded': return t('trucks_status_loaded');
      case 'in_progress': return t('trucks_status_inprogress');
      case 'reconciling': return t('trucks_status_reconciling');
      case 'closed': return t('trucks_status_closed');
      case 'returned': return t('trucks_status_returned');
      default: return s ?? '—';
    }
  }

  // 0059 — any trip that is not closed can be edited (returns recorded on the
  // load still block it server-side; the button hides there).
  $: loadEditable = (l: any) => l.status !== 'closed' && !(l.items ?? []).some((it: any) => (it.returned_quantity ?? 0) > 0);

  async function reopenLoad(l: any) {
    if (!confirm(t('tl_reopen_confirm'))) return;
    busy = true;
    error = '';
    try {
      await invoke('cloud_reopen_truck_trip', { loadId: l.id });
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  let deletingLoad: any = null;

  async function deleteLoad() {
    if (!deletingLoad) return;
    busy = true;
    error = '';
    try {
      await invoke('cloud_delete_truck_load', { loadId: deletingLoad.id });
      deletingLoad = null;
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function saveLoad() {
    busy = true;
    error = '';
    msg = '';
    try {
      const payload = items
        .filter((i) => i.quantity > 0)
        .map((i) => ({ product_id: i.product_id, quantity: i.quantity }));
      if (editing) {
        await invoke('cloud_update_truck_load', {
          loadId: editing.id,
          sellerId: sellerId || null,
          routeId: routeId || null,
          routeDate,
          items: payload,
          name: loadName || null,
          notes: null,
        });
      } else {
        // 0059: the on-board (carry) portion of each line — the server
        // validates it against the truck's pool and skips warehouse stock.
        const carryover = items
          .filter((i) => i.quantity > 0 && Math.min(i.carry, i.quantity) > 0)
          .map((i) => ({ product_id: i.product_id, quantity: Math.min(i.carry, i.quantity) }));
        await invoke('cloud_create_truck_load', {
          sellerId,
          routeId: routeId || null,
          routeDate,
          items: payload,
          name: loadName || null,
          truckId: truckId || null,
          driverName: driverName.trim() || null,
          carryover: carryover.length > 0 ? carryover : null,
        });
      }
      showForm = false;
      editing = null;
      items = [];
      routeId = '';
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  function openReturns(l: any) {
    returnsLoad = l;
    returnQty = {};
    for (const it of l.items ?? []) {
      returnQty[it.product?.name ?? String(it.quantity)] = 0;
    }
  }

  async function saveReturns() {
    if (!returnsLoad) return;
    busy = true;
    error = '';
    try {
      const returns = (returnsLoad.items ?? [])
        .map((it: any) => ({
          product_id: it.product_id,
          quantity: returnQty[it.product?.name ?? String(it.quantity)] ?? 0,
        }))
        .filter((r: any) => r.quantity > 0);
      if (returns.length === 0) {
        returnsLoad = null;
        return;
      }
      await invoke('cloud_record_truck_return', {
        loadId: returnsLoad.id,
        returns,
      });
      returnsLoad = null;
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function printSheet(l: any) {
    const items: PrintableItem[] = (l.items ?? []).map((it: any) => ({
      name: it.product?.name ?? 'Article',
      quantity: Number(it.quantity ?? 0),
      unitPrice: 0,
      totalPrice: 0,
      notes: it.returned_quantity ? `Retourné: ${it.returned_quantity}` : undefined,
    }));

    const doc: PrintableDocument = {
      id: l.id,
      documentNumber: `LOAD-${l.id}`,
      documentType: 'stock_operation',
      title: 'BON DE CHARGEMENT CAMION',
      date: String(l.route_date || new Date().toISOString()).slice(0, 10),
      party: l.seller ? {
        name: l.seller.full_name || 'Vendeur / Chauffeur',
        type: 'employee',
      } : undefined,
      items,
      subtotal: 0,
      discountTotal: 0,
      taxTotal: 0,
      grandTotal: 0,
      notes: `Statut: ${l.status || 'loaded'}${l.name ? ' • ' + l.name : ''}`,
      footerNote: 'Bon de chargement et de distribution mobile.',
    };

    const result = await printService.printDocument(doc);
    if (!result.ok && result.mode !== 'disabled') {
      error = result.message;
    }
  }

  onMount(load);
  $: fmt = (v: number) => new Intl.NumberFormat('fr-DZ').format(v);
</script>

<div class="p-4 md:p-6 space-y-4">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <Truck class="w-5 h-5 text-sky-500" />
        {t('tl_title')}
      </h1>
      <p class="text-xs text-pos-muted">Titaou One — {t('tl_returns')}</p>
    </div>
    <div class="flex gap-2">
      <button type="button" on:click={load} disabled={loading}
        class="p-2 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer">
        <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      </button>
      <button type="button" on:click={openCreate}
        class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-sky-600 hover:bg-sky-700 text-white rounded-xl cursor-pointer">
        <Plus class="w-3.5 h-3.5" />{t('tl_new')}
      </button>
    </div>
  </div>

  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">❌ {error}</p>
  {/if}

  {#if !loading && loads.length === 0 && !error}
    <p class="text-center text-pos-muted text-sm py-12">🚚 {t('tl_empty')}</p>
  {/if}

  {#if loads.length > 0}
    <div class="grid gap-3 md:grid-cols-2">
      {#each loads as l (l.id)}
        <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border space-y-2">
          <div class="flex items-start justify-between">
            <div>
              <p class="text-sm font-black text-pos-text">{l.name || l.seller?.full_name || '—'}</p>
              <p class="text-[10px] text-pos-muted">{String(l.route_date).slice(0, 10)} · {statusLabel(l.status)}</p>
            </div>
            <div class="flex gap-1">
              <button type="button" class="p-1.5 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer" on:click={() => printSheet(l)} title={t('tl_print')}>
                <Printer class="w-3.5 h-3.5" />
              </button>
              {#if loadEditable(l)}
                <button type="button" class="p-1.5 text-amber-600 hover:bg-amber-50 rounded-lg cursor-pointer" on:click={() => startEdit(l)} title={t('tl_edit')}>
                  <Pencil class="w-3.5 h-3.5" />
                </button>
              {/if}
              {#if l.status !== 'closed'}
                <button type="button" class="p-1.5 text-amber-600 hover:bg-amber-50 rounded-lg cursor-pointer" on:click={() => openReturns(l)} title={t('tl_returns')}>
                  <Undo2 class="w-3.5 h-3.5" />
                </button>
              {:else}
                <button type="button" class="p-1.5 text-sky-600 hover:bg-sky-50 rounded-lg cursor-pointer" on:click={() => reopenLoad(l)} disabled={busy} title={t('tl_reopen')}>
                  <Undo2 class="w-3.5 h-3.5" />
                </button>
              {/if}
              {#if loadEditable(l)}
                <button type="button" class="p-1.5 text-rose-600 hover:bg-rose-50 rounded-lg cursor-pointer" on:click={() => (deletingLoad = l)} title={t('tl_delete')}>
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              {/if}
            </div>
          </div>
          <div class="space-y-1">
            {#each l.items ?? [] as it (it.product?.name)}
              <div class="flex justify-between text-[11px] bg-slate-50 dark:bg-slate-800 rounded-lg px-2.5 py-1.5">
                <span class="font-bold text-pos-text truncate">{it.product?.name ?? '—'}</span>
                <span class="font-mono shrink-0">
                  {it.quantity}
                  {#if (it.returned_quantity ?? 0) > 0}
                    <span class="text-emerald-600">(+{it.returned_quantity})</span>
                  {/if}
                </span>
              </div>
            {/each}
          </div>
          {#if l.status === 'closed' && l.reopened_at}
            <p class="text-[10px] text-sky-600 font-bold">↻ {t('tl_reopened')}</p>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- New / edit load modal -->
{#if showForm}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showForm = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-xl max-h-[85vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between">
        <h3 class="text-sm font-black text-pos-text flex items-center gap-2"><Truck class="w-4 h-4 text-sky-500" />{editing ? t('tl_edit_title') : t('tl_new')}</h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (showForm = false)}><X class="w-4 h-4" /></button>
      </div>
      <div class="p-4 overflow-y-auto space-y-3">
        {#if editing && editing.status === 'in_progress'}
          <p class="text-[11px] font-bold text-amber-700 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800 rounded-xl px-3 py-2">⚠️ {t('tl_edit_active_warn')}</p>
        {/if}
        {#if editing && editing.status === 'reconciling'}
          <p class="text-[11px] font-bold text-sky-700 bg-sky-50 dark:bg-sky-950/40 border border-sky-200 dark:border-sky-800 rounded-xl px-3 py-2">ℹ️ {t('tl_edit_reconcile_warn')}</p>
        {/if}
        <div class="grid grid-cols-2 gap-2.5">
          <div class="col-span-2">
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('routes_name')}</label>
            <input type="text" bind:value={loadName} placeholder="Tournée eau — {routeDate}"
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
          </div>
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('tl_seller')}</label>
            <select bind:value={sellerId}
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
              <option value="">—</option>
              {#each staff as s (s.id)}<option value={s.id}>{s.full_name} ({s.role})</option>{/each}
            </select>
          </div>
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('trucks_plate')} / {t('nav_trucks')}</label>
            <select bind:value={truckId} on:change={() => { const tr = trucksList.find((x) => x.id === truckId); driverName = tr?.driver_name ?? ''; void applyPool(); }}
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
              <option value="">—</option>
              {#each trucksList as tr (tr.id)}<option value={tr.id}>{tr.name} ({tr.plate || '—'})</option>{/each}
            </select>
          </div>
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('trucks_driver')}</label>
            <input type="text" bind:value={driverName} placeholder="Ahmed"
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
          </div>
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('tl_date')}</label>
            <input type="date" bind:value={routeDate}
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
          </div>
          <div class="col-span-2">
            <label class="block text-[10px] font-black text-pos-muted mb-1">Route (auto-fill)</label>
            <div class="flex gap-1.5 items-center">
              <select bind:value={routeId} on:change={fillFromRoute}
                class="flex-1 px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
                <option value="">—</option>
                {#each routes as r (r.id)}
                  <option value={r.id}>
                    {String(r.route_date).slice(0, 10)} · {r.seller_name} · {r.stop_count} stops {r.stop_preview ? '· ' + r.stop_preview : ''}
                  </option>
                {/each}
              </select>
              {#if routeId}
                <button type="button" class="p-1.5 text-sky-600 hover:bg-sky-50 rounded-lg cursor-pointer"
                  title="View route" on:click={() => viewRoute(routeId)}>
                  <Eye class="w-4 h-4" />
                </button>
              {/if}
            </div>
          </div>
        </div>

        <div>
          <p class="text-[10px] font-black text-pos-muted uppercase mb-1.5">{t('tl_products')}</p>
          {#if items.length === 0}
            <p class="text-[11px] text-pos-muted">— {t('fo_empty')}</p>
          {/if}
          {#each items as it, idx (it.product_id)}
            <div class="flex items-center gap-2 mb-1.5">
              <Package class="w-3.5 h-3.5 text-pos-muted shrink-0" />
              <span class="text-xs font-bold text-pos-text flex-1 truncate">
                {it.product_name}
                {#if Math.min(it.carry, it.quantity) > 0}
                  <span class="text-[9px] font-black text-sky-700 bg-sky-50 dark:bg-sky-950/60 dark:text-sky-300 rounded-md px-1.5 py-0.5 ml-1 whitespace-nowrap">↻ {Math.min(it.carry, it.quantity)} {t('tl_carry')}</span>
                {/if}
              </span>
              <input type="number" step="1" min="0" bind:value={it.quantity}
                class="w-24 px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs text-pos-text font-mono outline-none" />
              {#if truckPool[it.product_id]}
                <button type="button" class="p-1 text-rose-500 hover:bg-rose-50 rounded-lg cursor-pointer shrink-0"
                  title={t('tl_carry_remove')}
                  on:click={() => { it.quantity -= Math.min(it.carry, it.quantity); it.carry = 0; items = [...items]; }}>
                  <X class="w-3 h-3" />
                </button>
              {/if}
            </div>
          {/each}
          {#if Object.keys(truckPool).length > 0}
            <p class="text-[10px] text-pos-muted">↻ {t('tl_carry_hint')}</p>
          {/if}
        </div>

        <div class="flex justify-end gap-2 pt-2">
          <button type="button" on:click={() => (showForm = false)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">✕</button>
          <button type="button" on:click={saveLoad} disabled={busy || !sellerId || items.length === 0}
            class="flex items-center gap-1.5 px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
            <Truck class="w-3.5 h-3.5" />{t('tl_save')}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- Delete load confirm -->
{#if deletingLoad}
  <div class="fixed inset-0 z-[60] bg-black/50 flex items-center justify-center p-4" on:click={() => (deletingLoad = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-rose-300 dark:border-rose-800 w-full max-w-xs p-5 space-y-3" on:click|stopPropagation>
      <div class="flex items-center gap-2 text-rose-600">
        <AlertTriangle class="w-5 h-5" />
        <h3 class="text-sm font-black">Supprimer le chargement ?</h3>
      </div>
      <p class="text-[11px] text-pos-muted">{deletingLoad.name || deletingLoad.seller?.full_name || '—'} — la marchandise revient au stock automatiquement.</p>
      <div class="flex justify-end gap-2">
        <button type="button" on:click={() => (deletingLoad = null)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">Annuler</button>
        <button type="button" on:click={deleteLoad} disabled={busy}
          class="px-4 py-2 text-[11px] font-black bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">Supprimer</button>
      </div>
    </div>
  </div>
{/if}

<!-- Returns modal -->
{#if returnsLoad}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (returnsLoad = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-md p-5 space-y-3" on:click|stopPropagation>
      <h3 class="text-sm font-black text-pos-text flex items-center gap-2"><Undo2 class="w-4 h-4 text-amber-500" />{t('tl_returns')}</h3>
      <div class="space-y-1.5 max-h-72 overflow-y-auto">
        {#each returnsLoad.items ?? [] as it (it.product?.name)}
          <div class="flex items-center gap-2">
            <span class="text-xs font-bold text-pos-text flex-1 truncate">{it.product?.name ?? '—'}</span>
            <span class="text-[10px] text-pos-muted font-mono">/ {it.quantity}</span>
            <input type="number" step="1" min="0" max={it.quantity}
              bind:value={returnQty[it.product?.name ?? String(it.quantity)]}
              class="w-24 px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs text-pos-text font-mono outline-none" />
          </div>
        {/each}
      </div>
      <div class="flex justify-end gap-2 pt-2">
        <button type="button" on:click={() => (returnsLoad = null)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">✕</button>
        <button type="button" on:click={saveReturns} disabled={busy}
          class="px-4 py-2 text-[11px] font-black bg-amber-600 hover:bg-amber-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">OK</button>
      </div>
    </div>
  </div>
{/if}

{#if viewingRoute}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (viewingRoute = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-md max-h-[80vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between sticky top-0 bg-inherit rounded-t-2xl">
        <h3 class="text-sm font-black text-pos-text">Feuille de route — {String(viewingRoute.route.route_date).slice(0, 10)}</h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (viewingRoute = null)}>
          <X class="w-4 h-4" />
        </button>
      </div>
      <div class="p-4 overflow-y-auto">
        <p class="text-xs font-bold text-pos-muted mb-2">{viewingRoute.route.seller?.full_name ?? '—'} · {viewingRoute.route.status}</p>
        <table class="w-full text-xs">
          <thead><tr class="text-pos-muted font-black">
            <th class="p-2 text-start">#</th><th class="p-2 text-start">Client</th><th class="p-2 text-end">Total</th>
          </tr></thead>
          <tbody>
            {#each viewingRoute.stops as st, i}
              <tr class="border-t border-pos-border">
                <td class="p-2">{i + 1}</td>
                <td class="p-2 font-bold">{st.client?.name ?? '—'}</td>
                <td class="p-2 text-end font-mono">{((st.order?.total_amount ?? 0) / 100).toLocaleString('fr-DZ')} DA</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </div>
{/if}
