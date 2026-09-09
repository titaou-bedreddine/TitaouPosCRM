<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { Cloud, RefreshCw, Plug, Unplug, FlaskConical, AlertTriangle, CheckCircle2, KeyRound, Lock } from 'lucide-svelte';

  let status: any = null;
  let url = '';
  let anonKey = '';
  let email = '';
  let password = '';
  let busy = false;
  let msg = '';
  let error = '';

  async function refresh() {
    try {
      status = await invoke<any>('cloud_get_status');
    } catch {
      status = null;
    }
  }

  let saved: any = null;
  let setupCode = '';
  let setupBusy = false;

  onMount(async () => {
    await refresh();
    try {
      saved = await invoke<any>('cloud_get_saved_config');
      if (saved?.url && !url) url = saved.url;
      if (saved?.email && !email) email = saved.email;
    } catch {}
  });
  // Poll while the tab is open (the backend loop cycles every 5s).
  const poll = setInterval(refresh, 5000);
  onDestroy(() => clearInterval(poll));

  async function connect() {
    error = '';
    msg = '';
    busy = true;
    try {
      const r = await invoke<any>('cloud_configure', {
        url, anonKey, email, password,
      });
      msg = `✅ ${t('cloud_status_online')} — pushed ${r.pushed ?? 0}, failed ${r.failed ?? 0}`;
      password = '';
      await refresh();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Connection failed';
    } finally {
      busy = false;
    }
  }

  async function testConn() {
    error = '';
    msg = '';
    busy = true;
    try {
      const r = await invoke<any>('cloud_test_connection', {
        url, anonKey, email, password,
      });
      msg = `✅ ${r.full_name} (${r.role}) — ${r.organization_id?.slice(0, 8)}…`;
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Test failed';
    } finally {
      busy = false;
    }
  }

  async function syncNow() {
    error = '';
    msg = '';
    busy = true;
    try {
      const r = await invoke<any>('cloud_sync_now');
      msg = `⇅ pushed ${r.pushed ?? 0}, retried ${r.retried ?? 0}`;
      await refresh();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Sync failed';
    } finally {
      busy = false;
    }
  }

  async function disconnect() {
    busy = true;
    try {
      await invoke('cloud_disconnect');
      msg = '';
      await refresh();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function connectWithCode() {
    error = '';
    msg = '';
    setupBusy = true;
    try {
      await invoke('cloud_setup_code', { code: setupCode.trim() });
      msg = t('cloud_activated');
      setupCode = '';
      await refresh();
      try { saved = await invoke<any>('cloud_get_saved_config'); if (saved?.url && !url) url = saved.url; if (saved?.email && !email) email = saved.email; } catch {}
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Activation failed';
    } finally {
      setupBusy = false;
    }
  }

  $: configured = !!status?.url;
  $: online = !!status?.online;
</script>

<div class="max-w-4xl space-y-6">
  <div>
    <h2 class="text-base font-black text-pos-text flex items-center gap-2">
      <Cloud class="w-5 h-5 text-sky-500" />
      {t('cloud_title')}
    </h2>
    <p class="text-xs text-pos-muted mt-0.5">{t('cloud_desc')}</p>
  </div>

  <!-- Setup code: the customer-facing path (one code, nothing else) -->
  <div class="p-5 bg-gradient-to-br from-sky-50 to-indigo-50 dark:from-slate-800/60 dark:to-slate-800/30 rounded-2xl border border-sky-200 dark:border-sky-900 space-y-3">
    <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
      <KeyRound class="w-4 h-4 text-sky-600" />
      {t('cloud_setup_code')}
    </h3>
    <p class="text-xs text-pos-muted">{t('cloud_setup_code_hint')}</p>
    {#if saved && !saved.product_ready}
      <p class="text-[11px] font-bold text-amber-600">⚠️ {t('cloud_setup_unavailable')}</p>
    {/if}
    <div class="flex gap-2">
      <input type="text" bind:value={setupCode} placeholder="TITAO-XXXX-XXXX-XXXX" maxlength="24"
        class="flex-1 px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-black tracking-widest uppercase text-pos-text outline-none focus:border-sky-500"
        on:keydown={(e) => { if (e.key === 'Enter') connectWithCode(); }} />
      <button type="button" on:click={connectWithCode} disabled={setupBusy || !setupCode.trim() || status?.coordinator === false || (saved && !saved.product_ready)}
        class="flex items-center gap-1.5 px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
        <KeyRound class="w-3.5 h-3.5" />{t('cloud_setup_connect')}
      </button>
    </div>
    {#if saved?.has_password}
      <p class="text-[10px] font-bold text-emerald-600 flex items-center gap-1">
        <Lock class="w-3 h-3" />{t('cloud_saved_note')}
      </p>
    {/if}
  </div>

  {#if status && status.coordinator === false}
    <p class="text-[11px] font-bold text-amber-600 bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800 rounded-xl px-3 py-2">
      ⚠️ {t('cloud_not_coordinator')}
    </p>
  {/if}

  {#if msg}
    <p class="text-[11px] font-bold text-emerald-600 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 rounded-xl px-3 py-2 flex items-center gap-1.5">
      <CheckCircle2 class="w-3.5 h-3.5 shrink-0" />{msg}
    </p>
  {/if}
  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2 flex items-center gap-1.5">
      <AlertTriangle class="w-3.5 h-3.5 shrink-0" />{error}
    </p>
  {/if}

  <!-- Status grid (when configured) -->
  {#if configured}
    <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
      <div class="flex items-start justify-between">
        <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
          <span class="w-3 h-3 rounded-full {online ? 'bg-emerald-500 animate-pulse' : 'bg-rose-500'}"></span>
          {online ? t('cloud_status_online') : t('cloud_status_offline')}
        </h3>
        <div class="flex gap-1.5">
          <button type="button" on:click={syncNow} disabled={busy || status?.coordinator === false}
            class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
            <RefreshCw class="w-3.5 h-3.5 {busy ? 'animate-spin' : ''}" />{t('cloud_sync_now')}
          </button>
          <button type="button" on:click={disconnect} disabled={busy || status?.coordinator === false}
            class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
            <Unplug class="w-3.5 h-3.5" />{t('cloud_disconnect')}
          </button>
        </div>
      </div>

      <div class="grid grid-cols-2 md:grid-cols-4 gap-2">
        <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
          <p class="text-[9px] font-black text-pos-muted uppercase">{t('cloud_last_push')}</p>
          <p class="text-xs font-black text-pos-text">{status?.last_push || '—'}</p>
        </div>
        <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
          <p class="text-[9px] font-black text-pos-muted uppercase">{t('cloud_last_pull')}</p>
          <p class="text-xs font-black text-pos-text">{status?.last_pull || '—'}</p>
        </div>
        <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
          <p class="text-[9px] font-black text-pos-muted uppercase">{t('cloud_pending')}</p>
          <p class="text-xs font-black {status?.pending_outbox > 0 ? 'text-amber-500' : 'text-pos-text'}">{status?.pending_outbox ?? 0}</p>
        </div>
        <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
          <p class="text-[9px] font-black text-pos-muted uppercase">{t('cloud_failed')}</p>
          <p class="text-xs font-black {status?.failed_outbox > 0 ? 'text-rose-500' : 'text-pos-text'}">{status?.failed_outbox ?? 0}</p>
        </div>
      </div>

      {#if status?.last_error}
        <p class="text-[10px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">
          {t('cloud_last_error')}: {status.last_error}
        </p>
      {/if}

      <p class="text-[10px] text-pos-muted font-mono truncate">{status?.url} · {status?.email}</p>
    </div>
  {/if}

  <!-- Configure / re-configure form -->
  <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
    <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
      <Plug class="w-4 h-4 text-sky-500" />
      {configured ? 'Reconnect / ' + t('cloud_email') : t('cloud_title')}
    </h3>
    <div class="grid md:grid-cols-2 gap-2.5">
      <div class="md:col-span-2">
        <label class="block text-[10px] font-black text-pos-muted mb-1">{t('cloud_url')}</label>
        <input type="text" bind:value={url} placeholder="https://xxxx.supabase.co"
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      </div>
      <div class="md:col-span-2">
        <label class="block text-[10px] font-black text-pos-muted mb-1">{t('cloud_anon_key')}</label>
        <input type="text" bind:value={anonKey} placeholder="sb_publishable_…"
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      </div>
      <div>
        <label class="block text-[10px] font-black text-pos-muted mb-1">{t('cloud_email')}</label>
        <input type="email" bind:value={email} placeholder="owner@shop.dz"
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      </div>
      <div>
        <label class="block text-[10px] font-black text-pos-muted mb-1">{t('cloud_password')}</label>
        <input type="password" bind:value={password}
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      </div>
    </div>
    <p class="text-[10px] text-pos-muted">🔒 {t('cloud_client_only_note')}</p>
    <div class="flex gap-2">
      <button type="button" on:click={connect} disabled={busy || !url || !anonKey || !email || !password || status?.coordinator === false}
        class="flex items-center gap-1.5 px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
        <Cloud class="w-3.5 h-3.5" />{t('cloud_connect')}
      </button>
      <button type="button" on:click={testConn} disabled={busy || !url || !anonKey || !email || !password}
        class="flex items-center gap-1.5 px-4 py-2 text-[11px] font-black bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 dark:hover:bg-slate-600 disabled:opacity-40 text-pos-text rounded-xl cursor-pointer">
        <FlaskConical class="w-3.5 h-3.5" />{t('cloud_test')}
      </button>
    </div>
  </div>
</div>
