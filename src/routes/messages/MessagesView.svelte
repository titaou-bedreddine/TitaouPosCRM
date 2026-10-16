<script lang="ts">
  // Messages — chat between the POS desk (this software) and the field app
  // users, over Supabase chat_messages (free-plan friendly text rows).
  // recipient_id NULL = the desk thread (any field user can write there).
  // OWNER RULE: the left panel ALWAYS lists every team member — tap one to
  // open a chat, even with no messages yet (first message starts it).
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { MessageSquare, RefreshCw, Send, UserCheck, X } from 'lucide-svelte';

  let msgs: any[] = [];
  let members: any[] = []; // cloud profiles (id, full_name, role)
  let loading = true;
  let error = '';
  let busy = false;
  let lastRefresh = '';

  let selectedPeerId: string | null = null;

  let draft = '';
  let refreshTimer: any = null;

  function nameOf(id: string | null): string {
    return (members ?? []).find((m) => m.id === id)?.full_name ?? '—';
  }

  // Partner of a message from MY point of view (I am the desk).
  function partnerOf(m: any): string | null {
    if (m.recipient_id != null) return m.recipient_id; // I sent it
    return m.sender_id; // they wrote to the desk
  }

  $: memberRows = (members ?? []).map((m) => {
    const list = (msgs ?? []).filter((mm) => partnerOf(mm) === m.id);
    const last = list[list.length - 1] ?? null;
    return {
      id: m.id,
      name: m.full_name,
      role: m.role,
      last,
      unread: list.filter((mm) => mm.recipient_id != null && !mm.read_at).length,
    };
  });

  $: threadMsgs = selectedPeerId == null
    ? []
    : (msgs ?? []).filter((m) => partnerOf(m) === selectedPeerId);

  async function load() {
    error = '';
    try {
      try {
        members = await invoke<any[]>('cloud_team_members');
      } catch {
        members = [];
      }
      msgs = (await invoke<any[]>('cloud_chat_list').catch(() => [])) ?? [];
      lastRefresh = new Date().toLocaleTimeString();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Failed';
    } finally {
      loading = false;
    }
  }

  async function send() {
    const body = draft.trim();
    if (!body || busy || selectedPeerId == null) return;
    busy = true;
    error = '';
    try {
      await invoke('cloud_chat_send', { recipientId: selectedPeerId, body });
      draft = '';
      await load();
      await tick();
      const el = document.getElementById('chatScroll');
      el?.scrollTo({ top: el.scrollHeight });
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Send failed';
    } finally {
      busy = false;
    }
  }

  async function openThread(peer: string | null) {
    selectedPeerId = peer;
    const peerMsgs = (msgs ?? []).filter(
      (m) => m.recipient_id != null && !m.read_at && partnerOf(m) === peer
    );
    if (peerMsgs.length > 0 && peer != null) {
      try {
        await invoke('cloud_chat_mark_read', { senderId: peer });
      } catch { /* read receipts are best-effort */ }
    }
    await tick();
    const el = document.getElementById('chatScroll');
    el?.scrollTo({ top: el.scrollHeight });
  }

  onMount(async () => {
    await load();
    refreshTimer = setInterval(load, 20000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });
</script>

<div class="p-4 md:p-6 h-full flex flex-col">
  <div class="flex items-center justify-between shrink-0 pb-3 border-b border-pos-border">
    <div>
      <h1 class="text-lg font-black text-pos-text flex items-center gap-2">
        <MessageSquare class="w-5 h-5 text-sky-500" />
        Messages / الرسائل
      </h1>
      <p class="text-[10px] text-pos-muted">اضغط على عضو لبدء المحادثة · Tap a team member to chat</p>
    </div>
    <button type="button" on:click={load} disabled={loading}
      class="p-2 text-pos-muted hover:text-sky-600 border border-pos-border rounded-xl cursor-pointer">
      <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
    </button>
  </div>

  {#if error}
    <p class="mt-2 text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2 shrink-0">❌ {error}</p>
  {/if}

  <div class="flex-1 min-h-0 mt-3 grid grid-cols-[250px_1fr] gap-3">
    <!-- People panel: ALWAYS lists every team member -->
    <div class="bg-pos-card border border-pos-border rounded-2xl overflow-y-auto">
      <p class="px-3 py-2 text-[10px] font-black text-pos-muted uppercase border-b border-pos-border/60 sticky top-0 bg-pos-card">Team / الفريق — {memberRows.length}</p>
      {#if memberRows.length === 0}
        <p class="text-[11px] text-pos-muted p-3">
          No team members yet — add them on the Employees page (Field Team toggle) and they appear here.
        </p>
      {/if}
      {#each memberRows as m (m.id)}
        <button type="button" on:click={() => openThread(m.id)}
          class="w-full text-start px-3 py-2.5 border-b border-pos-border/40 hover:bg-sky-50 dark:hover:bg-sky-950/40 transition cursor-pointer
          {selectedPeerId === m.id ? 'bg-sky-100/70 dark:bg-sky-950/60' : ''}">
          <div class="flex items-center justify-between gap-1">
            <span class="text-[11px] font-black text-pos-text truncate">
              <UserCheck class="w-3 h-3 inline -mt-0.5 text-sky-500" />
              {m.name}
            </span>
            {#if m.unread > 0}
              <span class="px-1.5 py-0.5 rounded-full bg-rose-600 text-white text-[9px] font-black shrink-0">{m.unread}</span>
            {/if}
          </div>
          <p class="text-[10px] text-pos-muted truncate">
            {#if m.role}<span class="uppercase font-bold">{m.role}</span> · {/if}
            {#if m.last}{m.last.body}{:else}Tap to start / ابدأ المحادثة{/if}
          </p>
        </button>
      {/each}
    </div>

    <!-- Thread view -->
    <div class="bg-pos-card border border-pos-border rounded-2xl flex flex-col min-h-0">
      <div class="px-4 py-2.5 border-b border-pos-border flex items-center justify-between shrink-0">
        <p class="text-xs font-black text-pos-text">
          {#if selectedPeerId == null}
            <span class="text-pos-muted font-bold">اختر عضواً من القائمة / Pick a team member →</span>
          {:else}
            {nameOf(selectedPeerId)}
          {/if}
        </p>
        <span class="text-[9px] font-mono text-pos-muted">{lastRefresh}</span>
      </div>

      <div id="chatScroll" class="flex-1 overflow-y-auto p-4 space-y-2">
        {#if selectedPeerId == null}
          <div class="h-full flex flex-col items-center justify-center text-center gap-2">
            <MessageSquare class="w-8 h-8 text-sky-300" />
            <p class="text-xs font-bold text-pos-text">Select a person on the left →</p>
            <p class="text-[11px] text-pos-muted">Their chat opens here. Your first message starts the conversation on their Android app.</p>
          </div>
        {:else if threadMsgs.length === 0}
          <p class="text-[11px] text-pos-muted text-center py-8">
            No messages with {nameOf(selectedPeerId)} yet — say hello! 👋
          </p>
        {:else}
          {#each threadMsgs as m (m.id)}
            <div class="flex {partnerOf(m) === m.sender_id ? 'justify-start' : 'justify-end'}">
              <div class="max-w-[75%] px-3 py-2 rounded-2xl text-xs
                {partnerOf(m) === m.sender_id ? 'bg-slate-100 dark:bg-slate-800 text-pos-text' : 'bg-sky-600 text-white'}">
                <p class="font-bold whitespace-pre-wrap">{m.body}</p>
                <p class="text-[9px] opacity-70 font-mono mt-0.5">{String(m.created_at || '').slice(0, 16).replace('T', ' ')}</p>
              </div>
            </div>
          {/each}
        {/if}
      </div>

      <div class="p-3 border-t border-pos-border flex items-center gap-2 shrink-0">
        {#if selectedPeerId !== null}
          <input
            type="text"
            bind:value={draft}
            on:keydown={(e) => e.key === 'Enter' && send()}
            placeholder="Message to {nameOf(selectedPeerId)}…"
            class="flex-1 px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
          <button type="button" on:click={send} disabled={busy || !draft.trim()}
            class="px-4 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white text-xs font-black rounded-xl cursor-pointer flex items-center gap-1.5">
            <Send class="w-4 h-4" />Send
          </button>
        {:else}
          <p class="text-[11px] text-pos-muted font-bold w-full">…then write here. The message lands on their Android app.</p>
        {/if}
      </div>
    </div>
  </div>
</div>
