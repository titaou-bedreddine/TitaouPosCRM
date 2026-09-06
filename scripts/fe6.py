import io

path = 'src/lib/components/CashDrawerModal.svelte'
with io.open(path, 'r', encoding='utf-8') as f:
    s = f.read()

# Add helper + bind to reactive isOpen for focus
subs = [
    ('''  export let isOpen = false;''',
     '''  export let isOpen = false;

  // Popup UX: autofocus the first field; Enter confirms; Esc closes.
  function autofocusSelect(node: HTMLInputElement) {
    setTimeout(() => {
      node.focus();
      node.select();
    }, 60);
  }
  function modalKey(e: KeyboardEvent, confirm: () => void) {
    if (e.key === 'Enter') {
      e.preventDefault();
      confirm();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  }''', 'helpers'),
]
for old, new, label in subs:
    if old not in s:
        print('MISS', label)
        continue
    s = s.replace(old, new, 1)
    print('ok:', label)

# Wire the three first-field inputs (startup amount, in/out amount, close counted)
s = s.replace(
    '''              <input
                type="number"
                bind:value={amount}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                placeholder="10000"''',
    '''              <input
                type="number"
                bind:value={amount}
                use:autofocusSelect
                on:keydown={(e) => modalKey(e, handleAction)}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                placeholder="10000"''', 'startup autofocus')
s = s.replace(
    '''              <input
                type="number"
                bind:value={amount}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xl font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
              />''',
    '''              <input
                type="number"
                bind:value={amount}
                use:autofocusSelect
                on:keydown={(e) => modalKey(e, handleAction)}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xl font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
              />''', 'in/out autofocus')
s = s.replace(
    '''              <input
                type="number"
                bind:value={countedCash}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xl font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-rose-500"
              />''',
    '''              <input
                type="number"
                bind:value={countedCash}
                use:autofocusSelect
                on:keydown={(e) => modalKey(e, handleAction)}
                on:focus={(e) => (e.target as HTMLInputElement).select()}
                class="w-full px-3 py-2.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xl font-bold font-mono text-pos-text outline-none focus:ring-2 focus:ring-rose-500"
              />''', 'close autofocus')

with io.open(path, 'w', encoding='utf-8', newline='') as f:
    f.write(s)
print('CashDrawerModal UX done')
