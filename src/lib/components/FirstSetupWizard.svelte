<script lang="ts">
  // First-launch setup wizard: shop identity, contact and logo. Shows once
  // (until the user completes it), before any other UI.
  // Step 2 (LAN): name this PC + network role (Server / Client / Automatic).
  import { invoke } from '@tauri-apps/api/core';
  import { t, currentLocale } from '../i18n';
  import { Store, Phone, MapPin, Check, Server, MonitorSmartphone, Sparkles, RefreshCw } from 'lucide-svelte';

  export let onDone: () => void = () => {};

  let step = 1;
  let shopNameFr = '';
  let shopNameAr = '';
  let shopPhone = '';
  let shopAddress = '';
  let shopRc = '';
  let shopNif = '';
  let ownerName = '';
  let logoDataUrl: string | null = null;
  let isSaving = false;

  // --- LAN step state ---
  let pcName = '';
  let netRole: 'server' | 'client' | 'automatic' = 'automatic';
  let discovered: any[] = [];
  let discovering = false;
  let joinError = '';
  let manualIp = '';
  let probed: any = null;

  async function searchServers() {
    discovering = true;
    joinError = '';
    try {
      // Two probe waves a second apart so fresh startups answer quickly.
      discovered = await invoke<any[]>('network_discovered_servers');
      await new Promise((r) => setTimeout(r, 1200));
      discovered = await invoke<any[]>('network_discovered_servers');
    } catch {
      discovered = [];
    } finally {
      discovering = false;
    }
  }

  $: if (step === 2) {
    searchServers();
  }

  async function joinDiscovered(srv: any) {
    joinError = '';
    try {
      await invoke('network_join_server', {
        addr: `${srv.ip}:${srv.port ?? 8080}`,
        shopId: srv.shop_id || '',
      });
      await finishNetwork();
    } catch (e: any) {
      joinError = typeof e === 'string' ? e : e?.message || String(e);
    }
  }

  async function probeManual() {
    joinError = '';
    probed = null;
    try {
      probed = await invoke('network_probe_server', { addr: manualIp });
    } catch (e: any) {
      joinError = typeof e === 'string' ? e : e?.message || String(e);
    }
  }

  async function joinManual() {
    if (!probed) return;
    joinError = '';
    try {
      await invoke('network_join_server', { addr: manualIp, shopId: probed.shop_id || '' });
      await finishNetwork();
    } catch (e: any) {
      joinError = typeof e === 'string' ? e : e?.message || String(e);
    }
  }

  async function finishNetwork() {
    try {
      isSaving = true;
      await invoke('network_set_setup', { pcName, role: netRole });
      if (netRole === 'server') {
        await invoke('network_become_server', {
          shopName: shopNameFr.trim() || shopNameAr.trim() || null,
        });
      }
      await finish();
    } catch (e: any) {
      alert('Network setup failed: ' + (e?.message || e));
      isSaving = false;
    }
  }

  async function handleLogo(e: Event) {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => (logoDataUrl = String(reader.result));
    reader.readAsDataURL(file);
  }

  async function finish() {
    try {
      isSaving = true;
      const settings: Record<string, string> = {
        shop_name_fr: shopNameFr,
        shop_name_ar: shopNameAr,
        shop_phone: shopPhone,
        shop_address: shopAddress,
        shop_rc: shopRc,
        shop_nif: shopNif,
        shop_owner_name: ownerName,
        first_setup_completed: 'true',
      };
      if (logoDataUrl) {
        settings['shop_logo_data'] = logoDataUrl;
      }
      await invoke('set_multiple_settings', { settings });
      onDone();
    } catch (e: any) {
      alert('Setup failed: ' + (e?.message || e));
    } finally {
      isSaving = false;
    }
  }
</script>

