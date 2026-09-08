<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { normalizeBarcode } from '../utils/barcode';
  import type { Customer } from '../types';
  import { X, Check, Search, Banknote, Layers, UserPlus } from 'lucide-svelte';

  export let isOpen = false;
  export let totalAmount = 0;
  export let onClose: () => void;
  export let onConfirmCredit: (customerId: number, customerName: string, paidAmount: number, remaining: number) => void;

  let customers: Customer[] = [];
  let searchQuery = '';
  // AZERTY-normalized mirror of the search box (scanners may emit & é " ...).
  $: searchQueryN = normalizeBarcode(searchQuery).toLowerCase();
  let selectedCustomerId: number | null = null;
  let selectedCustomerName = '';
  let paidAmount: number | null = null;
  let searchInput: HTMLInputElement;
  let amountInput: HTMLInputElement;

  // Quick-add customer form
  let showQuickAdd = false;
  let newCustomerName = '';
  let newCustomerPhone = '';
  let isSavingCustomer = false;
  let quickAddError = '';

  $: filteredCustomers = customers.filter(c =>
    c.name.toLowerCase().includes(searchQueryN) ||
    (c.phone && c.phone.includes(searchQueryN))
  );
  $: numericPaid = paidAmount === null || isNaN(paidAmount as number) ? 0 : (paidAmount as number);
  $: remaining = Math.max(0, totalAmount - numericPaid);
  $: paidInvalid = numericPaid < 0 || numericPaid > totalAmount;

  function focusAndSelect(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  async function loadCustomers() {
    try {
      customers = await invoke<Customer[]>('list_customers');
      if (customers.length > 0 && !selectedCustomerId) {
        const walkin = customers.find(c => c.id === 1);
        const pick = walkin || customers[0];
        selectedCustomerId = pick.id;
        selectedCustomerName = pick.name;
      }
    } catch (e) {
      console.error(e);
    }
  }

  onMount(async () => {
    await loadCustomers();
  });

  $: if (isOpen) {
    // Default the paid amount to the full total; lower it for partial payment.
    paidAmount = totalAmount;
    showQuickAdd = false;
    quickAddError = '';
    loadCustomers();
  }

  function selectCustomer(c: Customer) {
    selectedCustomerId = c.id;
    selectedCustomerName = c.name;
  }

  async function handleQuickAdd() {
    if (!newCustomerName.trim()) {
      quickAddError = 'Customer name is required / اسم الزبون مطلوب';
      return;
    }
    try {
      isSavingCustomer = true;
      quickAddError = '';
      const phone = newCustomerPhone.trim() ? newCustomerPhone.trim() : null;
      const newId = await invoke<number>('save_customer', {
        name: newCustomerName.trim(),
        phone,
        email: null,
        address: null,
        rc: null,
        nif: null,
        nis: null,
        ai: null,
        initialDebt: 0,
        notes: null,
        customerId: null,
      });
      await loadCustomers();
      selectedCustomerId = newId;
      selectedCustomerName = newCustomerName.trim();
      newCustomerName = '';
      newCustomerPhone = '';
      showQuickAdd = false;
    } catch (e: any) {
      quickAddError = typeof e === 'string' ? e : e?.message || 'Failed to create customer';
    } finally {
      isSavingCustomer = false;
    }
  }

  function handleConfirm() {
    if (!selectedCustomerId || paidInvalid) return;
    onConfirmCredit(selectedCustomerId, selectedCustomerName, Math.round(numericPaid), remaining);
    onClose();
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-lg overflow-hidden animate-in zoom-in-95 duration-150 flex flex-col max-h-[85vh]">
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-amber-600/10 text-amber-600 flex items-center justify-center font-bold">
            <Layers class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">Credit Sale / بيع بالدين (Dette Client)</h3>
            <p class="text-xs text-amber-600 font-bold">Total Due: {totalAmount.toLocaleString()} DZD</p>
          </div>
        </div>
        <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1.5 rounded-xl cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>

      <!-- Search + Quick Add -->
      <div class="p-4 border-b border-pos-border space-y-2">
        <div class="relative">
          <Search class="w-4 h-4 text-pos-muted absolute start-3 top-2.5" />
          <input
            bind:this={searchInput}
            use:focusAndSelect
            type="text"
            bind:value={searchQuery}
            placeholder="Search customer name or phone..."
            class="w-full ps-9 pe-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none"
          />
        </div>
        {#if showQuickAdd}
          <div class="p-3 bg-slate-100 dark:bg-slate-800/60 rounded-xl border border-pos-border space-y-2 animate-in fade-in duration-100">
            <div class="flex gap-2">
              <input
                type="text"
                bind:value={newCustomerName}
                placeholder="New customer name / اسم الزبون"
                class="flex-1 px-3 py-2 bg-pos-card border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none"
              />
              <input
                type="text"
                bind:value={newCustomerPhone}
                placeholder="Phone (optional)"
                class="w-32 px-3 py-2 bg-pos-card border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none"
              />
            </div>
            {#if quickAddError}
              <p class="text-[10px] font-bold text-rose-600">{quickAddError}</p>
            {/if}
            <div class="flex justify-end gap-2">
              <button on:click={() => (showQuickAdd = false)} class="px-3 py-1.5 text-xs font-bold text-pos-muted hover:text-pos-text cursor-pointer">Cancel</button>
              <button
                type="button"
                on:click={handleQuickAdd}
                disabled={isSavingCustomer}
                class="px-4 py-1.5 bg-amber-600 hover:bg-amber-700 disabled:opacity-40 text-white text-xs font-black rounded-xl cursor-pointer"
              >
                Save Customer
              </button>
            </div>
          </div>
        {:else}
          <button
            type="button"
            on:click={() => (showQuickAdd = true)}
            class="w-full py-2 bg-amber-50 dark:bg-amber-950/40 border border-dashed border-amber-400 text-amber-600 text-xs font-black rounded-xl flex items-center justify-center gap-1.5 cursor-pointer hover:bg-amber-100 dark:hover:bg-amber-900/40 transition"
          >
            <UserPlus class="w-3.5 h-3.5" />
            <span>Quick Add Customer / إضافة زبون سريع</span>
          </button>
        {/if}
      </div>

      <!-- Customer List -->
      <div class="p-4 overflow-y-auto flex-1 space-y-2 max-h-52">
        {#each filteredCustomers as c}
          <button
            type="button"
            on:click={() => selectCustomer(c)}
            class="w-full p-3 rounded-2xl border text-start transition flex items-center justify-between cursor-pointer {selectedCustomerId === c.id ? 'bg-amber-500/10 border-amber-500 ring-2 ring-amber-400' : 'bg-pos-card border-pos-border hover:bg-slate-50 dark:hover:bg-slate-800/50'}"
          >
            <div>
              <p class="text-xs font-black text-pos-text">{c.name}</p>
              <p class="text-[10px] text-pos-muted">{c.phone || 'No phone'} • {c.address || 'Alger'}</p>
            </div>
            <div class="text-end">
              <span class="text-[10px] text-pos-muted font-bold block">Current Debt:</span>
              <span class="text-xs font-mono font-black text-rose-600">{c.balance.toLocaleString()} DZD</span>
            </div>
          </button>
        {/each}
      </div>

      <!-- Amount Paid Now -->
      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 space-y-3">
        <div>
          <label class="block text-[10px] font-black text-pos-muted uppercase tracking-wider mb-1 flex items-center gap-1">
            <Banknote class="w-3.5 h-3.5" />
            Amount Paid Now (DZD) — can be 0
          </label>
          <input
            bind:this={amountInput}
            type="number"
            bind:value={paidAmount}
            min="0"
            max={totalAmount}
            step="any"
            on:keydown={(e) => e.key === 'Enter' && handleConfirm()}
            class="w-full px-3 py-2 bg-pos-card border rounded-xl text-xl font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-amber-500 {paidInvalid ? 'border-rose-500' : 'border-pos-border'}"
          />
          {#if paidInvalid}
            <p class="text-[10px] font-bold text-rose-600 mt-1">Amount cannot exceed the total ({totalAmount.toLocaleString()} DZD)</p>
          {/if}
        </div>
        <div class="grid grid-cols-2 gap-2">
          <div class="p-2.5 bg-emerald-100/60 dark:bg-emerald-950/50 rounded-xl border border-emerald-300/60 dark:border-emerald-800/60 text-center">
            <span class="text-[9px] font-black text-pos-muted uppercase block">Paid Now</span>
            <span class="text-lg font-black font-mono text-emerald-600">{numericPaid.toLocaleString()} DZD</span>
          </div>
          <div class="p-2.5 bg-rose-100/60 dark:bg-rose-950/50 rounded-xl border border-rose-300/60 dark:border-rose-800/60 text-center">
            <span class="text-[9px] font-black text-pos-muted uppercase block">Remaining Debt (Reste)</span>
            <span class="text-lg font-black font-mono text-rose-600">{remaining.toLocaleString()} DZD</span>
          </div>
        </div>
        <div class="flex items-center justify-between">
          <span class="text-[10px] text-pos-muted font-bold text-start">Prints 2 copies with CREDIT watermark</span>
          <div class="flex items-center gap-2">
            <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl cursor-pointer">Cancel</button>
            <button
              type="button"
              on:click={handleConfirm}
              disabled={!selectedCustomerId || paidInvalid}
              class="px-6 py-2 bg-amber-600 hover:bg-amber-700 disabled:opacity-40 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5"
            >
              <Check class="w-4 h-4" />
              <span>Confirm Credit Sale (تأكيد البيع بالدين)</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
{/if}
