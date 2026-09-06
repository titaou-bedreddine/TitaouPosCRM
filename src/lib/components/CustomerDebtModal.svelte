<script lang="ts">
  import type { Customer } from '../types';
  import { invoke } from '@tauri-apps/api/core';
  import { currentUser } from '../stores/auth';
  import { activeSession } from '../stores/session';
  import { QrCode, Printer, FileText, X, Check, DollarSign, ShieldAlert, Eraser } from 'lucide-svelte';
  import { printHtmlSilently } from '../utils/printer';

  export let isOpen = false;
  export let customer: Customer | null = null;
  export let onClose: () => void;
  export let onPaymentRecorded: () => void = () => {};

  let paymentAmount = 0;
  let paymentDate = new Date().toISOString().split('T')[0];
  let paymentMethod = 'cash';
  let reference = '';
  let notes = '';
  let autoPrint = true;
  let isSubmitting = false;
  let errorMsg = '';

  // Focus the amount field as soon as the modal opens (Enter confirms).
  function autofocusSelect(node: HTMLInputElement) {
    setTimeout(() => {
      node.focus();
      node.select();
    }, 60);
  }

  // Clear-debt (forgiveness) sub-modal state.
  let showClearPanel = false;
  let clearReason = '';
  let clearAdminPassword = '';
  let clearError = '';
  let isClearing = false;

  $: if (customer) {
    paymentAmount = customer.balance;
  }

  async function handleRecordPayment() {
    if (!customer || !$currentUser) return;
    if (paymentAmount <= 0) {
      errorMsg = 'Please enter a valid payment amount.';
      return;
    }
    try {
      isSubmitting = true;
      errorMsg = '';
      await invoke('record_customer_debt_payment', {
        input: {
          customer_id: customer.id,
          amount: paymentAmount,
          payment_method: paymentMethod,
          reference: reference || null,
          session_id: paymentMethod === 'cash' ? $activeSession?.id : null,
          user_id: $currentUser.id,
          notes: notes || null,
        }
      });
      if (autoPrint) {
        const voucherHtml = `
          <div style="text-align:center; font-family:monospace; font-size:10px; width:72mm; margin:0 auto; padding:2mm;">
            <p style="font-size:14px; font-weight:900;">PAYMENT RECEIPT / وصل دفع</p>
            <hr style="border-top:1px dashed #000; margin:4px 0;" />
            <p>Client: <strong>${customer.name}</strong></p>
            <p>Amount: <strong>${paymentAmount.toLocaleString()} DZD</strong></p>
            <p>Method: <strong>${paymentMethod.toUpperCase()}</strong></p>
            ${reference ? `<p>Ref: ${reference}</p>` : ''}
            ${notes ? `<p>Notes: ${notes}</p>` : ''}
            <p style="font-size:8px; margin-top:6px;">TitaouPOS • Titaou Bedreddine</p>
          </div>
        `;
        const r = await printHtmlSilently(voucherHtml, 'Debt Payment Receipt', { widthMm: 72 });
        if (!r.ok) console.error('Voucher print failed:', r.message);
      }
      onPaymentRecorded();
      onClose();
    } catch (err: any) {
      errorMsg = typeof err === 'string' ? err : err.message || 'Payment recording failed';
    } finally {
      isSubmitting = false;
    }
  }

  // CLEAR debt = admin-gated forgiveness: balance becomes 0, NO cash-in /
  // cash-out / drawer movement is created (no money entered the register),
  // and the cleared amount is archived (who, when, previous debt, reason).
  async function handleClearDebt() {
    if (!customer || !$currentUser) return;
    if (!clearAdminPassword.trim()) {
      clearError = 'Admin password required / كلمة مرور المسؤول مطلوبة';
      return;
    }
    try {
      isClearing = true;
      clearError = '';
      await invoke('clear_customer_debt', {
        customerId: customer.id,
        reason: clearReason || null,
        adminPassword: clearAdminPassword,
        userId: $currentUser.id,
      });
      onPaymentRecorded();
      onClose();
    } catch (err: any) {
      clearError = typeof err === 'string' ? err : err.message || 'Clearing debt failed';
    } finally {
      isClearing = false;
    }
  }
</script>