<div class="fixed inset-0 z-[100] bg-slate-950/95 backdrop-blur-sm flex items-center justify-center p-6">
  <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-lg p-8 space-y-6 max-h-[92vh] overflow-y-auto">
    <div class="text-center">
      <div class="w-16 h-16 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center mx-auto mb-3">
        <Store class="w-8 h-8" />
      </div>
      <h1 class="text-2xl font-black text-pos-text">Welcome to TitaouPosCRM</h1>
      <p class="text-xs text-pos-muted mt-1">
        First-time setup — configure your shop identity (مرحباً — إعداد بيانات محلّك)
      </p>
    </div>

    {#if step === 1}
      <div class="space-y-3">
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Shop Name (Français)</label>
          <input type="text" bind:value={shopNameFr} placeholder="Superette Titaou" class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500" />
        </div>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">اسم المحل (بالعربية)</label>
          <input type="text" bind:value={shopNameAr} placeholder="سوبريت تيتاو" class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500" dir="rtl" />
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1 flex items-center gap-1"><Phone class="w-3 h-3" /> Phone</label>
            <input type="text" bind:value={shopPhone} placeholder="0550..." class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm text-pos-text font-mono outline-none" />
          </div>
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Owner Name</label>
            <input type="text" bind:value={ownerName} placeholder="Owner / المسؤول" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm text-pos-text outline-none" />
          </div>
        </div>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1 flex items-center gap-1"><MapPin class="w-3 h-3" /> Address</label>
          <input type="text" bind:value={shopAddress} placeholder="Rue..., Alger" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm text-pos-text outline-none" />
        </div>
      </div>
    {:else if step === 2}
      <!-- LAN: name this PC + choose the network role -->
      <div class="space-y-4">
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">PC Name / اسم هذا الجهاز</label>
          <input type="text" bind:value={pcName} placeholder="POS-01" class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm font-black text-pos-text font-mono outline-none focus:ring-2 focus:ring-sky-500" />
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-2">Network Role / دور الشبكة</label>
          <div class="grid grid-cols-3 gap-2">
            <button type="button" on:click={() => (netRole = 'server')}
              class="p-3 rounded-xl border-2 text-start transition cursor-pointer {netRole === 'server' ? 'border-sky-500 bg-sky-50 dark:bg-sky-950/40' : 'border-pos-border hover:border-sky-300'}">
              <Server class="w-5 h-5 {netRole === 'server' ? 'text-sky-600' : 'text-pos-muted'}" />
              <p class="text-xs font-black text-pos-text mt-1">Server</p>
              <p class="text-[9px] text-pos-muted leading-snug mt-0.5">Main shop database — accepts other terminals</p>
            </button>
            <button type="button" on:click={() => (netRole = 'client')}
              class="p-3 rounded-xl border-2 text-start transition cursor-pointer {netRole === 'client' ? 'border-sky-500 bg-sky-50 dark:bg-sky-950/40' : 'border-pos-border hover:border-sky-300'}">
              <MonitorSmartphone class="w-5 h-5 {netRole === 'client' ? 'text-sky-600' : 'text-pos-muted'}" />
              <p class="text-xs font-black text-pos-text mt-1">Client</p>
              <p class="text-[9px] text-pos-muted leading-snug mt-0.5">Connects to another TitaouPosCRM terminal</p>
            </button>
            <button type="button" on:click={() => (netRole = 'automatic')}
              class="p-3 rounded-xl border-2 text-start transition cursor-pointer {netRole === 'automatic' ? 'border-sky-500 bg-sky-50 dark:bg-sky-950/40' : 'border-pos-border hover:border-sky-300'}">
              <Sparkles class="w-5 h-5 {netRole === 'automatic' ? 'text-sky-600' : 'text-pos-muted'}" />
              <p class="text-xs font-black text-pos-text mt-1">Automatic</p>
              <p class="text-[9px] text-pos-muted leading-snug mt-0.5">Discover & elect the shop server (recommended)</p>
            </button>
          </div>
        </div>

        {#if netRole === 'client'}
          <!-- Client: show discovered TitaouPosCRM shops -->
          <div class="p-3 bg-slate-50 dark:bg-slate-800/60 rounded-xl border border-pos-border space-y-2">
            <div class="flex items-center justify-between">
              <p class="text-[11px] font-black text-pos-text">Searching for TitaouPosCRM servers…</p>
              <button type="button" on:click={searchServers} class="p-1 text-sky-600 hover:text-sky-700 cursor-pointer" disabled={discovering}>
                <RefreshCw class="w-3.5 h-3.5 {discovering ? 'animate-spin' : ''}" />
              </button>
            </div>
            {#if joinError}
              <p class="text-[10px] font-bold text-rose-600">{joinError}</p>
            {/if}
            {#if discovered.length}
              {#each discovered as srv}
                <div class="flex items-center justify-between p-2 bg-white dark:bg-slate-900 rounded-lg border border-pos-border">
                  <div>
                    <p class="text-xs font-black text-pos-text">{srv.shop_name || 'TitaouPosCRM Shop'}</p>
                    <p class="text-[9px] text-pos-muted font-mono">{srv.pc_name} • {srv.ip}:{srv.port ?? 8080} • ● Online</p>
                  </div>
                  <button type="button" on:click={() => joinDiscovered(srv)} class="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-700 text-white text-[10px] font-black rounded-lg cursor-pointer">Join</button>
                </div>
              {/each}
            {:else if !discovering}
              <p class="text-[10px] text-pos-muted font-bold">No TitaouPosCRM server was found on this network.</p>
            {/if}
            <div class="pt-1 flex gap-1.5">
              <input type="text" bind:value={manualIp} placeholder="Enter server IP manually (192.168.x.x:8080)" class="flex-1 px-2.5 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-[10px] font-mono text-pos-text outline-none" />
              <button type="button" on:click={probeManual} class="px-2.5 py-1.5 bg-slate-200 dark:bg-slate-700 text-pos-text text-[10px] font-black rounded-lg cursor-pointer">Probe</button>
            </div>
            {#if probed}
              <div class="flex items-center justify-between p-2 bg-emerald-50 dark:bg-emerald-950/40 rounded-lg border border-emerald-200 dark:border-emerald-800">
                <p class="text-[10px] font-black text-pos-text">{probed.shop_name || 'Shop'} — {probed.server_pc}</p>
                <button type="button" on:click={joinManual} class="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-700 text-white text-[10px] font-black rounded-lg cursor-pointer">Join</button>
              </div>
            {/if}
          </div>
        {:else if netRole === 'server'}
          <p class="text-[10px] text-pos-muted bg-sky-50 dark:bg-sky-950/30 border border-sky-200 dark:border-sky-800 rounded-xl p-3">
            This PC will store the main shop database and accept other TitaouPosCRM terminals on the LAN.
            PC name: <b class="font-mono">{pcName || 'POS-01'}</b>
          </p>
        {:else}
          <p class="text-[10px] text-pos-muted bg-sky-50 dark:bg-sky-950/30 border border-sky-200 dark:border-sky-800 rounded-xl p-3">
            TitaouPosCRM will discover the shop network automatically and elect the server.
            Nothing else to configure — IP, port and database are handled for you.
          </p>
        {/if}
      </div>
    {:else}
      <div class="space-y-3">
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">RC</label>
            <input type="text" bind:value={shopRc} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm text-pos-text font-mono outline-none" />
          </div>
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">NIF</label>
            <input type="text" bind:value={shopNif} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm text-pos-text font-mono outline-none" />
          </div>
        </div>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-2">Shop Logo (optional)</label>
          <div class="flex items-center gap-4">
            <div class="w-20 h-20 rounded-2xl bg-white border-2 border-dashed border-pos-border flex items-center justify-center overflow-hidden shrink-0">
              {#if logoDataUrl}
                <img src={logoDataUrl} alt="Logo" class="w-full h-full object-contain" />
              {:else}
                <Store class="w-8 h-8 text-pos-muted/40" />
              {/if}
            </div>
            <label class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-bold rounded-xl cursor-pointer">
              Choose Logo
              <input type="file" accept="image/*" on:change={handleLogo} class="hidden" />
            </label>
          </div>
        </div>
        <p class="text-[11px] text-pos-muted bg-sky-50 dark:bg-sky-950/30 border border-sky-200 dark:border-sky-800 rounded-xl p-3">
          Licensing note: one license covers one PC (18000 DZD), each additional network PC +5000 DZD.
          Activation follows in Settings → Activation after this setup.
        </p>
      </div>
    {/if}

    <div class="flex items-center justify-between pt-4 border-t border-pos-border">
      <span class="text-[10px] font-black text-pos-muted">STEP {step} / 3</span>
      <div class="flex items-center gap-2">
        {#if step > 1}
          <button type="button" on:click={() => (step = step - 1)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-bold rounded-xl cursor-pointer">Back</button>
        {/if}
        {#if step === 1}
          <button
            type="button"
            on:click={() => (shopNameFr.trim() || shopNameAr.trim() ? (step = 2) : undefined)}
            class="px-6 py-2.5 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl shadow-md cursor-pointer"
          >
            Continue
          </button>
        {:else if step === 2}
          <button
            type="button"
            on:click={() => (pcName.trim() ? (step = 3) : undefined)}
            disabled={!pcName.trim()}
            class="px-6 py-2.5 bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white text-xs font-black rounded-xl shadow-md cursor-pointer"
          >
            Continue
          </button>
        {:else}
          <button
            type="button"
            on:click={finish}
            disabled={isSaving || !(shopNameFr.trim() || shopNameAr.trim())}
            class="px-6 py-2.5 bg-emerald-600 hover:bg-emerald-700 disabled:opacity-40 text-white text-xs font-black rounded-xl shadow-md cursor-pointer flex items-center gap-1.5"
          >
            <Check class="w-4 h-4" />
            {isSaving ? 'Saving...' : 'Finish Setup'}
          </button>
        {/if}
      </div>
    </div>
  </div>
</div>
