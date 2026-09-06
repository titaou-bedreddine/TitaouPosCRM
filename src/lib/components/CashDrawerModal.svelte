<script lang="ts">
  import { t } from '../i18n';
  import { invoke } from '@tauri-apps/api/core';
  import { activeSession } from '../stores/session';
  import { currentUser } from '../stores/auth';
  import { DollarSign, ArrowDownCircle, ArrowUpCircle, Lock, X, Check, Wallet, Clock } from 'lucide-svelte';

  export let isOpen = false;
  export let onClose: () => void;

  let mode: 'startup' | 'in' | 'out' | 'close' = 'in';
  let amount = 0;
  let reason = '';
  let countedCash = 0;
  let errorMsg = '';
  let isSubmitting = false;
  let staleWarn = false;

  $: if (isOpen) {
    reason = '';
    if (!$activeSession) {
      mode = 'startup';
      amount = 0;
    } else if (($activeSession as any).is_stale) {
      // Session left open from a previous day: prompt to close it first,
      // then the caller will offer opening today's fresh session.
      mode = 'close';
      countedCash = $activeSession.expected_cash || 0;
      staleWarn = true;
    } else {
      mode = 'in';
      amount = 0;
      countedCash = $activeSession.expected_cash || 0;
      staleWarn = false;
    }
  }

  async function handleAction() {
    if (!$currentUser) return;
    try {
      isSubmitting = true;
      errorMsg = '';

      // Empty number inputs bind as null; the backend expects an integer.
      const safeAmount = Math.round(Number(amount) || 0);
      const safeCounted = Math.round(Number(countedCash) || 0);

      if (mode === 'startup') {
        const session = await invoke<any>('open_cash_session', {
          userId: $currentUser.id,
          registerId: 1,
          openingAmount: safeAmount,
          notes: reason || 'Startup Cash / رصيد افتتاحي',
        });
        $activeSession = session;
        onClose();
      } else if (mode === 'in' || mode === 'out') {
        if (!$activeSession) {
          errorMsg = 'No active cash session found. Opening session first...';
          const session = await invoke<any>('open_cash_session', {
            userId: $currentUser.id,
            registerId: 1,
            openingAmount: 0,
            notes: 'Auto-opened Session',
          });
          $activeSession = session;
        }

        await invoke('add_cash_movement', {
          sessionId: $activeSession.id,
          userId: $currentUser.id,
          movementType: mode === 'in' ? 'cash_in' : 'cash_out',
          amount: safeAmount,
          reason: reason || (mode === 'in' ? 'Cash In / إيداع' : 'Cash Out / سحب'),
        });

        const updated = await invoke<any>('get_active_cash_session', { userId: $currentUser.id });
        $activeSession = updated;
        onClose();
      } else if (mode === 'close') {
        if (!$activeSession) return;
        await invoke('close_cash_session', {
          sessionId: $activeSession.id,
          actualCash: safeCounted,
          notes: reason || null,
        });
        $activeSession = null;
        if (staleWarn) {
          // Stale session closed — stay open and offer today's fresh session.
          staleWarn = false;
          mode = 'startup';
          amount = 0;
          reason = '';
        } else {
          onClose();
        }
      }
    } catch (err: any) {
      errorMsg = typeof err === 'string' ? err : err.message || 'Operation failed';
    } finally {
      isSubmitting = false;
    }
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-md overflow-hidden animate-in fade-in duration-150">
      <!-- Header -->
      <div class="flex items-center justify-between px-5 py-3.5 border-b border-pos-border bg-slate-50 dark:bg-slate-800/50">
        <h3 class="font-extrabold text-base text-pos-text flex items-center gap-2">
          <DollarSign class="w-5 h-5 text-amber-500" />
          <span>{mode === 'startup' ? 'Open Cash Session (فتح الصندوق)' : 'Cash Drawer Options (الصندوق)'}</span>
        </h3>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1 rounded cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      <div class="p-5 space-y-4">
        {#if staleWarn && mode === 'close'}
          <div class="p-3 bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300 text-xs font-bold rounded-xl flex items-start gap-2">
            <Clock class="w-4 h-4 shrink-0 mt-0.5" />
            <span>This session was left open from a previous day. Close it now, then open today's fresh session / هذه الجلسة قديمة — أغلقها ثم افتح جلسة جديدة</span>
          </div>
        {/if}
        {#if errorMsg}
          <div class="p-3 bg-rose-100 text-rose-700 text-xs font-bold rounded-xl">{errorMsg}</div>
        {/if}

        {#if $activeSession}
          <!-- Current Balance Badge -->
          <div class="p-3.5 bg-sky-50 dark:bg-sky-950/50 border border-sky-200 dark:border-sky-800 rounded-xl flex items-center justify-between">
            <div class="flex items-center gap-2">
              <Wallet class="w-4 h-4 text-sky-600" />
              <span class="text-xs font-bold text-sky-900 dark:text-sky-200">Current Session Balance:</span>
            </div>
            <span class="font-mono font-black text-base text-sky-600">
              {$activeSession.expected_cash.toLocaleString()} DZD
            </span>
          </div>

          <!-- Mode Selector Tabs -->
          <div class="grid grid-cols-3 gap-2">
            <button
              type="button"
              on:click={() => mode = 'in'}
              class="p-2.5 rounded-xl border font-bold text-xs flex flex-col items-center gap-1 transition cursor-pointer {mode === 'in' ? 'border-emerald-500 bg-emerald-50 dark:bg-emerald-950 text-emerald-600' : 'border-pos-border text-pos-muted'}"
            >
              <ArrowDownCircle class="w-4 h-4" />
              <span>Deposit (إيداع)</span>
            </button>

            <button
              type="button"
              on:click={() => mode = 'out'}
              class="p-2.5 rounded-xl border font-bold text-xs flex flex-col items-center gap-1 transition cursor-pointer {mode === 'out' ? 'border-amber-500 bg-amber-50 dark:bg-amber-950 text-amber-600' : 'border-pos-border text-pos-muted'}"
            >
              <ArrowUpCircle class="w-4 h-4" />
              <span>Withdrawal (سحب)</span>
            </button>

            <button
              type="button"
              on:click={() => mode = 'close'}
              class="p-2.5 rounded-xl border font-bold text-xs flex flex-col items-center gap-1 transition cursor-pointer {mode === 'close' ? 'border-rose-500 bg-rose-50 dark:bg-rose-950 text-rose-600' : 'border-pos-border text-pos-muted'}"
            >
              <Lock class="w-4 h-4" />
              <span>Close Session</span>
            </button>
          </div>
        {/if}

        <!-- Inputs Form -->
        <div class="space-y-3">
          {#if mode === 'startup'}
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Enter Startup Cash / Fond de Caisse (DZD)</label>
              <input
                type="number"
                bind:value={amount}
                use:autofocusSelect
                on:keydown={(e) => modalKey(e, handleAction)}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                placeholder="10000"
                class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xl font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
              />
            </div>
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Notes (Optional)</label>
              <input
                type="text"
                bind:value={reason}
                placeholder="e.g. Morning Shift Startup"
                class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text outline-none"
              />
            </div>
          {:else if mode === 'in' || mode === 'out'}
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Amount (DZD)</label>
              <input
                type="number"
                bind:value={amount}
                use:autofocusSelect
                on:keydown={(e) => modalKey(e, handleAction)}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xl font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
              />
            </div>
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Reason / Description</label>
              <input
                type="text"
                bind:value={reason}
                placeholder="Reason..."
                class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text outline-none"
              />
            </div>
          {:else if mode === 'close'}
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Counted Cash in Drawer (DZD)</label>
              <input
                type="number"
                bind:value={countedCash}
                use:autofocusSelect
                on:keydown={(e) => modalKey(e, handleAction)}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xl font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-rose-500"
              />
            </div>
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Closing Notes</label>
              <input
                type="text"
                bind:value={reason}
                placeholder="Closing notes..."
                class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text outline-none"
              />
            </div>
          {/if}
        </div>
      </div>

      <!-- Footer Buttons -->
      <div class="px-5 py-3.5 border-t border-pos-border bg-slate-50 dark:bg-slate-800/50 flex justify-end gap-2">
        <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl">
          Cancel
        </button>
        <button
          type="button"
          on:click={handleAction}
          disabled={isSubmitting}
          class="px-5 py-2 {mode === 'close' ? 'bg-rose-600 hover:bg-rose-700' : 'bg-sky-600 hover:bg-sky-700'} text-white font-extrabold text-xs rounded-xl transition flex items-center gap-1.5 cursor-pointer shadow-xs"
        >
          <Check class="w-4 h-4" />
          <span>Confirm</span>
        </button>
      </div>
    </div>
  </div>
{/if}