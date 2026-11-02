<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { RefreshCw, Trash2, Database, AlertTriangle } from 'lucide-svelte';
  import { t } from '../i18n';

  const TABLES: { key: string; label: string }[] = [
    { key: 'products', label: 'Products / منتجات' },
    { key: 'clients', label: 'Clients / عملاء' },
    { key: 'suppliers', label: 'Suppliers / موردين' },
    { key: 'employees', label: 'Employees / موظفون' },
    { key: 'trucks', label: 'Trucks / شاحنات' },
    { key: 'transactions', label: 'Transactions / معاملات' },
  ];

  let activeTable = 'products';
  let rows: any[] = [];
  let loading = false;
  let error = '';
  let adminPassword = '';
  let deleteRowId = '';
  let msg = '';
  // Styled confirm modal state (no window.confirm in Tauri webview).
  let deleteTarget: any = null;
  let pwVerified: boolean | null = null;

  const LABELS: Record<string, string> = {
    products: 'Products / منتجات',
    clients: 'Clients / عملاء',
    suppliers: 'Suppliers / موردين',
    employees: 'Employees / موظفون',
    trucks: 'Trucks / شاحنات',
    transactions: 'Transactions / معاملات',
  };

  function displayValue(v: any): string {
    if (v === null || v === undefined) return '—';
    if (typeof v === 'boolean') return v ? '✓' : '✗';
    return String(v);
  }

  async function load() {
    loading = true; error = '';
    try {
      rows = await invoke<any[]>('cloud_browse_table', { table: activeTable, limit: 200 });
      if (!rows.length) msg = 'Empty — nothing in the cloud for this table.';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Load failed';
      rows = [];
    } finally { loading = false; }
  }

  function pick(key: string) {
    activeTable = key;
    load();
  }

  function deleteRow(row: any) {
    if (!adminPassword) { msg = 'Admin password required first / كلمة المرور مطلوبة'; return; }
    deleteTarget = row;
    msg = '';
  }

  async function confirmDeleteRow() {
    const row = deleteTarget;
    if (!row) return;
    try {
      await invoke('cloud_delete_row', { table: activeTable, id: row.id, adminPassword: adminPassword });
      rows = rows.filter((r) => r.id !== row.id);
      deleteTarget = null;
      msg = 'Deleted from the cloud ✓';
    } catch (e: any) {
      const m = typeof e === 'string' ? e : e?.message || 'Delete failed';
      if (m.toLowerCase().includes('restrict') || m.toLowerCase().includes('violates')) {
        // Sold/billed row: offer the deactivate fallback right here.
        try {
          await invoke('cloud_deactivate_row', { table: activeTable, id: row.id, adminPassword: adminPassword });
          rows = rows.filter((r) => r.id !== row.id);
          deleteTarget = null;
          msg = 'It was in use — DEACTIVATED instead (history kept) ✓';
        } catch (e2: any) {
          error = typeof e2 === 'string' ? e2 : e2?.message || 'Deactivate failed';
          deleteTarget = null;
        }
      } else {
        msg = m;
        deleteTarget = null;
      }
    }
  }

  async function verifyPassword() {
    try {
      const ok = await invoke<boolean>('verify_admin_password', { password: adminPassword });
      pwVerified = ok;
    } catch {
      pwVerified = false;
    }
  }
</script>

