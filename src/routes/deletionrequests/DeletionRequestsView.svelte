<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { FileWarning, RefreshCw, Check, X } from 'lucide-svelte';

  let requests: any[] = [];
  let loading = true;
  let error = '';
  let busy = '';

  async function load() {
    loading = true;
    error = '';
    try {
      requests = await invoke<any[]>('cloud_deletion_requests');
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
      requests = [];
    } finally {
      loading = false;
    }
  }

  async function approve(r: any) {
    busy = r.id;
    error = '';
    try {
      await invoke('cloud_approve_deletion', { requestId: r.id });
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = '';
    }
  }

  async function reject(r: any) {
    busy = r.id;
    error = '';
    try {
      await invoke('cloud_reject_deletion', { requestId: r.id });
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = '';
    }
  }

  onMount(load);
</script>

<div class="p-4 md:p-6 space-y-4">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <FileWarning class="w-5 h-5 text-amber-500" />
        {t('dr_title')}
      </h1>
      <p class="text-xs text-pos-muted">{t('dr_pending')} — TitaouCRM</p>
    </div>
    <button type="button" on:click={load} disabled={loading}
      class="p-2 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer">
      <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
    </button>
  </div>

  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">❌ {error}</p>
  {/if}

  {#if !loading && requests.length === 0 && !error}
    <p class="text-center text-pos-muted text-sm py-12">✅ {t('dr_empty')}</p>
  {/if}

  {#if requests.length > 0}
    <div class="grid gap-3 md:grid-cols-2">
      {#each requests as r (r.id)}
        <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border space-y-2">
          <div class="flex items-start justify-between gap-2">
            <div>
              <p class="text-sm font-black text-pos-text">{r.client?.name || '—'}</p>
              <p class="text-[10px] text-pos-muted">{t('dr_by')}: {r.requested_by_profile?.full_name || '—'} · {new Date(r.created_at).toLocaleString()}</p>
            </div>
            <span class="px-2 py-0.5 rounded-full text-[9px] font-black bg-amber-100 text-amber-700 shrink-0">{t('dr_pending')}</span>
          </div>
          <p class="text-[11px] text-pos-text bg-slate-50 dark:bg-slate-800 rounded-xl p-2.5">📝 {r.reason}</p>
          <div class="flex gap-2 pt-1">
            <button type="button" on:click={() => approve(r)} disabled={busy === r.id}
              class="flex-1 flex items-center justify-center gap-1.5 px-3 py-2 text-[11px] font-black bg-emerald-600 hover:bg-emerald-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
              <Check class="w-3.5 h-3.5" />{t('dr_approve')}
            </button>
            <button type="button" on:click={() => reject(r)} disabled={busy === r.id}
              class="flex items-center justify-center gap-1.5 px-3 py-2 text-[11px] font-black bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
              <X class="w-3.5 h-3.5" />{t('dr_reject')}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
