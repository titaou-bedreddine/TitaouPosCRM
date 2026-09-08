<script lang="ts">
  import { onMount } from 'svelte';
  import { t, currentLocale, setLocale } from './lib/i18n';
  import { currentUser, isAuthenticated, logout } from './lib/stores/auth';
  import { activeSession } from './lib/stores/session';
  import { invoke } from '@tauri-apps/api/core';
  import type { CashSession } from './lib/types';

  // Routes
  import PosView from './routes/pos/PosView.svelte';
  import SalesView from './routes/sales/SalesView.svelte';
  import CashRegisterView from './routes/cash/CashRegisterView.svelte';
  import PurchasesView from './routes/purchases/PurchasesView.svelte';
  import CustomersView from './routes/customers/CustomersView.svelte';
  import SuppliersView from './routes/suppliers/SuppliersView.svelte';
  import InventoryView from './routes/inventory/InventoryView.svelte';
  import ExpensesView from './routes/expenses/ExpensesView.svelte';
  import PayrollView from './routes/payroll/PayrollView.svelte';
  import DashboardView from './routes/dashboard/DashboardView.svelte';
  import SettingsView from './routes/settings/SettingsView.svelte';
  import NotificationsView from './routes/notifications/NotificationsView.svelte';
  import LoginView from './routes/auth/LoginView.svelte';
  import FieldOrdersView from './routes/fieldorders/FieldOrdersView.svelte';
  import TeamView from './routes/team/TeamView.svelte';
  import TruckLoadsView from './routes/truckloads/TruckLoadsView.svelte';
  import DeletionRequestsView from './routes/deletionrequests/DeletionRequestsView.svelte';
  import CashDrawerModal from './lib/components/CashDrawerModal.svelte';
  import FirstSetupWizard from './lib/components/FirstSetupWizard.svelte';
  import NetworkStatusIndicator from './lib/components/NetworkStatusIndicator.svelte';
  import { networkEvents, networkStatus } from './lib/stores/network';

    import { stockWarningModal } from './lib/stores/cart';

  // Icons
  import {
    LayoutDashboard, ShoppingCart, Receipt, DollarSign, UsersRound, FileWarning,
    Package, TrendingDown, Users, Settings, LogOut,
    Truck, FileSpreadsheet, UserCheck, Wifi, Moon, Sun, CreditCard, Bell, Lock, AlertTriangle
  } from 'lucide-svelte';

  let currentRoute = 'pos';
  // Purchase QR scan (PUR:<invoice>): open the purchases page focused on
  // that invoice.
  let purchasesFocusInvoice: string | null = null;
  // Sidebar drawer-kick feedback (success/failure shown under the button).
  let drawerMsg = '';
  let netToast = '';

  // Route-level access control. Administrators see everything; a Cashier is
  // limited to POS, sales history, expenses and customers (so they can
  // register walk-ins and take payments); everyone else (Manager, custom
  // roles) keeps full navigation.
  const CASHIER_ALLOWED = new Set(['pos', 'sales', 'expenses', 'customers']);
  function isAdmin(user: any): boolean {
    return user?.role_id === 1 || user?.role_name === 'Administrator';
  }
  function isCashier(user: any): boolean {
    return user?.role_id === 2 || user?.role_name === 'Cashier';
  }
  function routeAllowed(route: string, user: any): boolean {
    if (!user) return false;
    if (route === 'settings') {
      return isAdmin(user) || adminSettingsUnlocked;
    }
    if (isAdmin(user)) return true;
    if (isCashier(user)) return CASHIER_ALLOWED.has(route);
    return true; // Manager & custom roles
  }

  let showAdminAuthModal = false;
  let adminAuthPassword = '';
  let adminAuthError = '';
  let adminSettingsUnlocked = false;

  function handleNavigateSettings() {
    if (isAdmin($currentUser) || adminSettingsUnlocked) {
      currentRoute = 'settings';
      return;
    }
    adminAuthPassword = '';
    adminAuthError = '';
    showAdminAuthModal = true;
  }

  async function confirmAdminAuth() {
    try {
      const ok = await invoke<boolean>('verify_admin_password', { password: adminAuthPassword });
      if (!ok) {
        adminAuthError = 'Wrong admin password / كلمة المرور غير صحيحة';
        return;
      }
      adminSettingsUnlocked = true;
      showAdminAuthModal = false;
      currentRoute = 'settings';
    } catch (e: any) {
      adminAuthError = typeof e === 'string' ? e : e?.message || 'Verification failed';
    }
  }

  function settingsAllowed(user: any): boolean {
    return true;
  }
  // Bounce unauthorized routes (e.g. after login or logout).
  $: if (!routeAllowed(currentRoute, $currentUser)) {
    currentRoute = 'pos';
  }
  let isDarkMode = false;
  // Telegram master switch (quick toggle): false = silent. Flip requires
  // the admin password. Quiet windows ("HH:MM-HH:MM" list) live in the
  // Notifications settings tab.
  let telegramMasterOn = true;
  let showTelegramPinModal = false;
  let telegramPinInput = '';
  let telegramPinError = '';
  let pendingTelegramState = true;

  async function loadTelegramMaster() {
    try {
      const v = await invoke<string | null>('get_setting', { key: 'telegram_master_enabled' });
      telegramMasterOn = v !== 'false';
    } catch {
      telegramMasterOn = true;
    }
  }

  function askTelegramToggle(next: boolean) {
    if (next === telegramMasterOn) return;
    pendingTelegramState = next;
    telegramPinInput = '';
    telegramPinError = '';
    showTelegramPinModal = true;
  }

  async function confirmTelegramToggle() {
    try {
      const ok = await invoke<boolean>('verify_admin_password', { password: telegramPinInput });
      if (!ok) {
        telegramPinError = 'Wrong admin password / كلمة المرور غير صحيحة';
        return;
      }
      await invoke('set_setting', {
        key: 'telegram_master_enabled',
        value: pendingTelegramState ? 'true' : 'false',
      });
      telegramMasterOn = pendingTelegramState;
      showTelegramPinModal = false;
    } catch (e: any) {
      telegramPinError = typeof e === 'string' ? e : e?.message || 'Failed';
    }
  }
  let newUpdateAvailable = false;
  let updateTag = '';
  let updateStatus: '' | 'downloading' | 'ready' | 'restarting' | 'error' = '';
  let updateProgress = 0;
  let updateError = '';
  let isCashDrawerOpen = false;
  let isDeveloperPopupOpen = false;
  let showFirstSetup = false;

  // Set by notifications: open this product's editor when the POS mounts.
  let posOpenProductId: number | null = null;

  // Download the signed update package in-app, install it, and relaunch —
  // no browser, no separate installer window.
  async function installUpdate() {
    try {
      updateStatus = 'downloading';
      updateError = '';
      updateProgress = 0;
      const { check } = await import('@tauri-apps/plugin-updater');
      const { relaunch } = await import('@tauri-apps/plugin-process');
      const update = await check();
      if (!update) {
        updateStatus = '';
        newUpdateAvailable = false;
        return;
      }
      updateTag = update.version;
      let downloaded = 0;
      let total = 0;
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            total = event.data.contentLength ?? 0;
            break;
          case 'Progress':
            downloaded += event.data.chunkLength;
            updateProgress = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
            break;
          case 'Finished':
            updateProgress = 100;
            break;
        }
      });
      updateStatus = 'restarting';
      await relaunch();
    } catch (e: any) {
      console.error('Auto-update failed:', e);
      updateStatus = 'error';
      updateError = typeof e === 'string' ? e : e?.message || 'Update failed';
    }
  }

  // Load the open cash session once the user is logged in. A session left
  // open from a previous calendar day is auto-closed at midnight by the
  // backend; on login we re-read so the register never starts on a stale
  // or missing session. Closing the APP never closes a session.
  // NOTE: must stay OUTSIDE onMount — a `$:` inside it is a dead JS label
  // that runs once (before login, user null) and never again, leaving the
  // POS showing "cash session closed" until the register page is visited.
  let sessionLoadInFlight = false;

  async function loadActiveSession() {
    if (!$currentUser || sessionLoadInFlight) return;
    sessionLoadInFlight = true;
    try {
      const session = await invoke<CashSession | null>('get_active_cash_session', { userId: $currentUser.id });
      $activeSession = session;
      // Session left open from a previous day (edge case the auto-close
      // missed, e.g. app closed before midnight tick): prompt to close it.
      if (session && (session as any).is_stale) {
        isCashDrawerOpen = true;
      }
    } catch (err) {
      console.error(err);
    } finally {
      sessionLoadInFlight = false;
    }
  }

  $: if ($currentUser) {
    loadActiveSession();
  }

  // Real-time LAN events from the shop server: session events invalidate the
  // locally cached register view so a connected terminal stays in sync.
  let lastNetEventTs = 0;
  // Client-mode session safety: when the backend reports the user token is
  // gone (stale after a server restart), log the UI out cleanly instead of
  // hitting "Log in before running shop operations" on every operation.
  let lastLoggedIn: boolean | null = null;
  $: if ($currentUser && $networkStatus) {
    const li = !!$networkStatus.logged_in;
    if (lastLoggedIn === null) {
      lastLoggedIn = li;
    } else if (lastLoggedIn && !li) {
      lastLoggedIn = li;
      handleLogout();
    } else {
      lastLoggedIn = li;
    }
  }
  $: if ($networkEvents[0] && $networkEvents[0].ts !== lastNetEventTs) {
    lastNetEventTs = $networkEvents[0].ts;
    const t0 = String($networkEvents[0].type || '');
    if (t0.startsWith('session_') || t0 === 'settings_updated') {
      loadActiveSession();
    }
    // OFFLINE CLIENT (user decision 2026-09-08): the terminal worked offline
    // against its local DB; on reconnect tell the cashier what happened.
    if (t0 === 'offline_session_upgraded') {
      netToast = '✅ Shop server is back — your session is now fully connected. Records created while offline stayed on THIS terminal. / عاد الاتصال بالخادم — السجلات أثناء الانقطاع بقيت على هذا الجهاز';
      setTimeout(() => (netToast = ''), 12000);
    } else if (t0 === 'offline_session_rejected') {
      netToast = '⚠️ Server is back but rejected your saved credentials — please log in again on this terminal. / عاد الخادم لكنه رفض بيانات الحساب — أعد تسجيل الدخول';
      setTimeout(() => (netToast = ''), 12000);
    }
  }

  let sidebarVersion = '';
  onMount(async () => {
    loadTelegramMaster();
    try {
      sidebarVersion = await invoke<string>('get_app_version');
    } catch {
      sidebarVersion = '';
    }
    // Disable right-click context menu across Tauri POS desktop app
    window.addEventListener('contextmenu', (e) => e.preventDefault());

    // F5 must NEVER reload the webview (it wiped the session = looked like
    // a logout). The POS binds its own F5 action per the shortcuts scheme.
    window.addEventListener('keydown', (e) => {
      if (e.key === 'F5' || (e.ctrlKey && e.key === 'r')) {
        e.preventDefault();
      }
    });

    // First-launch setup wizard: shows once until completed.
    try {
      const setup = await invoke<string | null>('get_setting', { key: 'first_setup_completed' });
      if (!setup) {
        showFirstSetup = true;
      }
    } catch {
      // Missing setting = never ran: show the wizard.
      showFirstSetup = true;
    }

    // Auto-select text on input focus for fast barcode/number replacement.
    // Fields marked data-no-autoselect (e.g. invoice numbers the user edits
    // mid-string) are skipped so the caret never jumps.
    window.addEventListener('focusin', (e) => {
      const target = e.target as HTMLElement;
      if (target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA')) {
        if (target.hasAttribute('data-no-autoselect')) return;
        const input = target as HTMLInputElement;
        if (input.type === 'number' || input.type === 'text') {
          setTimeout(() => input.select(), 10);
        }
      }
    });

    // Background automatic update check on startup
    setTimeout(async () => {
      try {
        const update = await invoke<any>('check_github_update');
        if (update && update.has_update) {
          newUpdateAvailable = true;
          updateTag = update.tag_name;
        }
      } catch (e) {
        console.warn('Auto update check note:', e);
      }
    }, 1500);

    // Recurring Telegram recap scheduler: polls the interval setting every
    // 5 minutes and sends the recap when the window has elapsed.
    setInterval(async () => {
      try {
        const s = await invoke<Record<string, string>>('get_all_settings');
        if (s['notify_recap_enabled'] !== 'true') return;
        const intervalMin = parseInt(s['recap_interval_minutes'] || '60', 10);
        const lastStr = s['last_recap_at'] || '';
        let due = true;
        if (lastStr) {
          const last = new Date(lastStr.replace(' ', 'T'));
          if (!isNaN(last.getTime())) {
            due = Date.now() - last.getTime() >= intervalMin * 60 * 1000;
          }
        }
        if (due) {
          await invoke('send_telegram_recap');
        }
      } catch (e) {
        // Silent: scheduling must never disturb the cashier.
      }
    }, 5 * 60 * 1000);

    // Employee payroll reminder scheduler (Req 26): one scan per minute is
    // cheap (a settings read + a tiny query); reminders dedup server-side so
    // repeated scans never duplicate notifications.
    setInterval(async () => {
      try {
        await invoke('run_payroll_reminder_scan');
      } catch {
        // Silent — reminders are best-effort.
      }
    }, 60 * 1000);
    // Also fire one scan shortly after login/startup so the day's
    // reminders appear immediately, not at the next minute boundary.
    setTimeout(async () => {
      try {
        await invoke('run_payroll_reminder_scan');
      } catch { /* silent */ }
    }, 4000);

    // Scheduled daily backup ("every day at HH:MM"): the tick just pings the
    // backend, which decides if the window arrived and stamps the day so it
    // runs at most once per day (calling create_backup here directly would
    // have backed up on every 60s tick).
    setInterval(async () => {
      try {
        await invoke('run_scheduled_backup');
      } catch {
        // Silent — backups must never disturb the cashier.
      }
    }, 60 * 1000);

    // Backup-on-close: safety copy right before the app unloads. The POS
    // single-instance plugin means closing the window ends the session.
    window.addEventListener('beforeunload', () => {
      try {
        // Best-effort: an async invoke may not finish before the window
        // dies, but a copy of a small SQLite file typically lands in time.
        invoke('get_all_settings').then((s) => {
          const map = s as Record<string, string>;
          if (map['backup_on_close'] === 'true') {
            invoke('create_backup', { tag: 'exit' }).catch(() => {});
          }
        }).catch(() => {});
      } catch {
        // Never block the close.
      }
    });
  });

  function toggleTheme() {
    isDarkMode = !isDarkMode;
    if (isDarkMode) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
  }

  function handleLogout() {
    adminSettingsUnlocked = false;
    logout();
  }
