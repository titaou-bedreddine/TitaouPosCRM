<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { Truck, RefreshCw, X, Receipt, Pencil, Trash2, Plus } from 'lucide-svelte';

  let orders: any[] = [];
  let loading = true;
  let error = '';
  let selected: any = null;
  let lines: any[] = [];
  let busy = false;

  async function load() {
    loading = true;
    error = '';
    try {
      orders = await invoke<any[]>('cloud_field_orders', { limit: 200 });
      // Member names resolved client-side (the FK runs through auth.users —
      // PostgREST cannot embed profiles here).
      const members = await invoke<any[]>('cloud_team_members').catch(() => []);
      const names: Record<string, string> = {};
      for (const m of members) names[m.id] = m.full_name;
      for (const o of orders) {
        if (o.member_name === '—' || !o.member_name) o.member_name = names[o.member_id] ?? o.member_name ?? '—';
      }
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed to load';
      orders = [];
    } finally {
      loading = false;
    }
  }

  let editMode = false;
  let editQty: Record<string, number> = {};
  let editNotes = '';

  function startEdit() {
    editMode = true;
    editQty = {};
    editNotes = selected?.notes ?? '';
    for (const ln of lines) {
      editQty[ln.product_id] = ln.quantity;
    }
  }

  async function saveEdit() {
    if (!selected) return;
    busy = true;
    error = '';
    try {
      const items = lines.map((ln) => ({
        product_id: ln.product_id,
        quantity: editQty[ln.product_id] ?? ln.quantity,
        unit_price: ln.unit_price,
      }));
      await invoke('cloud_field_order_edit', {
        orderId: selected.crm_id,
        items,
        notes: editNotes,
      });
      editMode = false;
      await load();
      const fresh = orders.find((o: any) => o.crm_id === selected.crm_id);
      if (fresh) selected = fresh;
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function deleteOrder() {
    if (!selected) return;
    if (!confirm('Supprimer cette commande ? (stock + dues reversés)')) return;
    busy = true;
    error = '';
    try {
      await invoke('cloud_field_order_delete', { orderId: selected.crm_id });
      selected = null;
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function openOrder(o: any) {
    selected = o;
    lines = [];
    try {
      lines = await invoke<any[]>('cloud_field_order_lines', { crmId: o.crm_id });
    } catch {
      lines = [];
    }
  }

  onMount(load);

  $: fmt = (v: number) => new Intl.NumberFormat('fr-DZ').format(v) + ' DA';
  $: due = (o: any) => Math.max(0, (o.total_amount ?? 0) - (o.amount_paid ?? 0));
</script>

<div class="p-4 md:p-6 space-y-4">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <Truck class="w-5 h-5 text-sky-500" />
        {t('fo_title')}
      </h1>
      <p class="text-xs text-pos-muted">
        {t('fo_member')} · {t('fo_status')} · {t('fo_payment')} — TitaouCRM
      </p>
    </div>
    <button type="button" on:click={load} disabled={loading}
      class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
      <RefreshCw class="w-3.5 h-3.5 {loading ? 'animate-spin' : ''}" />
    </button>
  </div>

  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">❌ {error}</p>
  {/if}

  {#if !loading && orders.length === 0 && !error}
    <div class="text-center py-16 text-pos-muted">
      <Truck class="w-12 h-12 mx-auto mb-3 opacity-30" />
      <p class="text-sm font-bold">{t('fo_empty')}</p>
    </div>
  {/if}

  {#if orders.length > 0}
    <div class="overflow-x-auto rounded-2xl border border-pos-border bg-white dark:bg-slate-900">
      <table class="w-full text-xs">
        <thead>
          <tr class="bg-slate-50 dark:bg-slate-800 text-pos-muted font-black">
            <th class="p-3 text-start">{t('fo_client')}</th>
            <th class="p-3 text-start">{t('fo_member')}</th>
            <th class="p-3 text-center">{t('fo_status')}</th>
            <th class="p-3 text-center">{t('fo_payment')}</th>
            <th class="p-3 text-end">{t('fo_total')}</th>
            <th class="p-3 text-end">{t('fo_due')}</th>
            <th class="p-3 text-center">—</th>
          </tr>
        </thead>
        <tbody>
          {#each orders as o (o.crm_id)}
            <tr class="border-t border-pos-border hover:bg-slate-50 dark:hover:bg-slate-800/50 cursor-pointer" on:click={() => openOrder(o)}>
              <td class="p-3 font-bold text-pos-text">{o.client_name}</td>
              <td class="p-3 text-pos-muted">{o.member_name}</td>
              <td class="p-3 text-center">
                <span class="px-2 py-0.5 rounded-full text-[9px] font-black
                  {o.status === 'delivered' ? 'bg-emerald-100 text-emerald-700' : o.status === 'cancelled' ? 'bg-rose-100 text-rose-700' : 'bg-amber-100 text-amber-700'}">
                  {o.status}
                </span>
              </td>
              <td class="p-3 text-center">
                <span class="px-2 py-0.5 rounded-full text-[9px] font-black
                  {o.payment_status === 'paid' ? 'bg-emerald-100 text-emerald-700' : o.payment_status === 'partial' ? 'bg-amber-100 text-amber-700' : 'bg-rose-100 text-rose-700'}">
                  {o.payment_status}
                </span>
              </td>
              <td class="p-3 text-end font-black text-pos-text">{fmt(o.total_amount)}</td>
              <td class="p-3 text-end font-black {due(o) > 0 ? 'text-rose-500' : 'text-pos-muted'}">{fmt(due(o))}</td>
              <td class="p-3 text-center"><Receipt class="w-3.5 h-3.5 inline text-pos-muted" /></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<!-- Details modal -->
<svelte:window on:keydown={(e) => { if (e.key === 'Escape') selected = null; }} />
{#if selected}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (selected = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-2xl max-h-[85vh] flex flex-col" on:click|stopPropagation>
      <div class="p-4 border-b border-pos-border flex items-center justify-between sticky top-0 bg-inherit rounded-t-2xl">
        <h3 class="text-sm font-black text-pos-text">{selected.client_name}</h3>
        <button type="button" class="p-1 text-pos-muted hover:text-pos-text cursor-pointer" on:click={() => (selected = null)}>
          <X class="w-4 h-4" />
        </button>
      </div>
      <div class="p-4 overflow-y-auto space-y-3">
        <div class="flex items-center justify-end gap-2">
          <button type="button" class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-amber-500 hover:bg-amber-600 text-white rounded-xl cursor-pointer"
            on:click={() => startEdit()}>
            <Pencil class="w-3.5 h-3.5" />Modifier
          </button>
          <button type="button" class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-rose-600 hover:bg-rose-700 text-white rounded-xl cursor-pointer"
            on:click={() => deleteOrder()}>
            <Trash2 class="w-3.5 h-3.5" />Supprimer
          </button>
        </div>
        <div class="grid grid-cols-3 gap-2 text-xs">
          <div class="p-2.5 bg-slate-50 dark:bg-slate-800 rounded-xl">
            <p class="text-[9px] font-black text-pos-muted uppercase">{t('fo_member')}</p>
            <p class="font-bold text-pos-text">{selected.member_name}</p>
          </div>
          <div class="p-2.5 bg-slate-50 dark:bg-slate-800 rounded-xl">
            <p class="text-[9px] font-black text-pos-muted uppercase">{t('fo_total')}</p>
            <p class="font-black text-pos-text">{fmt(selected.total_amount)}</p>
          </div>
          <div class="p-2.5 bg-slate-50 dark:bg-slate-800 rounded-xl">
            <p class="text-[9px] font-black text-pos-muted uppercase">{t('fo_due')}</p>
            <p class="font-black {due(selected) > 0 ? 'text-rose-500' : 'text-pos-text'}">{fmt(due(selected))}</p>
          </div>
        </div>
        <div>
          <p class="text-[10px] font-black text-pos-muted uppercase mb-1.5">{t('fo_details')}</p>
          <table class="w-full text-xs">
            <thead>
              <tr class="text-pos-muted font-black">
                <th class="p-2 text-start">#</th>
                <th class="p-2 text-end">{t('fo_lines')}</th>
                <th class="p-2 text-end">{t('tl_qty')}</th>
                <th class="p-2 text-end">{t('fo_total')}</th>
              </tr>
            </thead>
            <tbody>
              {#each lines as ln, i}
                <tr class="border-t border-pos-border">
                  <td class="p-2">{i + 1}. {ln.product_name}</td>
                  <td class="p-2 text-end font-mono">{ln.unit_price}</td>
                  <td class="p-2 text-end">
                    {#if editMode}
                      <input type="number" step="0.001" min="0"
                        bind:value={editQty[ln.product_id]}
                        class="w-20 px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs font-mono text-pos-text outline-none text-end" />
                    {:else}
                      <span class="font-mono">{ln.quantity}</span>
                    {/if}
                  </td>
                  <td class="p-2 text-end font-black">
                    {#if editMode}
                      {Math.round((editQty[ln.product_id] ?? ln.quantity) * ln.unit_price)}
                    {:else}
                      {ln.line_total}
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        {#if editMode}
          <textarea bind:value={editNotes} rows="2" placeholder="Notes"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
          <div class="flex justify-end gap-2">
            <button type="button" on:click={() => (editMode = false)}
              class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">Annuler</button>
            <button type="button" on:click={saveEdit} disabled={busy}
              class="px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">Enregistrer</button>
          </div>
        {/if}
        {#if !editMode && selected.notes}
          <p class="text-[11px] text-pos-muted bg-slate-50 dark:bg-slate-800 rounded-xl p-3">📝 {selected.notes}</p>
        {/if}
      </div>
    </div>
  </div>
{/if}