{#if isOpen && customer}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-4xl max-h-[90vh] overflow-y-auto animate-in fade-in duration-150 p-6 space-y-5">
      <!-- Top Header Card -->
      <div class="flex items-start justify-between border-b border-pos-border/60 pb-4">
        <div class="flex items-center gap-4">
          <!-- QR Code Badge -->
          <div class="w-16 h-16 bg-slate-100 dark:bg-slate-800 rounded-xl border border-pos-border flex flex-col items-center justify-center p-1 shrink-0">
            <QrCode class="w-8 h-8 text-sky-600" />
            <span class="text-[9px] font-mono text-pos-muted font-bold mt-0.5">{customer.qr_code || 'CUST'}</span>
          </div>
          <div>
            <h2 class="text-xl font-black text-pos-text">Customer Profile: {customer.name}</h2>
            <p class="text-xs text-pos-muted mt-0.5 font-semibold">Phone: {customer.phone || 'N/A'} • ID: {customer.id}</p>
            {#if customer.rc}
              <p class="text-[11px] text-pos-muted font-mono">RC: {customer.rc} | NIF: {customer.nif || '-'}</p>
            {/if}
          </div>
        </div>

        <div class="flex items-center gap-2">
          <button class="px-3 py-1.5 bg-sky-50 dark:bg-sky-950/60 border border-sky-200 dark:border-sky-800 text-sky-700 dark:text-sky-300 rounded-lg text-xs font-bold flex items-center gap-1.5 cursor-pointer">
            <FileText class="w-3.5 h-3.5" />
            <span>Recap Invoice</span>
          </button>
          <button class="px-3 py-1.5 bg-sky-50 dark:bg-sky-950/60 border border-sky-200 dark:border-sky-800 text-sky-700 dark:text-sky-300 rounded-lg text-xs font-bold flex items-center gap-1.5 cursor-pointer">
            <Printer class="w-3.5 h-3.5" />
            <span>Print Summary</span>
          </button>
          <button on:click={onClose} class="p-1.5 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer">
            <X class="w-5 h-5" />
          </button>
        </div>
      </div>

      {#if errorMsg}
        <div class="p-3 bg-rose-100 text-rose-700 text-xs font-bold rounded-lg">{errorMsg}</div>
      {/if}

      <!-- Metric Cards (3 Columns) -->
      <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div class="bg-pos-card border border-pos-border rounded-xl p-4 text-center shadow-xs">
          <span class="text-xs font-bold text-pos-muted">Total Purchases</span>
          <div class="text-2xl font-black font-mono text-sky-600 mt-1">
            {(customer.total_purchases || 0).toLocaleString()} DZD
          </div>
        </div>

        <div class="bg-pos-card border border-pos-border rounded-xl p-4 text-center shadow-xs">
          <span class="text-xs font-bold text-pos-muted">Initial Debt (Remaining)</span>
          <div class="text-2xl font-black font-mono text-amber-500 mt-1">
            {(customer.initial_debt || 0).toLocaleString()} DZD
          </div>
        </div>

        <div class="bg-pos-card border-2 border-rose-400 dark:border-rose-800 rounded-xl p-4 text-center shadow-xs bg-rose-50/20">
          <span class="text-xs font-black text-rose-600 uppercase tracking-wider">Total Debt (Crédit)</span>
          <div class="text-3xl font-black font-mono text-rose-600 mt-1">
            {customer.balance.toLocaleString()} DZD
          </div>
        </div>
      </div>

      <!-- Record Debt Payment Form -->
      <div class="bg-slate-50 dark:bg-slate-800/40 border border-pos-border rounded-xl p-5 space-y-4">
        <h4 class="font-extrabold text-sm text-pos-text flex items-center gap-2">
          <DollarSign class="w-4 h-4 text-emerald-500" />
          <span>Record Debt Repayment (تسديد دَيْن)</span>
        </h4>

        <div class="grid grid-cols-1 md:grid-cols-4 gap-3">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Enter amount paid by customer (DZD)</label>
            <input
              type="number"
              bind:value={paymentAmount}
              on:keydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); handleRecordPayment(); } else if (e.key === 'Escape') { e.preventDefault(); onClose(); } }}
              use:autofocusSelect
              on:focus={(e) => (e.target as HTMLInputElement).select()}
              class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded-lg text-lg font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
            />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Payment Date</label>
            <input
              type="date"
              bind:value={paymentDate}
              class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded-lg text-xs font-bold text-pos-text outline-none"
            />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Payment Method</label>
            <select
              bind:value={paymentMethod}
              class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded-lg text-xs font-bold text-pos-text outline-none"
            >
              <option value="cash">Cash (Direct into Register)</option>
              <option value="bank_transfer">Bank Transfer (Virement)</option>
              <option value="cheque">Cheque</option>
            </select>
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Reference (optional)</label>
            <input
              type="text"
              bind:value={reference}
              placeholder="Cheque / transfer number..."
              class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded-lg text-xs text-pos-text outline-none"
            />
          </div>
        </div>

        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Notes</label>
          <input
            type="text"
            bind:value={notes}
            placeholder="Additional notes..."
            class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded-lg text-xs text-pos-text outline-none"
          />
        </div>

        <div class="flex items-center justify-between pt-2">
          <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer">
            <input type="checkbox" bind:checked={autoPrint} class="rounded text-sky-600" />
            <span>Auto-print repayment receipt</span>
          </label>

          <div class="flex items-center gap-2">
            <button
              type="button"
              on:click={() => { showClearPanel = !showClearPanel; clearError = ''; clearAdminPassword = ''; }}
              class="px-3 py-2 bg-rose-50 dark:bg-rose-950/50 hover:bg-rose-100 dark:hover:bg-rose-950 border border-rose-300 dark:border-rose-800 text-rose-700 dark:text-rose-300 font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer transition"
              title="Clear (forgive) this customer's debt — admin password required, archived, no cash movement"
            >
              <Eraser class="w-3.5 h-3.5" />
              <span>Clear Debt (مصالحة)</span>
            </button>
            <button
              type="button"
              on:click={handleRecordPayment}
              disabled={isSubmitting}
              class="px-6 py-2.5 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white font-extrabold text-sm rounded-xl transition shadow-md flex items-center gap-2 cursor-pointer"
            >
              <Check class="w-4 h-4" />
              <span>Confirm Payment</span>
            </button>
          </div>
        </div>

        <!-- Clear Debt (forgiveness) panel — admin password required -->
        {#if showClearPanel}
          <div class="p-4 bg-rose-50/70 dark:bg-rose-950/30 border-2 border-dashed border-rose-300 dark:border-rose-800 rounded-xl space-y-3">
            <div class="flex items-center gap-2 text-rose-700 dark:text-rose-300">
              <ShieldAlert class="w-4 h-4" />
              <p class="font-black text-xs">Clear Customer Debt — Forgiveness (مصالحة الدين)</p>
            </div>
            <p class="text-[11px] text-pos-muted font-bold">
              The debt becomes <span class="text-rose-600 font-black">0</span> with <span class="font-black">no cash movement</span> — this is
              forgiveness, not a payment: nothing is added to the register. The cleared amount ({customer.balance.toLocaleString()} DZD) is archived with
              your name, reason and timestamp.
            </p>
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label class="block text-[11px] font-bold text-pos-muted mb-1">Reason (optional / السبب)</label>
                <input
                  type="text"
                  bind:value={clearReason}
                  placeholder="Client left the country / زبون غادر..."
                  class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded-lg text-xs text-pos-text outline-none"
                />
              </div>
              <div>
                <label class="block text-[11px] font-bold text-pos-muted mb-1">Admin Password (كلمة المرور) *</label>
                <input
                  type="password"
                  bind:value={clearAdminPassword}
                  placeholder="••••••••"
                  class="w-full px-3 py-2 bg-pos-card border border-pos-border rounded-lg text-xs font-mono text-pos-text outline-none"
                />
              </div>
            </div>
            {#if clearError}
              <p class="text-[11px] font-bold text-rose-600">{clearError}</p>
            {/if}
            <div class="flex justify-end">
              <button
                type="button"
                on:click={handleClearDebt}
                disabled={isClearing}
                class="px-5 py-2 bg-rose-600 hover:bg-rose-700 disabled:opacity-50 text-white font-black text-xs rounded-xl cursor-pointer shadow-md flex items-center gap-1.5"
              >
                {#if isClearing}Clearing…{:else}<Eraser class="w-3.5 h-3.5" /><span>Clear Debt to 0</span>{/if}
              </button>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}