<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { normalizeBarcode } from '../../lib/utils/barcode';
  import type { Employee } from '../../lib/types';
  import { t } from '../../lib/i18n';
  import {
    Plus, Users, Award, DollarSign, Calendar, AlertTriangle,
    Check, X, Printer, UserCheck, CreditCard, Clock, QrCode,
    UsersRound, Mail, ShieldCheck, KeyRound
  } from 'lucide-svelte';
  import { printHtmlSilently, entityQrDataUrl } from '../../lib/utils/printer';
  import { printService } from '../../lib/services/printService';
  import { Pencil, Trash2 } from 'lucide-svelte';
  import { activeSession } from '../../lib/stores/session';
  import { currentUser } from '../../lib/stores/auth';

  let employees: Employee[] = [];
  // Clicked from an expense advance row: pre-filter to that employee.
  export let focusName = '';
  let lastFocusName = '';
  $: if (focusName && focusName !== lastFocusName && employees.length) {
    lastFocusName = focusName;
    employeeSearch = focusName;
  }
  let isAddOpen = false;
  // Editing reuses the add modal; delete asks for confirmation.
  let editingEmployeeId: number | null = null;
  let employeeToDelete: Employee | null = null;

  let code = '';
  let name = '';
  let jobTitle = '';
  let baseSalary = 0;
  let phone = '';
  let email = '';
  let rfidCode = '';
  let startDate = localDateStr();
  let employeeSearch = '';
  // AZERTY-normalized mirror of the search box (scanners may emit & é " ...).
  $: employeeSearchN = normalizeBarcode(employeeSearch).toLowerCase();
  // Search across name, employee code, job title and RFID tag. Names are
  // compared with the SAME AZERTY normalization as the query (a 'é' in the
  // name never hid a match). '#N' targets employee id N directly — legacy
  // advance rows booked their recipient as "#<id>".
  $: filteredEmployees = employeeSearch.trim()
    ? employees.filter(e => {
        const q = employeeSearchN;
        const idMatch = /^#(\d+)$/.exec(employeeSearch.trim());
        if (idMatch) return e.id === Number(idMatch[1]);
        return (
          normalizeBarcode((e.full_name || '')).toLowerCase().includes(q) ||
          (e.employee_code || '').toLowerCase().includes(q) ||
          normalizeBarcode((e as any).job_title || '').toLowerCase().includes(q) ||
          (((e as any).rfid_code || '')).toLowerCase().includes(q)
        );
      })
    : employees;

  // Advance modal state
  let selectedEmpForAdvance: Employee | null = null;
  let advanceAmount = 0;
  let advanceReason = 'Avance sur salaire';
  // LOCAL date (toISOString is UTC and recorded "yesterday" between
  // 00:00-01:00 local, which made current-month history miss records).
  function localDateStr(): string {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
  }
  let advanceDate = localDateStr();

  // Absence modal state
  let selectedEmpForAbsence: Employee | null = null;
  let absenceDays = 1;
  let absenceReason = 'Absence non justifiée';
  let absenceDate = localDateStr();

  // Raw persisted logs from backend
  let allAdvances: any[] = [];
  let allAbsences: any[] = [];

  // Active advance/absence maps by employee ID for current cycle
  let advancesMap: Record<number, number> = {};
  let absencesMap: Record<number, number> = {};

  // Full advance log per employee for the history section.
  let advanceLog: any[] = [];
  let historyEmployee: Employee | null = null;
  let absenceLog: any[] = [];

  // Employee monthly pay-period cycle calculation respecting individual Start Date
  function getEmployeePayPeriod(emp: Employee, refDate: Date = new Date()): { startStr: string; endStr: string; label: string } {
    const rawStart = emp.salary_start_date || (emp as any).hire_date;
    const currentYear = refDate.getFullYear();
    const currentMonth = refDate.getMonth(); // 0-11
    const currentDay = refDate.getDate();

    if (!rawStart || !/^\d{4}-\d{2}-\d{2}$/.test(rawStart.trim())) {
      // Safe backward-compatible default: 1st of current calendar month to last day of month
      const start = new Date(currentYear, currentMonth, 1);
      const end = new Date(currentYear, currentMonth + 1, 0);
      const startStr = `${start.getFullYear()}-${String(start.getMonth() + 1).padStart(2, '0')}-01`;
      const endStr = `${end.getFullYear()}-${String(end.getMonth() + 1).padStart(2, '0')}-${String(end.getDate()).padStart(2, '0')}`;
      return { startStr, endStr, label: `${startStr} ➔ ${endStr}` };
    }

    const [sY, sM, sD] = rawStart.trim().split('-').map(Number);
    const startDay = Math.max(1, Math.min(31, sD || 1));

    if (startDay === 1) {
      const start = new Date(currentYear, currentMonth, 1);
      const end = new Date(currentYear, currentMonth + 1, 0);
      const startStr = `${start.getFullYear()}-${String(start.getMonth() + 1).padStart(2, '0')}-01`;
      const endStr = `${end.getFullYear()}-${String(end.getMonth() + 1).padStart(2, '0')}-${String(end.getDate()).padStart(2, '0')}`;
      return { startStr, endStr, label: `${startStr} ➔ ${endStr}` };
    }

    const startFull = new Date(sY, sM - 1, startDay);
    if (startFull > refDate) {
      const endDay = startDay - 1;
      let endMonth = sM; // 1-based next month
      let endYear = sY;
      if (endMonth > 12) {
        endMonth = 1;
        endYear++;
      }
      const maxEndDays = new Date(endYear, endMonth, 0).getDate();
      const clampedEndDay = Math.min(endDay, maxEndDays);
      const startStr = rawStart;
      const endStr = `${endYear}-${String(endMonth).padStart(2, '0')}-${String(clampedEndDay).padStart(2, '0')}`;
      return { startStr, endStr, label: `${startStr} ➔ ${endStr}` };
    }

    let cycleStartYear: number;
    let cycleStartMonth: number;
    let cycleEndYear: number;
    let cycleEndMonth: number;

    if (currentDay >= startDay) {
      cycleStartYear = currentYear;
      cycleStartMonth = currentMonth;
      cycleEndMonth = currentMonth + 1;
      cycleEndYear = cycleStartYear;
      if (cycleEndMonth > 11) {
        cycleEndMonth = 0;
        cycleEndYear++;
      }
    } else {
      cycleStartMonth = currentMonth - 1;
      cycleStartYear = currentYear;
      if (cycleStartMonth < 0) {
        cycleStartMonth = 11;
        cycleStartYear--;
      }
      cycleEndYear = currentYear;
      cycleEndMonth = currentMonth;
    }

    const maxStartDays = new Date(cycleStartYear, cycleStartMonth + 1, 0).getDate();
    const actualStartDay = Math.min(startDay, maxStartDays);

    const endDayWanted = startDay - 1;
    const maxEndDays = new Date(cycleEndYear, cycleEndMonth + 1, 0).getDate();
    const actualEndDay = Math.min(endDayWanted === 0 ? maxEndDays : endDayWanted, maxEndDays);

    const startStr = `${cycleStartYear}-${String(cycleStartMonth + 1).padStart(2, '0')}-${String(actualStartDay).padStart(2, '0')}`;
    const endStr = `${cycleEndYear}-${String(cycleEndMonth + 1).padStart(2, '0')}-${String(actualEndDay).padStart(2, '0')}`;

    return {
      startStr,
      endStr,
      label: `${startStr} ➔ ${endStr}`,
    };
  }

  function rebuildMaps() {
    advancesMap = {};
    absencesMap = {};
    for (const emp of employees) {
      const period = getEmployeePayPeriod(emp);
      const advTotal = allAdvances
        .filter((a) => a.employee_id === emp.id && a.date >= period.startStr && a.date <= period.endStr)
        .reduce((sum, a) => sum + (a.amount || 0), 0);
      advancesMap[emp.id] = advTotal;

      const absTotal = allAbsences
        .filter((a) => a[1] === emp.id && a[4] >= period.startStr && a[4] <= period.endStr)
        .reduce((sum, a) => sum + (a[2] || 0), 0);
      absencesMap[emp.id] = absTotal;
    }
  }

  async function openAdvanceHistory(emp: Employee) {
    historyEmployee = emp;
    try {
      advanceLog = await invoke<any[]>('list_employee_advances', {
        employeeId: emp.id,
        month: null,
      });
    } catch {
      advanceLog = [];
    }
    try {
      absenceLog = await invoke<any[]>('list_employee_absences', {
        employeeId: emp.id,
        month: null,
      });
    } catch {
      absenceLog = [];
    }
  }

  // ── Sorting (name / presence days / payroll / advances, both ways) ──────
  let sortKey: 'name' | 'presence' | 'payroll' | 'advances' = 'name';
  let sortDir: 'asc' | 'desc' = 'asc';

  // Estimated net for the CURRENT cycle (base − advances − absence deduction).
  function netFor(emp: Employee): number {
    const adv = advancesMap[emp.id] || 0;
    const absent = absencesMap[emp.id] || 0;
    const dailyRate = Math.round(emp.base_salary / 30);
    return Math.max(0, emp.base_salary - adv - absent * dailyRate);
  }

  // Presence days for the cycle: cycle length minus absent days.
  function presenceDays(emp: Employee): number {
    const p = getEmployeePayPeriod(emp);
    const start = new Date(p.startStr + 'T00:00:00').getTime();
    const end = new Date(p.endStr + 'T00:00:00').getTime();
    const cycleLen = Math.max(1, Math.round((end - start) / 86400000) + 1);
    return Math.max(0, cycleLen - (absencesMap[emp.id] || 0));
  }

  $: sortedEmployees = [...filteredEmployees].sort((a, b) => {
    let d = 0;
    if (sortKey === 'name') d = (a.full_name || '').localeCompare(b.full_name || '');
    else if (sortKey === 'presence') d = presenceDays(a) - presenceDays(b);
    else if (sortKey === 'payroll') d = netFor(a) - netFor(b);
    else if (sortKey === 'advances') d = (advancesMap[a.id] || 0) - (advancesMap[b.id] || 0);
    return sortDir === 'asc' ? d : -d;
  });
  function toggleSortDir() {
    sortDir = sortDir === 'asc' ? 'desc' : 'asc';
  }

  // ── Top summary cards (whole payroll, current cycle) ────────────────────
  $: totalEmployees = employees.length;
  $: payrollToPay = employees.reduce((s, e) => s + netFor(e), 0);
  $: totalAdvancesAmount = employees.reduce((s, e) => s + (advancesMap[e.id] || 0), 0);
  $: totalAbsenceDaysCount = employees.reduce((s, e) => s + (absencesMap[e.id] || 0), 0);

  // ── Password-gated field-team member editing (all fields) ───────────────
  let editGateTarget: { member: any; employee: Employee | null } | null = null;
  let editGatePassword = "";
  let editGateError = "";
  let editGateBusy = false;

  function requestEditFieldMember(member: any) {
    const employee = employees.find(
      (e) => ((e as any).email || "").trim().toLowerCase() === (member.email || "").trim().toLowerCase()
    ) || null;
    editGateTarget = { member, employee };
    editGatePassword = "";
    editGateError = "";
  }

  async function confirmEditGate() {
    if (!editGateTarget) return;
    editGateBusy = true;
    editGateError = "";
    try {
      const ok = await invoke<boolean>("verify_admin_password", { password: editGatePassword });
      if (!ok) {
        editGateError = "Wrong admin password / كلمة المرور غير صحيحة";
        return;
      }
      const { member, employee } = editGateTarget;
      editGateTarget = null;
      if (employee) {
        openEditEmployee(employee);
      } else {
        // Not on payroll yet: open the create form prefilled from the account.
        openAddEmployee();
        name = member.full_name || "";
        isFieldTeam = true;
        fieldRole = member.role || "seller";
        fieldAdminPassword = editGatePassword;
        jobTitle = member.role || "";
        email = member.email || "";
        await generateEmployeeCode();
      }
    } catch (e: any) {
      editGateError = typeof e === "string" ? e : e?.message || "Verification failed";
    } finally {
      editGateBusy = false;
    }
  }


  // FIELD TEAM — one people model. Field members are CRM profiles (preseller
  // / seller / admin) that log into the mobile app; when they also exist as
  // local employees (linked by EMAIL) they show as REGULAR EMPLOYEE CARDS
  // (with the FIELD badge) and get the exact same payroll surface: advances
  // (booked as expenses), absences, history and slips. The old "Field Team"
  // sidebar page is blended into THIS page — no separate members strip.
  let fieldMembers: any[] = [];
  async function loadFieldMembers() {
    try {
      fieldMembers = await invoke<any[]>('cloud_team_members');
    } catch (e) {
      console.warn('Field team unavailable (offline/cloud off?):', e);
      fieldMembers = [];
    }
  }
  function fieldMemberFor(emp: Employee | null): any | null {
    if (!emp) return null;
    const mail = ((emp as any).email || '').trim().toLowerCase();
    if (!mail) return null;
    return fieldMembers.find((m) => (m.email || '').trim().toLowerCase() === mail) || null;
  }

  onMount(async () => {
    await loadEmployees();
    await loadAdvances();
    await loadAbsences();
    await loadFieldMembers();
  });

  async function loadAdvances() {
    try {
      allAdvances = await invoke<any[]>('list_employee_advances', {
        employeeId: null,
        month: null,
      });
      rebuildMaps();
    } catch (e) {
      console.warn('Could not load advances:', e);
    }
  }

  async function loadAbsences() {
    try {
      allAbsences = await invoke<any[]>('list_employee_absences', {
        employeeId: null,
        month: null,
      });
      rebuildMaps();
    } catch (e) {
      console.warn('Could not load absences:', e);
    }
  }

  async function loadEmployees() {
    try {
      employees = await invoke<Employee[]>('list_employees');
      rebuildMaps();
    } catch (e) {
      console.error(e);
    }
  }

  // Generate the next free employee code (EMP-01, EMP-02...) — computed
  // server-side against ALL rows: soft-deleted employees keep their codes
  // (UNIQUE constraint), so a guess over the active list alone collided on
  // save ("unique constraint failed: employees.employee_code").
  async function generateEmployeeCode() {
    try {
      code = await invoke<string>('next_employee_code');
    } catch (e) {
      console.warn('Could not generate code:', e);
    }
  }

  // ── Field-team creation/edit options (revealed by the toggle) ──────────
  let isFieldTeam = false;
  let fieldRole = 'seller';
  let fieldPassword = '';
  // The CALLER's own POS password — the cloud edge functions re-verify it.
  let fieldAdminPassword = '';
  let fieldResult = '';
  let fieldError = '';

  function openAddEmployee() {
    editingEmployeeId = null;
    code = '';
    name = '';
    jobTitle = '';
    baseSalary = 0;
    phone = '';
    email = '';
    rfidCode = '';
    startDate = localDateStr();
    // Field options reset; role defaults to seller (the common case).
    isFieldTeam = false;
    fieldRole = 'seller';
    fieldPassword = '';
    fieldAdminPassword = '';
    fieldResult = '';
    fieldError = '';
    isAddOpen = true;
    // The form sits above the card grid — bring it into view (owner: the
    // button 'didn't work' when the field-team cards pushed it out of sight).
    setTimeout(() => {
      document.getElementById('newEmployeeForm')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 60);
  }

  function openEditEmployee(emp: Employee) {
    editingEmployeeId = emp.id;
    code = emp.employee_code || '';
    name = emp.full_name || '';
    jobTitle = emp.job_title || '';
    baseSalary = emp.base_salary || 0;
    phone = (emp as any).phone || '';
    email = (emp as any).email || '';
    rfidCode = ((emp as any).rfid_code as string) || '';
    startDate = emp.salary_start_date || (emp as any).hire_date || localDateStr();
    // Toggle ON (prefilled) when this employee is linked to a field account.
    isFieldTeam = !!fieldMemberFor(emp);
    fieldRole = fieldMemberFor(emp)?.role || 'seller';
    fieldPassword = '';
    fieldAdminPassword = '';
    fieldResult = '';
    fieldError = '';
    isAddOpen = true;
  }

  let errorToast = '';
  let errorToastTimer: any = null;
  function showError(msg: string) {
    errorToast = msg;
    clearTimeout(errorToastTimer);
    errorToastTimer = setTimeout(() => {
      errorToast = '';
    }, 4500);
  }

  // Deleting an employee is destructive: type DELETE + admin password.
  let delConfirmText = '';
  let delAdminPassword = '';
  let delError = '';

  async function handleDeleteEmployee() {
    if (!employeeToDelete) return;
    if (delConfirmText.trim().toUpperCase() !== 'DELETE') {
      delError = t('delete_type_delete');
      return;
    }
    if (!delAdminPassword.trim()) {
      delError = t('admin_password_required');
      return;
    }
    try {
      const ok = await invoke<boolean>('verify_admin_password', { password: delAdminPassword });
      if (!ok) {
        delError = t('admin_password_wrong');
        return;
      }
      await invoke('delete_employee', { employeeId: employeeToDelete.id });
      employeeToDelete = null;
      delConfirmText = '';
      delAdminPassword = '';
      delError = '';
      await loadEmployees();
    } catch (e: any) {
      showError('Failed to delete employee: ' + (e.message || e));
    }
  }

  async function handleSaveEmployee() {
    if (!name.trim() || !code.trim()) {
      showError('Name and code are required / الاسم والرمز إجباريان');
      return;
    }
    // Field team requires enough to provision the mobile account.
    if (isFieldTeam && editingEmployeeId === null && !email.trim()) {
      showError('Field team members need an email (their app login) / البريد إجباري لعضو الفريق الميداني');
      return;
    }
    if (isFieldTeam && !fieldAdminPassword.trim() && (editingEmployeeId === null || fieldPassword.trim())) {
      showError('Your own password is required to provision the field account / كلمة مرورك إجبارية لإنشاء الحساب الميداني');
      return;
    }
    try {
      await invoke('save_employee', {
        code,
        name,
        phone: phone || null,
        email: email || null,
        nationalId: null,
        jobTitle,
        baseSalary,
        salaryType: 'monthly',
        salaryStartDate: startDate || null,
        hireDate: startDate || localDateStr(),
        notes: null,
        rfidCode: rfidCode || null,
        employeeId: editingEmployeeId,
      });
    } catch (e: any) {
      // Editing reuses the same employee_code — show why it failed.
      showError('Save failed: ' + (typeof e === 'string' ? e : e.message || e));
      return;
    }

    // FIELD TEAM provisioning: invite (new) or role/password update (linked
    // edit). The employee row is already saved — a cloud failure surfaces a
    // message but never loses the payroll record.
    if (isFieldTeam) {
      fieldResult = '';
      fieldError = '';
      try {
        // A member already in the CRM updates in place; a brand-new email
        // invites a fresh account. Either way the payroll row is kept.
        const existingMember =
          fieldMembers.find((m) => (m.email || '').trim().toLowerCase() === email.trim().toLowerCase()) || null;
        if (existingMember) {
          await invoke('cloud_team_update', {
            userId: existingMember.id,
            fullName: name.trim(),
            role: fieldRole,
            password: fieldPassword.trim() || null,
            adminPassword: fieldAdminPassword.trim() || null,
          });
          fieldResult = 'Field account updated. / تم تحديث الحساب الميداني';
        } else {
          await invoke('cloud_team_invite', {
            email: email.trim(),
            fullName: name.trim(),
            role: fieldRole,
            password: fieldPassword.trim() || null,
            adminPassword: fieldAdminPassword.trim() || null,
          });
          fieldResult = 'Field account created — this member can log into the mobile app. / تم إنشاء الحساب الميداني';
        }
      } catch (e: any) {
        fieldError = typeof e === 'string' ? e : (e as any)?.message || 'Field account failed';
      }
    }

    isAddOpen = false;
    editingEmployeeId = null;
    code = '';
    name = '';
    rfidCode = '';
    email = '';
    jobTitle = '';
    baseSalary = 0;
    phone = '';
    startDate = localDateStr();
    isFieldTeam = false;
    fieldPassword = '';
    fieldAdminPassword = '';
    await loadEmployees();
    await loadFieldMembers();
    // Keep the provisioning result visible on the page when cloud failed —
    // the employee is saved; the field account may not be.
    if (fieldError) {
      showError('Employee saved, but the field account failed: ' + fieldError);
      fieldError = '';
    }
  }

  async function recordAdvance() {
    if (!selectedEmpForAdvance || advanceAmount <= 0) return;
    const emp = selectedEmpForAdvance;
    try {
      // Persisted + booked as an "Avances Salaires" expense; cash advances
      // leave the drawer (register + statistics reflect them).
      await invoke('record_employee_advance', {
        input: {
          employee_id: emp.id,
          amount: advanceAmount,
          reason: advanceReason || 'Avance sur salaire',
          date: advanceDate,
          session_id: $activeSession?.id ?? null,
          user_id: $currentUser?.id || 1,
        },
      });
      await loadAdvances();
    } catch (e: any) {
      showError('Failed to record advance: ' + (typeof e === 'string' ? e : e.message || e));
    }
    selectedEmpForAdvance = null;
    advanceAmount = 0;
  }

  async function recordAbsence() {
    if (!selectedEmpForAbsence || absenceDays <= 0) return;
    const emp = selectedEmpForAbsence;
    try {
      await invoke('record_employee_absence', {
        employeeId: emp.id,
        days: absenceDays,
        reason: absenceReason || 'Absence / غياب',
        date: absenceDate || localDateStr(),
      });
      await loadAbsences();
    } catch (e) {
      console.error('Could not record absence:', e);
      showError('Could not record absence: ' + e);
    }
    selectedEmpForAbsence = null;
    absenceDays = 1;
  }

  // Employee QR card: wallet-size card with the employee QR — scanning it
  // on the POS sets this employee as the cart customer.
  async function printEmployeeQrCard(emp: Employee) {
    const qr = await entityQrDataUrl(emp.qr_code || `EMP-${emp.employee_code}`, 300).catch(() => '');
    const html = `<div style="width:60mm;font-family:monospace;font-size:10px;text-align:center;padding:3mm;">
      <p style="font-size:13px;font-weight:900;margin:0;">${emp.full_name}</p>
      <p style="font-size:9px;margin:2px 0;">${emp.employee_code} • ${emp.job_title || ''}</p>
      <img src="${qr}" alt="QR" style="width:34mm;height:34mm;margin:4mm auto;" />
      <p style="font-size:8px;">Titaou One • ${new Date().toLocaleDateString('fr-FR')}</p>
    </div>`;
    const r = await printHtmlSilently(html, 'Employee Card ' + emp.employee_code, { widthMm: 60 });
    if (!r.ok) showError('Print failed: ' + r.message);
  }

  async function printPayrollSlip(emp: Employee) {
    const advances = advancesMap[emp.id] || 0;
    const daysAbsent = absencesMap[emp.id] || 0;
    const dailyRate = Math.round(emp.base_salary / 30);
    const absenceDeduction = daysAbsent * dailyRate;
    const netSalary = Math.max(0, emp.base_salary - advances - absenceDeduction);

    try {
      const r = await printService.printPayment({
        receiptNumber: `PAYROLL-${emp.employee_code}`,
        date: new Date().toISOString().slice(0, 10),
        title: 'BULLETIN DE PAIE / كشف راتب',
        partyName: `${emp.full_name} (${emp.job_title || 'Employé'})`,
        partyType: 'employee',
        amount: netSalary,
        paymentMethod: 'ESPECES',
        notes: `Salaire Base: ${emp.base_salary.toLocaleString()} DZD • Avances: ${advances.toLocaleString()} DZD • Absences (${daysAbsent} j): -${absenceDeduction.toLocaleString()} DZD`,
      });
      if (!r.ok && r.mode !== 'disabled') showError('Print failed: ' + r.message);
    } catch (e: any) {
      showError('Print error: ' + (e?.message || e));
    }
  }
</script>

<div class="p-6 space-y-6 overflow-y-auto h-full bg-pos-bg">
  <div class="flex items-center justify-between">
    <div>
      <h1 class="text-xl font-black text-pos-text flex items-center gap-2">
        <UserCheck class="w-6 h-6 text-sky-600" />
        <span>{t('pay_title')}</span>
      </h1>
      <p class="text-xs text-pos-muted mt-0.5">{t('pay_subtitle')}</p>
    </div>
    <button
      on:click={openAddEmployee}
      class="px-4 py-2.5 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl transition flex items-center gap-1.5 shadow-md cursor-pointer active:scale-95"
    >
      <Plus class="w-4 h-4" />
      <span>New Employee (موظف جديد)</span>
    </button>
  </div>

  <!-- Payroll summary cards (whole payroll, current cycle) -->
  <div class="grid grid-cols-2 lg:grid-cols-4 gap-3">
    <div class="bg-pos-card border border-pos-border p-4 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-10 h-10 rounded-xl bg-sky-50 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold shrink-0">
        <Users class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-[10px] font-bold text-pos-muted uppercase">Employees / الموظفون</p>
        <p class="text-xl font-black font-mono text-pos-text">{totalEmployees}</p>
      </div>
    </div>
    <div class="bg-pos-card border border-pos-border p-4 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-10 h-10 rounded-xl bg-emerald-50 dark:bg-emerald-950 text-emerald-600 flex items-center justify-center font-bold shrink-0">
        <DollarSign class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-[10px] font-bold text-pos-muted uppercase">Payroll to pay (cycle) / للأجرة</p>
        <p class="text-xl font-black font-mono text-emerald-600">{payrollToPay.toLocaleString()} DZD</p>
      </div>
    </div>
    <div class="bg-pos-card border border-pos-border p-4 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-10 h-10 rounded-xl bg-amber-50 dark:bg-amber-950 text-amber-600 flex items-center justify-center font-bold shrink-0">
        <CreditCard class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-[10px] font-bold text-pos-muted uppercase">Advances total / التسبيقات</p>
        <p class="text-xl font-black font-mono text-amber-600">{totalAdvancesAmount.toLocaleString()} DZD</p>
      </div>
    </div>
    <div class="bg-pos-card border border-pos-border p-4 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-10 h-10 rounded-xl bg-rose-50 dark:bg-rose-950 text-rose-600 flex items-center justify-center font-bold shrink-0">
        <Calendar class="w-5 h-5" />
      </div>
      <div class="min-w-0">
        <p class="text-[10px] font-bold text-pos-muted uppercase">Absence days / أيام الغياب</p>
        <p class="text-xl font-black font-mono text-rose-600">{totalAbsenceDaysCount} days</p>
      </div>
    </div>
  </div>

  <!-- FIELD TEAM roster - full-size cards like the employee cards: the app
       account AND its payroll side by side. Editing requires your password. -->
  {#if fieldMembers.length > 0}
    <div class="space-y-2">
      <h2 class="text-sm font-black text-pos-text flex items-center gap-2">
        <UsersRound class="w-4 h-4 text-purple-600" />
        Field Team / الفريق الميداني — {fieldMembers.length}
      </h2>
      <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        {#each fieldMembers as f (f.id)}
          {@const linkedEmp = employees.find((e) => ((e as any).email || "").trim().toLowerCase() === (f.email || "").trim().toLowerCase()) || null}
          <div class="relative bg-pos-card border border-pos-border rounded-3xl p-5 shadow-xs flex flex-col justify-between space-y-4 hover:shadow-md transition group">
            <div>
              <div class="flex items-center justify-between mb-2">
                <span class="text-[10px] px-2.5 py-0.5 rounded-full font-black uppercase
                  {f.role === 'admin' ? 'bg-violet-100 text-violet-800 dark:bg-violet-950 dark:text-violet-300'
                    : f.role === 'preseller' ? 'bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300'
                    : 'bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300'}">
                  <UsersRound class="w-3 h-3 inline -mt-0.5" /> {f.role}
                </span>
                <span class="text-[10px] px-2.5 py-0.5 rounded-full font-black uppercase
                  {linkedEmp ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' : 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-200'}">
                  {linkedEmp ? "PAYROLL · #" + linkedEmp.employee_code : "APP ONLY"}
                </span>
              </div>
              <div class="absolute top-3 end-3 opacity-0 group-hover:opacity-100 transition">
                <button
                  type="button"
                  on:click={() => requestEditFieldMember(f)}
                  class="p-1.5 bg-white/95 dark:bg-slate-800/95 text-pos-muted hover:text-sky-600 rounded-lg shadow-xs cursor-pointer"
                  title="Edit member (all fields) — requires your password"
                >
                  <Pencil class="w-3.5 h-3.5" />
                </button>
              </div>
              <h3 class="font-black text-base text-pos-text leading-tight flex items-center gap-1.5">
                {f.full_name}
              </h3>
              <p class="text-[11px] text-pos-muted font-mono truncate mt-0.5 flex items-center gap-1">
                <Mail class="w-3 h-3 text-sky-500 shrink-0" />{f.email || "—"}
              </p>
            </div>

            {#if linkedEmp}
              {@const advances = advancesMap[linkedEmp.id] || 0}
              {@const daysAbsent = absencesMap[linkedEmp.id] || 0}
              <div class="space-y-1.5 p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border text-xs">
                <div class="flex items-center justify-between">
                  <span class="text-pos-muted font-bold">{t('pay_base_monthly')}:</span>
                  <span class="font-bold font-mono text-pos-text">{linkedEmp.base_salary.toLocaleString()} DZD</span>
                </div>
                {#if advances > 0}
                  <div class="flex items-center justify-between text-rose-600">
                    <span>{t('pay_advances')}:</span>
                    <span class="font-mono font-bold">-{advances.toLocaleString()} DZD</span>
                  </div>
                {/if}
                {#if daysAbsent > 0}
                  <div class="flex items-center justify-between text-rose-600">
                    <span>Absences ({daysAbsent} days):</span>
                    <span class="font-mono font-bold">-{(daysAbsent * Math.round(linkedEmp.base_salary / 30)).toLocaleString()} DZD</span>
                  </div>
                {/if}
                <div class="pt-1.5 border-t border-pos-border flex items-center justify-between">
                  <span class="font-black text-pos-text">Estimated Net:</span>
                  <span class="font-black font-mono text-sm text-emerald-600">{netFor(linkedEmp).toLocaleString()} DZD</span>
                </div>
              </div>
            {:else}
              <div class="p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-dashed border-pos-border text-[11px] text-pos-muted font-bold">
                Mobile-app account only — no payroll record yet. Advances / absences / slip unlock once on payroll.
              </div>
            {/if}

            <div class="grid grid-cols-1 gap-2 text-xs">
              <button
                type="button"
                on:click={() => requestEditFieldMember(f)}
                class="py-2 text-white font-bold rounded-xl border text-center cursor-pointer transition shadow-2xs flex items-center justify-center gap-1.5 {linkedEmp ? 'bg-sky-600 hover:bg-sky-700' : 'bg-violet-600 hover:bg-violet-700'}"
                title="Edit member — requires your password"
              >
                <Pencil class="w-3.5 h-3.5" />
                {linkedEmp ? t('team_edit') : "+ Add to payroll (تسجيل)"}
              </button>
            </div>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if isAddOpen}
    <div id="newEmployeeForm" class="bg-pos-card border border-pos-border rounded-2xl p-5 shadow-md space-y-4 animate-in zoom-in-95 duration-150 scroll-mt-4">
      <div class="flex items-center justify-between border-b border-pos-border pb-3">
        <h3 class="font-black text-sm text-pos-text">{t('pay_modal_new')}</h3>
        <button on:click={() => (isAddOpen = false)} class="text-pos-muted hover:text-pos-text"><X class="w-4 h-4" /></button>
      </div>
      <div class="grid grid-cols-1 md:grid-cols-6 gap-4 text-xs">
        <div>
          <label class="block font-bold text-pos-muted mb-1">{t('pay_field_code')} *</label>
          <div class="flex items-center gap-1.5">
            <input type="text" bind:value={code} placeholder="EMP-01" class="flex-1 px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-pos-text font-mono font-bold outline-none" />
            <button
              type="button"
              on:click={generateEmployeeCode}
              class="px-2.5 py-2 bg-sky-600 hover:bg-sky-700 text-white text-[10px] font-black rounded-xl cursor-pointer shrink-0"
              title="Generate the next free code"
            >
              GEN
            </button>
          </div>
        </div>
        <div>
          <label class="block font-bold text-pos-muted mb-1">{t('pay_field_name')} *</label>
          <input type="text" bind:value={name} placeholder="Ahmed Benali" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-pos-text font-bold outline-none" />
        </div>
        <div>
          <label class="block font-bold text-pos-muted mb-1">Start Date / تاريخ البدء *</label>
          <input
            type="date"
            bind:value={startDate}
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-pos-text font-mono font-bold outline-none"
          />
        </div>
        <div>
          <label class="block font-bold text-pos-muted mb-1">{t('pay_field_job')} *</label>
          <input type="text" bind:value={jobTitle} placeholder="Head Cashier" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-pos-text outline-none" />
        </div>
        <div>
          <label class="block font-bold text-pos-muted mb-1">{t('pay_field_salary')} *</label>
          <input type="number" bind:value={baseSalary} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl font-mono font-black text-pos-text outline-none" />
        </div>
        <div>
          <label class="block font-bold text-pos-muted mb-1">{t('emp_rfid')}</label>
          <input
            type="text"
            bind:value={rfidCode}
            placeholder="RFID-XXXX"
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-pos-text font-mono text-[11px] font-bold outline-none"
            title="Scan the employee RFID tag here — searchable from the POS search bar"
          />
        </div>
      </div>

      <!-- FIELD TEAM: mark the member as a mobile-app field worker. Trade
           employees leave it off; field team gets app access + full payroll. -->
      <div class="border border-pos-border rounded-2xl p-4 space-y-3 bg-slate-50 dark:bg-slate-800/40">
        <label class="flex items-center gap-2.5 cursor-pointer select-none w-fit">
          <input type="checkbox" bind:checked={isFieldTeam} class="w-4 h-4 accent-violet-600 cursor-pointer" />
          <UsersRound class="w-4 h-4 text-violet-600" />
          <span class="text-xs font-black text-pos-text">Field Team member (عضو الفريق الميداني)</span>
          <span class="text-[10px] text-pos-muted font-bold">— works the field with the mobile app (seller / preseller)</span>
        </label>

        {#if isFieldTeam}
          <div class="grid grid-cols-1 md:grid-cols-4 gap-3 text-xs">
            <div>
              <label class="block font-bold text-pos-muted mb-1">
                <Mail class="w-3 h-3 inline text-sky-500" /> App login email / البريد *
              </label>
              <input type="email" bind:value={email} placeholder="member@shop.dz"
                class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-pos-text font-mono font-bold outline-none" />
            </div>
            <div>
              <label class="block font-bold text-pos-muted mb-1">Field role / الدور *</label>
              <select bind:value={fieldRole}
                class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-pos-text font-bold outline-none">
                <option value="seller">seller</option>
                <option value="preseller">preseller</option>
                <option value="admin">admin</option>
              </select>
            </div>
            <div>
              <label class="block font-bold text-pos-muted mb-1">
                <KeyRound class="w-3 h-3 inline text-sky-500" /> App password / كلمة المرور
              </label>
              <input type="text" bind:value={fieldPassword} placeholder="(leave empty to keep/invite default)"
                class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-pos-text font-mono font-bold outline-none" />
            </div>
            <div>
              <label class="block font-bold text-pos-muted mb-1">Your password (confirm) / كلمة مرورك *</label>
              <input type="password" bind:value={fieldAdminPassword}
                class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-pos-text font-mono font-bold outline-none" />
            </div>
          </div>
          {#if fieldResult}
            <p class="text-[11px] font-bold text-emerald-600">✅ {fieldResult}</p>
          {/if}
          {#if fieldError}
            <p class="text-[11px] font-bold text-rose-600">❌ {fieldError}</p>
          {/if}
          <p class="text-[10px] text-pos-muted font-bold leading-relaxed">
            Creates the member's mobile-app account (role + password) AND a payroll record — advances, absences and slips work exactly like any employee.
            <br/>ينشئ حساب التطبيق الميداني (الدور + كلمة المرور) وسجل الأجرة — التسبيقات والغيابات تعمل مثل أي موظف.
          </p>
        {/if}
      </div>
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (isAddOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={handleSaveEmployee} class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl shadow-xs cursor-pointer">{t('pay_save_employee')}</button>
      </div>
    </div>
  {/if}

  <!-- Employee search: name, code, job title, RFID tag + sort -->
  <div class="flex items-center gap-2 flex-wrap">
    <div class="relative flex-1 min-w-[220px] max-w-md">
      <input
        type="text"
        bind:value={employeeSearch}
        placeholder={t('emp_search')}
        class="w-full pe-8 px-4 py-2.5 bg-pos-card border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
      />
      {#if employeeSearch}
        <button
          type="button"
          on:click={() => (employeeSearch = '')}
          class="absolute end-2 top-2.5 text-pos-muted hover:text-rose-500 rounded-full p-0.5 cursor-pointer"
          title="Clear"
        ><X class="w-4 h-4" /></button>
      {/if}
    </div>

    <!-- Sort: name / presence days / payroll amount / advances (±) -->
    <div class="flex items-center gap-1.5">
      <span class="text-[10px] font-black text-pos-muted uppercase">Sort</span>
      <select bind:value={sortKey}
        class="px-2 py-2 bg-pos-card border border-pos-border rounded-xl text-[11px] font-bold text-pos-text outline-none cursor-pointer">
        <option value="name">Name</option>
        <option value="presence">Days of presence</option>
        <option value="payroll">Payroll amount</option>
        <option value="advances">Advances amount</option>
      </select>
      <button
        type="button"
        on:click={toggleSortDir}
        class="px-2.5 py-2 bg-pos-card border border-pos-border rounded-xl text-[11px] font-black text-sky-600 hover:bg-slate-50 dark:hover:bg-slate-800 cursor-pointer transition"
        title="Toggle direction (asc / desc)"
      >
        {sortDir === 'asc' ? '↑ Asc' : '↓ Desc'}
      </button>
    </div>

    {#if employeeSearch}
      <span class="text-[10px] font-bold text-pos-muted">
        {filteredEmployees.length} / {employees.length}
      </span>
    {/if}
  </div>

  <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
    {#each sortedEmployees as emp}
      {@const advances = advancesMap[emp.id] || 0}
      {@const daysAbsent = absencesMap[emp.id] || 0}
      {@const dailyRate = Math.round(emp.base_salary / 30)}
      {@const absenceDeduction = daysAbsent * dailyRate}
      {@const netSalary = Math.max(0, emp.base_salary - advances - absenceDeduction)}
      {@const fieldMember = fieldMemberFor(emp)}
      <div class="relative bg-pos-card border border-pos-border rounded-3xl p-5 shadow-xs flex flex-col justify-between space-y-4 hover:shadow-md transition group">
        <div>
          <div class="flex items-center justify-between mb-2">
            <span class="font-mono text-xs font-black text-sky-600 bg-sky-50 dark:bg-sky-950 px-2 py-0.5 rounded-lg border border-sky-200 dark:border-sky-800">
              #{emp.employee_code}
            </span>
            <div class="flex items-center gap-1">
              {#if fieldMember}
                <span class="text-[9px] px-2 py-0.5 rounded-full font-black uppercase bg-violet-100 text-violet-800 dark:bg-violet-950 dark:text-violet-300"
                  title="Field team — has a mobile-app account ({fieldMember.email || ''})">
                  <UsersRound class="w-3 h-3 inline -mt-0.5" /> FIELD · {fieldMember.role}
                </span>
              {/if}
              <span class="text-[10px] px-2.5 py-0.5 rounded-full font-black uppercase bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300">
                ACTIVE
              </span>
            </div>
          </div>
          <div class="absolute top-3 end-3 flex items-center gap-1 opacity-0 group-hover:opacity-100 transition">
            <button
              type="button"
              on:click={() => openEditEmployee(emp)}
              class="p-1.5 bg-white/95 dark:bg-slate-800/95 text-pos-muted hover:text-sky-600 rounded-lg shadow-xs cursor-pointer"
              title="Edit employee (تعديل)"
            >
              <Pencil class="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              on:click={() => (employeeToDelete = emp)}
              class="p-1.5 bg-white/95 dark:bg-slate-800/95 text-pos-muted hover:text-rose-600 rounded-lg shadow-xs cursor-pointer"
              title="Delete employee (حذف)"
            >
              <Trash2 class="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              on:click={() => printEmployeeQrCard(emp)}
              class="p-1.5 bg-white/95 dark:bg-slate-800/95 text-pos-muted hover:text-sky-600 rounded-lg shadow-xs cursor-pointer"
              title={t('emp_print_qr')}
            >
              <QrCode class="w-3.5 h-3.5" />
            </button>
            <button
              type="button"
              on:click={() => openAdvanceHistory(emp)}
              class="px-2 py-1 bg-white/95 dark:bg-slate-800/95 text-pos-muted hover:text-sky-600 rounded-lg shadow-xs cursor-pointer text-[10px] font-black"
              title="Avances & payments history (السجل)"
            >
              History
            </button>
          </div>

          <h3 class="font-black text-base text-pos-text leading-tight">{emp.full_name}</h3>
          <p class="text-xs text-pos-muted mt-0.5">{emp.job_title}</p>
        </div>

        <div class="space-y-1.5 p-3 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border text-xs">
          <div class="flex items-center justify-between text-[11px] font-bold text-pos-muted border-b border-pos-border/40 pb-1">
            <span>Start / البدء:</span>
            <span class="font-mono text-pos-text">{emp.salary_start_date || '01 (Défaut)'}</span>
          </div>
          <div class="flex items-center justify-between text-[11px] font-bold text-sky-600 dark:text-sky-400 border-b border-pos-border/40 pb-1">
            <span>Cycle / الفترة:</span>
            <span class="font-mono">{getEmployeePayPeriod(emp).label}</span>
          </div>
          <div class="flex items-center justify-between">
            <span class="text-pos-muted font-bold">{t('pay_base_monthly')}:</span>
            <span class="font-bold font-mono text-pos-text">{emp.base_salary.toLocaleString()} DZD</span>
          </div>
          {#if advances > 0}
            <div class="flex items-center justify-between text-rose-600">
              <span>{t('pay_advances')}:</span>
              <span class="font-mono font-bold">-{advances.toLocaleString()} DZD</span>
            </div>
          {/if}
          {#if daysAbsent > 0}
            <div class="flex items-center justify-between text-rose-600">
              <span>Absences ({daysAbsent} days):</span>
              <span class="font-mono font-bold">-{absenceDeduction.toLocaleString()} DZD</span>
            </div>
          {/if}
          <div class="pt-1.5 border-t border-pos-border flex items-center justify-between">
            <span class="font-black text-pos-text">Estimated Net:</span>
            <span class="font-black font-mono text-sm text-emerald-600">{netSalary.toLocaleString()} DZD</span>
          </div>
        </div>

        <div class="grid grid-cols-3 gap-2 pt-1 text-xs">
          <button
            type="button"
            on:click={() => (selectedEmpForAdvance = emp)}
            class="py-2 bg-amber-50 dark:bg-amber-950/40 hover:bg-amber-100 text-amber-800 dark:text-amber-300 font-bold rounded-xl border border-amber-200 dark:border-amber-800 text-center cursor-pointer transition shadow-2xs"
            title="Record Salary Advance"
          >
            + Avance
          </button>

          <button
            type="button"
            on:click={() => (selectedEmpForAbsence = emp)}
            class="py-2 bg-rose-50 dark:bg-rose-950/40 hover:bg-rose-100 text-rose-800 dark:text-rose-300 font-bold rounded-xl border border-rose-200 dark:border-rose-800 text-center cursor-pointer transition shadow-2xs"
            title="Record Absence / Leave"
          >
            + Absence
          </button>

          <button
            type="button"
            on:click={() => printPayrollSlip(emp)}
            class="py-2 bg-sky-600 hover:bg-sky-700 text-white font-bold rounded-xl flex items-center justify-center gap-1 cursor-pointer transition shadow-xs"
            title="Print Monthly Slip"
          >
            <Printer class="w-3.5 h-3.5" />
            <span>Fiche</span>
          </button>
        </div>
      </div>
    {/each}
  </div>
</div>

<!-- Modal: Advance on Salary -->
{#if selectedEmpForAdvance}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl p-6 shadow-2xl max-w-sm w-full space-y-4 animate-in zoom-in-95 duration-150">
      <div class="flex items-center justify-between border-b border-pos-border pb-3">
        <h3 class="font-black text-sm text-pos-text">Record Advance (تسبيق راتب)</h3>
        <button on:click={() => (selectedEmpForAdvance = null)} class="text-pos-muted cursor-pointer"><X class="w-4 h-4" /></button>
      </div>
      <p class="text-xs text-pos-muted font-bold">Employee: {selectedEmpForAdvance.full_name}</p>
      <div class="space-y-3">
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Advance Amount (DZD) / المبلغ</label>
          <input type="number" min="500" step="500" bind:value={advanceAmount} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-mono font-black text-pos-text outline-none" />
        </div>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Date / التاريخ</label>
          <input type="date" bind:value={advanceDate} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-bold text-pos-text outline-none" />
        </div>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Reason / السبب أو ملاحظات</label>
          <input type="text" bind:value={advanceReason} placeholder="Avance sur salaire" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-bold text-pos-text outline-none" />
        </div>
      </div>
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (selectedEmpForAdvance = null)} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={recordAdvance} class="px-4 py-1.5 bg-amber-600 hover:bg-amber-700 text-white text-xs font-black rounded-xl cursor-pointer">Confirm Advance</button>
      </div>
    </div>
  </div>
{/if}

<!-- Modal: Absence Recording -->
{#if selectedEmpForAbsence}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl p-6 shadow-2xl max-w-sm w-full space-y-4 animate-in zoom-in-95 duration-150">
      <div class="flex items-center justify-between border-b border-pos-border pb-3">
        <h3 class="font-black text-sm text-pos-text">Record Absence (تسجيل غياب)</h3>
        <button on:click={() => (selectedEmpForAbsence = null)} class="text-pos-muted cursor-pointer"><X class="w-4 h-4" /></button>
      </div>
      <p class="text-xs text-pos-muted font-bold">Employee: {selectedEmpForAbsence.full_name}</p>
      <div class="space-y-3">
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Number of Days / عدد الأيام</label>
          <input type="number" min="1" max="30" bind:value={absenceDays} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-mono font-black text-pos-text outline-none" />
        </div>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Date / التاريخ</label>
          <input type="date" bind:value={absenceDate} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-bold text-pos-text outline-none" />
        </div>
        <div>
          <label class="block text-xs font-bold text-pos-muted mb-1">Reason / السبب</label>
          <input type="text" bind:value={absenceReason} placeholder="Absence non justifiée" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-bold text-pos-text outline-none" />
        </div>
      </div>
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (selectedEmpForAbsence = null)} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={recordAbsence} class="px-4 py-1.5 bg-rose-600 hover:bg-rose-700 text-white text-xs font-black rounded-xl cursor-pointer">Confirm Deduction</button>
      </div>
    </div>
  </div>
{/if}
<!-- Employee Advances History -->
{#if historyEmployee}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-md p-6 space-y-4">
      <div class="flex items-center justify-between">
        <h3 class="font-black text-sm text-pos-text">{t('pay_advances_history')} — {historyEmployee.full_name}</h3>
        <button on:click={() => (historyEmployee = null)} class="text-pos-muted hover:text-pos-text p-1 rounded cursor-pointer">
          <X class="w-5 h-5" />
        </button>
      </div>
      <div class="max-h-72 overflow-y-auto space-y-1.5">
        {#each advanceLog as adv}
          <div class="flex items-center justify-between p-2 bg-slate-50 dark:bg-slate-800/40 rounded-lg text-xs">
            <span class="font-mono text-pos-muted">{adv.date}</span>
            <span class="text-pos-text truncate max-w-[160px]">{adv.reason || 'Avance sur salaire'}</span>
            <span class="font-mono font-black text-rose-600">-{adv.amount.toLocaleString()} DZD</span>
          </div>
        {/each}
        {#each absenceLog as ab}
          <div class="flex items-center justify-between p-2 bg-amber-50 dark:bg-amber-950/30 rounded-lg text-xs">
            <span class="font-mono text-pos-muted">{ab[4]}</span>
            <span class="text-pos-text truncate max-w-[160px]">{ab[3] || 'Absence / غياب'}</span>
            <span class="font-mono font-black text-amber-600">{ab[2]} day(s)</span>
          </div>
        {/each}
        {#if advanceLog.length === 0 && absenceLog.length === 0}
          <p class="text-xs text-pos-muted text-center py-4">{t('pay_no_advances')}</p>
        {/if}
      </div>
    </div>
  </div>
{/if}

<!-- Delete Employee Confirmation -->
{#if employeeToDelete}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4">
      <h3 class="font-black text-sm text-pos-text flex items-center gap-2 text-rose-600">
        <Trash2 class="w-5 h-5" />
        <span>{t('pay_delete_employee')}</span>
      </h3>
      <p class="text-xs text-pos-muted">
        {t('pay_delete_confirm_hint')} <strong class="text-pos-text">{employeeToDelete.full_name}</strong>?
      </p>
      <input
        type="text"
        bind:value={delConfirmText}
        placeholder="DELETE"
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-rose-300 dark:border-rose-800 rounded-xl text-xs font-mono font-black text-rose-600 outline-none"
      />
      <input
        type="password"
        bind:value={delAdminPassword}
        placeholder={t('admin_password')}
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono text-pos-text outline-none"
      />
      {#if delError}
        <p class="text-[11px] font-bold text-rose-600">{delError}</p>
      {/if}
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (employeeToDelete = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={handleDeleteEmployee} class="px-4 py-2 bg-rose-600 text-white text-xs font-black rounded-xl cursor-pointer shadow-md">Confirm Delete</button>
      </div>
    </div>
  </div>
{/if}

<!-- Field-team member edit gate: all fields editable, but YOUR password first -->
{#if editGateTarget}
  <div class="fixed inset-0 z-[60] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95 duration-150">
      <div class="flex items-center gap-3 text-sky-600">
        <ShieldCheck class="w-5 h-5 shrink-0" />
        <h3 class="font-black text-sm text-pos-text">
          Edit {editGateTarget.employee ? "member" : "member → add to payroll"}: {editGateTarget.member.full_name}
        </h3>
      </div>
      <p class="text-xs text-pos-muted leading-relaxed">
        All fields will be editable — confirm with <strong class="text-pos-text">your own password</strong> first.
        <br/>كل الحقول قابلة للتعديل — أكّد بكلمة مرورك أولاً.
      </p>
      <input
        type="password"
        bind:value={editGatePassword}
        on:keydown={(e) => e.key === "Enter" && confirmEditGate()}
        placeholder="Your password / كلمة مرورك"
        class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
      />
      {#if editGateError}
        <p class="text-[11px] font-bold text-rose-600">{editGateError}</p>
      {/if}
      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (editGateTarget = null)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
        <button on:click={confirmEditGate} disabled={editGateBusy || !editGatePassword.trim()}
          class="px-4 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white text-xs font-black rounded-xl cursor-pointer">
          {editGateBusy ? "Checking…" : "Unlock editing"}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if errorToast}
  <div class="fixed bottom-6 end-6 z-[70] bg-rose-600 text-white px-5 py-3 rounded-2xl shadow-2xl flex items-center gap-3 text-xs font-bold animate-in slide-in-from-bottom-3 duration-200">
    <AlertTriangle class="w-4 h-4 shrink-0" />
    <span>{errorToast}</span>
  </div>
{/if}
