<script lang="ts">
  // Persistent LAN network status indicator (sidebar) + Network Details
  // popup — shop, role, coordinator, devices and the live event feed.
  import { networkStatus, networkEvents, refreshNetworkStatus } from '../stores/network';
  import { t, currentLocale } from '../i18n';
  import {
    Wifi, WifiOff, RefreshCw, Server, MonitorSmartphone, Activity, X, ShieldAlert, Globe
  } from 'lucide-svelte';

  let popupOpen = false;
  let busy = false;

  $: status = $networkStatus;

  function modeVisual(s: any): { cls: string; dot: string; key: string } {
    if (!s || !s.enabled) return { cls: 'text-pos-muted', dot: 'bg-slate-400', key: 'net_mode_disabled' };
    switch (s.mode) {
      case 'server':
      case 'standalone':
        return { cls: 'text-emerald-600 dark:text-emerald-400', dot: 'bg-emerald-500 animate-pulse', key: 'net_mode_server' };
      case 'connected':
        return { cls: 'text-emerald-600 dark:text-emerald-400', dot: 'bg-emerald-500 animate-pulse', key: 'net_mode_connected' };
      case 'searching':
        return { cls: 'text-amber-600 dark:text-amber-400', dot: 'bg-amber-500 animate-pulse', key: 'net_mode_searching' };
      case 'reconnecting':
        return { cls: 'text-amber-600 dark:text-amber-400', dot: 'bg-amber-500 animate-pulse', key: 'net_mode_reconnecting' };
      case 'offline':
        // A logged-in offline client is WORKING (local fallback session) —
        // amber "working offline" instead of a red failure label.
        if (status?.offline_session) {
          return { cls: 'text-amber-600 dark:text-amber-400', dot: 'bg-amber-500', key: 'net_mode_working_offline' };
        }
        return { cls: 'text-rose-600 dark:text-rose-400', dot: 'bg-rose-500', key: 'net_mode_offline' };
      default:
        return { cls: 'text-pos-muted', dot: 'bg-slate-400', key: 'net_mode_disabled' };
    }
  }

  $: visual = modeVisual(status);

  async function manualRefresh() {
    busy = true;
    await refreshNetworkStatus();
    busy = false;
  }

  function fmtTime(ts?: number): string {
    if (!ts) return '—';
    try {
      return new Date(ts * 1000).toLocaleTimeString();
    } catch {
      return '—';
    }
  }
</script>

<!-- Indicator -->
<button
  type="button"
  on:click={() => (popupOpen = true)}
  class="w-full flex items-center gap-2 px-2 py-1.5 bg-pos-card rounded-lg border border-pos-border/60 hover:border-sky-400 transition cursor-pointer text-start"
  title={$networkStatus?.shop_name || 'TitaouPosCRM Network'}
