<script lang="ts">
  // Live Map — FREE stack, no API key: OpenStreetMap tiles + Leaflet CDN.
  // - Trucks: last GPS pushed by the seller app (every minute while a trip
  //   runs); >10 min stale = grey. Click a truck button (or marker) for its
  //   money card: sales, debts, expenses, cash, trips.
  // - Clients: CRM clients with GPS as markers; a side list + detail card
  //   with total sales / total debts / current balance / sales history.
  // Client money lives in the POS ledger — matched to the cloud client by
  // NAME (walk-ins and unmapped clients show the CRM balance only).
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t, currentLocale } from '../../lib/i18n';
  import type { Customer, Sale } from '../../lib/types';
  import { MapPin, RefreshCw, Truck as TruckIcon, Route as RouteIcon, Users, X, Receipt, History } from 'lucide-svelte';

  // Deep-links: Trucks page (Position / Checkpoints) + Clients page (map btn)
  export let focusTruckId = '';
  export let focusCheckpoints = false;
  export let focusClientId = '';

  let clients: any[] = [];
  let trucks: any[] = [];
  let positions: any[] = [];
  let truckStats: any[] = []; // all-time stats_truck_trips rows (per truck)
  let localCustomers: Customer[] = [];
  let checkpointNames = new Set<string>();
  let loading = true;
  let error = '';
  let lastRefresh = '';
  let mapEl: HTMLDivElement | undefined;

  // TIME MACHINE: pick a truck + a calendar day -> its whole recorded trail
  // for that day, stops labelled by nearest client (<=120 m). Positions are
  // kept on the CRM for 7 days (migration 0054).
  let tmTruckId = '';
  let tmDate = '';
  let tmLoading = false;
  let tmPoints: { lat: number; lng: number; at: string; client: string | null }[] = [];
  let tmLayer: any = null;
  let tmError = '';

  function haversineMeters(lat1: number, lng1: number, lat2: number, lng2: number): number {
    const R = 6371000;
    const dLat = ((lat2 - lat1) * Math.PI) / 180;
    const dLng = ((lng2 - lng1) * Math.PI) / 180;
    const a =
      Math.sin(dLat / 2) * Math.sin(dLat / 2) +
      Math.cos((lat1 * Math.PI) / 180) * Math.cos((lat2 * Math.PI) / 180) * Math.sin(dLng / 2) * Math.sin(dLng / 2);
    return R * 2 * Math.atan2(Math.sqrt(a), Math.sqrt(1 - a));
  }

  // Sidebars + detail cards
  let showClientsPanel = false;
  let showTrucksPanel = false;
  let selectedClient: any = null;
  let selectedClientLocal: Customer | null = null;
  let selectedClientHistory: Sale[] = [];
  let selectedTruck: any = null;
  let selectedTruckName = '';

  type LMap = any;
  let L: any = null;
  let map: LMap | null = null;
  let clientLayer: any = null;
  let truckLayer: any = null;
  let refreshTimer: any = null;
  let lastTruckFocusKey = '';
  let lastClientFocusKey = '';

  const STALE_MS = 10 * 60 * 1000;

  async function ensureLeaflet(): Promise<any> {
    if ((window as any).L) return (window as any).L;
    if (!document.getElementById('leaflet-css')) {
      const css = document.createElement('link');
      css.id = 'leaflet-css';
      css.rel = 'stylesheet';
      css.href = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.css';
      document.head.appendChild(css);
    }
    await new Promise<void>((resolve, reject) => {
      const js = document.createElement('script');
      js.src = 'https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
      js.onload = () => resolve();
      js.onerror = () => reject(new Error('Leaflet CDN unreachable'));
      document.head.appendChild(js);
    });
    return (window as any).L;
  }

  async function loadData() {
    error = '';
    try {
      [clients, trucks, positions, truckStats, localCustomers] = await Promise.all([
        invoke<any[]>('cloud_clients_positions').catch(() => []),
        invoke<any[]>('cloud_list_trucks').catch(() => []),
        invoke<any[]>('cloud_truck_positions', { fromDate: null, toDate: null }).catch(() => []),
        invoke<any[]>('cloud_stats_truck_trips', { fromDate: null, toDate: null }).catch(() => []),
        invoke<any[]>('list_customers').catch(() => []),
      ]);
      lastRefresh = new Date().toLocaleTimeString();
      await rebuildLayers();
      await applyFocus();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      loading = false;
    }
  }

  async function loadTripClientNames(truckId: string) {
    checkpointNames = new Set();
    try {
      const loads: any[] = await invoke('cloud_truck_loads').catch(() => []);
      const active = (loads ?? []).find(
        (l) => l.truck_id === truckId && ['loaded', 'in_progress', 'reconciling'].includes(l.status)
      );
      if (!active) return;
      const orders = await invoke<any[]>('cloud_trip_orders', { loadId: active.id }).catch(() => []);
      const names = new Set<string>();
      for (const o of orders ?? []) {
        const name = o?.client?.name;
        if (name) names.add(String(name).trim().toLowerCase());
      }
      checkpointNames = names;
    } catch {
      checkpointNames = new Set();
    }
  }

  function latestPerTruck(): any[] {
    const seen = new Set<string>();
    const out: any[] = [];
    for (const p of positions ?? []) {
      if (seen.has(p.truck_id)) continue;
      seen.add(p.truck_id);
      out.push(p);
    }
    return out;
  }

  function truckName(id: string): string {
    return trucks.find((x) => x.id === id)?.name ?? 'Truck';
  }

  // ── Detail card data ───────────────────────────────────────────────────────
  function hasGps(c: any): boolean {
    const lat = Number(c?.latitude), lng = Number(c?.longitude);
    return Number.isFinite(lat) && Number.isFinite(lng) && !(lat === 0 && lng === 0);
  }

  async function openClient(c: any) {
    selectedClient = c;
    showClientsPanel = false;
    selectedClientLocal = (localCustomers ?? []).find(
      (x) => (x.name || '').trim().toLowerCase() === String(c.name || '').trim().toLowerCase()
    ) ?? null;
    selectedClientHistory = [];
    // Sales history from the POS ledger (field + POS sales synced locally).
    if (selectedClientLocal) {
      try {
        const sales: Sale[] = await invoke('list_sales', {
          startDate: null, endDate: null, userId: null, limit: 500,
        });
        selectedClientHistory = (sales ?? []).filter((s) => s.customer_id === selectedClientLocal!.id).slice(0, 30);
      } catch { selectedClientHistory = []; }
    }
    // Pan to the marker — max zoomed in (owner request). Clients WITHOUT
    // GPS (never captured / NULL / 0,0) must not pan to the Gulf of Guinea:
    // the detail card explains it instead.
    if (map && L && hasGps(c)) {
      map.flyTo([Number(c.latitude), Number(c.longitude)], 18, { duration: 0.8 });
    }
  }

  function truckStatsFor(id: string) {
    const rows = (truckStats ?? []).filter((tp) => tp.out_truck_id === id);
    const sum = (k: string) => rows.reduce((s, r) => s + Number(r[k] ?? 0), 0);
    return {
      trips: rows.length,
      sales: sum('out_sales_value'),
      debts: sum('out_outstanding'),
      expenses: sum('out_expenses'),
      cash: sum('out_cash_value'),
    };
  }

  async function openTruck(tr: any) {
    selectedTruck = tr;
    selectedTruckName = tr.name;
    showTrucksPanel = false;
    const pos = latestPerTruck().find((p) => p.truck_id === tr.id);
    if (map && pos && hasGps({ latitude: pos.latitude, longitude: pos.longitude })) {
      map.flyTo([Number(pos.latitude), Number(pos.longitude)], 17, { duration: 0.8 });
    }
    // No position yet: just leave the view; the money card still opens.
  }

  // ── Layers ─────────────────────────────────────────────────────────────────
  async function rebuildLayers() {
    if (!map || !L) return;
    if (clientLayer) clientLayer.remove();
    if (truckLayer) truckLayer.remove();
    clientLayer = L.layerGroup().addTo(map);
    truckLayer = L.layerGroup().addTo(map);

    for (const c of clients ?? []) {
      const lat = Number(c.latitude);
      const lng = Number(c.longitude);
      if (!Number.isFinite(lat) || !Number.isFinite(lng) || (lat === 0 && lng === 0)) continue;
      const isCheckpoint = checkpointNames.has(String(c.name || '').trim().toLowerCase());
      const marker = L.circleMarker([lat, lng], {
        radius: isCheckpoint ? 8 : 6,
        color: isCheckpoint ? '#f59e0b' : '#0284c7',
        fillColor: isCheckpoint ? '#fbbf24' : '#38bdf8',
        fillOpacity: 0.85,
        weight: 2,
      });
      marker.bindPopup(
        `<b>${escapeHtml(c.name || '—')}</b>` +
        (isCheckpoint ? '<br/><span style="color:#f59e0b;font-weight:900">CHECKPOINT / محطة</span>' : '') +
        `<br/>${escapeHtml(c.address || '')}` +
        `<br/><span style="color:#0284c7;font-weight:700">click for details</span>`
      );
      marker.on('click', () => openClient(c));
      marker.addTo(clientLayer);
    }

    for (const p of latestPerTruck()) {
      const lat = Number(p.latitude);
      const lng = Number(p.longitude);
      if (!Number.isFinite(lat) || !Number.isFinite(lng)) continue;
      const ageMs = Date.now() - new Date(p.recorded_at).getTime();
      const fresh = ageMs < STALE_MS;
      const ageMin = Math.max(0, Math.round(ageMs / 60000));
      const icon = L.divIcon({
        className: '',
        html: `<div style="display:flex;align-items:center;justify-content:center;width:30px;height:30px;border-radius:9999px;` +
          `background:${fresh ? '#059669' : '#94a3b8'};color:white;box-shadow:0 0 0 4px ${fresh ? 'rgba(5,150,105,.25)' : 'rgba(148,163,184,.2)'};` +
          `font-size:15px;">🚚</div>`,
        iconSize: [30, 30],
        iconAnchor: [15, 15],
      });
      const marker = L.marker([lat, lng], { icon, zIndexOffset: 500 });
      marker.bindPopup(
        `<b>${escapeHtml(truckName(p.truck_id))}</b>` +
        `<br/>${fresh ? '🟢 LIVE' : `⚪ ${ageMin} min ago`} · ${new Date(p.recorded_at).toLocaleString()}`
      );
      marker.on('click', () => {
        const tr = trucks.find((x) => x.id === p.truck_id);
        if (tr) openTruck(tr);
      });
      marker.addTo(truckLayer);
    }
  }

  async function applyFocus() {
    if (!map || !L) return;
    if (focusTruckId) {
      const key = `t:${focusTruckId}:${focusCheckpoints ? 'cp' : 'pos'}`;
      if (lastTruckFocusKey !== key) {
        lastTruckFocusKey = key;
        if (focusCheckpoints) await loadTripClientNames(focusTruckId);
        await rebuildLayers();
        const pos = latestPerTruck().find((p) => p.truck_id === focusTruckId);
        if (pos) map.flyTo([Number(pos.latitude), Number(pos.longitude)], 17, { duration: 0.8 });
        const tr = trucks.find((x) => x.id === focusTruckId);
        if (tr) await openTruck(tr);
      }
    }
    if (focusClientId) {
      const key = `c:${focusClientId}`;
      if (lastClientFocusKey !== key) {
        lastClientFocusKey = key;
        const c = (clients ?? []).find((x) => x.id === focusClientId);
        if (c) await openClient(c);
      }
    }
  }

  $: if (map && L) { void applyFocus(); }

  function escapeHtml(s: string): string {
    return String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c] as string));
  }

  async function runTimeMachine() {
    tmError = '';
    if (!tmTruckId || !tmDate) {
      tmError = 'Pick a truck and a date / اختر شاحنة وتاريخاً';
      return;
    }
    tmLoading = true;
    try {
      const rows = await invoke<any[]>('cloud_truck_positions', {
        fromDate: `${tmDate} 00:00:00`,
        toDate: `${tmDate} 23:59:59`,
      }).catch(() => []);
      tmPoints = (rows ?? [])
        .filter((r) => r.truck_id === tmTruckId)
        .map((r) => {
          const lat = Number(r.latitude), lng = Number(r.longitude);
          let client: string | null = null;
          let best = 120;
          for (const c of clients ?? []) {
            const clat = Number(c.latitude), clng = Number(c.longitude);
            if (!Number.isFinite(clat) || !Number.isFinite(clng)) continue;
            const d = haversineMeters(lat, lng, clat, clng);
            if (d <= best) {
              best = d;
              client = c.name;
            }
          }
          return { lat, lng, at: String(r.recorded_at || ''), client };
        })
        .filter((p) => Number.isFinite(p.lat) && Number.isFinite(p.lng))
        .sort((a, b) => a.at.localeCompare(b.at));
      if (tmLayer) tmLayer.remove();
      tmLayer = null;
      if (map && L) {
        tmLayer = L.layerGroup().addTo(map);
        if (tmPoints.length > 1) {
          L.polyline(tmPoints.map((p) => [p.lat, p.lng]), { color: '#7c3aed', weight: 3, opacity: 0.8 }).addTo(tmLayer);
        }
        tmPoints.forEach((p, i) => {
          const at = p.at.slice(11, 16);
          const label = p.client ? `🛑 ${p.client}` : `#${i + 1} · ${at}`;
          const marker = L.circleMarker([p.lat, p.lng], {
            radius: p.client ? 8 : 4,
            color: p.client ? '#f59e0b' : '#7c3aed',
            fillColor: p.client ? '#fbbf24' : '#a78bfa',
            fillOpacity: 0.9,
            weight: 2,
          });
          marker.bindPopup(`<b>${escapeHtml(label)}</b><br/>${p.at.slice(0, 19).replace('T', ' ')}`);
          marker.addTo(tmLayer);
        });
        if (tmPoints.length > 0) {
          map.flyToBounds(L.latLngBounds(tmPoints.map((p) => [p.lat, p.lng] as any)).pad(0.25));
        } else {
          tmError = 'No recorded positions for that day (kept 7 days).';
        }
      }
    } catch (e: any) {
      tmError = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      tmLoading = false;
    }
  }

  onMount(async () => {
    try {
      L = await ensureLeaflet();
    } catch (e: any) {
      error = 'Map library could not load (offline?): ' + (e?.message || e);
      loading = false;
      return;
    }
    map = L.map(mapEl!, { zoomControl: true }).setView([36.2864, 6.9726], 11); // Khenchela, Algeria
    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
      maxZoom: 19,
      attribution: '&copy; OpenStreetMap contributors',
    }).addTo(map);
    await loadData();
    refreshTimer = setInterval(async () => {
      try {
        positions = await invoke<any[]>('cloud_truck_positions').catch(() => []);
        lastRefresh = new Date().toLocaleTimeString();
        await rebuildLayers();
        await applyFocus();
      } catch { /* silent */ }
    }, 30000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
    if (tmLayer) tmLayer.remove();
    if (map) { map.remove(); map = null; }
  });
