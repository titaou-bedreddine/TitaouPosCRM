<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { Route, Plus, RefreshCw, Trash2, X, Eye, Printer, Edit2, AlertTriangle } from 'lucide-svelte';

  let routes: any[] = [];
  let staff: any[] = [];
  let routable: any[] = [];
  let loading = true;
  let error = '';
  let msg = '';

  // New route form
  let showForm = false;
  let sellerId = '';
  let routeDate = new Date().toISOString().slice(0, 10);
  let picked: Record<string, boolean> = {};
  let weekdayPlan: Record<string, string[]> = {};
  let busy = false;

  let viewing: any = null;
  let editing: any = null;
  let deleting: string | null = null;
  let editSellerId = '';
  let editDate = '';
  let editStatus = 'planned';

  function toggleWeekday(orderId: string, day: string) {
    const days = weekdayPlan[orderId] ?? [];
    weekdayPlan[orderId] = days.includes(day)
      ? days.filter((d) => d !== day)
      : [...days, day];
    weekdayPlan = { ...weekdayPlan };
  }

  async function viewRoute(id: string) {
    viewing = await invoke<any>('cloud_route_detail', { routeId: id });
  }

  async function editRoute(id: string) {
    const detail = await invoke<any>('cloud_route_detail', { routeId: id });
    editing = detail;
    editSellerId = detail.route.seller_id ?? '';
    editDate = String(detail.route.route_date).slice(0, 10);
    editStatus = detail.route.status ?? 'planned';
  }

  async function saveEditedRoute() {
    busy = true;
    try {
      await invoke('cloud_update_route', {
        routeId: editing.route.id,
        sellerId: editSellerId,
        routeDate: editDate,
        status: editStatus,
        stops: editing.stops.map((st: any) => ({
          order_id: st.order_id,
          client_id: st.client_id ?? st.client?.id,
        })),
      });
      editing = null;
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function printRoute(id: string) {
    const detail = await invoke<any>('cloud_route_detail', { routeId: id });
    const dateStr = String(detail.route.route_date).slice(0, 10);
    const rows = (detail.stops ?? [])
      .map((st: any, i: number) =>
        '<tr><td style="padding:4px 8px;border-bottom:1px solid #ccc">' + (i + 1) + '</td>' +
        '<td style="padding:4px 8px;border-bottom:1px solid #ccc">' + (st.client?.name ?? '—') + '</td>' +
        '<td style="padding:4px 8px;border-bottom:1px solid #ccc;text-align:right">' +
        ((st.order?.total_amount ?? 0) / 100).toLocaleString('fr-DZ') + ' DA</td>' +
        '<td style="padding:4px 8px;border-bottom:1px solid #ccc;text-align:center">' + (st.status ?? '') + '</td></tr>')
      .join('');
    const html = '<!doctype html><html><head><meta charset="utf-8">' +
      '<style>body{font-family:Segoe UI,Arial;font-size:12px}table{width:100%}h2{margin:0 0 4px}</style></head>' +
      '<body><h2>Feuille de route — ' + dateStr + '</h2>' +
      '<p>' + (detail.route.seller?.full_name ?? '—') + ' · ' + detail.route.status + '</p>' +
      '<table><tr><th align="start" style="padding:4px 8px">#</th><th align="start" style="padding:4px 8px">Client</th>' +
      '<th align="end" style="padding:4px 8px">Total</th><th style="padding:4px 8px">Statut</th></tr>' + rows + '</table></body></html>';
    import('@tauri-apps/api/core').then(({ invoke }) =>
      invoke('print_html_direct', { html, title: 'Route ' + dateStr, paper: { widthMm: 80 } })
        .catch((e: any) => (error = String(e))));
  }
  function askRemoveRoute(id: string) {
    deleting = id;
  }

  async function confirmRemoveRoute() {
    if (!deleting) return;
    busy = true;
    try {
      await invoke('cloud_delete_route', { routeId: deleting });
      deleting = null;
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }


  async function load() {
    loading = true;
    error = '';
    try {
      [routes, staff, routable] = await Promise.all([
        invoke<any[]>('cloud_recent_routes', { limit: 50 }).catch(() => []),
        invoke<any[]>('cloud_field_staff').catch(() => []),
        invoke<any[]>('cloud_routable_orders').catch(() => []),
      ]);
      // Enrich routes with stop counts + totals (separate light query per view).
      for (const r of routes) {
        try {
          const lines = await invoke<any[]>('cloud_route_order_items', { routeId: r.id }).catch(() => []);
          r.stop_count = (r.route_stops ?? lines)?.length ?? 0;
        } catch { r.stop_count = 0; }
      }
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      loading = false;
    }
  }

  async function createRoute() {
    busy = true;
    error = '';
    msg = '';
    try {
      const ids = Object.keys(picked).filter((k) => picked[k]);
      await invoke('cloud_create_route', {
        sellerId,
        routeDate,
        orderIds: ids,
        notes: null,
      });
      showForm = false;
      picked = {};
      for (const [orderId, days] of Object.entries(weekdayPlan)) {
        if (days.length === 0) continue;
        const order = routable.find((o: any) => o.id === orderId);
        const clientId = order?.client_id;
        if (clientId) {
          try {
            await invoke('cloud_set_client_visit_days', { clientId, days });
          } catch { /* best-effort */ }
        }
      }
      weekdayPlan = {};
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }


  onMount(load);
  $: fmt = (v: number) => new Intl.NumberFormat('fr-DZ').format(v);
  $: pickedCount = Object.values(picked).filter(Boolean).length;
</script>

<div class="p-4 md:p-6 space-y-4">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <Route class="w-5 h-5 text-sky-500" />
        {t('routes_title')}
      </h1>
      <p class="text-xs text-pos-muted">TitaouCRM</p>
    </div>
    <div class="flex gap-2">
      <button type="button" on:click={load} disabled={loading}
        class="p-2 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer">
        <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      </button>
      <button type="button" on:click={() => { showForm = true; picked = {}; sellerId = staff[0]?.id ?? ''; }}
        class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-sky-600 hover:bg-sky-700 text-white rounded-xl cursor-pointer">
        <Plus class="w-3.5 h-3.5" />{t('routes_new')}
      </button>
    </div>
  </div>

  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">❌ {error}</p>
  {/if}
  {#if msg}
    <p class="text-[11px] font-bold text-emerald-600">✅ {msg}</p>
  {/if}

  {#if !loading && routes.length === 0 && !error}
    <p class="text-center text-pos-muted text-sm py-12">🗺️ {t('routes_empty')}</p>
  {/if}

  {#if routes.length > 0}
    <div class="grid gap-3 md:grid-cols-2">
      {#each routes as r (r.id)}
        <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border space-y-1.5">
          <div class="flex items-start justify-between">
            <div>
              <p class="text-sm font-black text-pos-text">{String(r.route_date).slice(0, 10)}</p>
              <p class="text-[10px] text-pos-muted">{r.status} · {t('routes_stops')}: {r.stop_count ?? 0}</p>
            </div>
            <div class="flex gap-1">
              <button type="button" class="p-1.5 text-sky-600 hover:bg-sky-50 rounded-lg cursor-pointer"
                title="View" on:click={() => viewRoute(r.id)}>
                <Eye class="w-3.5 h-3.5" />
              </button>
              <button type="button" class="p-1.5 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer"
                title="Print" on:click={() => printRoute(r.id)}>
                <Printer class="w-3.5 h-3.5" />
              </button>
              <button type="button" class="p-1.5 text-amber-600 hover:bg-amber-50 rounded-lg cursor-pointer"
                title="Edit" on:click={() => editRoute(r.id)}>
                <Edit2 class="w-3.5 h-3.5" />
              </button>
              <button type="button" class="p-1.5 text-rose-600 hover:bg-rose-50 rounded-lg cursor-pointer"
                title="Delete" on:click={() => askRemoveRoute(r.id)}>
                <Trash2 class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- New route modal -->
{#if showForm}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showForm = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-lg max-h-[85vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between">
        <h3 class="text-sm font-black text-pos-text">{t('routes_new')}</h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (showForm = false)}>
          <X class="w-4 h-4" />
        </button>
      </div>
      <div class="p-4 overflow-y-auto space-y-3">
        <div class="grid grid-cols-2 gap-2.5">
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('routes_seller')}</label>
            <select bind:value={sellerId}
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
              <option value="">—</option>
              {#each staff as s (s.id)}
                <option value={s.id}>{s.full_name} ({s.role})</option>
              {/each}
            </select>
          </div>
          <div>
            <label class="block text-[10px] font-black text-pos-muted mb-1">{t('routes_date')}</label>
            <input type="date" bind:value={routeDate}
              class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
          </div>
        </div>

        <div>
          <p class="text-[10px] font-black text-pos-muted uppercase mb-1.5">
            {t('routes_orders')} ({pickedCount})
          </p>
          {#if routable.length === 0}
            <p class="text-[11px] text-pos-muted">—</p>
          {:else}
            <div class="space-y-1.5 max-h-64 overflow-y-auto">
              {#each routable as o (o.id)}
                <label class="flex items-center gap-2.5 p-2.5 bg-slate-50 dark:bg-slate-800 rounded-xl cursor-pointer">
                  <input type="checkbox" bind:checked={picked[o.id]} class="rounded text-sky-600 w-4 h-4" />
                  <span class="flex-1 text-xs font-bold text-pos-text truncate">{o.client?.name ?? '—'}</span>
                  <span class="text-[10px] font-mono font-black text-sky-600">{fmt(o.total_amount / 100)} DA</span>
                </label>
              {/each}
            </div>
          {/if}
        </div>

        <div>
          <p class="text-[10px] font-black text-pos-muted uppercase mb-1">Plan hebdo (optionnel)</p>
          <p class="text-[9px] text-pos-muted mb-1.5">
            Coche un jour pour mémoriser ce client sur ce jour (ex. dim: 1-6, lun: 7-11). Pas obligatoire.
          </p>
          {#each routable as o (o.id)}
            {#if picked[o.id]}
              <div class="flex items-center gap-1 flex-wrap mb-1">
                <span class="text-[10px] font-bold w-28 truncate">{o.client?.name}</span>
                {#each ['sunday','monday','tuesday','wednesday','thursday','friday','saturday'] as day}
                  <button type="button"
                    class="px-1.5 py-0.5 text-[9px] font-black rounded-full cursor-pointer {(weekdayPlan[o.id] ?? []).includes(day) ? 'bg-sky-600 text-white' : 'bg-slate-200 dark:bg-slate-800 text-pos-muted'}"
                    on:click={() => toggleWeekday(o.id, day)}>
                    {day.slice(0, 3)}
                  </button>
                {/each}
              </div>
            {/if}
          {/each}
        </div>

        <div class="flex justify-end gap-2 pt-2">
          <button type="button" on:click={createRoute} disabled={busy || !sellerId || pickedCount === 0}
            class="flex items-center gap-1.5 px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
            <Route class="w-3.5 h-3.5" />{t('routes_create')}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

{#if viewing}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (viewing = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-lg max-h-[80vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between sticky top-0 bg-inherit rounded-t-2xl">
        <h3 class="text-sm font-black text-pos-text">Feuille de route — {String(viewing.route.route_date).slice(0, 10)}</h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (viewing = null)}><X class="w-4 h-4" /></button>
      </div>
      <div class="p-4 overflow-y-auto">
        <p class="text-xs font-bold text-pos-muted mb-2">{viewing.route.seller?.full_name ?? '—'} · {viewing.route.status}</p>
        <table class="w-full text-xs">
          <thead><tr class="text-pos-muted font-black">
            <th class="p-2 text-start">#</th><th class="p-2 text-start">Client</th>
            <th class="p-2 text-end">Total</th><th class="p-2 text-center">Statut</th>
          </tr></thead>
          <tbody>
            {#each viewing.stops as st, i}
              <tr class="border-t border-pos-border">
                <td class="p-2">{i + 1}</td>
                <td class="p-2 font-bold">{st.client?.name ?? '—'}</td>
                <td class="p-2 text-end font-mono">{((st.order?.total_amount ?? 0) / 100).toLocaleString('fr-DZ')} DA</td>
                <td class="p-2 text-center">{st.status}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </div>
{/if}

{#if editing}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (editing = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-md p-5 space-y-3" on:click|stopPropagation>
      <h3 class="text-sm font-black text-pos-text">Modifier la tournée</h3>
      <select bind:value={editSellerId} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
        {#each staff as s (s.id)}<option value={s.id}>{s.full_name} ({s.role})</option>{/each}
      </select>
      <div class="grid grid-cols-2 gap-2">
        <input type="date" bind:value={editDate} class="px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
        <select bind:value={editStatus} class="px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
          <option value="planned">planned</option>
          <option value="in_progress">in_progress</option>
          <option value="completed">completed</option>
        </select>
      </div>
      <p class="text-[10px] text-pos-muted">{editing.stops.length} stops (order kept)</p>
      <div class="flex justify-end gap-2 pt-2">
        <button type="button" on:click={() => (editing = null)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">✕</button>
        <button type="button" on:click={saveEditedRoute} disabled={busy || !editSellerId || !editDate}
          class="px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">OK</button>
      </div>
    </div>
  </div>
{/if}

{#if deleting}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (deleting = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-rose-300 dark:border-rose-800 w-full max-w-xs p-5 space-y-3" on:click|stopPropagation>
      <div class="flex items-center gap-2 text-rose-600">
        <AlertTriangle class="w-5 h-5" />
        <h3 class="text-sm font-black">Supprimer la tournée ?</h3>
      </div>
      <p class="text-[11px] text-pos-muted">Les arrêts seront supprimés ; les commandes restent intactes.</p>
      <div class="flex justify-end gap-2">
        <button type="button" on:click={() => (deleting = null)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">Annuler</button>
        <button type="button" on:click={confirmRemoveRoute} disabled={busy}
          class="px-4 py-2 text-[11px] font-black bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">Supprimer</button>
      </div>
    </div>
  </div>
{/if}