>
  {#if status?.enabled && (status?.mode === 'connected' || status?.serving)}
    <Wifi class="w-3.5 h-3.5 text-emerald-500 shrink-0" />
  {:else if status?.enabled && (status?.mode === 'searching' || status?.mode === 'reconnecting')}
    <RefreshCw class="w-3.5 h-3.5 text-amber-500 shrink-0 animate-spin" />
  {:else if status?.enabled && status?.mode === 'offline' && status?.offline_session}
    <ShieldAlert class="w-3.5 h-3.5 text-amber-500 shrink-0" />
  {:else if status?.enabled && status?.mode === 'offline'}
    <ShieldAlert class="w-3.5 h-3.5 text-rose-500 shrink-0" />
  {:else}
    <WifiOff class="w-3.5 h-3.5 text-pos-muted shrink-0" />
  {/if}
  <span class="text-[10px] font-black truncate {visual.cls}">
    {t(visual.key, $currentLocale)}
  </span>
  <span class="ms-auto text-[9px] font-bold text-pos-muted truncate max-w-[70px]">
    {status?.pc_name || ''}
  </span>
</button>

<svelte:window on:keydown={(e) => { if (e.key === 'Escape' && popupOpen) popupOpen = false; }} />

<!-- Details popup -->
{#if popupOpen}
  <div class="fixed inset-0 z-[90] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4" role="presentation" on:click|self={() => (popupOpen = false)}>
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-md max-h-[90vh] flex flex-col animate-in zoom-in-95 duration-150">
      <div class="flex items-start justify-between p-6 pb-3 border-b border-pos-border shrink-0">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center shrink-0">
            <Globe class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text">TitaouPosCRM Network</h3>
            <p class="text-[10px] text-pos-muted font-bold">{status?.shop_name || '—'}</p>
          </div>
        </div>
        <button type="button" on:click={() => (popupOpen = false)} class="p-1.5 hover:bg-slate-100 dark:hover:bg-slate-800 rounded-lg cursor-pointer">
          <X class="w-4 h-4 text-pos-muted" />
        </button>
      </div>
      <div class="overflow-y-auto p-6 pt-4 space-y-4">

      <div class="grid grid-cols-2 gap-2 text-xs">
        <div class="p-3 bg-slate-50 dark:bg-slate-800/60 rounded-xl border border-pos-border/60">
          <p class="text-[9px] font-black text-pos-muted uppercase mb-1">{t('net_status_label', $currentLocale)}</p>
          <p class="flex items-center gap-1.5 font-black {visual.cls}">
            <span class="w-2 h-2 rounded-full {visual.dot}"></span>
            {t(visual.key, $currentLocale)}
          </p>
        </div>
        <div class="p-3 bg-slate-50 dark:bg-slate-800/60 rounded-xl border border-pos-border/60">
          <p class="text-[9px] font-black text-pos-muted uppercase mb-1">{t('net_role_label', $currentLocale)}</p>
          <p class="font-black text-pos-text capitalize flex items-center gap-1">
            <Server class="w-3.5 h-3.5 text-sky-500" />
            {status?.role || '—'}
          </p>
        </div>
        <div class="p-3 bg-slate-50 dark:bg-slate-800/60 rounded-xl border border-pos-border/60">
          <p class="text-[9px] font-black text-pos-muted uppercase mb-1">{t('net_this_pc', $currentLocale)}</p>
          <p class="font-black text-pos-text truncate">{status?.pc_name || '—'}</p>
          <p class="text-[9px] font-mono text-pos-muted truncate">{status?.node_id || ''}</p>
        </div>
        <div class="p-3 bg-slate-50 dark:bg-slate-800/60 rounded-xl border border-pos-border/60">
          <p class="text-[9px] font-black text-pos-muted uppercase mb-1">{t('net_coordinator', $currentLocale)}</p>
          <p class="font-black text-pos-text truncate">
            {status?.coordinator?.pc_name || (status?.serving ? status?.pc_name : '—')}
          </p>
          <p class="text-[9px] font-mono text-pos-muted">
            {#if status?.server_url}
              {status.server_url?.replace('http://', '')}
            {:else if status?.serving}
              {(status?.lan_ips?.[0] || '') + ':' + (status?.port || '')}
            {:else}—{/if}
          </p>
        </div>
      </div>

      <div class="p-3 bg-slate-50 dark:bg-slate-800/60 rounded-xl border border-pos-border/60">
        <p class="text-[9px] font-black text-pos-muted uppercase mb-2 flex items-center gap-1">
          <MonitorSmartphone class="w-3.5 h-3.5" />
          {t('net_devices', $currentLocale)}
          {#if status?.devices_count}<span class="text-sky-600">({status.devices_count})</span>{/if}
        </p>
        {#if status?.devices?.length}
          <div class="space-y-1.5">
            {#each status.devices as d}
              <div class="flex items-center justify-between text-[11px]">
                <span class="flex items-center gap-1.5 font-bold text-pos-text truncate">
                  <span class="w-1.5 h-1.5 rounded-full {d.online ? 'bg-emerald-500' : 'bg-slate-400'}"></span>
                  {d.pc_name}
                </span>
                <span class="font-mono text-[9px] text-pos-muted">{d.ip} • {d.app_version}</span>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-[10px] text-pos-muted font-bold">
            {#if status?.serving}
              {t('net_no_devices_yet', $currentLocale)}
            {:else}
              {t('net_no_devices_client', $currentLocale)}
            {/if}
          </p>
        {/if}
      </div>

      <!-- Discovery diagnostics: what the wire REALLY did -->
      <div class="p-3 bg-slate-50 dark:bg-slate-800/60 rounded-xl border border-pos-border/60">
        <p class="text-[9px] font-black text-pos-muted uppercase mb-2">
          Discovery (UDP 50110)
        </p>
        <div class="grid grid-cols-2 gap-x-3 gap-y-1 text-[10px] font-mono">
          <span class="text-pos-muted">probes sent:</span>
          <span class="font-black text-pos-text">{status?.discovery_diag?.probes_sent ?? 0}</span>
          <span class="text-pos-muted">answers received:</span>
          <span class="font-black {status?.discovery_diag?.answers_received ? 'text-emerald-600' : 'text-rose-600'}">{status?.discovery_diag?.answers_received ?? 0}</span>
          <span class="text-pos-muted">announces received:</span>
          <span class="font-black {status?.discovery_diag?.announces_received ? 'text-emerald-600' : 'text-rose-600'}">{status?.discovery_diag?.announces_received ?? 0}</span>
          <span class="text-pos-muted">announces sent:</span>
          <span class="font-black text-pos-text">{status?.discovery_diag?.announces_sent ?? 0}</span>
          <span class="text-pos-muted">last answer from:</span>
          <span class="font-black text-pos-text truncate">{status?.discovery_diag?.last_answer_from || '—'}</span>
          <span class="text-pos-muted">last server:</span>
          <span class="font-black text-pos-text truncate">{status?.last_server || '—'}</span>
        </div>
      </div>

      <div class="p-3 bg-slate-50 dark:bg-slate-800/60 rounded-xl border border-pos-border/60">
        <p class="text-[9px] font-black text-pos-muted uppercase mb-2 flex items-center gap-1">
          <Activity class="w-3.5 h-3.5" />
          {t('net_recent_activity', $currentLocale)}
        </p>
        {#if $networkEvents.length || status?.events?.length}
          <div class="space-y-1 max-h-28 overflow-y-auto">
            {#each ($networkEvents.length ? $networkEvents : status?.events || []) as ev}
              <div class="flex items-center justify-between text-[10px]">
                <span class="font-bold text-pos-text truncate">{ev.type}</span>
                <span class="font-mono text-[9px] text-pos-muted">{fmtTime(ev.ts)}</span>
              </div>
            {/each}
          </div>
        {:else}
          <p class="text-[10px] text-pos-muted font-bold">—</p>
        {/if}
      </div>

      <div class="flex items-center justify-between pt-2 border-t border-pos-border">
        <span class="text-[9px] font-mono text-pos-muted">
          {t('net_term_label', $currentLocale)}: {status?.term ?? '—'}
        </span>
        <button
          type="button"
          on:click={manualRefresh}
          disabled={busy}
          class="px-3 py-1.5 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white text-[10px] font-black rounded-lg cursor-pointer flex items-center gap-1"
        >
          <RefreshCw class="w-3 h-3 {busy ? 'animate-spin' : ''}" />
          {t('net_refresh', $currentLocale)}
        </button>
      </div>
      </div>
    </div>
  </div>
{/if}
