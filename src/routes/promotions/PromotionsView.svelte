<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { Percent, RefreshCw, Plus, X, Trash2, Edit2, BadgePercent } from 'lucide-svelte';

  let promos: any[] = [];
  let products: any[] = [];
  let loading = true;
  let error = '';
  let busy = false;

  let showForm = false;
  let editId: string | null = null;
  let fName = '';
  let fDesc = '';
  let fType: 'percent' | 'fixed' = 'percent';
  let fValue = 10;
  let fProductId = '';
  let fMinQty = 1;
  let fEndsAt = '';
  let fActive = true;

  async function load() {
    loading = true;
    error = '';
    try {
      [promos, products] = await Promise.all([
        invoke<any[]>('cloud_promotions_list').catch(() => []),
        invoke<any[]>('cloud_products_for_promos').catch(() => []),
      ]);
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      loading = false;
    }
  }

  async function save() {
    busy = true;
    error = '';
    try {
      await invoke('cloud_promotion_save', {
        id: editId,
        name: fName,
        description: fDesc || null,
        discountType: fType,
        discountValue: Math.round(fValue),
        productId: fProductId,
        minQuantity: fMinQty,
        endsAt: fEndsAt ? new Date(fEndsAt).toISOString() : null,
        isActive: fActive,
      });
      showForm = false;
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function remove(id: string) {
    if (!confirm(t('promo_confirm_delete'))) return;
    busy = true;
    try {
      await invoke('cloud_promotion_delete', { id });
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  function openNew() {
    editId = null;
    fName = '';
    fDesc = '';
    fType = 'percent';
    fValue = 10;
    fProductId = products[0]?.id ?? '';
    fMinQty = 1;
    fEndsAt = '';
    fActive = true;
    showForm = true;
  }

  function openEdit(pr: any) {
    editId = pr.id;
    fName = pr.name;
    fDesc = pr.description ?? '';
    fType = pr.discount_type;
    fValue = pr.discount_value;
    fProductId = pr.product_id;
    fMinQty = pr.min_quantity;
    fEndsAt = pr.ends_at ? String(pr.ends_at).slice(0, 10) : '';
    fActive = pr.is_active;
    showForm = true;
  }

  onMount(load);
  $: fmt = (v: number) => new Intl.NumberFormat('fr-DZ').format(v);
</script>

<div class="p-4 md:p-6 space-y-4">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <BadgePercent class="w-5 h-5 text-amber-500" />
        {t('promo_title')}
      </h1>
      <p class="text-xs text-pos-muted">{t('promo_subtitle')}</p>
    </div>
    <div class="flex gap-2">
      <button type="button" on:click={load} disabled={loading}
        class="p-2 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer">
        <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      </button>
      <button type="button" on:click={openNew}
        class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-sky-600 hover:bg-sky-700 text-white rounded-xl cursor-pointer">
        <Plus class="w-3.5 h-3.5" />{t('promo_new')}
      </button>
    </div>
  </div>

  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">❌ {error}</p>
  {/if}

  {#if !loading && promos.length === 0 && !error}
    <p class="text-center text-pos-muted text-sm py-12">🏷️ {t('promo_empty')}</p>
  {/if}

  {#if promos.length > 0}
    <div class="grid gap-3 md:grid-cols-2">
      {#each promos as pr (pr.id)}
        <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border space-y-2">
          <div class="flex items-start justify-between gap-2">
            <div>
              <p class="text-sm font-black text-pos-text">{pr.name}</p>
              <p class="text-[10px] text-pos-muted">{pr.product?.name ?? '—'}</p>
            </div>
            <span class="px-2 py-0.5 rounded-full text-[9px] font-black shrink-0
              {pr.is_active ? 'bg-emerald-100 text-emerald-700' : 'bg-slate-200 text-slate-600'}">
              {pr.is_active ? t('team_active') : 'OFF'}
            </span>
          </div>
          <p class="text-[11px] text-pos-text bg-slate-50 dark:bg-slate-800 rounded-xl p-2.5 font-bold">
            {pr.discount_type === 'percent'
              ? `−${pr.discount_value}%`
              : `−${fmt(pr.discount_value / 100)} DA`}
            {t('promo_min_qty')}: {pr.min_quantity}
            {#if pr.ends_at}<span class="text-pos-muted"> · {String(pr.ends_at).slice(0, 10)}</span>{/if}
          </p>
          {#if pr.description}
            <p class="text-[10px] text-pos-muted">📝 {pr.description}</p>
          {/if}
          <div class="flex gap-2 pt-1">
            <button type="button" class="flex-1 flex items-center justify-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-sky-600 hover:bg-sky-700 text-white rounded-xl cursor-pointer"
              on:click={() => openEdit(pr)}>
              <Edit2 class="w-3.5 h-3.5" />{t('team_edit')}
            </button>
            <button type="button" class="flex items-center justify-center px-3 py-1.5 text-[11px] font-black bg-rose-600 hover:bg-rose-700 text-white rounded-xl cursor-pointer"
              on:click={() => remove(pr)}>
              <Trash2 class="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<!-- Create / edit modal -->
{#if showForm}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showForm = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-md p-5 space-y-3" on:click|stopPropagation>
      <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
        <Percent class="w-4 h-4 text-sky-500" />{t('promo_new')}
      </h3>
      <input type="text" bind:value={fName} placeholder={t('promo_name')}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      <select bind:value={fProductId}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
        {#each products as pr}
          <option value={pr.id}>{pr.name}</option>
        {/each}
      </select>
      <div class="grid grid-cols-2 gap-2.5">
        <select bind:value={fType}
          class="px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
          <option value="percent">% — {t('promo_percent')}</option>
          <option value="fixed">{t('promo_fixed')}</option>
        </select>
        <input type="number" bind:value={fValue} min="1"
          class="px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
        <input type="number" step="0.001" min="0.001" bind:value={fMinQty}
          class="px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
        <input type="date" bind:value={fEndsAt}
          class="px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      </div>
      <textarea bind:value={fDesc} rows="2" placeholder={t('dr_reason')}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer">
        <input type="checkbox" bind:checked={fActive} class="rounded text-sky-600" />
        {t('team_active')}
      </label>
      <div class="flex justify-end gap-2 pt-2">
        <button type="button" on:click={() => (showForm = false)}
          class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">
          <X class="w-4 h-4" />
        </button>
        <button type="button" on:click={save} disabled={busy || !fName || !fProductId}
          class="px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">OK</button>
      </div>
    </div>
  </div>
{/if}