<div class="max-w-5xl space-y-4">
  <div>
    <h2 class="text-base font-black text-pos-text flex items-center gap-2">
      <Database class="w-4 h-4 text-sky-500" /> Cloud Data — mirror of the cloud DB
    </h2>
    <p class="text-xs text-pos-muted">Everything living on the cloud for your organization. Delete what you don't want — rows sold at least once may refuse (then purge/deactivate instead). / كل شيء على السحابة — احذف ما لا تريده.</p>
  </div>

  <div class="flex flex-wrap gap-1.5">
    {#each TABLES as tb}
      <button type="button" on:click={() => pick(tb.key)}
        class="px-3 py-1.5 text-[11px] font-black rounded-xl border cursor-pointer transition {activeTable === tb.key ? 'bg-sky-600 text-white border-sky-600' : 'text-pos-muted border-pos-border hover:border-slate-300'}">
        {tb.label}
      </button>
    {/each}
    <button type="button" on:click={load} disabled={loading}
      class="p-2 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer">
      <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
    </button>
  </div>

  <div class="flex flex-wrap items-center gap-2">
    <input type="password" bind:value={adminPassword} placeholder="Admin password (for deletes) / كلمة المرور"
      class="flex-1 min-w-52 px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none" />
    <button type="button" on:click={verifyPassword} disabled={!adminPassword}
      class="px-3 py-2 text-[11px] font-black bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 dark:hover:bg-slate-600 disabled:opacity-40 text-pos-text rounded-xl cursor-pointer transition">
      Check
    </button>
    {#if pwVerified !== null}
      <span class="text-[11px] font-black {pwVerified ? 'text-emerald-600' : 'text-rose-600'}">
        {pwVerified ? '✓ correct' : '✗ wrong'}
      </span>
    {/if}
  </div>

  {#if error}
    <div class="flex items-center gap-2 p-3 bg-rose-50 dark:bg-rose-950/40 rounded-xl border border-rose-200 dark:border-rose-900 text-[11px] font-bold text-rose-700 dark:text-rose-300"
      role="presentation">
      <AlertTriangle class="w-3.5 h-3.5 shrink-0" />{error}
    </div>
  {:else if loading}
    <p class="text-xs text-pos-muted font-bold">Loading…</p>
  {:else if rows.length === 0}
    <p class="text-xs text-pos-muted">{msg || '—'}</p>
  {:else}
    <div class="rounded-2xl border border-pos-border overflow-x-auto">
      <table class="w-full text-[10px]">
        <thead>
          <tr class="bg-slate-100 dark:bg-slate-800 text-pos-muted font-black uppercase">
            <th class="p-2 text-start">ID</th>
            {#each Object.keys(rows[0]).filter((k) => k !== 'id') as col}
              <th class="p-2 text-start">{col.replace(/_/g, ' ')}</th>
            {/each}
            <th class="p-2 text-end"></th>
          </tr>
        </thead>
        <tbody>
          {#each rows as row (row.id)}
            <tr class="border-t border-pos-border {deleteRowId === row.id ? 'bg-rose-50/50' : ''}">
              <td class="p-2 font-mono text-pos-muted">{String(row.id).slice(0, 8)}</td>
              {#each Object.keys(row).filter((k) => k !== 'id') as col}
                <td class="p-2 font-bold text-pos-text max-w-40 truncate">{displayValue(row[col])}</td>
              {/each}
              <td class="p-2 text-end">
                <button type="button" title="Delete from cloud"
                  on:click={(e) => { deleteRowId = row.id; deleteRow(row); (e.currentTarget as HTMLElement).blur?.(); }}
                  class="p-1.5 text-rose-500 hover:bg-rose-50 dark:hover:bg-rose-950/40 border border-rose-200 dark:border-rose-900 rounded-lg cursor-pointer transition">
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  {#if msg}
    <p class="text-[11px] font-bold text-pos-text">{msg}</p>
  {/if}

  {#if deleteTarget}
    <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (deleteTarget = null)} role="presentation">
      <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-sm p-5 space-y-3" on:click|stopPropagation>
        <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
          <Trash2 class="w-4 h-4 text-rose-500" />
          Delete from cloud — {LABELS[activeTable]}
        </h3>
        <p class="text-[11px] text-pos-muted font-bold">
          {Object.entries(deleteTarget).filter(([k]) => k !== 'id').map(([k, v]) => k + ': ' + displayValue(v)).join(' • ') || 'Record'}
          <br />ID: {String(deleteTarget.id).slice(0, 8)}
        </p>
        <p class="text-[10px] text-rose-600 font-bold">Deleted rows cannot be restored from the app. / الحذف نهائي من التطبيق.</p>
        <div class="flex justify-end gap-2 pt-1">
          <button type="button" on:click={() => (deleteTarget = null)}
            class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">✕</button>
          <button type="button" on:click={confirmDeleteRow} disabled={loading}
            class="px-4 py-2 text-[11px] font-black bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">
            Delete
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
