<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { Route, Plus, RefreshCw, Trash2, X } from 'lucide-svelte';

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
  let busy = false;

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
      await load();
      msg = '✅';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function removeRoute(id: string) {
    if (!confirm('Delete this route? (stops cascade; orders stay)')) return;
    busy = true;
    try {
      await invoke('cloud_delete_route', { routeId: id });
      await load();
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
      <button type="button" on:click={() => { showForm = true; picked = {}; sellerId = staff.find((s) => s.role === 'seller')?.id ?? ''; }}
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
            <button type="button" class="p-1.5 text-rose-600 hover:bg-rose-50 rounded-lg cursor-pointer"
              on:click={() => removeRoute(r.id)}>
              <Trash2 class="w-3.5 h-3.5" />
            </button>
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
