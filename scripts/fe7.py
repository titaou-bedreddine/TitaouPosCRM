import io

def edit(path, subs):
    with io.open(path, 'r', encoding='utf-8') as f:
        s = f.read()
    for old, new, label in subs:
        if old not in s:
            print('MISS', label)
            continue
        s = s.replace(old, new, 1)
        print('ok:', label)
    with io.open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(s)

# ===== 1) SuppliersView: QR icon per row → print the supplier card =====
edit('src/routes/suppliers/SuppliersView.svelte', [
    ('''import {
    Phone, Mail, MapPin, Building, FileSpreadsheet, Pin, PinOff''',
     '''import {
    Phone, Mail, MapPin, Building, FileSpreadsheet, Pin, PinOff, QrCode''', 'QrCode import'),
])
with io.open('src/routes/suppliers/SuppliersView.svelte', 'r', encoding='utf-8') as f:
    s = f.read()
i = s.find("on:click={() => toggleSupplierPin(s)}")
insert_before = s.rfind('<button', 0, i)
# Insert a QR button right before the pin button in the actions row
qr_btn = '''                    <button
                      type="button"
                      on:click={(ev) => { ev.stopPropagation(); printSupplierCard(s); }}
                      class="p-1.5 rounded-lg cursor-pointer transition text-pos-muted hover:text-sky-600"
                      title={t('supplier_print_qr')}
                    >
                      <QrCode class="w-3.5 h-3.5" />
                    </button>
'''
s = s[:insert_before] + qr_btn + s[insert_before:]
# ensure t import exists
if "from '../../lib/i18n'" not in s:
    s = s.replace("<script lang=\"ts\">", "<script lang=\"ts\">\n  import { t } from '../../lib/i18n';", 1)
else:
    import re
    m = re.search(r"import \{([^}]*)\} from '\.\./\.\./lib/i18n';", s)
    if m and ' t' not in m.group(1).replace(' t,', '') and not m.group(1).strip().startswith('t,'):
        s = s.replace(m.group(0), m.group(0).replace('import {', 'import { t,', 1), 1)
with io.open('src/routes/suppliers/SuppliersView.svelte', 'w', encoding='utf-8', newline='') as f:
    f.write(s)
print('suppliers QR button done')

# ===== 2) PayrollView: QR icon per employee card → print employee QR card =====
edit('src/routes/payroll/PayrollView.svelte', [
    ('''  import { printHtmlSilently } from '../../lib/utils/printer';''',
     '''  import { printHtmlSilently, entityQrDataUrl } from '../../lib/utils/printer';''', 'printer import'),
    ('''  function printPayrollSlip(emp: Employee) {''',
     '''  // Employee QR card: prints a wallet-size card with the employee QR —
  // scanning it on the POS sets this employee as the cart customer.
  async function printEmployeeQrCard(emp: Employee) {
    const qr = await entityQrDataUrl(emp.qr_code || `EMP-${emp.employee_code}`, 300).catch(() => '');
    const html = `<div style="width:60mm;font-family:monospace;font-size:10px;text-align:center;padding:3mm;">
      <p style="font-size:13px;font-weight:900;margin:0;">${emp.full_name}</p>
      <p style="font-size:9px;margin:2px 0;">${emp.employee_code} • ${emp.job_title || ''}</p>
      <img src="${qr}" alt="QR" style="width:34mm;height:34mm;margin:4mm auto;" />
      <p style="font-size:8px;">TitaouPOS • ${new Date().toLocaleDateString('fr-FR')}</p>
    </div>`;
    const r = await printHtmlSilently(html, 'Employee Card ' + emp.employee_code, { widthMm: 60 });
    if (!r.ok) showError('Print failed: ' + r.message);
  }

  function printPayrollSlip(emp: Employee) {''', 'qr card fn'),
])
# Add a QR button in the employee card hover actions (before History button)
with io.open('src/routes/payroll/PayrollView.svelte', 'r', encoding='utf-8') as f:
    s = f.read()
old = '''            <button
              type="button"
              on:click={() => openAdvanceHistory(emp)}'''
new = '''            <button
              type="button"
              on:click={() => printEmployeeQrCard(emp)}
              class="p-1.5 bg-white/95 dark:bg-slate-800/95 text-pos-muted hover:text-sky-600 rounded-lg shadow-xs cursor-pointer"
              title={t('emp_print_qr')}
            >
              <QrCode class="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              on:click={() => openAdvanceHistory(emp)}'''
assert old in s
s = s.replace(old, new, 1)
# QrCode icon import check
import re
m = re.search(r"import \{([^}]*)\} from 'lucide-svelte';\nimport \{ Pencil, Trash2 \} from 'lucide-svelte';", s)
if m and not re.search(r'\bQrCode\b', m.group(1)):
    s = s.replace("import { Pencil, Trash2 } from 'lucide-svelte';", "import { Pencil, Trash2, QrCode } from 'lucide-svelte';", 1)
with io.open(path, 'w', encoding='utf-8', newline='') as f:
    f.write(s)
print('payroll QR button done')
