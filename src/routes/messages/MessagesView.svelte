<script lang="ts">
  // Messages — chat between the POS desk (this software) and the field app
  // users, over Supabase chat_messages (free-plan friendly text rows).
  // recipient_id NULL = the desk thread (any field user can write there).
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { MessageSquare, RefreshCw, Send, UserCheck, X } from 'lucide-svelte';

  let msgs: any[] = [];
  let members: any[] = []; // cloud profiles (id, full_name, role)
  let loading = true;
  let error = '';
  let busy = false;
  let lastRefresh = '';

  let selectedPeerId: string | null = null; // null = desk thread (incoming broadcasts)
  let draft = '';
  let refreshTimer: any = null;

  function nameOf(id: string | null): string {
    if (!id) return 'Desk (POS) / المكتب';
    return (members ?? []).find((m) => m.id === id)?.full_name ?? '—';
  }

  // Partner of a message from MY point of view (I am the desk).
  function partnerOf(m: any): string | null {
    if (m.recipient_id != null) return m.recipient_id; // I sent it
    return m.sender_id; // they wrote to the desk
  }

  $: threads = (() => {
    const byPeer = new Map<string | null, any[]>();
    for (const m of msgs ?? []) {
      const peer = partnerOf(m);
      if (peer === null) continue; // shouldn't happen from the desk view
      const list = byPeer.get(peer) ?? [];
      list.push(m);
      byPeer.set(peer, list);
    }
    return [...byPeer.entries()]
      .map(([peer, list]) => ({
        peer,
        name: nameOf(peer),
        last: list[list.length - 1],
        unread: list.filter((m) => m.recipient_id != null && !m.read_at).length,
      }))
      .sort((a, b) => String(b.last?.created_at || '').localeCompare(String(a.last?.created_at || '')));
  })();

  $: threadMsgs = (msgs ?? []).filter((m) => {
    const peer = partnerOf(m);
    if (selectedPeerId == null) {
      // "All field team" view on the desk: every incoming desk message.
      return m.sender_id != null && m.recipient_id == null;
    }
    return peer === selectedPeerId;
  });

  async function load() {
    error = '';
    try {
      // Members first so thread names resolve.
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
    if (!body || busy) return;
    busy = true;
    error = '';
    try {
      await invoke('cloud_chat_send', {
        recipientId: selectedPeerId,
        body,
      });
      draft = '';
      await load();
      await tick();
    } catch (e: any) {
      error = typeof e === 'string' ? e : e?.message || 'Send failed';
    } finally {
      busy = false;
    }
  }

  async function openThread(peer: string | null) {
    selectedPeerId = peer;
    // Mark received messages of this thread read.
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
      <p class="text-[10px] text-pos-muted">Chat with the field app users — the desk thread reaches everyone</p>
    </div>
    <button type="button" on:click={load} disabled={loading}
      class="p-2 text-pos-muted hover:text-sky-600 border border-pos-border rounded-xl cursor-pointer">
      <RefreshCw class="w-4 h-4 {loading ? 'animate-spin' : ''}" />
    </button>
  </div>

  {#if error}
    <p class="mt-2 text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2 shrink-0">❌ {error}</p>
  {/if}

  <div class="flex-1 min-h-0 mt-3 grid grid-cols-[240px_1fr] gap-3">
    <!-- Threads -->
    <div class="bg-pos-card border border-pos-border rounded-2xl overflow-y-auto">
      <p class="px-3 py-2 text-[10px] font-black text-pos-muted uppercase border-b border-pos-border/60 sticky top-0 bg-pos-card">Conversations</p>
      {#if threads.length === 0}
        <p class="text-[11px] text-pos-muted p-3">No messages yet — the field app users appear here when they write.</p>
      {/if}
      {#each threads as th (th.peer ?? 'desk')}
        <button type="button" on:click={() => openThread(th.peer)}
          class="w-full text-start px-3 py-2.5 border-b border-pos-border/40 hover:bg-sky-50 dark:hover:bg-sky-950/40 transition cursor-pointer
          {selectedPeerId === th.peer ? 'bg-sky-100/70 dark:bg-sky-950/60' : ''}">
          <div class="flex items-center justify-between gap-1">
            <span class="text-[11px] font-black text-pos-text truncate">
              <UserCheck class="w-3 h-3 inline -mt-0.5 {th.peer ? 'text-sky-500' : 'text-emerald-500'}" />
              {th.peer ? th.name : 'Field team / الفريق'}
            </span>
            {#if th.unread > 0}
              <span class="px-1.5 py-0.5 rounded-full bg-rose-600 text-white text-[9px] font-black shrink-0">{th.unread}</span>
            {/if}
          </div>
          <p class="text-[10px] text-pos-muted truncate">{th.last?.body || ''}</p>
        </button>
      {/each}
    </div>

    <!-- Thread view -->
    <div class="bg-pos-card border border-pos-border rounded-2xl flex flex-col min-h-0">
      <div class="px-4 py-2.5 border-b border-pos-border flex items-center justify-between shrink-0">
        <p class="text-xs font-black text-pos-text">
          {#if selectedPeerId == null}
            Field team / الفريق الميداني (desk thread)
          {:else}
            {nameOf(selectedPeerId)}
          {/if}
        </p>
        <span class="text-[9px] font-mono text-pos-muted">{lastRefresh}</span>
      </div>

      <div id="chatScroll" class="flex-1 overflow-y-auto p-4 space-y-2">
        {#each threadMsgs as m (m.id)}
          <div class="flex {m.sender_id != null && m.recipient_id != null && m.recipient_id !== '' && m.recipient_id != null ? '' : ''}">
            <div class="max-w-[75%] px-3 py-2 rounded-2xl text-xs
              {m.recipient_id == null && m.sender_id != null
                ? 'bg-slate-100 dark:bg-slate-800 text-pos-text'
                : 'bg-sky-600 text-white'}">
              {#if m.recipient_id == null}
                <p class="text-[9px] font-black opacity-70 uppercase">{nameOf(m.sender_id)}</p>
              {/if}
              <p class="font-bold whitespace-pre-wrap">{m.body}</p>
              <p class="text-[9px] opacity-70 font-mono mt-0.5">{String(m.created_at || '').slice(0, 16).replace('T', ' ')}</p>
            </div>
          </div>
        {/each}
        {#if threadMsgs.length === 0}
          <p class="text-[11px] text-pos-muted text-center py-8">No messages in this view yet.</p>
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
          <p class="text-[11px] text-pos-muted font-bold w-full">Pick a person on the left to answer them privately — desk messages reach every field app user.</p>
        {/if}
      </div>
    </div>
  </div>
</div>
