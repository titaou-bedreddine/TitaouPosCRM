<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { Truck, Plus, RefreshCw, Printer, Package, X, Undo2, Eye } from 'lucide-svelte';

  let loads: any[] = [];
  let staff: any[] = [];
  let routes: any[] = [];
  let loading = true;
  let error = '';
  let msg = '';

  // New load form
  let showForm = false;
  let sellerId = '';
  let routeId = '';
  let routeDate = new Date().toISOString().slice(0, 10);
  let items: { product_id: string; product_name: string; quantity: number; unit_price: number }[] = [];
  let busy = false;

  // Returns modal
  let returnsLoad: any = null;
  let returnQty: Record<string, number> = {};

  async function load() {
    loading = true;
    error = '';
    try {
      [loads, staff, routes] = await Promise.all([
        invoke<any[]>('cloud_truck_loads').catch(() => []),
        invoke<any[]>('cloud_field_staff').catch(() => []),
        invoke<any[]>('cloud_recent_routes', { limit: 30 }).catch(() => []),
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
      const agg = new Map<string, { product_id: string; product_name: string; quantity: number; unit_price: number }>();
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

  async function createLoad() {
    busy = true;
    error = '';
    msg = '';
    try {
      const payload = items
        .filter((i) => i.quantity > 0)
        .map((i) => ({ product_id: i.product_id, quantity: i.quantity }));
      await invoke('cloud_create_truck_load', {
        sellerId,
        routeId: routeId || null,
        routeDate,
        items: payload,
      });
      showForm = false;
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

  function printSheet(l: any) {
    const rows = (l.items ?? [])
      .map(
        (it: any) =>
          `<tr><td style="padding:4px 8px;border-bottom:1px solid #ccc">${it.product?.name ?? '—'}</td>
           <td style="padding:4px 8px;border-bottom:1px solid #ccc;text-align:right">${it.quantity}</td>
           <td style="padding:4px 8px;border-bottom:1px solid #ccc;text-align:right">${it.returned_quantity ?? 0}</td></tr>`
      )
      .join('');
    const html = `<!doctype html><html><head><meta charset="utf-8"><style>body{font-family:Segoe UI,Arial;font-size:12px}h2{margin:0 0 4px}table{width:100%}</style></head>
      <body><h2>${t('tl_title')}</h2>
      <p>${t('tl_seller')}: ${l.seller?.full_name ?? '—'} · ${String(l.route_date).slice(0, 10)} · ${l.status}</p>
      <table><tr><th align="start" style="padding:4px 8px">#</th><th align="end" style="padding:4px 8px">${t('tl_qty')}</th><th align="end" style="padding:4px 8px">${t('tl_returned')}</th></tr>${rows}</table></body></html>`;
    import('@tauri-apps/api/core')
      .then(({ invoke }) =>
        invoke('print_html_direct', {
          html,
          title: `${t('tl_title')} ${String(l.route_date).slice(0, 10)}`,
          paper: { widthMm: 80 },
        }).catch((e: any) => (error = String(e)))
      );
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
      <p class="text-xs text-pos-muted">TitaouCRM — {t('tl_returns')}</p>
    </div>
    <div class="flex gap-2">
      <button type="button" on:click={load} disabled={loading}
        class="p-2 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer">
        <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      </button>
      <button type="button" on:click={() => ((showForm = true), (items = []), (routeId = ''))}
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
              <p class="text-sm font-black text-pos-text">{l.seller?.full_name ?? '—'}</p>
              <p class="text-[10px] text-pos-muted">{String(l.route_date).slice(0, 10)} · {l.status}</p>
            </div>
            <div class="flex gap-1">
              <button type="button" class="p-1.5 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer" on:click={() => printSheet(l)} title={t('tl_print')}>
                <Printer class="w-3.5 h-3.5" />
              </button>
              {#if l.status !== 'closed'}
                <button type="button" class="p-1.5 text-amber-600 hover:bg-amber-50 rounded-lg cursor-pointer" on:click={() => openReturns(l)} title={t('tl_returns')}>
                  <Undo2 class="w-3.5 h-3.5" />
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
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- New load modal -->
{#if showForm}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showForm = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-xl max-h-[85vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between">
        <h3 class="text-sm font-black text-pos-text flex items-center gap-2"><Truck class="w-4 h-4 text-sky-500" />{t('tl_new')}</h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (showForm = false)}><X class="w-4 h-4" /></button>
      </div>
      <div class="p-4 overflow-y-auto space-y-3">
        <div class="grid grid-cols-2 gap-2.5">
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('tl_seller')}</label>
            <select bind:value={sellerId}
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
              <option value="">—</option>
              {#each staff as s (s.id)}<option value={s.id}>{s.full_name} ({s.role})</option>{/each}
            </select>
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
          {#each items as it (it.product_id)}
            <div class="flex items-center gap-2 mb-1.5">
              <Package class="w-3.5 h-3.5 text-pos-muted shrink-0" />
              <span class="text-xs font-bold text-pos-text flex-1 truncate">{it.product_name}</span>
              <input type="number" step="0.001" min="0" bind:value={it.quantity}
                class="w-24 px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs text-pos-text font-mono outline-none" />
            </div>
          {/each}
        </div>

        <div class="flex justify-end gap-2 pt-2">
          <button type="button" on:click={() => (showForm = false)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">{t('cloud_disconnect') ? '✕' : ''}</button>
          <button type="button" on:click={createLoad} disabled={busy || !sellerId || items.length === 0}
            class="flex items-center gap-1.5 px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
            <Truck class="w-3.5 h-3.5" />{t('tl_save')}
          </button>
        </div>
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
            <input type="number" step="0.001" min="0" max={it.quantity}
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
