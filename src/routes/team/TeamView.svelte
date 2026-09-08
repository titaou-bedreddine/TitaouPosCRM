<script lang="ts">
  import { onMount } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { UsersRound, UserPlus, RefreshCw, X, Edit2, Trash2, ShieldCheck } from 'lucide-svelte';

  let members: any[] = [];
  let loading = true;
  let error = '';
  let msg = '';

  // Invite modal
  let showInvite = false;
  let invEmail = '';
  let invName = '';
  let invRole = 'preseller';
  let invPassword = '';
  let busy = false;

  // Edit modal
  let editMember: any = null;
  let editName = '';
  let editRole = 'preseller';
  let editPassword = '';

  async function load() {
    loading = true;
    error = '';
    try {
      members = await invoke<any[]>('cloud_team_members');
      msg = '';
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      loading = false;
    }
  }

  async function invite() {
    busy = true;
    error = '';
    try {
      await invoke('cloud_team_invite', {
        email: invEmail, fullName: invName, role: invRole,
        password: invPassword || null,
      });
      showInvite = false;
      invEmail = invName = invPassword = '';
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function saveEdit() {
    busy = true;
    error = '';
    try {
      await invoke('cloud_team_update', {
        userId: editMember.id,
        fullName: editName,
        role: editRole,
        password: editPassword || null,
      });
      editMember = null;
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  async function remove(m: any) {
    if (!confirm(t('team_confirm_delete'))) return;
    busy = true;
    error = '';
    try {
      await invoke('cloud_team_delete', { userId: m.id });
      await load();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      busy = false;
    }
  }

  function openEdit(m: any) {
    editMember = m;
    editName = m.full_name;
    editRole = m.role;
    editPassword = '';
  }

  onMount(load);
</script>

<div class="p-4 md:p-6 space-y-4">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <UsersRound class="w-5 h-5 text-sky-500" />
        {t('team_title')}
      </h1>
      <p class="text-xs text-pos-muted">TitaouCRM — preseller / seller / admin</p>
    </div>
    <div class="flex gap-2">
      <button type="button" on:click={load} disabled={loading}
        class="p-2 text-pos-muted hover:text-pos-text rounded-xl cursor-pointer">
        <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
      </button>
      <button type="button" on:click={() => (showInvite = true)}
        class="flex items-center gap-1.5 px-3 py-1.5 text-[11px] font-black bg-sky-600 hover:bg-sky-700 text-white rounded-xl cursor-pointer">
        <UserPlus class="w-3.5 h-3.5" />{t('team_invite')}
      </button>
    </div>
  </div>

  {#if error}
    <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">❌ {error}</p>
  {/if}

  {#if !loading && members.length === 0 && !error}
    <p class="text-center text-pos-muted text-sm py-12">☁️ {t('cloud_status_disabled')}</p>
  {/if}

  {#if members.length > 0}
    <div class="overflow-x-auto rounded-2xl border border-pos-border bg-white dark:bg-slate-900">
      <table class="w-full text-xs">
        <thead>
          <tr class="bg-slate-50 dark:bg-slate-800 text-pos-muted font-black">
            <th class="p-3 text-start">{t('team_fullname')}</th>
            <th class="p-3 text-start">Email</th>
            <th class="p-3 text-center">{t('team_role')}</th>
            <th class="p-3 text-center">{t('team_active')}</th>
            <th class="p-3 text-end"></th>
          </tr>
        </thead>
        <tbody>
          {#each members as m (m.id)}
            <tr class="border-t border-pos-border">
              <td class="p-3 font-bold text-pos-text">{m.full_name}</td>
              <td class="p-3 text-pos-muted font-mono text-[10px]">{m.email}</td>
              <td class="p-3 text-center">
                <span class="px-2 py-0.5 rounded-full text-[9px] font-black
                  {m.role === 'admin' ? 'bg-violet-100 text-violet-700' : m.role === 'preseller' ? 'bg-sky-100 text-sky-700' : 'bg-emerald-100 text-emerald-700'}">
                  {m.role}
                </span>
              </td>
              <td class="p-3 text-center">
                {#if m.is_active}
                  <ShieldCheck class="w-4 h-4 text-emerald-500 inline" />
                {:else}
                  <span class="text-rose-500 font-black">✕</span>
                {/if}
              </td>
              <td class="p-3 text-end whitespace-nowrap">
                <button type="button" class="p-1.5 text-sky-600 hover:bg-sky-50 rounded-lg cursor-pointer" on:click={() => openEdit(m)} title={t('team_edit')}>
                  <Edit2 class="w-3.5 h-3.5" />
                </button>
                <button type="button" class="p-1.5 text-rose-600 hover:bg-rose-50 rounded-lg cursor-pointer" on:click={() => remove(m)} title={t('team_delete')}>
                  <Trash2 class="w-3.5 h-3.5" />
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<!-- Invite modal -->
{#if showInvite}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (showInvite = false)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-md p-5 space-y-3" on:click|stopPropagation>
      <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
        <UserPlus class="w-4 h-4 text-sky-500" />{t('team_invite')}
      </h3>
      <input type="email" bind:value={invEmail} placeholder="email@shop.dz"
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      <input type="text" bind:value={invName} placeholder={t('team_fullname')}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      <select bind:value={invRole}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
        <option value="preseller">preseller</option>
        <option value="seller">seller</option>
        <option value="admin">admin</option>
      </select>
      <input type="text" bind:value={invPassword} placeholder="({t('cloud_password')})"
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      <div class="flex justify-end gap-2 pt-2">
        <button type="button" on:click={() => (showInvite = false)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">{t('cloud_disconnect').split(' ')[0] === 'Disconnect' ? 'Cancel' : 'إلغاء'}</button>
        <button type="button" on:click={invite} disabled={busy || !invEmail || !invName}
          class="px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">OK</button>
      </div>
    </div>
  </div>
{/if}

<!-- Edit modal -->
{#if editMember}
  <div class="fixed inset-0 z-50 bg-black/50 flex items-center justify-center p-4" on:click={() => (editMember = null)} role="presentation">
    <div class="bg-white dark:bg-slate-900 rounded-2xl border border-pos-border w-full max-w-md p-5 space-y-3" on:click|stopPropagation>
      <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
        <Edit2 class="w-4 h-4 text-sky-500" />{t('team_edit')} — {editMember.full_name}
      </h3>
      <input type="text" bind:value={editName} placeholder={t('team_fullname')}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      <select bind:value={editRole}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none">
        <option value="preseller">preseller</option>
        <option value="seller">seller</option>
        <option value="admin">admin</option>
      </select>
      <input type="text" bind:value={editPassword} placeholder="({t('cloud_password')})"
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
      <div class="flex justify-end gap-2 pt-2">
        <button type="button" on:click={() => (editMember = null)} class="px-4 py-2 text-[11px] font-black text-pos-muted hover:text-pos-text cursor-pointer">
          <X class="w-4 h-4" />
        </button>
        <button type="button" on:click={saveEdit} disabled={busy || !editName}
          class="px-4 py-2 text-[11px] font-black bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white rounded-xl cursor-pointer">OK</button>
      </div>
    </div>
  </div>
{/if}