</script>

<div class="p-4 md:p-6 h-full flex flex-col">
  <div class="flex items-center justify-between shrink-0 mb-2">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <MapPin class="w-5 h-5 text-sky-500" />
        {t('maps_title', $currentLocale)}
      </h1>
      <p class="text-[10px] text-pos-muted">Positions des camions en temps réel + clients / مواقع الشاحنات المباشرة والعملاء</p>
    </div>
    <div class="flex items-center gap-2">
      <!-- Side list toggles: every label on the map is a button -->
      <button type="button" on:click={() => { showClientsPanel = !showClientsPanel; showTrucksPanel = false; }}
        class="flex items-center gap-1.5 px-3 py-2 text-[11px] font-black rounded-xl cursor-pointer transition
        {showClientsPanel ? 'bg-sky-600 text-white' : 'bg-pos-card text-pos-muted hover:text-pos-text border border-pos-border'}">
        <Users class="w-3.5 h-3.5" />Clients ({(clients ?? []).length})
      </button>
      <button type="button" on:click={() => { showTrucksPanel = !showTrucksPanel; showClientsPanel = false; }}
        class="flex items-center gap-1.5 px-3 py-2 text-[11px] font-black rounded-xl cursor-pointer transition
        {showTrucksPanel ? 'bg-emerald-600 text-white' : 'bg-pos-card text-pos-muted hover:text-pos-text border border-pos-border'}">
        <TruckIcon class="w-3.5 h-3.5" />Trucks ({(trucks ?? []).length})
      </button>
      <button type="button" on:click={loadData} disabled={loading}
        class="p-2 text-pos-muted hover:text-sky-600 border border-pos-border rounded-xl cursor-pointer" title="Refresh">
        <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      </button>
    </div>
  </div>

  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2 shrink-0 mb-2">❌ {error}</p>

  <!-- Time machine: past stops of a selected truck on a chosen day -->
  <div class="flex items-center gap-2 flex-wrap shrink-0 bg-purple-50 dark:bg-purple-950/30 border border-purple-200 dark:border-purple-900 rounded-2xl p-2.5">
    <span class="text-[10px] font-black uppercase text-purple-700 dark:text-purple-300">Time machine / الأرشيف</span>
    <select bind:value={tmTruckId}
      class="px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-[11px] font-bold text-pos-text outline-none cursor-pointer">
      <option value="">— Truck / الشاحنة —</option>
      {#each (trucks ?? []) as tr (tr.id)}
        <option value={tr.id}>{tr.name}</option>
      {/each}
    </select>
    <input type="date" bind:value={tmDate}
      class="px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-[11px] font-bold text-pos-text outline-none" />
    <button type="button" on:click={runTimeMachine} disabled={tmLoading || !tmTruckId || !tmDate}
      class="px-3 py-1.5 bg-purple-600 hover:bg-purple-700 disabled:opacity-40 text-white text-[11px] font-black rounded-xl cursor-pointer">
      {tmLoading ? '…' : 'Show stops / العرض'}
    </button>
    {#if tmPoints.length > 0}
      <span class="text-[10px] font-bold text-pos-muted">{tmPoints.length} pts · {tmPoints.filter((p) => p.client).length} stops</span>
    {/if}
    {#if tmError}
      <span class="text-[10px] font-bold text-rose-600">{tmError}</span>
    {/if}
  </div>
  {/if}

  <div class="relative flex-1 min-h-[320px] rounded-2xl overflow-hidden border border-pos-border">
    <div bind:this={mapEl} class="absolute inset-0 z-0" style="background:#e5eaf0;"></div>
    {#if loading}
      <div class="absolute inset-0 flex items-center justify-center text-xs text-pos-muted">Loading map…</div>
    {/if}

    <!-- Clients side list -->
    {#if showClientsPanel}
      <div class="absolute z-[500] top-0 start-0 h-full w-72 bg-pos-card/95 backdrop-blur border-e border-pos-border shadow-xl flex flex-col">
        <div class="flex items-center justify-between px-3 py-2 border-b border-pos-border shrink-0">
          <p class="text-xs font-black text-pos-text flex items-center gap-1.5"><Users class="w-3.5 h-3.5 text-sky-500" />Clients</p>
          <button type="button" on:click={() => (showClientsPanel = false)} class="text-pos-muted hover:text-pos-text cursor-pointer"><X class="w-4 h-4" /></button>
        </div>
        <div class="overflow-y-auto p-2 space-y-1">
          {#each (clients ?? []).filter((c) => Number(c.latitude) !== 0 && Number.isFinite(Number(c.latitude))) as c (c.id)}
            <button type="button" on:click={() => openClient(c)}
              class="w-full text-start px-2.5 py-2 rounded-xl hover:bg-sky-50 dark:hover:bg-sky-950/40 transition cursor-pointer {selectedClient?.id === c.id ? 'bg-sky-100/70 dark:bg-sky-950/60' : ''}">
              <p class="text-[11px] font-black text-pos-text truncate">{c.name}</p>
              <p class="text-[9px] text-pos-muted truncate">{c.address || '—'}</p>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- Trucks side list -->
    {#if showTrucksPanel}
      <div class="absolute z-[500] top-0 start-0 h-full w-72 bg-pos-card/95 backdrop-blur border-e border-pos-border shadow-xl flex flex-col">
        <div class="flex items-center justify-between px-3 py-2 border-b border-pos-border shrink-0">
          <p class="text-xs font-black text-pos-text flex items-center gap-1.5"><TruckIcon class="w-3.5 h-3.5 text-emerald-500" />Trucks</p>
          <button type="button" on:click={() => (showTrucksPanel = false)} class="text-pos-muted hover:text-pos-text cursor-pointer"><X class="w-4 h-4" /></button>
        </div>
        <div class="overflow-y-auto p-2 space-y-1">
          {#each (trucks ?? []) as tr (tr.id)}
            {@const pos = latestPerTruck().find((p) => p.truck_id === tr.id)}
            <button type="button" on:click={() => openTruck(tr)}
              class="w-full text-start px-2.5 py-2 rounded-xl hover:bg-emerald-50 dark:hover:bg-emerald-950/40 transition cursor-pointer {selectedTruck?.id === tr.id ? 'bg-emerald-100/70 dark:bg-emerald-950/60' : ''}">
              <p class="text-[11px] font-black text-pos-text">🚚 {tr.name}</p>
              <p class="text-[9px] text-pos-muted">{pos ? new Date(pos.recorded_at).toLocaleString() : 'لا توجد نقطة / no fix yet'}</p>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <!-- CLIENT detail card -->
    {#if selectedClient}
      <div class="absolute z-[500] top-3 end-3 w-80 max-h-[85%] bg-pos-card/97 backdrop-blur border border-pos-border rounded-2xl shadow-2xl flex flex-col">
        <div class="flex items-start justify-between p-3 border-b border-pos-border shrink-0">
          <div class="min-w-0">
            <p class="text-xs font-black text-pos-text truncate">{selectedClient.name}</p>
            <p class="text-[9px] text-pos-muted truncate">{selectedClient.address || '—'}</p>
            {#if !hasGps(selectedClient)}
              <p class="text-[9px] font-black text-amber-600">⚠ No GPS recorded / لا توجد إحداثيات</p>
            {/if}
          </div>
          <button type="button" on:click={() => (selectedClient = null)} class="text-pos-muted hover:text-pos-text cursor-pointer shrink-0"><X class="w-4 h-4" /></button>
        </div>
        <div class="p-3 space-y-2 overflow-y-auto">
          <div class="grid grid-cols-3 gap-2 text-center">
            <div class="p-2 bg-sky-50 dark:bg-sky-950/40 rounded-xl">
              <p class="text-[8px] font-black text-sky-600 uppercase">Total Sales</p>
              <p class="text-[11px] font-black font-mono text-pos-text">
                {selectedClientLocal ? (selectedClientLocal.total_purchases ?? 0).toLocaleString() : '—'}
              </p>
            </div>
            <div class="p-2 bg-rose-50 dark:bg-rose-950/40 rounded-xl">
              <p class="text-[8px] font-black text-rose-600 uppercase">Total Debts</p>
              <p class="text-[11px] font-black font-mono text-rose-600">
                {selectedClientLocal ? selectedClientLocal.balance.toLocaleString() : Number(selectedClient.current_balance ?? 0).toLocaleString()}
              </p>
            </div>
            <div class="p-2 bg-emerald-50 dark:bg-emerald-950/40 rounded-xl">
              <p class="text-[8px] font-black text-emerald-600 uppercase">Balance</p>
              <p class="text-[11px] font-black font-mono text-emerald-700">
                {selectedClientLocal ? selectedClientLocal.balance.toLocaleString() : Number(selectedClient.current_balance ?? 0).toLocaleString()} DZD
              </p>
            </div>
          </div>
          <p class="text-[9px] font-black text-pos-muted uppercase flex items-center gap-1">
            <History class="w-3 h-3" />Sales history {selectedClientHistory.length ? `(${selectedClientHistory.length})` : ''}
          </p>
          <div class="max-h-40 overflow-y-auto space-y-1">
            {#each selectedClientHistory as s (s.id)}
              <div class="flex items-center justify-between text-[10px] px-2 py-1.5 bg-slate-50 dark:bg-slate-800/60 rounded-lg">
                <span class="font-mono text-sky-600 font-bold truncate">#{s.sale_number}</span>
                <span class="font-mono font-black text-pos-text">{s.total_amount.toLocaleString()} DZD</span>
              </div>
            {/each}
            {#if selectedClientHistory.length === 0}
              <p class="text-[10px] text-pos-muted">{selectedClientLocal ? 'No sales recorded.' : 'Not linked to the POS ledger — balance from the CRM.'}</p>
            {/if}
          </div>
        </div>
      </div>
    {/if}

    <!-- TRUCK detail card -->
    {#if selectedTruck}
      {@const st = truckStatsFor(selectedTruck.id)}
      <div class="absolute z-[500] top-3 end-3 w-80 bg-pos-card/97 backdrop-blur border border-pos-border rounded-2xl shadow-2xl">
        <div class="flex items-start justify-between p-3 border-b border-pos-border">
          <div class="min-w-0">
            <p class="text-xs font-black text-pos-text truncate">🚚 {selectedTruck.name}</p>
            <p class="text-[9px] text-pos-muted font-mono">{selectedTruck.plate || '—'} · {selectedTruck.driver_name || '—'}</p>
          </div>
          <button type="button" on:click={() => (selectedTruck = null)} class="text-pos-muted hover:text-pos-text cursor-pointer shrink-0"><X class="w-4 h-4" /></button>
        </div>
        <div class="p-3 grid grid-cols-2 gap-2 text-center">
          <div class="p-2 bg-sky-50 dark:bg-sky-950/40 rounded-xl">
            <p class="text-[8px] font-black text-sky-600 uppercase">Sales / المبيعات</p>
            <p class="text-[11px] font-black font-mono text-pos-text">{Math.round(st.sales / 100).toLocaleString()} DZD</p>
          </div>
          <div class="p-2 bg-amber-50 dark:bg-amber-950/40 rounded-xl">
            <p class="text-[8px] font-black text-amber-600 uppercase">Debts / الديون</p>
            <p class="text-[11px] font-black font-mono text-amber-600">{Math.round(st.debts / 100).toLocaleString()} DZD</p>
          </div>
          <div class="p-2 bg-rose-50 dark:bg-rose-950/40 rounded-xl">
            <p class="text-[8px] font-black text-rose-600 uppercase">Expenses</p>
            <p class="text-[11px] font-black font-mono text-rose-600">{Math.round(st.expenses / 100).toLocaleString()} DZD</p>
          </div>
          <div class="p-2 bg-emerald-50 dark:bg-emerald-950/40 rounded-xl">
            <p class="text-[8px] font-black text-emerald-600 uppercase">Cash / النقد</p>
            <p class="text-[11px] font-black font-mono text-emerald-700">{Math.round(st.cash / 100).toLocaleString()} DZD</p>
          </div>
          <p class="col-span-2 text-[9px] text-pos-muted font-bold flex items-center justify-center gap-1">
            <Receipt class="w-3 h-3" />{st.trips} trips / تربات
          </p>
        </div>
      </div>
    {/if}
  </div>

  <div class="flex items-center gap-3 text-[10px] font-bold text-pos-muted shrink-0 mt-2 flex-wrap">
    <span class="flex items-center gap-1"><span class="w-2.5 h-2.5 rounded-full bg-emerald-600"></span>Truck live</span>
    <span class="flex items-center gap-1"><span class="w-2.5 h-2.5 rounded-full bg-slate-400"></span>Truck &gt; 10 min</span>
    <span class="flex items-center gap-1"><span class="w-2.5 h-2.5 rounded-full bg-sky-400"></span>Client</span>
    <span class="flex items-center gap-1"><span class="w-2.5 h-2.5 rounded-full bg-amber-400 border-2 border-amber-600"></span><RouteIcon class="w-3 h-3" />Checkpoint</span>
    {#if lastRefresh}<span class="ms-auto font-mono">{t('net_recent_activity', $currentLocale)}: {lastRefresh}</span>{/if}
  </div>
  <p class="text-[9px] text-pos-muted font-mono shrink-0 mt-1">Historique des positions: 7 derniers jours / سجل المواقع: آخر 7 أيام</p>
</div>