</script>

{#if netToast}
  <div class="fixed top-4 left-1/2 -translate-x-1/2 z-[95] max-w-xl w-[92%] px-4 py-3 rounded-2xl border shadow-2xl text-xs font-bold
    {netToast.startsWith('✅') ? 'bg-emerald-50 dark:bg-emerald-950/80 border-emerald-300 dark:border-emerald-700 text-emerald-800 dark:text-emerald-200' : 'bg-amber-50 dark:bg-amber-950/80 border-amber-300 dark:border-amber-700 text-amber-800 dark:text-amber-200'}">
    {netToast}
  </div>
{/if}

{#if showFirstSetup}
  <!-- First-run wizard shows BEFORE login so a fresh install configures the
       shop before any credentials are involved. -->
  <FirstSetupWizard onDone={() => (showFirstSetup = false)} />
{/if}

{#if !$isAuthenticated}
  <LoginView />
{:else}
  <div class="flex h-screen w-screen overflow-hidden bg-pos-bg text-pos-text">
    <!-- SIDEBAR -->
    <aside class="w-64 bg-pos-card border-e border-pos-border flex flex-col justify-between shrink-0 select-none shadow-xs">
      <!-- Top Brand Header -->
      <div class="p-3.5 border-b border-pos-border/60">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-xl overflow-hidden shadow-md shrink-0 bg-white flex items-center justify-center border border-pos-border/50">
            <img src="/logo.png" alt="TitaouPosCRM" class="w-full h-full object-contain p-0.5" />
          </div>
          <div class="min-w-0">
            <h1 class="font-black text-sm tracking-tight text-pos-text">TitaouPosCRM</h1>
            <p class="text-[9px] text-sky-600 font-bold truncate">Titaou Bedreddine 0553444057</p>
            <p class="text-[8px] text-pos-muted font-mono">{sidebarVersion ? 'v' + sidebarVersion : ''}</p>
          </div>
        </div>
      </div>

      <!-- Navigation Menu Links -->
      <nav class="flex-1 overflow-y-auto p-2.5 space-y-1">
        <button
          type="button"
          on:click={() => currentRoute = 'pos'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'pos' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <ShoppingCart class="w-4 h-4" />
          <span>{t('nav_pos', $currentLocale)}</span>
        </button>

        <button
          type="button"
          on:click={() => currentRoute = 'sales'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'sales' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <Receipt class="w-4 h-4" />
          <span>{t('nav_sales', $currentLocale)}</span>
        </button>

        {#if routeAllowed('cash', $currentUser)}
        <button
          type="button"
          on:click={() => currentRoute = 'cash'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'cash' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <DollarSign class="w-4 h-4 text-emerald-500" />
          <span>{t('nav_cash', $currentLocale)}</span>
        </button>
        {/if}

        {#if routeAllowed('purchases', $currentUser)}
        <button
          type="button"
          on:click={() => currentRoute = 'purchases'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'purchases' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <FileSpreadsheet class="w-4 h-4" />
          <span>{t('nav_purchases', $currentLocale)}</span>
        </button>
        {/if}

        {#if isAdmin($currentUser)}
        <button
          type="button"
          on:click={() => currentRoute = 'fieldorders'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'fieldorders' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <Truck class="w-4 h-4" />
          <span>{t('nav_fieldorders', $currentLocale)}</span>
        </button>
        <button
          type="button"
          on:click={() => currentRoute = 'truckloads'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'truckloads' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <FileSpreadsheet class="w-4 h-4 text-amber-500" />
          <span>{t('nav_truckloads', $currentLocale)}</span>
        </button>
        <button
          type="button"
          on:click={() => currentRoute = 'team'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'team' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <UsersRound class="w-4 h-4" />
          <span>{t('nav_team', $currentLocale)}</span>
        </button>
        <button
          type="button"
          on:click={() => currentRoute = 'deletionrequests'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'deletionrequests' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <FileWarning class="w-4 h-4 text-amber-500" />
          <span>{t('nav_deletionrequests', $currentLocale)}</span>
        </button>
        {/if}

        <button
          type="button"
          on:click={() => currentRoute = 'customers'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'customers' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <Users class="w-4 h-4" />
          <span>{t('nav_customers', $currentLocale)}</span>
        </button>

        {#if routeAllowed('suppliers', $currentUser)}
        <button
          type="button"
          on:click={() => currentRoute = 'suppliers'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'suppliers' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <Truck class="w-4 h-4" />
          <span>{t('nav_suppliers', $currentLocale)}</span>
        </button>
        {/if}

        {#if routeAllowed('inventory', $currentUser)}
        <button
          type="button"
          on:click={() => currentRoute = 'inventory'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'inventory' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <Package class="w-4 h-4" />
          <span>{t('nav_inventory', $currentLocale)}</span>
        </button>
        {/if}

        <button
          type="button"
          on:click={() => currentRoute = 'expenses'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'expenses' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <TrendingDown class="w-4 h-4" />
          <span>{t('nav_expenses', $currentLocale)}</span>
        </button>

        {#if routeAllowed('payroll', $currentUser)}
        <button
          type="button"
          on:click={() => currentRoute = 'payroll'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'payroll' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <UserCheck class="w-4 h-4" />
          <span>{t('nav_payroll', $currentLocale)}</span>
        </button>
        {/if}

        {#if routeAllowed('dashboard', $currentUser)}
        <button
          type="button"
          on:click={() => currentRoute = 'dashboard'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'dashboard' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <LayoutDashboard class="w-4 h-4" />
          <span>{t('nav_dashboard', $currentLocale)}</span>
        </button>
        {/if}

        {#if routeAllowed('notifications', $currentUser)}
        <button
          type="button"
          on:click={() => currentRoute = 'notifications'}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'notifications' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <Bell class="w-4 h-4" />
          <span>{t('nav_notifications')}</span>
        </button>
        {/if}

        {#if settingsAllowed($currentUser)}
        <button
          type="button"
          on:click={handleNavigateSettings}
          class="w-full flex items-center gap-3 px-3.5 py-2.5 rounded-xl text-xs font-bold transition cursor-pointer {currentRoute === 'settings' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-pos-text'}"
        >
          <Settings class="w-4 h-4" />
          <span>{t('nav_settings', $currentLocale)}</span>
          {#if !isAdmin($currentUser) && !adminSettingsUnlocked}
            <Lock class="w-3.5 h-3.5 ms-auto text-pos-muted/60" />
          {/if}
        </button>
        {/if}
      </nav>

      <!-- Bottom User Profile & Network Status -->
      <div class="p-3 border-t border-pos-border/60 bg-slate-50 dark:bg-slate-800/40 space-y-2">
        <!-- LAN shop network status indicator (click = details popup) -->
        <NetworkStatusIndicator />

        <!-- Quick Open Cash Drawer Button -->
        <button
          type="button"
          on:click={async () => {
            drawerMsg = '';
            try {
              const s = await invoke<Record<string, string>>('get_all_settings');
              const res = await invoke<string>('open_serial_cash_drawer', {
                comPort: parseInt(s['drawer_com_port'] || '1', 10) || 1,
                baudRate: parseInt(s['drawer_baud_rate'] || '9600', 10) || 9600,
              });
              drawerMsg = '✅ ' + res;
            } catch (e: any) {
              // Visible failure — the sidebar kick used to die silently.
              drawerMsg = '❌ ' + (typeof e === 'string' ? e : e?.message || String(e));
              console.warn('Drawer kick failed:', e);
            }
            setTimeout(() => (drawerMsg = ''), 2200);
          }}
          class="w-full py-2 bg-emerald-600 hover:bg-emerald-700 text-white font-bold text-xs rounded-xl shadow-xs flex items-center justify-center gap-2 cursor-pointer transition"
          title="Open Cash Drawer (F10)"
        >
          <CreditCard class="w-4 h-4" />
          <span>{t('btn_drawer')}</span>
        </button>
        {#if drawerMsg}
          <p class="text-[8px] font-bold px-1 truncate max-w-[230px] {drawerMsg.startsWith('✅') ? 'text-emerald-600' : 'text-rose-600'}" title={drawerMsg}>{drawerMsg}</p>
        {/if}

        <!-- Clean User Card Horizontal with Compact Language Selector -->
        <div class="p-2 bg-pos-card rounded-xl border border-pos-border shadow-xs space-y-2">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-2 min-w-0">
              <div class="w-7 h-7 rounded-lg bg-sky-100 dark:bg-sky-950 text-sky-600 dark:text-sky-300 flex items-center justify-center font-bold text-xs shrink-0">
                {$currentUser?.display_name?.charAt(0) || 'U'}
              </div>
              <div class="min-w-0">
                <p class="text-xs font-black text-pos-text truncate">{$currentUser?.display_name || 'Admin'}</p>
                <p class="text-[9px] text-pos-muted capitalize truncate">{$currentUser?.role_name || 'Administrator'}</p>
              </div>
            </div>

            <div class="flex items-center gap-1 shrink-0">
              <button
                type="button"
                on:click={() => askTelegramToggle(!telegramMasterOn)}
                class="p-1 rounded-lg cursor-pointer transition {telegramMasterOn ? 'text-sky-500 hover:text-sky-600' : 'text-pos-muted hover:text-pos-text'}"
                title={telegramMasterOn ? t('tg_quick_toggle') + ' — ON' : t('tg_quick_toggle') + ' — OFF'}
              >
                <Bell class="w-3.5 h-3.5 {telegramMasterOn ? '' : 'opacity-40 line-through'}" />
              </button>
              <button
                type="button"
                on:click={toggleTheme}
                class="p-1 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer transition"
                title="Toggle Theme"
              >
                {#if isDarkMode}
                  <Sun class="w-3.5 h-3.5 text-amber-400" />
                {:else}
                  <Moon class="w-3.5 h-3.5" />
                {/if}
              </button>
              <button
                type="button"
                on:click={handleLogout}
                class="p-1 text-rose-500 hover:text-rose-700 rounded-lg cursor-pointer transition"
                title="Sign Out"
              >
                <LogOut class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          <!-- Compact Mini Language Toggle -->
          <div class="flex items-center justify-between gap-1 pt-1.5 border-t border-pos-border/40 text-[10px]">
            <span></span>
            <div class="flex items-center gap-1 bg-slate-100 dark:bg-slate-800/80 p-0.5 rounded-lg">
              <button
                type="button"
                on:click={() => setLocale('ar')}
                class="px-1.5 py-0.5 rounded text-[10px] font-bold transition cursor-pointer {$currentLocale === 'ar' ? 'bg-sky-600 text-white font-black' : 'text-pos-muted hover:text-pos-text'}"
              >
                عربي
              </button>
              <button
                type="button"
                on:click={() => setLocale('fr')}
                class="px-1.5 py-0.5 rounded text-[10px] font-bold transition cursor-pointer {$currentLocale === 'fr' ? 'bg-sky-600 text-white font-black' : 'text-pos-muted hover:text-pos-text'}"
              >
                FR
              </button>
              <button
                type="button"
                on:click={() => setLocale('en')}
                class="px-1.5 py-0.5 rounded text-[10px] font-bold transition cursor-pointer {$currentLocale === 'en' ? 'bg-sky-600 text-white font-black' : 'text-pos-muted hover:text-pos-text'}"
              >
                EN
              </button>
            </div>
          </div>
        </div>

        <!-- Developer Credit Horizontal Bar -->
        <div class="px-2 py-1 bg-white dark:bg-slate-900 rounded-lg text-[9px] text-pos-muted text-center border border-pos-border/40 truncate">
          Created by <span class="font-bold text-sky-600">Titaou Bedreddine</span> (0553444057)
        </div>
      </div>
    </aside>

    <!-- MAIN ROUTE CONTENT -->
    <main class="flex-1 flex flex-col overflow-hidden bg-pos-bg">
      {#if newUpdateAvailable}
        <div class="bg-gradient-to-r from-sky-600 to-indigo-600 text-white px-4 py-2 text-xs font-bold flex items-center justify-between shadow-md shrink-0 animate-in slide-in-from-top duration-200">
          <div class="flex items-center gap-2">
            <span class="px-2 py-0.5 bg-white/20 rounded-md font-black">🚀 New Update</span>
            {#if updateStatus === 'downloading'}
              <span>Downloading {updateTag}… {updateProgress}%</span>
            {:else if updateStatus === 'restarting'}
              <span>Installing update — the app will restart automatically…</span>
            {:else if updateStatus === 'error'}
              <span class="text-rose-200">Update failed: {updateError}</span>
            {:else}
              <span>A new version ({updateTag}) is available! / يتوفر إصدار جديد من البرنامج</span>
            {/if}
          </div>
          <div class="flex items-center gap-2">
            {#if updateStatus === 'downloading'}
              <div class="w-40 h-2 bg-white/25 rounded-full overflow-hidden">
                <div class="h-full bg-white transition-all" style="width: {updateProgress}%"></div>
              </div>
            {:else if updateStatus === 'error'}
              <button
                type="button"
                on:click={installUpdate}
                class="px-3 py-1 bg-white text-sky-700 hover:bg-slate-100 rounded-lg text-xs font-black shadow-xs cursor-pointer transition"
              >
                Retry
              </button>
              <button on:click={() => { newUpdateAvailable = false; updateStatus = ''; }} class="p-1 hover:bg-white/20 rounded cursor-pointer font-mono text-xs">
                ✕
              </button>
            {:else if updateStatus === 'restarting'}
              <span class="animate-pulse">⏳</span>
            {:else}
              <button
                type="button"
                on:click={installUpdate}
                class="px-3 py-1 bg-white text-sky-700 hover:bg-slate-100 rounded-lg text-xs font-black shadow-xs cursor-pointer transition flex items-center gap-1"
              >
                <span>Update Now ({updateTag})</span>
              </button>
              <button on:click={() => (newUpdateAvailable = false)} class="p-1 hover:bg-white/20 rounded cursor-pointer font-mono text-xs">
                ✕
              </button>
            {/if}
          </div>
        </div>
      {/if}

      <div class="flex-1 overflow-hidden">
        {#if currentRoute === 'pos'}
          <PosView onNavigate={(r) => (currentRoute = r)} initialOpenProductId={posOpenProductId} onProductOpened={() => (posOpenProductId = null)} onOpenPurchase={(inv) => { purchasesFocusInvoice = inv; currentRoute = 'purchases'; }} />
        {:else if currentRoute === 'sales'}
          <SalesView onRequestPosRoute={() => (currentRoute = 'pos')} />
        {:else if currentRoute === 'cash'}
          <CashRegisterView />
        {:else if currentRoute === 'purchases'}
          <PurchasesView focusInvoice={purchasesFocusInvoice} />
        {:else if currentRoute === 'fieldorders'}
          <FieldOrdersView />
        {:else if currentRoute === 'team'}
          <TeamView />
        {:else if currentRoute === 'truckloads'}
          <TruckLoadsView />
        {:else if currentRoute === 'deletionrequests'}
          <DeletionRequestsView />
        {:else if currentRoute === 'customers'}
        <CustomersView />
      {:else if currentRoute === 'suppliers'}
        <SuppliersView />
      {:else if currentRoute === 'inventory'}
        <InventoryView />
      {:else if currentRoute === 'expenses'}
        <ExpensesView />
      {:else if currentRoute === 'payroll'}
        <PayrollView />
      {:else if currentRoute === 'dashboard'}
        <DashboardView
          onEditSaleInPos={async (sale) => {
            // Load the versement's items into the POS cart to complete the
            // payment (sell) or cancel it (delete on checkout).
            try {
              const items = await invoke<any[]>('get_sale_items', { saleId: sale.id });
              const mapped = items.map((i) => ({
                product_id: i.product_id,
                sku: i.sku || '',
                barcode: i.barcode || '',
                name_ar: i.name_ar || '',
                name_fr: i.name_fr || '',
                name_en: i.name_en || '',
                image_path: i.image_path,
                unit_price: i.unit_price,
                quantity: i.quantity,
                discount_amount: i.discount_amount || 0,
                tax_amount: i.tax_amount || 0,
                total_price: i.total_price,
                is_refund: i.is_refund || false,
              }));
              const { clearCart, cartItems, posMode } = await import('./lib/stores/cart');
              clearCart();
              cartItems.set(mapped);
              posMode.set('sale');
              currentRoute = 'pos';
            } catch (e) {
              console.error('Versement load failed:', e);
            }
          }}
        />
      {:else if currentRoute === 'notifications'}
        <NotificationsView
          onRequestRoute={(r) => (currentRoute = r)}
          onOpenProduct={(productId) => {
            // Product alerts: jump to the POS and open that product's editor.
            currentRoute = 'pos';
            posOpenProductId = productId;
          }}
        />
      {:else if currentRoute === 'settings'}
        <SettingsView />
      {/if}
      </div>
    </main>
  </div>

  <!-- Stale-session prompt: close yesterday's session, then open today's -->
  {#if $isAuthenticated}
    <CashDrawerModal isOpen={isCashDrawerOpen} onClose={() => (isCashDrawerOpen = false)} />

    {#if isDeveloperPopupOpen}
      <div class="fixed inset-0 z-[70] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
        <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl w-full max-w-sm p-6 space-y-4">
          <h3 class="font-black text-sm text-pos-text">Contact the Developer (المطور)</h3>
          <p class="text-xs text-pos-muted">
            Titaou Bedreddine — TitaouPosCRM developer. Reach out for support, features or licensing.
          </p>
          <div class="grid grid-cols-2 gap-2">
            <button
              type="button"
              on:click={() => window.open('https://wa.me/213553444057', '_blank')}
              class="px-3 py-2 bg-emerald-50 dark:bg-emerald-950/40 hover:bg-emerald-100 text-emerald-700 dark:text-emerald-300 text-xs font-bold rounded-xl border border-emerald-200 dark:border-emerald-800 cursor-pointer"
            >
              WhatsApp
            </button>
            <button
              type="button"
              on:click={() => window.open('https://t.me/titaou_bedreddine', '_blank')}
              class="px-3 py-2 bg-sky-50 dark:bg-sky-950/40 hover:bg-sky-100 text-sky-700 dark:text-sky-300 text-xs font-bold rounded-xl border border-sky-200 dark:border-sky-800 cursor-pointer"
            >
              Telegram
            </button>
            <button
              type="button"
              on:click={() => window.open('tel:0553444057', '_blank')}
              class="px-3 py-2 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 text-pos-text text-xs font-bold rounded-xl border border-pos-border cursor-pointer"
            >
              Call 0553444057
            </button>
            <button
              type="button"
              on:click={() => window.open('https://afaqtech.netlify.app/', '_blank')}
              class="px-3 py-2 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 text-pos-text text-xs font-bold rounded-xl border border-pos-border cursor-pointer"
            >
              Website
            </button>
          </div>
          <button on:click={() => (isDeveloperPopupOpen = false)} class="w-full px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-bold rounded-xl cursor-pointer">
            Close
          </button>
        </div>
      </div>
    {/if}
  {/if}
{/if}

  <!-- Telegram toggle admin-password prompt -->
  {#if showTelegramPinModal}
    <div class="fixed inset-0 z-[80] bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-xs w-full space-y-3 animate-in zoom-in-95 duration-150">
        <h3 class="font-black text-sm text-pos-text">{t('tg_master_password')}</h3>
        <p class="text-[11px] font-bold text-pos-muted">
          {telegramMasterOn ? 'Disable' : 'Enable'} Telegram notifications?
        </p>
        <input
          type="password"
          bind:value={telegramPinInput}
          on:keydown={(e) => e.key === 'Enter' && confirmTelegramToggle()}
          placeholder={t('tg_master_password')}
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-sm font-mono font-bold text-pos-text outline-none"
        />
        {#if telegramPinError}
          <p class="text-[11px] font-bold text-rose-500">{telegramPinError}</p>
        {/if}
        <div class="flex justify-end gap-2 pt-1">
          <button on:click={() => (showTelegramPinModal = false)} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
          <button on:click={confirmTelegramToggle} class="px-4 py-1.5 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl cursor-pointer">Confirm</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Admin Auth Modal for Settings -->
  {#if showAdminAuthModal}
    <div class="fixed inset-0 z-[9999] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95">
        <div class="flex items-center gap-3 text-sky-600">
          <Lock class="w-6 h-6 shrink-0" />
          <h3 class="font-black text-sm text-pos-text">Admin Access Required / تأكيد صلاحيات المسؤول</h3>
        </div>
        <p class="text-xs text-pos-muted">
          Settings are restricted to administrators. Enter the Admin password to proceed.
        </p>

        <div class="space-y-1">
          <label class="block text-xs font-bold text-pos-muted">Admin Password / كلمة المرور</label>
          <input
            type="password"
            bind:value={adminAuthPassword}
            placeholder="••••••••"
            autofocus
            on:keydown={(e) => { if (e.key === 'Enter') confirmAdminAuth(); }}
            class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
          />
          {#if adminAuthError}
            <p class="text-[11px] font-bold text-rose-500">{adminAuthError}</p>
          {/if}
        </div>

        <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
          <button on:click={() => (showAdminAuthModal = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
            Cancel / إلغاء
          </button>
          <button
            on:click={confirmAdminAuth}
            class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl cursor-pointer shadow-md transition"
          >
            Unlock Settings / فتح الإعدادات
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Stock 0 / Negative Stock Warning Modal -->
  {#if $stockWarningModal}
    <div class="fixed inset-0 z-[9999] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95">
        <div class="flex items-center gap-3 text-rose-500">
          <div class="w-10 h-10 rounded-2xl bg-rose-100 dark:bg-rose-950/60 flex items-center justify-center shrink-0">
            <AlertTriangle class="w-5 h-5 text-rose-600" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text">Stock Insuffisant / نفاد المخزون</h3>
            <p class="text-[11px] text-pos-muted">Out of Stock Warning</p>
          </div>
        </div>

        <div class="bg-slate-50 dark:bg-slate-800/60 border border-pos-border rounded-2xl p-4 space-y-2">
          <p class="font-black text-sm text-pos-text">{$stockWarningModal.productName}</p>
          <div class="flex items-center justify-between text-xs pt-1 border-t border-pos-border/40">
            <span class="text-pos-muted">Disponible / المتاح:</span>
            <span class="font-mono font-black {$stockWarningModal.available <= 0 ? 'text-rose-600' : 'text-amber-600'}">
              {$stockWarningModal.available} pcs
            </span>
          </div>
          <div class="flex items-center justify-between text-xs">
            <span class="text-pos-muted">Requis / المطلوب:</span>
            <span class="font-mono font-black text-sky-600">
              {$stockWarningModal.requested} pcs
            </span>
          </div>
        </div>

        <p class="text-[11px] text-pos-muted text-center leading-relaxed">
          La vente en stock négatif est désactivée dans les paramètres.<br/>
          تم تعطيل البيع في حالة نفاد المخزون في الإعدادات.
        </p>

        <div class="pt-2 border-t border-pos-border">
          <button
            type="button"
            autofocus
            on:click={() => stockWarningModal.set(null)}
            on:keydown={(e) => { if (e.key === 'Enter' || e.key === 'Escape') stockWarningModal.set(null); }}
            class="w-full py-2.5 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl cursor-pointer shadow-md transition active:scale-95 text-center"
          >
            Compris / حسناً / Understand
          </button>
        </div>
      </div>
    </div>
  {/if}
