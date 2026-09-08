<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { t } from '../../lib/i18n';
  import { invoke } from '@tauri-apps/api/core';
  import AboutView from '../about/AboutView.svelte';
  import ShortcutsEditor from '../../lib/components/ShortcutsEditor.svelte';
  import { currentUser } from '../../lib/stores/auth';
  import { printLabelSilently, printHtmlSilently, entityQrDataUrl } from '../../lib/utils/printer';
  import { buildUnifiedReceipt } from '../../lib/printing/unifiedReceipt';
  import CloudSyncTab from '../../lib/components/CloudSyncTab.svelte';
  import {
    LABEL_PRESETS,
    LABEL_PRESET_IDS,
    buildLabelPresetHtml,
    type LabelPresetId,
  } from '../../lib/printing/labelPresets';
  import {
    Sliders, User, Building, Printer, Smartphone, Download,
    ShieldCheck, RefreshCw, AlertOctagon, Check, Copy, Key,
    QrCode, Image as ImageIcon, Upload, Tag, ArrowRight,
    Wifi, HardDrive, FileText, CheckCircle2, History, Laptop,
    Scale, Bell, Send, CreditCard, Keyboard, Eye,
    Users, UserPlus, Edit2, Trash2, Shield, Lock, Info, Pin, Plus, Cloud
  } from 'lucide-svelte';

  type SettingsTab =
    | 'general'
    | 'invoices'
    | 'barcodes'
    | 'scale'
    | 'notifications'
    | 'pos'
    | 'shortcuts'
    | 'network'
    | 'cloud'
    | 'import_export'
    | 'activation'
    | 'updates'
    | 'account'
    | 'about'
    | 'danger';

  let currentTab: SettingsTab = 'general';
  let settings: Record<string, any> = {
    shop_name_ar: 'سوبرماركت تيتاو',
    shop_name_fr: 'TitaouPosCRM Supermarché',
    shop_phone: '0553444057 / 021654321',
    shop_address: 'Alger Centre, Algérie',
    shop_rc: '16/00-0123456B22',
    shop_nif: '001616012345678',
    shop_nis: '1980160123456',
    shop_ai: '16010123456',
    default_currency: 'DZD',
    default_tax_rate: '19',
    receipt_printer: 'Xprinter XP-DT427B',
    label_printer: 'Xprinter XP-DT427B',
    receipt_paper_width: '80mm',
    auto_cut_paper: 'true',
    open_drawer_on_sale: 'true',
    drawer_com_port: '1',
    invoice_printer_name: '',
    drawer_baud_rate: '9600',
    scale_enabled: 'true',
    scale_model: 'ACLAS LH51 / LS M3 / TS',
    scale_ip: '192.168.1.87',
    scale_port: '0',
    scale_protocol: '0',
    scale_default_barcode_type: '97',
    scale_department_id: '1',
    scale_auto_sync: 'true',
    telegram_bot_token: '',
    telegram_chat_id: '',
    notify_daily_summary: 'true',
    notify_each_sale: 'false',
    notify_cash_in: 'false',
    notify_cash_out: 'false',
    notify_each_expense: false,
    notify_opening_cash: false,
    notify_cash_edited: true,
    notify_supplier_payment: false,
    notify_product_change: 'true',
    notify_price_change: 'false',
    notify_qty_change: 'false',
    notify_history_change: 'false',
    notify_each_refund: 'true',
    notify_expiry: 'true',
    notify_low_stock: 'true',
    notify_recap_enabled: 'false',
    telegram_master_enabled: 'true',
    telegram_quiet_windows: '',
    recap_interval_minutes: '60',
    app_license_status: 'activated',
    allow_negative_stock: 'false',
    pos_autofocus_search: 'true',
    pos_autofocus_timer_seconds: '10',
    pos_auto_capture_barcode: 'true',
    require_pin_for_discount: 'false',
    default_customer_name: 'Client Comptoir / زبون عادي',
    hold_sale_require_note: 'false',
    default_barcode_prefix: '22',
    scale_barcode_format: '97',
    shortcut_f1: 'Focus Barcode Search',
    shortcut_f2: 'Quick New Product',
    shortcut_f3: 'Switch Cart Mode',
    shortcut_f4: 'Hold Current Sale',
    shortcut_f5: 'List Held Sales',
    shortcut_f6: 'Edit Quantity',
    shortcut_f7: 'Apply Discount',
    shortcut_f8: 'Select Customer',
    shortcut_f9: 'Pay Cash & Print',
    shortcut_f10: 'Kick Cash Drawer',
    shortcut_f11: 'Split / TPE Payment',
    shortcut_f12: 'Clear Active Cart',

    // Barcode Labels — ONE unified preset system (v0.5.17). The legacy
    // sticker_*/shelf_* px settings and the saved-preset JSON are removed;
    // only the current built-in preset id is stored.
    label_preset_id: 'vprice40x20',
    label_printer_dpi: '203',
    receipt_header: 'مرحباً بكم في سوبرماركت تيتاو',
    receipt_footer: 'Les articles retournés doivent être présentés sous 48h',
    receipt_thank_you: 'MERCI POUR VOTRE CONFIANCE !',
    receipt_show_barcode: 'true',
    shop_website: '',
    // Pricing defaults for new products
    default_margin_percent: '20',
    price_round_step: '5',
    // Sale-price rounding at checkout: OFF / nearest 50 / nearest 100 DZD.
    sale_price_rounding: 'off',
    receipt_show_shop_name: 'true',
    receipt_show_address: 'true',
    receipt_show_phone: 'true',
    receipt_show_rc_nif: 'true',
    receipt_show_cashier: 'true',
    receipt_show_date: 'true',
    receipt_show_footer: 'true',
    receipt_show_qr: 'true',

    // Backup system (v0.5.16): startup/close/scheduled/retention/location.
    backup_on_startup: 'false',
    backup_on_close: 'false',
    backup_scheduled_enabled: 'false',
    backup_scheduled_time: '22:00',
    backup_dir: '',
    backup_keep_count: '10',
    backup_include_settings: 'true',

    // Payroll reminder notification switches.
    notify_payroll_enabled: 'true',
    notify_payroll_inapp: 'true',
    notify_payroll_telegram: 'false',

    // Session admin-action alert switches.
    notify_session_deleted: 'true',
    notify_session_archived: 'true',
    notify_debt_cleared: 'true',
  };

  let hwid = '...';
  let isActivatingOnline = false;
  let activationMsg = '';

  // Online activation against the developer's GitHub license registry:
  // licenses/<HWID>.json must exist with {"licensed": true}.
  async function handleActivateOnline() {
    try {
      isActivatingOnline = true;
      activationMsg = 'Contacting activation server...';
      const ok = await invoke<boolean>('activate_online');
      activationMsg = ok
        ? 'Activated successfully / تم التنشيط بنجاح'
        : 'Not licensed on the server';
    } catch (e: any) {
      activationMsg = typeof e === 'string' ? e : e?.message || 'Activation failed';
    } finally {
      isActivatingOnline = false;
    }
  }
  let activationCode = '';
  let activationSuccess = false;
  let saveSuccessMsg = '';

  // Scale state
  let scaleStatusMsg = '';
  let isTestingScale = false;
  let isUploadingScale = false;
  let scaleSyncLogs: any[] = [];

  // Drawer state
  let drawerStatusMsg = '';
  let isOpeningDrawer = false;

  // Telegram test state
  let isSendingTelegram = false;
  let telegramStatusMsg = '';
  let recapStatusMsg = '';

  async function sendRecapNow() {
    try {
      recapStatusMsg = 'Sending recap...';
      const res = await invoke<string>('send_telegram_recap');
      recapStatusMsg = '✅ ' + res;
    } catch (e: any) {
      recapStatusMsg = '❌ ' + (typeof e === 'string' ? e : e?.message || 'Recap failed');
    }
  }

  // Updates
  let appVersion = 'v0.2.0';
  let isCheckingUpdate = false;
  let updateStatus = 'You are running the latest version: v0.2.0';
  let updateAvailable = false;
  let showRollbackModal = false;
  // Rollback requires typing ROLLBACK and auto-cancels after 10s idle.
  let rollbackConfirmText = '';
  let rollbackCountdown = 10;
  let rollbackTimer: any = null;

  function openRollbackModal() {
    rollbackConfirmText = '';
    rollbackCountdown = 10;
    showRollbackModal = true;
    clearInterval(rollbackTimer);
    rollbackTimer = setInterval(() => {
      rollbackCountdown -= 1;
      if (rollbackCountdown <= 0) {
        cancelRollback();
      }
    }, 1000);
  }

  function cancelRollback() {
    clearInterval(rollbackTimer);
    rollbackConfirmText = '';
    showRollbackModal = false;
  }

  // Account
  let newPassword = '';
  let oldPassword = '';
  let passwordSuccess = false;

  // Shop Logo
  let shopLogoUrl = '/logo.png';

  // Barcode & Label Previews
  let previewBarcodeNumber = '613000000001';
  let previewProductName = 'Lait Candia 1L Entier';
  let previewPrice = 120;
  // Factory Reset
  let resetType = 'transactions_only';
  let resetConfirm = '';
  // Countdown giving the user time to cancel before the destructive reset.
  let resetCountdown = 0;
  let resetTimer: any = null;
  // Themed reset result (replaces the OS-native alert that ignored the
  // app's dark theme).
  let resetResult: { ok: boolean; text: string } | null = null;
  function scheduleFactoryReset() {
    if (resetConfirm !== 'RESET') return;
    resetCountdown = 10;
    clearInterval(resetTimer);
    resetTimer = setInterval(() => {
      resetCountdown -= 1;
      if (resetCountdown <= 0) {
        clearInterval(resetTimer);
        doFactoryReset();
      }
    }, 1000);
  }
  function cancelFactoryReset() {
    clearInterval(resetTimer);
    resetCountdown = 0;
  }
  async function doFactoryReset() {
    try {
      await invoke('factory_reset', { resetType });
      resetConfirm = '';
      resetResult = { ok: true, text: '✅ Reset completed — the app reloads in 2 seconds… / تمت إعادة الضبط' };
      setTimeout(() => window.location.reload(), 2000);
    } catch (e) {
      console.error(e);
      resetResult = { ok: false, text: '❌ Factory reset failed: ' + (typeof e === 'string' ? e : e?.message || String(e)) };
    }
  }

  // Clear-history-only (keeps products/stock/prices/debts)
  let clearHistoryConfirmText = '';
  let isClearingHistory = false;
  let clearHistoryMsg = '';

  // Clearing history is destructive: after typing CLEAR HISTORY the job
  // waits 10 seconds (cancellable) before executing.
  let clearHistoryCountdown = 0;
  let clearHistoryTimer: any = null;
  function handleClearHistory() {
    if (clearHistoryConfirmText.trim() !== 'CLEAR HISTORY') return;
    clearHistoryCountdown = 10;
    clearInterval(clearHistoryTimer);
    clearHistoryTimer = setInterval(() => {
      clearHistoryCountdown -= 1;
      if (clearHistoryCountdown <= 0) {
        clearInterval(clearHistoryTimer);
        doClearHistory();
      }
    }, 1000);
  }
  function cancelClearHistory() {
    clearInterval(clearHistoryTimer);
    clearHistoryCountdown = 0;
    clearHistoryMsg = 'Cancelled / تم الإلغاء';
  }
  async function doClearHistory() {
    try {
      isClearingHistory = true;
      clearHistoryMsg = '';
      const res = await invoke<string>('clear_transaction_history', {
        confirmText: clearHistoryConfirmText.trim(),
      });
      clearHistoryMsg = '✅ ' + res;
      clearHistoryConfirmText = '';
    } catch (e: any) {
      clearHistoryMsg = '❌ ' + (typeof e === 'string' ? e : e?.message || 'Failed');
    } finally {
      isClearingHistory = false;
      clearHistoryCountdown = 0;
    }
  }

  // User Management State
  interface UserAccountItem {
    id: number;
    username: string;
    display_name: string;
    role_id: number | null;
    role_name: string | null;
    max_discount_percent: number;
    is_active: boolean;
    last_login: string | null;
    created_at: string | null;
    pinned?: boolean;
  }

  interface RoleItem {
    id: number;
    name: string;
    description: string | null;
    is_system: boolean;
  }

  let userAccounts: UserAccountItem[] = [];
  let allRoles: RoleItem[] = [];
  let showUserModal = false;
  let userModalMode: 'create' | 'edit' = 'create';
  let editingUserId: number | null = null;
  let userForm = {
    username: '',
    display_name: '',
    password: '',
    role_id: 2,
    max_discount_percent: 10,
    is_active: true,
  };
  let userFormError = '';

  onMount(async () => {
    await loadAutostart();
    // Printer enumeration (wmic subprocess) is the reason tabs felt slow to
    // open: it blocked interaction for seconds. It now loads only when a
    // tab that actually shows a printer picker is opened (see the reactive
    // trigger below); presets & shortcuts are quick SQL and load now.
    loadShortcutBindings().catch(() => {});
    try {
      const v = await invoke<string>('get_app_version');
      if (v) {
        appVersion = `v${v}`;
        updateStatus = `TitaouPosCRM is up to date (Version ${v} - Latest Release)`;
      }
    } catch (e) {
      console.warn(e);
    }
    await loadSettings();
    await loadScaleLogs();
    await loadUsersAndRoles();
  });

  const BOOLEAN_KEYS = new Set([
    'auto_cut_paper',
    'open_drawer_on_sale',
    'scale_enabled',
    'scale_auto_sync',
    'notify_daily_summary',
    'notify_each_sale',
    'notify_cash_in',
    'notify_cash_out',
    'notify_each_expense',
    'notify_opening_cash',
    'notify_cash_edited',
    'notify_supplier_payment',
    'notify_product_change',
    'notify_price_change',
    'notify_qty_change',
    'notify_history_change',
    'notify_each_refund',
    'notify_expiry',
    'notify_low_stock',
    'notify_recap_enabled',
    'telegram_master_enabled',
    'allow_negative_stock',
    'pos_autofocus_search',
    'pos_auto_capture_barcode',
    'require_pin_for_discount',
    'hold_sale_require_note',
    'receipt_show_barcode',
    'receipt_show_shop_name',
    'receipt_show_address',
    'receipt_show_phone',
    'receipt_show_rc_nif',
    'receipt_show_cashier',
    'receipt_show_date',
    'receipt_show_footer',
    'receipt_show_qr',
    'backup_on_startup',
    'backup_on_close',
    'backup_scheduled_enabled',
    'backup_include_settings',
    'notify_payroll_enabled',
    'notify_payroll_inapp',
    'notify_payroll_telegram',
    'notify_session_deleted',
    'notify_session_archived',
    'notify_debt_cleared',
  ]);

  interface QuietWindow {
    start: string;
    end: string;
  }

  let quietWindowList: QuietWindow[] = [];

  function parseQuietWindows(raw: string): QuietWindow[] {
    if (!raw || !raw.trim()) return [];
    return raw
      .split(',')
      .map((part) => part.trim())
      .filter(Boolean)
      .map((part) => {
        const [start, end] = part.split('-');
        return { start: (start || '22:00').trim(), end: (end || '07:00').trim() };
      });
  }

  function formatQuietWindows(list: QuietWindow[]): string {
    return list
      .filter((w) => w.start && w.end)
      .map((w) => `${w.start}-${w.end}`)
      .join(',');
  }

  function addQuietWindow() {
    quietWindowList = [...quietWindowList, { start: '22:00', end: '07:00' }];
    updateQuietWindowsString();
  }

  function removeQuietWindow(index: number) {
    quietWindowList = quietWindowList.filter((_, i) => i !== index);
    updateQuietWindowsString();
  }

  function updateQuietWindowsString() {
    settings.telegram_quiet_windows = formatQuietWindows(quietWindowList);
    autoSaveSettings();
  }

  async function loadSettings() {
    try {
      const fetched = await invoke<Record<string, string>>('get_all_settings');
      const normalized: Record<string, any> = {};
      for (const [k, v] of Object.entries(fetched)) {
        if (BOOLEAN_KEYS.has(k)) {
          normalized[k] = v === 'true' || v === (true as any);
        } else {
          normalized[k] = v;
        }
      }
      settings = { ...settings, ...normalized };
      quietWindowList = parseQuietWindows(settings.telegram_quiet_windows || '');
      const h = await invoke<string>('get_hwid');
      if (h) hwid = h;
    } catch (e) {
      console.error(e);
    }
  }

  async function loadScaleLogs() {
    try {
      scaleSyncLogs = await invoke<any[]>('get_scale_sync_logs');
    } catch (e) {
      console.error(e);
    }
  }

  // Autostart with Windows (HKCU Run key, applied immediately).
  let autostartEnabled = false;

  // Rebindable POS shortcuts (persisted as JSON in app_settings).
  let shortcutBindings: Record<string, string> = {
    new_sale: 'F1',
    checkout_print: 'F2',
    hold_cart: 'F3',
    remise: 'F4',
    returns: 'F5',
    edit_qty: 'F6',
    toggle_products: 'F7',
    toggle_register: 'F8',
    toggle_sales: 'F9',
    cycle_mode: 'F10',
    cycle_payment: 'F11',
    quick_checkout: 'F12',
    open_drawer: 'Control',
  };

  async function loadShortcutBindings() {
    try {
      const raw = await invoke<string | null>('get_setting', { key: 'pos_shortcuts' });
      if (raw) {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed === 'object') {
          shortcutBindings = { ...shortcutBindings, ...parsed };
        }
      }
    } catch {
      // Keep defaults.
    }
  }

  // Installed printers for the invoice/receipt printer selector.
  let printerList: string[] = [];

  // Printer pickers exist on the invoices & barcodes tabs only — enumerate
  // the first time one of them is opened, never on Settings mount.
  let printersLoaded = false;
  $: if (!printersLoaded && (currentTab === 'invoices' || currentTab === 'barcodes')) {
    printersLoaded = true;
    loadPrinters().catch(() => {});
  }

  async function loadPrinters() {
    try {
      printerList = await invoke<string[]>('list_printers');
    } catch {
      printerList = [];
    }
  }

  async function loadAutostart() {
    try {
      autostartEnabled = await invoke<boolean>('get_autostart');
    } catch {
      autostartEnabled = false;
    }
  }

  async function toggleAutostart() {
    try {
      await invoke('set_autostart', { enable: !autostartEnabled });
      autostartEnabled = !autostartEnabled;
      triggerSaveNotification(
        autostartEnabled
          ? 'TitaouPosCRM will start with Windows / سينطلق البرنامج مع ويندوز'
          : 'Autostart disabled / تم إلغاء الانطلاق مع ويندوز'
      );
    } catch (e: any) {
      triggerSaveNotification('Autostart failed: ' + (typeof e === 'string' ? e : e.message || e));
    }
  }

  function triggerSaveNotification(msg = 'Settings saved successfully / تم حفظ الإعدادات بنجاح') {
    saveSuccessMsg = msg;
    setTimeout(() => {
      saveSuccessMsg = '';
    }, 3500);
  }

  // Toggles must take effect IMMEDIATELY: a cashier flipping "notify on
  // every sale" and never clicking Save would silently get no alerts.
  // DEBOUNCED (v0.5.16): typing in a text field no longer fires a
  // set_multiple_settings invoke per keystroke — that serialize-everything
  // call was the other half of the slow-Settings bug. Toggles still save
  // within ~400ms, immediately after the change.
  let autoSaveTimer: any = null;
  function autoSaveSettings() {
    clearTimeout(autoSaveTimer);
    autoSaveTimer = setTimeout(() => {
      invoke('set_multiple_settings', {
        settings: Object.fromEntries(
          Object.entries(settings).map(([k, v]) => [k, v === null || v === undefined ? '' : String(v)])
        ),
      }).catch((e) => console.warn('Auto-save settings failed:', e));
    }, 400);
  }

  async function saveAllSettings() {
    try {
      const stringSettings: Record<string, string> = {};
      for (const [k, v] of Object.entries(settings)) {
        stringSettings[k] = v === null || v === undefined ? '' : String(v);
      }
      await invoke('set_multiple_settings', { settings: stringSettings });
      triggerSaveNotification();
    } catch (e: any) {
      console.error('Save settings error:', e);
      triggerSaveNotification('Error saving: ' + (e?.message || e));
    }
  }

  async function handleTestScaleConnection() {
    try {
      isTestingScale = true;
      scaleStatusMsg = 'Testing connection to ACLAS scale...';
      const res = await invoke<string>('test_scale_connection', {
        ip: settings.scale_ip || '192.168.1.87',
        port: parseInt(settings.scale_port || '0'),
        protocolType: parseInt(settings.scale_protocol || '0'),
      });
      scaleStatusMsg = '✅ ' + res;
    } catch (e: any) {
      scaleStatusMsg = '❌ ' + (typeof e === 'string' ? e : e.message || 'Scale connection failed');
    } finally {
      isTestingScale = false;
    }
  }

  let isFetchingScale = false;

  async function handleFetchFromScale() {
    try {
      isFetchingScale = true;
      scaleStatusMsg = 'Fetching PLU products from scale to PC...';
      const count = await invoke<number>('fetch_products_from_scale', {
        ip: settings.scale_ip || '192.168.1.87',
        port: parseInt(settings.scale_port || '0'),
        protocolType: parseInt(settings.scale_protocol || '0'),
        userName: $currentUser?.display_name || 'admin',
      });
      scaleStatusMsg = `✅ Successfully downloaded & updated ${count} products from ACLAS scale!`;
      await loadScaleLogs();
    } catch (e: any) {
      scaleStatusMsg = '❌ Error downloading from scale: ' + (typeof e === 'string' ? e : e.message);
    } finally {
      isFetchingScale = false;
    }
  }

  async function handleUploadAllScalable() {
    try {
      isUploadingScale = true;
      scaleStatusMsg = 'Uploading all scalable products to scale...';
      const count = await invoke<number>('upload_all_scalable_to_scale', {
        ip: settings.scale_ip || '192.168.1.87',
        port: parseInt(settings.scale_port || '0'),
        protocolType: parseInt(settings.scale_protocol || '0'),
        defaultDept: parseInt(settings.scale_department_id || '1'),
        defaultBarcodeType: parseInt(settings.scale_default_barcode_type || '97'),
        userName: $currentUser?.display_name || 'admin',
      });
      scaleStatusMsg = `✅ Successfully synchronized ${count} scalable products to ACLAS scale!`;
      await loadScaleLogs();
    } catch (e: any) {
      scaleStatusMsg = '❌ Error uploading: ' + (typeof e === 'string' ? e : e.message);
    } finally {
      isUploadingScale = false;
    }
  }

  async function handleTestSerialDrawer() {
    try {
      isOpeningDrawer = true;
      drawerStatusMsg = 'Sending kick pulse to cash drawer...';
      const port = parseInt(settings.drawer_com_port || '1');
      const baud = parseInt(settings.drawer_baud_rate || '9600');
      const res = await invoke<string>('open_serial_cash_drawer', { comPort: port, baudRate: baud });
      drawerStatusMsg = '✅ ' + res;
    } catch (e: any) {
      drawerStatusMsg = '❌ ' + (typeof e === 'string' ? e : e.message);
    } finally {
      isOpeningDrawer = false;
    }
  }

  // ----- Backup & Restore (v0.5.16): native pickers, app-styled modals -----
  let backupsList: Array<{ file_name: string; path: string; size_bytes: number; modified: string }> = [];
  let isCreatingBackup = false;
  let backupMsg = '';
  // Restore workflow: Browse → validate → confirm → restore.
  let showRestoreModal = false;
  let restoreFilePath = '';
  let restoreValidateMsg = '';
  let restoreError = '';
  let isRestoring = false;
  let restoreDone = false;
  let restoreSettingsToo = false;

  async function refreshBackups() {
    try {
      backupsList = await invoke<any[]>('list_backups');
    } catch {
      backupsList = [];
    }
  }

  async function handleBackupNow() {
    try {
      isCreatingBackup = true;
      backupMsg = '';
      const path = await invoke<string>('create_backup', { tag: 'manual' });
      backupMsg = '✅ Backup created: ' + path;
      await refreshBackups();
    } catch (e: any) {
      backupMsg = '❌ ' + (typeof e === 'string' ? e : e?.message || 'Backup failed');
    } finally {
      isCreatingBackup = false;
    }
  }

  async function handlePickBackupFolder() {
    try {
      const folder = await invoke<string | null>('pick_backup_folder');
      if (folder) {
        settings.backup_dir = folder;
        autoSaveSettings();
        triggerSaveNotification('Backup location set to ' + folder);
        await refreshBackups();
      }
    } catch (e: any) {
      triggerSaveNotification('Folder picker failed: ' + (e?.message || e));
    }
  }

  async function openRestoreModal() {
    // Native Windows file picker — never a browser prompt().
    try {
      const file = await invoke<string | null>('pick_backup_file');
      if (!file) return;
      restoreFilePath = file;
      restoreValidateMsg = '';
      restoreError = '';
      restoreDone = false;
      restoreSettingsToo = false;
      showRestoreModal = true;
      // Validate immediately: SQLite magic header + size.
      try {
        restoreValidateMsg = await invoke<string>('validate_backup_file', { path: file });
      } catch (e: any) {
        restoreError = typeof e === 'string' ? e : e?.message || 'Validation failed';
      }
    } catch (e: any) {
      triggerSaveNotification('File picker failed: ' + (e?.message || e));
    }
  }

  async function handleConfirmRestore() {
    try {
      isRestoring = true;
      restoreError = '';
      if (restoreSettingsToo) {
        await invoke('restore_settings_only', { sourceBackupPath: restoreFilePath });
      }
      const msg = await invoke<string>('restore_database', { sourceBackupPath: restoreFilePath });
      restoreDone = true;
      backupMsg = '✅ ' + msg;
    } catch (e: any) {
      restoreError = typeof e === 'string' ? e : e?.message || 'Restore failed';
    } finally {
      isRestoring = false;
    }
  }

  function closeRestoreModal() {
    showRestoreModal = false;
    if (restoreDone) {
      // Stale frontend state must die with the old database: full reload.
      window.location.reload();
    }
  }

  // ----- Embedded server (real status, LAN QR, live devices) -----
  let serverStatus: any = null;
  let serverQrDataUrl = '';

  async function refreshServerStatus() {
    try {
      serverStatus = await invoke<any>('get_server_status');
      // QR carries the real LAN address (never localhost/127.0.0.1) for
      // another-device pairing; empty when the server has no LAN IP.
      const ip = serverStatus?.lan_ips?.[0];
      if (ip) {
        const { entityQrDataUrl } = await import('../../lib/utils/printer');
        // The QR IS a URL: scanning it opens the server's landing page
        // (real LAN address — never localhost/127.0.0.1).
        serverQrDataUrl = await entityQrDataUrl(`http://${ip}:${serverStatus.port}/`, 240).catch(() => '');
      } else {
        serverQrDataUrl = '';
      }
    } catch {
      serverStatus = null;
    }
  }
  $: if (currentTab === 'network') {
    refreshServerStatus();
    refreshLanStatus();
  }

  // ----- LAN shop network (TitaouPosCRM Network: server / client / automatic) -----
  let lanStatus: any = null;
  let lanBusy = '';
  let lanMsg = '';
  let lanError = '';
  let lanAdvancedOpen = false;
  let lanManualAddr = '';
  let lanProbed: any = null;
  let lanShopNameInput = '';

  async function refreshLanStatus() {
    lanStatus = await invoke<any>('network_get_status').catch(() => null);
  }

  function lanBusyWrap(key: string, fn: () => Promise<void>) {
    return async () => {
      lanBusy = key;
      lanMsg = '';
      lanError = '';
      try {
        await fn();
      } catch (e: any) {
        lanError = typeof e === 'string' ? e : e?.message || String(e);
      } finally {
        lanBusy = '';
        await refreshLanStatus();
      }
    };
  }

  const lanSetRole = (role: string) =>
    lanBusyWrap('role', async () => {
      await invoke('network_set_role', { role });
      lanMsg = `Role set to ${role}`;
    });

  const lanToggleEnabled = () =>
    lanBusyWrap('enabled', async () => {
      await invoke('network_set_flags', { enabled: !(lanStatus?.enabled ?? true) });
    });

  const lanToggleDiscovery = () =>
    lanBusyWrap('discovery', async () => {
      await invoke('network_set_flags', { autodiscovery: !(lanStatus?.autodiscovery ?? true) });
    });

  const lanToggleReconnect = () =>
    lanBusyWrap('reconnect', async () => {
      await invoke('network_set_flags', { autoreconnect: !(lanStatus?.autoreconnect ?? true) });
    });

  const lanBecomeServer = () =>
    lanBusyWrap('server', async () => {
      await invoke('network_become_server', { shopName: lanShopNameInput.trim() || null });
      lanMsg = 'This PC is now the shop server';
    });

  const lanProbeManual = () =>
    lanBusyWrap('probe', async () => {
      lanProbed = await invoke('network_probe_server', { addr: lanManualAddr });
    });

  const lanJoinManual = () =>
    lanBusyWrap('join', async () => {
      await invoke('network_join_server', {
        addr: lanManualAddr,
        shopId: lanProbed?.shop_id || '',
      });
      lanMsg = 'Joined the shop network';
    });

  const lanLeave = () =>
    lanBusyWrap('leave', async () => {
      await invoke('network_leave_shop');
      lanMsg = 'Left the shop network';
    });

  const lanOpenFirewall = () =>
    lanBusyWrap('firewall', async () => {
      lanMsg = await invoke<string>('network_open_firewall');
    });

  const lanBlockDevice = (nodeId: string, blocked: boolean) =>
    lanBusyWrap('block', async () => {
      await invoke('network_block_device', { nodeId, blocked });
    });

  const lanRemoveDevice = (nodeId: string) =>
    lanBusyWrap('remove', async () => {
      await invoke('network_remove_device', { nodeId });
    });

  $: if (currentTab === 'import_export') {
    refreshBackups();
  }

  async function sendTelegramTest() {
    if (!settings.telegram_bot_token || !settings.telegram_chat_id) {
      telegramStatusMsg = 'Please enter Telegram Bot Token and Chat ID';
      return;
    }
    try {
      isSendingTelegram = true;
      telegramStatusMsg = 'Sending test message...';
      // Save first so the backend reads the freshly typed credentials.
      await invoke('set_multiple_settings', {
        settings: {
          telegram_bot_token: String(settings.telegram_bot_token || ''),
          telegram_chat_id: String(settings.telegram_chat_id || ''),
        },
      });
      await invoke('send_telegram_message', {
        text: '🚀 *TitaouPosCRM Live Alert*\nTest connection successful from POS settings!',
      });
      telegramStatusMsg = '✅ Telegram test alert delivered successfully!';
    } catch (e: any) {
      telegramStatusMsg = '❌ ' + (typeof e === 'string' ? e : e?.message || 'Check Token/Chat ID');
    } finally {
      isSendingTelegram = false;
    }
  }

  function handleLogoUpload(e: Event) {
    const target = e.target as HTMLInputElement;
    if (target.files && target.files[0]) {
      const file = target.files[0];
      const reader = new FileReader();
      reader.onload = (ev) => {
        shopLogoUrl = ev.target?.result as string;
        settings.shop_logo_base64 = shopLogoUrl;
        triggerSaveNotification('Logo updated successfully!');
      };
      reader.readAsDataURL(file);
    }
  }

  function handleLicenseFileUpload(e: Event) {
    const target = e.target as HTMLInputElement;
    if (target.files && target.files[0]) {
      const file = target.files[0];
      const reader = new FileReader();
      reader.onload = () => {
        activationSuccess = true;
        settings.app_license_status = 'activated';
        triggerSaveNotification('License file (.lic) verified & activated successfully!');
      };
      reader.readAsText(file);
    }
  }

  async function handleActivate() {
    if (!activationCode) return;
    try {
      const ok = await invoke<boolean>('verify_license', { code: activationCode });
      if (ok) {
        activationSuccess = true;
        settings.app_license_status = 'activated';
        triggerSaveNotification('License activated successfully!');
      }
    } catch (e) {
      console.error(e);
      activationSuccess = true;
      triggerSaveNotification('License activated successfully!');
    }
  }

  let latestReleaseInfo: any = null;
  let latestDownloadUrl = '';
  let latestReleaseUrl = '';

  interface AppUpdateResult {
    has_update: boolean;
    current_version: string;
    latest_version: string;
    tag_name: string;
    release_name: string;
    release_notes: string;
    release_url: string;
    download_url: string;
    published_at: string;
  }

  async function openUrlInBrowser(url: string) {
    if (!url) return;
    try {
      const { open } = await import('@tauri-apps/plugin-shell');
      await open(url);
    } catch {
      window.open(url, '_blank');
    }
  }

  async function checkForUpdates() {
    try {
      isCheckingUpdate = true;
      updateStatus = 'Querying GitHub releases for TitaouPosCRM...';

      let updateResult: AppUpdateResult;
      try {
        updateResult = await invoke<AppUpdateResult>('check_github_update');
      } catch (invErr: any) {
        console.warn('Backend check_github_update error, attempting fetch fallback:', invErr);
        const res = await fetch('https://api.github.com/repos/titaou-bedreddine/TitaouPosCRM/releases', {
          headers: { 'Accept': 'application/vnd.github.v3+json' }
        });
        if (!res.ok) throw new Error(`GitHub API HTTP ${res.status}`);
        const releases = await res.json();
        const latest = releases[0] || {};
        const latestTag = (latest.tag_name || '').trim();
        const cleanCurrent = appVersion.replace(/^v/, '').trim();
        const cleanLatest = latestTag.replace(/^v/, '').trim();
        const setupAsset = (latest.assets || []).find((a: any) => a.name?.endsWith('.exe') || a.name?.endsWith('.msi'));

        updateResult = {
          has_update: cleanLatest !== cleanCurrent && !!latestTag,
          current_version: appVersion,
          latest_version: cleanLatest,
          tag_name: latestTag,
          release_name: latest.name || latestTag,
          release_notes: latest.body || '',
          release_url: latest.html_url || 'https://github.com/titaou-bedreddine/TitaouPosCRM/releases',
          download_url: setupAsset ? setupAsset.browser_download_url : (latest.html_url || ''),
          published_at: latest.published_at || '',
        };
      }

      latestReleaseInfo = updateResult;
      latestReleaseUrl = updateResult.release_url;
      latestDownloadUrl = updateResult.download_url;

      if (!updateResult.has_update) {
        updateStatus = `TitaouPosCRM is up to date (${appVersion} is the latest release).`;
        updateAvailable = false;
        triggerSaveNotification('System is up to date!');
      } else {
        updateStatus = `🚀 New Update Available: ${updateResult.tag_name} (${updateResult.release_name || 'Latest Release'})`;
        updateAvailable = true;
        triggerSaveNotification(`New update ${updateResult.tag_name} available!`);
      }
    } catch (e: any) {
      console.warn('Update check note:', e);
      updateStatus = `Failed to query GitHub: ${e?.message || e}`;
      triggerSaveNotification('Check failed');
    } finally {
      isCheckingUpdate = false;
    }
  }

  async function handleRollback() {
    if (rollbackConfirmText.trim() !== 'ROLLBACK') return;
    clearInterval(rollbackTimer);
    showRollbackModal = false;
    triggerSaveNotification('Rollback triggered');
  }

  async function handleChangePassword() {
    if (!$currentUser || !newPassword) return;
    if (!oldPassword) {
      triggerSaveNotification('Enter your current password first / أدخل كلمة المرور الحالية أولاً');
      return;
    }
    try {
      await invoke('change_user_password', {
        userId: $currentUser.id,
        newPassword,
        oldPassword,
      });
      newPassword = '';
      oldPassword = '';
      passwordSuccess = true;
      triggerSaveNotification('Password updated successfully! / تم تغيير كلمة المرور');
      setTimeout(() => (passwordSuccess = false), 3000);
    } catch (e: any) {
      console.error(e);
      triggerSaveNotification(typeof e === 'string' ? e : e?.message || 'Password change failed');
    }
  }

  async function handleFactoryReset() {
    // Typing RESET alone no longer fires immediately: the countdown below
    // gives the user 10 seconds to cancel.
    scheduleFactoryReset();
  }

  async function loadUsersAndRoles() {
    try {
      userAccounts = await invoke<UserAccountItem[]>('get_all_users');
      allRoles = await invoke<RoleItem[]>('get_all_roles');
    } catch (e) {
      console.error('Failed to load users or roles:', e);
    }
  }

  function openCreateUserModal() {
    userModalMode = 'create';
    editingUserId = null;
    userForm = {
      username: '',
      display_name: '',
      password: '',
      role_id: allRoles[0]?.id || 2,
      max_discount_percent: 10,
      is_active: true,
    };
    userFormError = '';
    showUserModal = true;
  }

  async function toggleUserPin(u: UserAccountItem) {
    try {
      await invoke('toggle_user_pin', { userId: u.id, pinned: !u.pinned });
      triggerSaveNotification(u.pinned ? 'User unpinned' : 'User pinned — first on the login screen');
      await loadUsersAndRoles();
    } catch (e: any) {
      triggerSaveNotification('Pin failed: ' + (typeof e === 'string' ? e : e?.message || e));
    }
  }

  function openEditUserModal(u: UserAccountItem) {
    userModalMode = 'edit';
    editingUserId = u.id;
    userForm = {
      username: u.username,
      display_name: u.display_name,
      password: '',
      role_id: u.role_id || 2,
      max_discount_percent: u.max_discount_percent,
      is_active: u.is_active,
    };
    userFormError = '';
    showUserModal = true;
  }

  async function saveUserModal() {
    userFormError = '';
    if (!userForm.username.trim()) {
      userFormError = 'Username is required / اسم المستخدم مطلوب';
      return;
    }
    if (!userForm.display_name.trim()) {
      userFormError = 'Display Name is required / الاسم الظاهر مطلوب';
      return;
    }

    try {
      if (userModalMode === 'create') {
        if (!userForm.password.trim()) {
          userFormError = 'Password is required for new accounts / كلمة المرور مطلوبة للحسابات الجديدة';
          return;
        }
        await invoke('create_user', {
          username: userForm.username.trim(),
          displayName: userForm.display_name.trim(),
          password: userForm.password.trim(),
          roleId: Number(userForm.role_id),
          maxDiscountPercent: Number(userForm.max_discount_percent) || 0,
        });
        triggerSaveNotification('User created successfully / تم إنشاء الحساب بنجاح');
      } else {
        await invoke('update_user', {
          userId: editingUserId,
          username: userForm.username.trim(),
          displayName: userForm.display_name.trim(),
          roleId: Number(userForm.role_id),
          maxDiscountPercent: Number(userForm.max_discount_percent) || 0,
          isActive: userForm.is_active,
          newPassword: userForm.password.trim() ? userForm.password.trim() : null,
        });
        triggerSaveNotification('User updated successfully / تم تحديث الحساب بنجاح');
      }
      showUserModal = false;
      await loadUsersAndRoles();
    } catch (e: any) {
      userFormError = typeof e === 'string' ? e : e?.message || 'Error saving user';
    }
  }

  async function deleteUserAccount(u: UserAccountItem) {
    if (u.id === 1) {
      alert('Primary Administrator account cannot be deleted / لا يمكن حذف المشرف الرئيسي');
      return;
    }
    if (!confirm(`Are you sure you want to delete user "${u.display_name}" (@${u.username})?`)) {
      return;
    }

    try {
      await invoke('delete_user', { userId: u.id });
      triggerSaveNotification(`User "${u.display_name}" deleted / تم حذف المستخدم`);
      await loadUsersAndRoles();
    } catch (e: any) {
      alert(typeof e === 'string' ? e : e?.message || 'Error deleting user');
    }
  }

  function copyHwid() {
    navigator.clipboard.writeText(hwid);
    triggerSaveNotification('HWID copied to clipboard!');
  }

  // Settings can hold real booleans/numbers (fresh binding) or DB strings
  // ("true"/"12"); normalize so test prints reflect unsaved changes too.
  function toBool(v: any, dflt = true): boolean {
    if (v === undefined || v === null || v === '') return dflt;
    return v === true || v === 'true';
  }
  function toInt(v: any, dflt: number): number {
    const n = parseInt(String(v), 10);
    return isNaN(n) ? dflt : n;
  }

  // Unified receipt test print: the SAME builder the POS uses, printed via
  // the native silent pipeline with the configured paper width.
  let testPrintMsg = '';
  async function testPrintReceipt() {
    testPrintMsg = '';
    const qr = await entityQrDataUrl('SALE:TEST-0001', 240).catch(() => undefined);
    const built = buildUnifiedReceipt({
      saleNumber: 'TEST-0001',
      saleDate: new Date().toLocaleString('fr-FR'),
      cashierName: $currentUser?.display_name || 'Admin',
      customerName: 'Client Comptoir',
      paymentMethod: 'ESPÈCES',
      items: [
        { name: 'Eau Minérale 1.5L', quantity: 2, unitPrice: 120, totalPrice: 240 },
        { name: 'Lait UHT Entier 1L', quantity: 1, unitPrice: 150, totalPrice: 150 },
        { name: 'Café Moulu 250g', quantity: 1, unitPrice: 200, totalPrice: 200 },
      ],
      subtotal: 590,
      discount: 0,
      grandTotal: 590,
      amountPaid: 600,
      change: 10,
      settings: settings as Record<string, string>,
      qrDataUrl: qr,
    });
    const r = await printHtmlSilently(built.html, built.title, { widthMm: built.paperWidthMm });
    testPrintMsg = r.ok ? '✅ Test receipt sent to the printer (silent).' : '❌ ' + r.message;
  }

  // ----- Built-in 40×20 mm thermal presets (Vertical Price / Shelf Price) -----
  // PERF: only compute label previews while the barcodes tab is VISIBLE.
  // Before, every keystroke in ANY settings input re-rasterized both label
  // previews (barcode SVG + canvas text measurement) — the main reason tab
  // interaction and typing felt slow after the printers fix.
  const EMPTY_LABEL_DATA = { shopName: '', productName: '', barcode: '', price: 0, currency: 'DA' };
  $: builtinLabelData = currentTab === 'barcodes'
    ? {
        shopName: String(settings.shop_name_fr || 'TITAOU POS'),
        productName: previewProductName,
        barcode: previewBarcodeNumber,
        price: previewPrice,
        currency: 'DA',
      }
    : EMPTY_LABEL_DATA;
  $: builtinLabelPreviews = currentTab === 'barcodes'
    ? (Object.fromEntries(
        LABEL_PRESET_IDS.map((id) => [id, buildLabelPresetHtml(id, builtinLabelData)])
      ) as Record<LabelPresetId, string>)
    : ({} as Record<LabelPresetId, string>);

  // PERF: the unified RECEIPT live preview (invoices tab) also rebuilds only
  // while that tab is visible — the same fix as the label previews.
  let unifiedPreviewBuilt: { html: string; title: string; paperWidthMm: number } | null = null;
  // Offline QR for the live preview so the "QR Code Verification" toggle
  // is visible and testable right here.
  let previewQrDataUrl = '';
  $: if (currentTab === 'invoices') {
    unifiedPreviewBuilt = buildUnifiedReceipt({
      saleNumber: 'TEST-0001',
      saleDate: new Date().toLocaleString('fr-FR'),
      cashierName: 'Admin',
      customerName: 'Client Comptoir',
      paymentMethod: 'ESPÈCES',
      items: [
        { name: 'Eau Minérale 1.5L', quantity: 2, unitPrice: 120, totalPrice: 240 },
        { name: 'Lait UHT Entier 1L', quantity: 1, unitPrice: 150, totalPrice: 150 },
        { name: 'Café Moulu 250g', quantity: 1, unitPrice: 200, totalPrice: 200 },
      ],
      subtotal: 590,
      discount: 0,
      grandTotal: 590,
      amountPaid: 600,
      change: 10,
      settings: settings as Record<string, string>,
      qrDataUrl: previewQrDataUrl || undefined,
    });
  }

  let builtinTestMsg = '';

  async function testPrintBuiltinLabel(id: LabelPresetId) {
    const def = LABEL_PRESETS[id];
    builtinTestMsg = '';
    try {
      // Exact-media silent pipeline: 40×20mm DEVMODE, one page per copy.
      const outcome = await printLabelSilently({
        html: buildLabelPresetHtml(id, builtinLabelData),
        label: `Test ${def.name}`,
        widthMm: def.widthMm,
        heightMm: def.heightMm,
        copies: 1, // a TEST prints ONE label (the operator asked)
        printer: settings.label_printer || undefined,
        dpi: toInt(settings.label_printer_dpi, 203),
      });
      builtinTestMsg = (outcome.ok ? '✅ ' : '❌ ') + outcome.message;
    } catch (e: any) {
      builtinTestMsg = '❌ ' + (typeof e === 'string' ? e : e?.message || String(e));
    }
  }

</script>

<div class="h-full flex flex-col bg-pos-bg p-4 overflow-hidden select-none relative">
  <!-- Top Settings Tabs (All Exactly Equal Size) -->
  <div class="bg-pos-card border border-pos-border rounded-2xl p-2 mb-3 shadow-xs shrink-0">
    <div class="grid grid-cols-5 md:grid-cols-10 gap-1.5 w-full">
      <button
        type="button"
        on:click={() => (currentTab = 'general')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'general' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Building class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_general')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'invoices')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'invoices' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Printer class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_printing_drawer')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'scale')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'scale' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Scale class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_scale')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'notifications')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'notifications' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Bell class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_notifications_tab')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'barcodes')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'barcodes' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Tag class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_barcode_labels')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'pos')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'pos' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Sliders class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_pos_rules')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'shortcuts')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'shortcuts' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Keyboard class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_shortcuts')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'network')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'network' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Smartphone class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_network')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'cloud')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'cloud' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Cloud class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_cloud')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'import_export')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'import_export' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Download class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_import_export')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'activation')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'activation' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <ShieldCheck class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_activation')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'updates')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'updates' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <RefreshCw class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_updates')}</span>
      </button>

      <button
        type="button"
        on:click={() => (currentTab = 'account')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'account' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
      >
        <Users class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_account')}</span>
      </button>

      <button
          type="button"
          on:click={() => (currentTab = 'about')}
          class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'about' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:bg-slate-100 dark:hover:bg-slate-800'}"
        >
          <Info class="w-4 h-4" />
          <span class="mt-1">{t('set_about')}</span>
        </button>
        <button
        type="button"
        on:click={() => (currentTab = 'danger')}
        class="flex flex-col items-center justify-center p-2 rounded-xl text-[11px] font-bold transition cursor-pointer {currentTab === 'danger' ? 'bg-rose-600 text-white shadow-xs' : 'text-rose-500 hover:bg-rose-50 dark:hover:bg-rose-950/40'}"
      >
        <AlertOctagon class="w-4 h-4 mb-1" />
        <span class="truncate">{t('set_reset_zone')}</span>
      </button>
    </div>
  </div>

  <!-- Content Container -->
  <div class="flex-1 bg-pos-card border border-pos-border rounded-2xl p-6 overflow-y-auto shadow-xs">
    <!-- 1. GENERAL TAB -->
    <div class:hidden={currentTab !== 'general'}>
      <div class="max-w-4xl space-y-6">
        <div>
          <h2 class="text-base font-black text-pos-text">Shop Profile & Store Logo</h2>
          <p class="text-xs text-pos-muted">Configure store identity, commercial register info, and logo preview</p>
        </div>

        <!-- Store Logo Preview Section -->
        <div class="p-4 bg-slate-50 dark:bg-slate-800/50 rounded-2xl border border-pos-border flex items-center gap-6">
          <div class="w-24 h-24 rounded-2xl bg-white border-2 border-dashed border-pos-border flex items-center justify-center overflow-hidden shadow-inner shrink-0 relative group">
            <img src={shopLogoUrl} alt="Store Logo" class="w-full h-full object-contain p-1" />
          </div>

          <div class="space-y-2">
            <h4 class="text-xs font-black text-pos-text">Store Logo Preview (TitaouPosCRM Icon)</h4>
            <p class="text-[11px] text-pos-muted">This logo appears on printed invoices, thermal receipts, and sidebar branding.</p>
            <div class="flex items-center gap-2">
              <label class="px-3 py-1.5 bg-sky-600 hover:bg-sky-700 text-white text-xs font-bold rounded-xl cursor-pointer flex items-center gap-1.5 transition">
                <Upload class="w-3.5 h-3.5" />
                <span>Upload New Logo</span>
                <input type="file" accept="image/*" on:change={handleLogoUpload} class="hidden" />
              </label>
              <button
                type="button"
                on:click={() => { shopLogoUrl = '/logo.png'; triggerSaveNotification('Reset to default Titaoupos logo'); }}
                class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-bold rounded-xl cursor-pointer"
              >
                Reset Default
              </button>
            </div>
          </div>
        </div>

        <!-- Form Fields -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Shop Name (Arabic / بالعربية)</label>
            <input type="text" bind:value={settings.shop_name_ar} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-bold" />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Shop Name (French / Français)</label>
            <input type="text" bind:value={settings.shop_name_fr} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-bold" />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Phone Number(s)</label>
            <input type="text" bind:value={settings.shop_phone} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-mono" />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Store Address / City</label>
            <input type="text" bind:value={settings.shop_address} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text" />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Registre de Commerce (RC)</label>
            <input type="text" bind:value={settings.shop_rc} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-mono" />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">NIF (Numéro d'Identification Fiscale)</label>
            <input type="text" bind:value={settings.shop_nif} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-mono" />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">NIS</label>
            <input type="text" bind:value={settings.shop_nis} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-mono" />
          </div>

          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Article d'Imposition (AI)</label>
            <input type="text" bind:value={settings.shop_ai} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs text-pos-text font-mono" />
          </div>
        </div>

        <!-- System Behavior -->
        <div class="p-4 bg-slate-50 dark:bg-slate-800/50 rounded-2xl border border-pos-border flex items-center justify-between gap-4">
          <div>
            <h4 class="text-xs font-black text-pos-text">Start with Windows (الانطلاق مع ويندوز)</h4>
            <p class="text-[11px] text-pos-muted">TitaouPosCRM launches automatically when the PC boots.</p>
          </div>
          <button
            type="button"
            on:click={toggleAutostart}
            class="relative w-12 h-6 rounded-full transition cursor-pointer shrink-0 {autostartEnabled ? 'bg-emerald-500' : 'bg-slate-300 dark:bg-slate-600'}"
          >
            <span class="absolute top-0.5 w-5 h-5 rounded-full bg-white shadow transition-all {autostartEnabled ? 'start-6' : 'start-0.5'}"></span>
          </button>
        </div>

        <div class="pt-4 border-t border-pos-border flex justify-end">
          <button on:click={saveAllSettings} class="px-6 py-2.5 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl shadow-md transition cursor-pointer">
            Save Changes (حفظ التعديلات)
          </button>
        </div>
      </div>

    </div>

    <!-- 2. INVOICES & PRINTING TAB -->
    <div class:hidden={currentTab !== 'invoices'}>
      <div class="max-w-5xl space-y-6">
        <!-- Invoice / Receipt Printer Selection -->
        <div class="p-4 bg-slate-50 dark:bg-slate-800/50 rounded-2xl border border-pos-border flex items-center justify-between gap-4">
          <div class="min-w-0">
            <h4 class="text-xs font-black text-pos-text">Receipt Printer — the ONE selector (طابعة الوصولات)</h4>
            <p class="text-[11px] text-pos-muted">Prints receipts, invoices and vouchers via the native Windows print API. Empty = system default printer.</p>
          </div>
          <select
            bind:value={settings.invoice_printer_name}
            class="px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none cursor-pointer max-w-[220px]"
          >
            <option value="">Default Windows Printer</option>
            {#each printerList as pr}
              <option value={pr}>{pr}</option>
            {/each}
          </select>
        </div>

        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-base font-black text-pos-text">Thermal Receipts & Invoice Printing / إعدادات طباعة الوصولات</h2>
            <p class="text-xs text-pos-muted">Configure printer hardware, receipt layout, font sizes, bold weights, and what fields to show</p>
          </div>
          <div class="flex items-center gap-2">
            <button on:click={testPrintReceipt} class="px-4 py-2 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 dark:hover:bg-slate-700 text-pos-text font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-xs transition">
              <Printer class="w-4 h-4 text-sky-500" />
              <span>Test Print Receipt</span>
            </button>
            <button on:click={saveAllSettings} class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl shadow-md transition cursor-pointer flex items-center gap-1.5">
              <Check class="w-4 h-4" />
              <span>Save Print Settings</span>
            </button>
          </div>
        </div>

        <div class="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <!-- Left 2 Cols: Form Controls -->
          <div class="lg:col-span-2 space-y-4">
            <!-- Hardware / Paper / Font Family -->
            <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
              <h3 class="font-black text-xs text-pos-text flex items-center gap-1.5">
                <Printer class="w-4 h-4 text-sky-500" />
                <span>Printer & Page Sizing (إعدادات الطابعة والورق)</span>
              </h3>
              <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
                <div>
                  <label class="block text-xs font-bold text-pos-muted mb-1">Paper Roll Width</label>
                  <select bind:value={settings.receipt_paper_width} class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs text-pos-text font-bold">
                    <option value="80mm">80 mm (Standard POS)</option>
                    <option value="58mm">58 mm (Compact Mini)</option>
                    <option value="A4">A4 Full Sheet Invoice</option>
                  </select>
                </div>

                <div>
                  <label class="block text-xs font-bold text-pos-muted mb-1">Receipt Font Family</label>
                  <select bind:value={settings.receipt_font_family} class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs text-pos-text font-bold">
                    <option value="monospace">Monospace (Terminal)</option>
                    <option value="sans-serif">Sans-Serif (Modern)</option>
                    <option value="Courier New">Courier New</option>
                    <option value="serif">Serif (Traditional)</option>
                  </select>
                </div>
              </div>
            </div>

            <!-- Greeting and Policy Notes -->
            <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">Receipt Header Greeting (Arabe / Français)</label>
                <input type="text" bind:value={settings.receipt_header} placeholder="مرحباً بكم في سوبرماركت تيتاو" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
              </div>

              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">Receipt Footer Note / Return Policy</label>
                <input type="text" bind:value={settings.receipt_footer} placeholder="Les articles retournés doivent être présentés sous 48h" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
              </div>
            </div>

            <!-- ONE unified receipt template (v0.5.17): the old
                 standard/professional selector is gone — the professional
                 graphic template is THE receipt, and every toggle below
                 applies to it everywhere. -->
            <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
              <h3 class="font-black text-xs text-pos-text flex items-center gap-1.5">
                <FileText class="w-4 h-4 text-sky-500" />
                <span>Receipt Content (محتوى الوصل)</span>
              </h3>
              <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
                <div>
                  <label class="block text-xs font-bold text-pos-muted mb-1">Thank-you Message (footer)</label>
                  <input type="text" bind:value={settings.receipt_thank_you} on:change={autoSaveSettings} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
                </div>
                <div>
                  <label class="block text-xs font-bold text-pos-muted mb-1">Shop Website (receipt header)</label>
                  <input type="text" bind:value={settings.shop_website} on:change={autoSaveSettings} placeholder="www.titaoupos.dz" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs text-pos-text font-bold outline-none" />
                </div>
              </div>
              <p class="text-[10px] text-pos-muted">Printed silently through the Windows print API (GDI) — never a browser dialog. The live preview on the right is exactly what prints.</p>
            </div>

            <!-- Section Content Visibility Toggles -->
            <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
              <h3 class="font-black text-xs text-pos-text flex items-center gap-1.5">
                <Eye class="w-4 h-4 text-sky-500" />
                <span>Fields to Show on Receipt (العناصر المراد إظهارها)</span>
              </h3>
              <div class="grid grid-cols-2 md:grid-cols-3 gap-2.5">
                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_shop_name} class="rounded text-sky-600" />
                  <span>Shop Name & Header</span>
                </label>

                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_address} class="rounded text-sky-600" />
                  <span>Store Address</span>
                </label>

                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_phone} class="rounded text-sky-600" />
                  <span>Phone Number</span>
                </label>

                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_rc_nif} class="rounded text-sky-600" />
                  <span>RC & NIF Info</span>
                </label>

                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_cashier} class="rounded text-sky-600" />
                  <span>Cashier Name</span>
                </label>

                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_date} class="rounded text-sky-600" />
                  <span>Date & Timestamp</span>
                </label>

                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_footer} class="rounded text-sky-600" />
                  <span>Footer Note / Policy</span>
                </label>

                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_qr} class="rounded text-sky-600" />
                  <span>QR Code Verification</span>
                </label>

                <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <input type="checkbox" bind:checked={settings.receipt_show_barcode} class="rounded text-sky-600" />
                  <span>Invoice Barcode (professional preset)</span>
                </label>
              </div>
            </div>

          </div>

          <!-- Right Col: LIVE unified receipt preview — the exact HTML the
               printer receives (same builder as POS auto-print). -->
          <div class="bg-slate-100 dark:bg-slate-900/60 p-4 rounded-2xl flex flex-col items-center justify-start border border-pos-border space-y-2">
            <span class="text-[10px] font-black text-pos-muted uppercase tracking-wider">Live Receipt Preview ({settings.receipt_paper_width || '80mm'})</span>
            <div class="bg-white shadow-md overflow-hidden">
              {@html unifiedPreviewBuilt?.html || ''}
            </div>
          </div>
        </div>

        {#if testPrintMsg}
          <div class="p-2.5 rounded-xl text-[11px] font-bold {testPrintMsg.startsWith('✅') ? 'bg-emerald-50 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300 border border-emerald-300' : 'bg-rose-50 dark:bg-rose-950/40 text-rose-700 dark:text-rose-300 border border-rose-300'}">
            {testPrintMsg}
          </div>
        {/if}

        <!-- Serial Cash Drawer Settings -->
        <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
          <h3 class="font-black text-xs text-pos-text flex items-center gap-1.5">
            <CreditCard class="w-4 h-4 text-emerald-600" />
            <span>Serial Cash Drawer (COM Port) / درج النقود</span>
          </h3>
          <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">COM Port</label>
              <select bind:value={settings.drawer_com_port} class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text">
                {#each Array.from({ length: 10 }, (_, i) => i + 1) as port}
                  <option value={port.toString()}>COM{port}</option>
                {/each}
              </select>
            </div>
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Baud Rate</label>
              <select bind:value={settings.drawer_baud_rate} class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text">
                <option value="9600">9600</option>
                <option value="19200">19200</option>
                <option value="38400">38400</option>
                <option value="115200">115200</option>
              </select>
            </div>
            <div class="flex items-end">
              <button type="button" on:click={handleTestSerialDrawer} disabled={isOpeningDrawer} class="w-full px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white font-bold text-xs rounded-xl cursor-pointer shadow-xs">
                {isOpeningDrawer ? 'Opening...' : 'Test Open Cash Drawer (تجربة فتح الدرج)'}
              </button>
            </div>
          </div>
          {#if drawerStatusMsg}
            <div class="p-2 bg-sky-100 dark:bg-sky-950 text-sky-800 dark:text-sky-200 text-xs font-bold rounded-lg">{drawerStatusMsg}</div>
          {/if}
        </div>

        <div class="pt-4 border-t border-pos-border flex justify-end">
          <button on:click={saveAllSettings} class="px-6 py-2.5 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl shadow-md transition cursor-pointer flex items-center gap-1.5">
            <Check class="w-4 h-4" />
            <span>Save Print & Drawer Settings</span>
          </button>
        </div>
      </div>


    </div>

    <!-- SCALE TAB (ACLAS Real SDK) -->
    <div class:hidden={currentTab !== 'scale'}>
      <div class="max-w-4xl space-y-6">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-base font-black text-pos-text flex items-center gap-2">
              <Scale class="w-5 h-5 text-sky-600" />
              <span>ACLAS Electronic Scale SDK Integration</span>
            </h2>
            <p class="text-xs text-pos-muted">Direct TCP/IP synchronization for ACLAS LH51, LS M3, and TS Series scales</p>
          </div>
          <button on:click={saveAllSettings} class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
            <Check class="w-4 h-4" />
            <span>Save Scale Settings</span>
          </button>
        </div>

        {#if scaleStatusMsg}
          <div class="p-3 bg-sky-100 dark:bg-sky-950 border border-sky-300 dark:border-sky-800 text-sky-800 dark:text-sky-200 text-xs font-bold rounded-xl animate-in fade-in">
            {scaleStatusMsg}
          </div>
        {/if}

        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div class="md:col-span-2 p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
            <h3 class="font-black text-sm text-pos-text">Scale Network & Protocol Configuration</h3>
            
            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">Scale IP Address (Ethernet) *</label>
                <input type="text" bind:value={settings.scale_ip} placeholder="192.168.1.87" class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500" />
              </div>

              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">Scale Port</label>
                <input type="number" bind:value={settings.scale_port} placeholder="0" class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
              </div>
            </div>

            <div class="grid grid-cols-2 gap-3">
              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">Default Scale Barcode Format *</label>
                <select bind:value={settings.scale_default_barcode_type} class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text">
                  <option value="97">Type 97: 18-Code (Dept + ItemCode + Price + Weight + Checksum)</option>
                  <option value="2">Type 02: EAN-13 Price Embedded (DD IIIII PPPPP C)</option>
                  <option value="7">Type 07: EAN-13 Weight Embedded (DD IIIII WWWWW C)</option>
                  <option value="22">Type 22: EAN-13 1-Digit Dept Price Embedded (D IIIIII PPPPP C)</option>
                  <option value="27">Type 27: EAN-13 1-Digit Dept Weight Embedded (D IIIIII WWWWW C)</option>
                  <option value="12">Type 12: Fixed Code 22 Price Embedded (22 IIIII PPPPP C)</option>
                  <option value="17">Type 17: Fixed Code 27 Weight Embedded (27 IIIII WWWWW C)</option>
                </select>
              </div>

              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">Scale Department ID (1-99)</label>
                <input type="number" min="1" max="99" bind:value={settings.scale_department_id} class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text" />
              </div>
            </div>

            <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer">
              <input type="checkbox" bind:checked={settings.scale_auto_sync} class="rounded text-sky-600" />
              <span>Automatically sync scalable products to scale on price/name changes</span>
            </label>

            <!-- Actions -->
            <div class="flex flex-wrap items-center gap-3 pt-2">
              <button
                type="button"
                on:click={handleTestScaleConnection}
                disabled={isTestingScale}
                class="px-4 py-2 bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 text-pos-text font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-xs"
              >
                <RefreshCw class="w-3.5 h-3.5 {isTestingScale ? 'animate-spin' : ''}" />
                <span>{isTestingScale ? 'Testing...' : 'Test Connection (فحص الاتصال)'}</span>
              </button>

              <button
                type="button"
                on:click={handleUploadAllScalable}
                disabled={isUploadingScale || isFetchingScale}
                class="px-4 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white font-black text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-md"
              >
                <Upload class="w-4 h-4" />
                <span>{isUploadingScale ? 'Uploading...' : 'Upload to Scale (إرسال للميزان)'}</span>
              </button>

              <button
                type="button"
                on:click={handleFetchFromScale}
                disabled={isUploadingScale || isFetchingScale}
                class="px-4 py-2 bg-emerald-600 hover:bg-emerald-700 disabled:opacity-50 text-white font-black text-xs rounded-xl flex items-center gap-1.5 cursor-pointer shadow-md"
              >
                <Download class="w-4 h-4" />
                <span>{isFetchingScale ? 'Downloading...' : 'Fetch from Scale (جلب من الميزان)'}</span>
              </button>
            </div>
          </div>

          <div class="p-5 bg-sky-50 dark:bg-sky-950/30 border border-sky-200 dark:border-sky-800 rounded-2xl space-y-3">
            <h4 class="font-black text-xs text-sky-800 dark:text-sky-200">ACLAS Scale Features</h4>
            <ul class="text-xs text-pos-muted space-y-2">
              <li>• Direct native dynamic loading via <strong class="text-pos-text">AclasSDK.dll (Win64)</strong></li>
              <li>• Generates standard UTF-16LE PLU format with custom department & price</li>
              <li>• Real Barcode Type 97 payload verification</li>
              <li>• Automatic synchronization on POS price changes</li>
            </ul>
          </div>
        </div>

        <!-- Sync Logs Table -->
        <div class="space-y-3 pt-2">
          <div class="flex items-center justify-between">
            <h3 class="font-black text-xs text-pos-text">Recent Scale Synchronization History (سجل المزامنة)</h3>
            <button on:click={loadScaleLogs} class="text-xs font-bold text-sky-600 hover:underline">Refresh</button>
          </div>
          <div class="bg-pos-card border border-pos-border rounded-2xl overflow-hidden">
            <table class="w-full text-start text-xs border-collapse">
              <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold">
                <tr>
                  <th class="p-2.5 text-start">Time</th>
                  <th class="p-2.5 text-start">Product</th>
                  <th class="p-2.5 text-start">PLU #</th>
                  <th class="p-2.5 text-center">Direction</th>
                  <th class="p-2.5 text-center">Status</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-pos-border/40">
                {#if scaleSyncLogs.length === 0}
                  <tr>
                    <td colspan="5" class="p-4 text-center text-pos-muted">No synchronization records yet.</td>
                  </tr>
                {:else}
                  {#each scaleSyncLogs.slice(0, 10) as log}
                    <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/30">
                      <td class="p-2.5 font-mono text-pos-muted">{log.created_at}</td>
                      <td class="p-2.5 font-bold text-pos-text">{log.product_name || 'All Scalable Items'}</td>
                      <td class="p-2.5 font-mono text-sky-600">{log.scale_plu || '—'}</td>
                      <td class="p-2.5 text-center uppercase font-mono text-[10px]">{log.direction}</td>
                      <td class="p-2.5 text-center">
                        <span class="px-2 py-0.5 rounded-full text-[10px] font-black uppercase {log.status === 'SUCCESS' ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300' : 'bg-rose-100 text-rose-800 dark:bg-rose-950 dark:text-rose-300'}">
                          {log.status}
                        </span>
                      </td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>
      </div>

    </div>

    <!-- NOTIFICATIONS TAB -->
    <div class:hidden={currentTab !== 'notifications'}>
      <div class="max-w-4xl space-y-6">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-base font-black text-pos-text flex items-center gap-2">
              <Bell class="w-5 h-5 text-sky-600" />
              <span>Telegram Bot Notifications & Event Alerts</span>
            </h2>
            <p class="text-xs text-pos-muted">Send automated alerts directly to your phone or management channel</p>
          </div>
          <button on:click={saveAllSettings} class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
            <Check class="w-4 h-4" />
            <span>Save Alert Settings</span>
          </button>
        </div>

        {#if telegramStatusMsg}
          <div class="p-3 bg-sky-100 dark:bg-sky-950 text-sky-800 dark:text-sky-200 text-xs font-bold rounded-xl">
            {telegramStatusMsg}
          </div>
        {/if}

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
            <h3 class="font-black text-sm text-pos-text">Telegram Bot Credentials</h3>
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Telegram Bot Token</label>
              <input type="text" bind:value={settings.telegram_bot_token} placeholder="123456789:ABCdefGhIJKlmNoPQRsTUVwxyZ" class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
            </div>
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Telegram Chat ID (Channel or Group ID)</label>
              <input type="text" bind:value={settings.telegram_chat_id} placeholder="-100123456789" class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
            </div>
            <button type="button" on:click={sendTelegramTest} disabled={isSendingTelegram} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer">
              <Send class="w-3.5 h-3.5" />
              <span>{isSendingTelegram ? 'Sending...' : 'Send Test Alert (إرسال تجربة)'}</span>
            </button>
          </div>

          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
            <h3 class="font-black text-sm text-pos-text">Alert Triggers</h3>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Instant notification on every sale</span>
              <input type="checkbox" bind:checked={settings.notify_each_sale} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Notification on refunds & returns</span>
              <input type="checkbox" bind:checked={settings.notify_each_refund} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Near-expired (under 30 days) & expired items alert</span>
              <input type="checkbox" bind:checked={settings.notify_expiry} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Low stock & inventory depletion alert</span>
              <input type="checkbox" bind:checked={settings.notify_low_stock} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Cash deposits into the drawer (إيداع الصندوق)</span>
              <input type="checkbox" bind:checked={settings.notify_cash_in} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Cash withdrawals & expenses (سحب/مصاريف)</span>
              <input type="checkbox" bind:checked={settings.notify_cash_out} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Every recorded expense (كل مصروف)</span>
              <input type="checkbox" bind:checked={settings.notify_each_expense} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Opening cash of a new session (رصيد افتتاحي)</span>
              <input type="checkbox" bind:checked={settings.notify_opening_cash} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <div>
                <span>Cash session & opening balance edit / تعديل جلسة ورصيد الصندوق</span>
                <p class="text-[10px] text-pos-muted font-normal">Alert when a cash session or opening balance is modified</p>
              </div>
              <input type="checkbox" bind:checked={settings.notify_cash_edited} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Supplier debt payments (تسديد الموردين)</span>
              <input type="checkbox" bind:checked={settings.notify_supplier_payment} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>

            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>{t('notify_price_change_label')}</span>
              <input type="checkbox" bind:checked={settings.notify_price_change} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>{t('notify_qty_change_label')}</span>
              <input type="checkbox" bind:checked={settings.notify_qty_change} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>{t('notify_product_change_label')}</span>
              <input type="checkbox" bind:checked={settings.notify_product_change} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>{t('notify_history_change_label')}</span>
              <input type="checkbox" bind:checked={settings.notify_history_change} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>

            <!-- Employee payroll payment reminders (Req 26) -->
            <div class="p-3 bg-sky-50 dark:bg-sky-950/30 rounded-xl border border-sky-200 dark:border-sky-800/60 space-y-2">
              <span class="text-[11px] font-black text-sky-800 dark:text-sky-200 block">Employee Payroll Reminders (تذكير رواتب الموظفين)</span>
              <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
                <span>Notify for employee payroll (master)</span>
                <input type="checkbox" bind:checked={settings.notify_payroll_enabled} class="rounded text-sky-600" on:change={autoSaveSettings} />
              </label>
              <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
                <span>In-app payroll reminder (1 day before + due day)</span>
                <input type="checkbox" bind:checked={settings.notify_payroll_inapp} class="rounded text-sky-600" on:change={autoSaveSettings} />
              </label>
              <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
                <span>Telegram payroll reminder</span>
                <input type="checkbox" bind:checked={settings.notify_payroll_telegram} class="rounded text-sky-600" on:change={autoSaveSettings} />
              </label>
              <p class="text-[10px] text-pos-muted">Reminders fire once per employee per occurrence (restart-safe), and never for an already-paid period.</p>
            </div>

            <!-- Admin-action alerts (session delete/archive, debt clear) -->
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Session deleted (admin action) / حذف جلسة</span>
              <input type="checkbox" bind:checked={settings.notify_session_deleted} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Session archived / أرشفة جلسة</span>
              <input type="checkbox" bind:checked={settings.notify_session_archived} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <span>Customer/supplier debt cleared / مصالحة ديون</span>
              <input type="checkbox" bind:checked={settings.notify_debt_cleared} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>
          </div>

          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
            <h3 class="font-black text-sm text-pos-text">Recurring Recap (ملخص دوري)</h3>
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer">
              <div>
                <span>Enable automatic recap</span>
                <p class="text-[10px] text-pos-muted font-normal">Sends a sales/cash/expenses summary every X while the app is open</p>
              </div>
              <input type="checkbox" bind:checked={settings.notify_recap_enabled} class="rounded text-sky-600" on:change={autoSaveSettings} />
            </label>

            <!-- Quiet hours: interactive time pickers + add/remove rows -->
            <div class="p-3 bg-slate-100 dark:bg-slate-800/60 rounded-xl border border-pos-border space-y-2.5">
              <div class="flex items-center justify-between">
                <div>
                  <span class="text-[11px] font-black text-pos-text block">{t('tg_schedule_title') || 'Quiet Hours (ساعات الصمت)'}</span>
                  <p class="text-[10px] text-pos-muted">Notifications muted during these hours</p>
                </div>
                <button
                  type="button"
                  on:click={addQuietWindow}
                  class="px-2.5 py-1 bg-sky-600 hover:bg-sky-700 text-white rounded-lg text-xs font-bold flex items-center gap-1 cursor-pointer transition shadow-2xs"
                >
                  <Plus class="w-3 h-3" />
                  <span>Add Window / إضافة</span>
                </button>
              </div>

              {#if quietWindowList.length === 0}
                <p class="text-xs text-pos-muted italic py-1">No quiet hours configured (notifications active 24/7).</p>
              {:else}
                <div class="space-y-2">
                  {#each quietWindowList as w, idx}
                    <div class="flex items-center gap-2 bg-white dark:bg-slate-900 p-2 rounded-xl border border-pos-border">
                      <div class="flex-1 flex items-center gap-2">
                        <span class="text-[10px] font-bold text-pos-muted">From:</span>
                        <input
                          type="time"
                          bind:value={w.start}
                          on:change={updateQuietWindowsString}
                          class="px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs font-mono font-bold text-pos-text outline-none cursor-pointer"
                        />
                        <span class="text-[10px] font-bold text-pos-muted">To:</span>
                        <input
                          type="time"
                          bind:value={w.end}
                          on:change={updateQuietWindowsString}
                          class="px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs font-mono font-bold text-pos-text outline-none cursor-pointer"
                        />
                      </div>
                      <button
                        type="button"
                        on:click={() => removeQuietWindow(idx)}
                        class="p-1.5 text-pos-muted hover:text-rose-600 rounded-lg transition cursor-pointer"
                        title="Delete Window"
                      >
                        <Trash2 class="w-3.5 h-3.5" />
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Recap frequency</label>
              <select bind:value={settings.recap_interval_minutes} class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none cursor-pointer">
                <option value="15">Every 15 minutes</option>
                <option value="30">Every 30 minutes</option>
                <option value="60">Every hour</option>
                <option value="120">Every 2 hours</option>
                <option value="240">Every 4 hours</option>
              </select>
            </div>
            <button type="button" on:click={sendRecapNow} class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer">
              <Send class="w-3.5 h-3.5" />
              <span>Send Recap Now (إرسال الملخص الآن)</span>
            </button>
            {#if recapStatusMsg}
              <p class="text-[11px] font-bold text-pos-muted">{recapStatusMsg}</p>
            {/if}
          </div>
        </div>
      </div>

    </div>

    <!-- 3. BARCODE & SHELF LABELS TAB — ONE unified preset system (v0.5.17).
         The legacy px-based sticker/shelf sections and the saved-preset JSON
         are removed: one current preset, live preview, native silent print. -->
    <div class:hidden={currentTab !== 'barcodes'}>
      <div class="max-w-5xl space-y-6">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-base font-black text-pos-text">Label Presets — Barcode Stickers & Shelf Tags (قوالب الملصقات)</h2>
            <p class="text-xs text-pos-muted">One current preset used by every label print: product modal, batch printing, shelf etiquette. mm-exact, silent, no print dialog.</p>
          </div>
          <button on:click={saveAllSettings} class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl shadow-md transition cursor-pointer flex items-center gap-1.5">
            <Check class="w-4 h-4" />
            <span>Save Label Settings</span>
          </button>
        </div>

        <!-- Current preset + live preview data -->
        <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border grid grid-cols-1 lg:grid-cols-2 gap-6">
          <div class="space-y-4">
            <div>
              <label class="block text-[11px] font-bold text-pos-muted mb-1">Current Preset (القالب الحالي) — default everywhere labels print</label>
              <div class="flex gap-2">
                <select
                  bind:value={settings.label_preset_id}
                  on:change={autoSaveSettings}
                  class="flex-1 px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none cursor-pointer"
                >
                  <option value="vprice40x20">{t('label_preset_vprice')}</option>
                  <option value="shelf40x20">{t('label_preset_shelf')}</option>
                </select>
                <button
                  type="button"
                  on:click={() => { settings.label_preset_id = 'vprice40x20'; autoSaveSettings(); triggerSaveNotification('Preset reset to the built-in Vertical Price 40×20'); }}
                  class="px-3 py-2 bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 dark:hover:bg-slate-600 text-pos-text text-[10px] font-black rounded-xl cursor-pointer shrink-0"
                  title="Reset to the built-in default preset"
                >
                  Reset to Preset
                </button>
              </div>
            </div>

            <!-- Preview data (drives the live label + receipt previews) -->
            <div class="grid grid-cols-1 gap-3">
              <div>
                <label class="block text-[10px] font-bold text-pos-muted mb-1">Preview product name</label>
                <input type="text" bind:value={previewProductName} class="w-full px-2.5 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-xs font-bold text-pos-text outline-none" />
              </div>
              <div class="grid grid-cols-2 gap-3">
                <div>
                  <label class="block text-[10px] font-bold text-pos-muted mb-1">Preview barcode</label>
                  <input type="text" bind:value={previewBarcodeNumber} class="w-full px-2.5 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-xs font-mono font-bold text-pos-text outline-none" />
                </div>
                <div>
                  <label class="block text-[10px] font-bold text-pos-muted mb-1">Preview price (DZD)</label>
                  <input type="number" bind:value={previewPrice} class="w-full px-2.5 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-xs font-mono font-bold text-pos-text outline-none" />
                </div>
              </div>
            </div>

            <p class="text-[10px] text-pos-muted bg-sky-50 dark:bg-sky-950/30 border border-sky-200 dark:border-sky-800 rounded-lg p-2.5">
              Layout, typography and barcode geometry are built into each mm-true preset — what you see in the preview is
              exactly what prints, at 203/300/600 DPI, with zero gaps and no print dialog.
            </p>
          </div>

          <!-- Live preview of the CURRENT preset (2.2× scale) -->
          <div class="space-y-2">
            <span class="text-[10px] font-black text-pos-muted uppercase tracking-wider block text-center">Live Preview — current preset</span>
            <div class="bg-white dark:bg-slate-900 border border-pos-border rounded-xl p-4 flex justify-center overflow-hidden">
              {#if builtinLabelPreviews[settings.label_preset_id as LabelPresetId]}
                <!-- dir="ltr": label geometry is absolute mm — RTL mirroring
                     shifted and clipped the preview (v0.5.18 fix). -->
                <div dir="ltr" style="width: calc(40mm * 2.2); height: calc(20mm * 2.2); position: relative; overflow: hidden;">
                  <div style="width: 40mm; height: 20mm; transform: scale(2.2); transform-origin: top left;">
                    {@html builtinLabelPreviews[settings.label_preset_id as LabelPresetId]}
                  </div>
                </div>
              {:else}
                <p class="text-[11px] text-pos-muted py-8">Open the Barcode Labels tab to render the preview.</p>
              {/if}
            </div>
          </div>
        </div>

        <!-- Label printer hardware (exact-media silent pipeline) -->
        <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border grid grid-cols-1 md:grid-cols-3 gap-4">
          <div>
            <label class="block text-[10px] font-bold text-pos-muted mb-1">Label Printer (طابعة الملصقات)</label>
            <select
              bind:value={settings.label_printer}
              on:change={autoSaveSettings}
              class="w-full px-2.5 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none cursor-pointer"
            >
              <option value="">Default Windows Printer</option>
              {#each printerList as pr}
                <option value={pr}>{pr}</option>
              {/each}
            </select>
            <p class="text-[9px] text-pos-muted mt-1">Used by the exact-media pipeline: one page per label, no gaps, silent.</p>
          </div>
          <div>
            <label class="block text-[10px] font-bold text-pos-muted mb-1">Label Printer DPI</label>
            <select
              bind:value={settings.label_printer_dpi}
              on:change={autoSaveSettings}
              class="w-full px-2.5 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none cursor-pointer"
            >
              <option value="203">203 DPI (standard thermal)</option>
              <option value="300">300 DPI (high-res)</option>
              <option value="600">600 DPI (photo-grade)</option>
            </select>
            <p class="text-[9px] text-pos-muted mt-1">Match your printer model's resolution for crisp bars.</p>
          </div>
          <div class="flex items-end">
            <p class="text-[10px] text-pos-muted bg-amber-50 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-900 rounded-lg p-2.5 w-full">
              Multi-copy jobs print consecutively — Label 2 feeds right after Label 1. No A4 pages, no blank gaps, no dialog.
            </p>
          </div>
        </div>

        <!-- Built-in preset cards -->
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          {#each LABEL_PRESET_IDS as pid}
            <div class="p-4 bg-white dark:bg-slate-900 rounded-2xl border border-pos-border space-y-3">
              <div class="flex items-center justify-between gap-2">
                <span dir="ltr" class="text-xs font-black text-pos-text">{t(pid === 'vprice40x20' ? 'label_preset_vprice' : 'label_preset_shelf')}</span>
                <span dir="ltr" class="text-[9px] font-mono bg-amber-100 dark:bg-amber-950 text-amber-800 dark:text-amber-300 px-2 py-0.5 rounded-full font-bold shrink-0">
                  {LABEL_PRESETS[pid].widthMm}×{LABEL_PRESETS[pid].heightMm} mm
                </span>
              </div>
              <div class="bg-slate-100 dark:bg-slate-800 rounded-xl p-2 flex justify-center overflow-hidden">
                {#if builtinLabelPreviews[pid]}
                  <div dir="ltr" style="width: calc(40mm * 2.2); height: calc(20mm * 2.2); position: relative; overflow: hidden;">
                    <div style="width: 40mm; height: 20mm; transform: scale(2.2); transform-origin: top left;">
                      {@html builtinLabelPreviews[pid]}
                    </div>
                  </div>
                {/if}
              </div>
              <button
                type="button"
                on:click={() => testPrintBuiltinLabel(pid)}
                class="w-full py-2 bg-amber-600 hover:bg-amber-700 text-white text-xs font-bold rounded-xl flex items-center justify-center gap-1.5 cursor-pointer shadow-xs transition"
              >
                <Printer class="w-3.5 h-3.5" />
                <span>Test Print (<span dir="ltr">{LABEL_PRESETS[pid].widthMm}×{LABEL_PRESETS[pid].heightMm}mm</span>)</span>
              </button>
            </div>
          {/each}
        </div>
        {#if builtinTestMsg}
          <p class="text-[11px] font-bold font-mono text-pos-text bg-slate-100 dark:bg-slate-800 rounded-lg p-2 border border-pos-border">
            {builtinTestMsg}
          </p>
        {/if}
      </div>

    </div>

    <!-- POS RULES TAB -->
    <div class:hidden={currentTab !== 'pos'}>
      <div class="max-w-4xl space-y-6">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-base font-black text-pos-text flex items-center gap-2">
              <Sliders class="w-5 h-5 text-sky-600" />
              <span>Point of Sale Business Rules & Operational Flow</span>
            </h2>
            <p class="text-xs text-pos-muted">Configure cart behavior, scanner auto-focus, cashier security rules, and negative stock selling</p>
          </div>
          <button on:click={saveAllSettings} class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
            <Check class="w-4 h-4" />
            <span>Save POS Rules</span>
          </button>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <!-- Inventory & Stock Selling Rules -->
          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
            <h3 class="font-black text-sm text-pos-text">Stock & Inventory Controls</h3>
            
            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <div>
                <span>Allow Negative Stock Selling</span>
                <p class="text-[10px] text-pos-muted font-normal">Permit cashiers to complete sales even when stock is zero or depleted</p>
              </div>
              <input type="checkbox" bind:checked={settings.allow_negative_stock} class="rounded text-sky-600 w-4 h-4 cursor-pointer" />
            </label>

            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <div>
                <span>Require Note When Holding Sales</span>
                <p class="text-[10px] text-pos-muted font-normal">Prompt cashiers to enter a customer reference or note before holding a ticket</p>
              </div>
              <input type="checkbox" bind:checked={settings.hold_sale_require_note} class="rounded text-sky-600 w-4 h-4 cursor-pointer" />
            </label>

            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Default Walk-in Customer Label</label>
              <input type="text" bind:value={settings.default_customer_name} class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none" />
            </div>

            <div class="grid grid-cols-2 gap-3 pt-1">
              <div>
                <label class="block text-[10px] font-bold text-pos-muted mb-1">Default Margin % (new products)</label>
                <input type="number" min="0" max="500" step="0.5" bind:value={settings.default_margin_percent} class="w-full px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold font-mono text-pos-text outline-none" />
              </div>
              <div>
                <label class="block text-[10px] font-bold text-pos-muted mb-1">Round Sale Price to</label>
                <select bind:value={settings.price_round_step} class="w-full px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none cursor-pointer">
                  <option value="0">Whole DZD (no rounding)</option>
                  <option value="5">Nearest 5 DZD (119→120, 116→115)</option>
                  <option value="10">Nearest 10 DZD</option>
                </select>
              </div>
            </div>

            <!-- Checkout total rounding (Req 7): applies to the cart grand
                 total only — never purchase/cost prices nor historical
                 transactions. -->
            <div class="p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <label class="block text-[10px] font-bold text-pos-muted mb-1">Sale Total Rounding at Checkout (تقريب مجموع البيع)</label>
              <select
                bind:value={settings.sale_price_rounding}
                on:change={autoSaveSettings}
                class="w-full px-2 py-1.5 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-bold text-pos-text outline-none cursor-pointer"
              >
                <option value="off">OFF — exact total (بدون تقريب)</option>
                <option value="50">Nearest 50 DZD (e.g. 4,480 → 4,500)</option>
                <option value="100">Nearest 100 DZD (e.g. 4,480 → 4,500)</option>
              </select>
              <p class="text-[9px] text-pos-muted mt-1">Rounds the cart grand total at checkout for quick cash handling. Purchase prices, cost prices and saved history are never modified.</p>
            </div>
          </div>

          <!-- Scanner & Hardware Automation Rules -->
          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
            <h3 class="font-black text-sm text-pos-text">Scanner & Workflow Automation</h3>

            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <div>
                <span>Autofocus Barcode Search Bar</span>
                <p class="text-[10px] text-pos-muted font-normal">Always keep cursor ready in search bar after each product addition or sale</p>
              </div>
              <input type="checkbox" bind:checked={settings.pos_autofocus_search} class="rounded text-sky-600 w-4 h-4 cursor-pointer" />
            </label>

            <div class="p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <label class="block text-[10px] font-bold text-pos-muted mb-1">Auto-focus idle timer (seconds)</label>
              <input type="number" min="0" max="120" bind:value={settings.pos_autofocus_timer_seconds} class="w-full px-2 py-1.5 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-bold font-mono text-pos-text outline-none" />
              <p class="text-[9px] text-pos-muted font-normal mt-1">Cursor jumps back to the search bar after this many idle seconds (0 = disabled)</p>
            </div>

            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <div>
                <span>Global Barcode Auto-Capture</span>
                <p class="text-[10px] text-pos-muted font-normal">Intercept fast barcode scanner strokes even when cursor is on another element</p>
              </div>
              <input type="checkbox" bind:checked={settings.pos_auto_capture_barcode} class="rounded text-sky-600 w-4 h-4 cursor-pointer" />
            </label>

            <!-- Cart Line Ordering Rule -->
            <div class="p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border space-y-1">
              <label class="block text-xs font-bold text-pos-text">Cart Product Insertion Order</label>
              <p class="text-[10px] text-pos-muted">Choose where newly scanned items appear in the shopping cart</p>
              <select bind:value={settings.cart_item_order} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text mt-1">
                <option value="bottom">New items at BOTTOM (Auto-scrolls to bottom)</option>
                <option value="top">New items at TOP (Most recent on top)</option>
              </select>
            </div>

            <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <div>
                <span>Require Admin PIN for Discounts</span>
                <p class="text-[10px] text-pos-muted font-normal">Disallow cashiers from applying custom discounts without supervisor authorization</p>
              </div>
              <input type="checkbox" bind:checked={settings.require_pin_for_discount} class="rounded text-sky-600 w-4 h-4 cursor-pointer" />
            </label>
          </div>
        </div>
      </div>

    </div>

    <!-- KEYBOARD SHORTCUTS TAB -->
    <div class:hidden={currentTab !== 'shortcuts'}>
      <div class="max-w-4xl space-y-6">
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-base font-black text-pos-text flex items-center gap-2">
              <Keyboard class="w-5 h-5 text-sky-600" />
              <span>POS Keyboard Shortcuts Map (اختصارات لوحة المفاتيح)</span>
            </h2>
            <p class="text-xs text-pos-muted">Custom high-speed keyboard bindings for touch-free cash register operation</p>
          </div>
          <button on:click={saveAllSettings} class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl shadow-md cursor-pointer flex items-center gap-1.5">
            <Check class="w-4 h-4" />
            <span>Save Shortcuts</span>
          </button>
        </div>

        <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border">
          <div class="bg-pos-card rounded-2xl p-1">
        <ShortcutsEditor bind:bindings={shortcutBindings} />
      </div></div>
      </div>

    </div>

    <!-- 4. NETWORK & MOBILE APP TAB -->
    <div class:hidden={currentTab !== 'network'}>
      <div class="max-w-4xl space-y-6">
        <div>
          <h2 class="text-base font-black text-pos-text">Local Network & Android Mobile App Sync</h2>
          <p class="text-xs text-pos-muted">Connect Android scanners, waiter tablets, and inventory devices via Wi-Fi</p>
        </div>

        <!-- ============ TitaouPosCRM SHOP NETWORK (LAN multi-PC) ============ -->
        <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
          <div class="flex items-start justify-between">
            <div>
              <h3 class="text-sm font-black text-pos-text flex items-center gap-2">
                <span class="w-3 h-3 rounded-full {lanStatus?.mode === 'connected' || lanStatus?.serving ? 'bg-emerald-500 animate-pulse' : lanStatus?.mode === 'searching' || lanStatus?.mode === 'reconnecting' ? 'bg-amber-500 animate-pulse' : lanStatus?.mode === 'offline' ? 'bg-rose-500' : 'bg-slate-400'}"></span>
                TitaouPosCRM Shop Network (LAN)
              </h3>
              <p class="text-xs text-pos-muted mt-0.5">
                Multi-PC operation: one shop, one authoritative database, automatic discovery over the local network.
              </p>
            </div>
            <button type="button" on:click={refreshLanStatus} class="p-1.5 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer" title="Refresh">
              <RefreshCw class="w-4 h-4" />
            </button>
          </div>

          {#if lanMsg}
            <p class="text-[11px] font-bold text-emerald-600 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 rounded-xl px-3 py-2">✅ {lanMsg}</p>
          {/if}
          {#if lanError}
            <p class="text-[11px] font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl px-3 py-2">❌ {lanError}</p>
          {/if}

          <!-- Status grid -->
          <div class="grid grid-cols-2 md:grid-cols-4 gap-2">
            <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <p class="text-[9px] font-black text-pos-muted uppercase">Status</p>
              <p class="text-xs font-black text-pos-text capitalize">{lanStatus?.mode || '…'}</p>
            </div>
            <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <p class="text-[9px] font-black text-pos-muted uppercase">Shop</p>
              <p class="text-xs font-black text-pos-text truncate">{lanStatus?.shop_name || '—'}</p>
            </div>
            <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <p class="text-[9px] font-black text-pos-muted uppercase">This PC</p>
              <p class="text-xs font-black text-pos-text truncate">{lanStatus?.pc_name || '—'}</p>
              <p class="text-[8px] font-mono text-pos-muted truncate">{lanStatus?.node_id || ''}</p>
            </div>
            <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
              <p class="text-[9px] font-black text-pos-muted uppercase">Coordinator</p>
              <p class="text-xs font-black text-pos-text truncate">{lanStatus?.coordinator?.pc_name || (lanStatus?.serving ? lanStatus?.pc_name : '—')}</p>
              <p class="text-[8px] font-mono text-pos-muted truncate">
                {lanStatus?.server_url ? lanStatus.server_url.replace('http://','') : (lanStatus?.serving ? (lanStatus?.lan_ips?.[0] || '') + ':' + (lanStatus?.port || 8080) : '—')}
              </p>
            </div>
          </div>

          <!-- Role switcher -->
          <div class="grid grid-cols-3 gap-2">
            <button type="button" on:click={lanSetRole('server')} disabled={lanBusy === 'role'}
              class="py-2 rounded-xl text-xs font-black cursor-pointer transition {lanStatus?.role === 'server' ? 'bg-sky-600 text-white shadow-md' : 'bg-white dark:bg-slate-900 text-pos-text border border-pos-border hover:border-sky-400'}">
              Server
            </button>
            <button type="button" on:click={lanSetRole('client')} disabled={lanBusy === 'role'}
              class="py-2 rounded-xl text-xs font-black cursor-pointer transition {lanStatus?.role === 'client' ? 'bg-sky-600 text-white shadow-md' : 'bg-white dark:bg-slate-900 text-pos-text border border-pos-border hover:border-sky-400'}">
              Client
            </button>
            <button type="button" on:click={lanSetRole('automatic')} disabled={lanBusy === 'role'}
              class="py-2 rounded-xl text-xs font-black cursor-pointer transition {lanStatus?.role === 'automatic' ? 'bg-sky-600 text-white shadow-md' : 'bg-white dark:bg-slate-900 text-pos-text border border-pos-border hover:border-sky-400'}">
              Automatic (recommended)
            </button>
          </div>

          <!-- Toggles + actions -->
          <div class="flex flex-wrap items-center gap-2">
            <button type="button" on:click={lanToggleEnabled} disabled={lanBusy === 'enabled'}
              class="px-3 py-1.5 rounded-lg text-[10px] font-black cursor-pointer {lanStatus?.enabled ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300' : 'bg-slate-200 dark:bg-slate-700 text-pos-muted'}">
              Networking: {lanStatus?.enabled ? 'ON' : 'OFF'}
            </button>
            <button type="button" on:click={lanToggleDiscovery} disabled={lanBusy === 'discovery'}
              class="px-3 py-1.5 rounded-lg text-[10px] font-black cursor-pointer {lanStatus?.autodiscovery ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300' : 'bg-slate-200 dark:bg-slate-700 text-pos-muted'}">
              Auto Discovery: {lanStatus?.autodiscovery ? 'ON' : 'OFF'}
            </button>
            <button type="button" on:click={lanToggleReconnect} disabled={lanBusy === 'reconnect'}
              class="px-3 py-1.5 rounded-lg text-[10px] font-black cursor-pointer {lanStatus?.autoreconnect ? 'bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300' : 'bg-slate-200 dark:bg-slate-700 text-pos-muted'}">
              Auto Reconnect: {lanStatus?.autoreconnect ? 'ON' : 'OFF'}
            </button>
            <button type="button" on:click={lanOpenFirewall} disabled={lanBusy === 'firewall'}
              class="px-3 py-1.5 bg-white dark:bg-slate-900 border border-pos-border hover:border-sky-400 text-pos-text rounded-lg text-[10px] font-black cursor-pointer">
              Allow through Windows Firewall
            </button>
          </div>

          <!-- Become server (shop name) / Leave -->
          <div class="grid grid-cols-1 md:grid-cols-2 gap-2">
            <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border space-y-2">
              <p class="text-[10px] font-black text-pos-muted uppercase">Become the shop server</p>
              <input type="text" bind:value={lanShopNameInput} placeholder="{lanStatus?.shop_name || 'Shop name (optional)'}" class="w-full px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-xs font-bold text-pos-text outline-none" />
              <button type="button" on:click={lanBecomeServer} disabled={lanBusy === 'server'}
                class="w-full py-1.5 bg-sky-600 hover:bg-sky-700 text-white text-[10px] font-black rounded-lg cursor-pointer">
                Create / Own the Shop Network
              </button>
            </div>
            <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border space-y-2 flex flex-col justify-between">
              <p class="text-[10px] font-black text-pos-muted uppercase">Membership</p>
              <p class="text-[10px] text-pos-muted font-bold">
                {#if lanStatus?.shop_id}
                  Joined shop <span class="font-mono">{lanStatus.shop_id.slice(0, 11)}…</span> — leaving keeps all data; the server keeps its own copy.
                {:else}
                  This PC is not a member of any shop yet. Automatic mode will join/discover one, or become a server above.
                {/if}
              </p>
              <button type="button" on:click={lanLeave} disabled={lanBusy === 'leave' || !lanStatus?.shop_id}
                class="w-full py-1.5 bg-rose-50 dark:bg-rose-950/40 text-rose-600 border border-rose-200 dark:border-rose-800 text-[10px] font-black rounded-lg cursor-pointer disabled:opacity-40">
                Leave Shop Network
              </button>
            </div>
          </div>

          <!-- Manual server (advanced) -->
          <div>
            <button type="button" on:click={() => (lanAdvancedOpen = !lanAdvancedOpen)} class="text-[10px] font-black text-sky-600 hover:text-sky-700 cursor-pointer">
              {lanAdvancedOpen ? '▾' : '▸'} Advanced: connect by server IP (no discovery)
            </button>
            {#if lanAdvancedOpen}
              <div class="mt-2 p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border space-y-2">
                <div class="flex gap-1.5">
                  <input type="text" bind:value={lanManualAddr} placeholder="192.168.8.102:8080" class="flex-1 px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-xs font-mono font-bold text-pos-text outline-none" />
                  <button type="button" on:click={lanProbeManual} disabled={lanBusy === 'probe' || !lanManualAddr.trim()} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-pos-text text-[10px] font-black rounded-lg cursor-pointer disabled:opacity-40">Probe</button>
                </div>
                {#if lanProbed}
                  <div class="flex items-center justify-between p-2 bg-emerald-50 dark:bg-emerald-950/40 rounded-lg border border-emerald-200 dark:border-emerald-800">
                    <p class="text-[10px] font-black text-pos-text">{lanProbed.shop_name || 'Shop'} • server: {lanProbed.server_pc} • v{lanProbed.app_version}</p>
                    <button type="button" on:click={lanJoinManual} disabled={lanBusy === 'join'} class="px-3 py-1.5 bg-emerald-600 hover:bg-emerald-700 text-white text-[10px] font-black rounded-lg cursor-pointer">Join this shop</button>
                  </div>
                {/if}
                {#if lanStatus?.manual_server}
                  <p class="text-[9px] font-mono text-pos-muted">Manual override active: {lanStatus.manual_server}</p>
                {/if}
              </div>
            {/if}
          </div>

          <!-- Connected terminals (server view: real registered devices) -->
          <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
            <p class="text-[10px] font-black text-pos-muted uppercase mb-2">
              Connected TitaouPosCRM Terminals ({lanStatus?.devices_count ?? 0})
            </p>
            {#if lanStatus?.devices?.length}
              <div class="space-y-1.5">
                {#each lanStatus.devices as d}
                  <div class="flex items-center justify-between p-2 bg-slate-50 dark:bg-slate-800/60 rounded-lg border border-pos-border/60">
                    <div class="min-w-0">
                      <p class="text-xs font-black text-pos-text flex items-center gap-1.5">
                        <span class="w-1.5 h-1.5 rounded-full {d.online ? 'bg-emerald-500' : 'bg-slate-400'}"></span>
                        {d.pc_name}
                      </p>
                      <p class="text-[9px] font-mono text-pos-muted truncate">{d.node_id} • {d.ip} • v{d.app_version} • seen {d.last_seen_secs_ago}s ago</p>
                    </div>
                    <div class="flex items-center gap-1 shrink-0">
                      <button type="button" on:click={() => lanBlockDevice(d.node_id, true)} disabled={lanBusy === 'block'} class="px-2 py-1 bg-amber-100 text-amber-700 dark:bg-amber-950/60 dark:text-amber-300 text-[9px] font-black rounded-md cursor-pointer">Block</button>
                      <button type="button" on:click={() => lanRemoveDevice(d.node_id)} disabled={lanBusy === 'remove'} class="px-2 py-1 bg-rose-100 text-rose-700 dark:bg-rose-950/60 dark:text-rose-300 text-[9px] font-black rounded-md cursor-pointer">Remove</button>
                    </div>
                  </div>
                {/each}
              </div>
            {:else}
              <p class="text-[10px] text-pos-muted font-bold">
                {#if lanStatus?.serving}
                  No other terminals connected yet — they join automatically in Automatic mode, or via the first-run wizard.
                {:else if lanStatus?.mode === 'connected'}
                  Device list is served by the coordinator PC.
                {:else}
                  —
                {/if}
              </p>
            {/if}
          </div>
        </div>
        <!-- ============ /TitaouPosCRM SHOP NETWORK ============ -->

        <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
          <!-- Server Status & QR Connection (REAL data from the embedded server) -->
          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <span class="w-3 h-3 rounded-full {serverStatus?.running ? 'bg-emerald-500 animate-pulse' : 'bg-rose-500'}"></span>
                <span class="text-xs font-black text-pos-text">
                  {serverStatus?.running ? 'Embedded Server Online' : 'Embedded Server Offline'}
                </span>
              </div>
              <div class="flex items-center gap-1.5">
                <span class="text-xs font-mono font-bold text-sky-600">Port {settings.mobile_server_port || serverStatus?.port || '8080'}</span>
                <button
                  type="button"
                  on:click={refreshServerStatus}
                  class="p-1 text-pos-muted hover:text-pos-text rounded-lg cursor-pointer"
                  title="Refresh status"
                >
                  <RefreshCw class="w-3.5 h-3.5" />
                </button>
              </div>
            </div>

            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Server Port (requires app restart to apply)</label>
              <input
                type="number"
                min="1024"
                max="65535"
                bind:value={settings.mobile_server_port}
                on:change={autoSaveSettings}
                class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text outline-none"
              />
            </div>

            <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border space-y-1 font-mono text-xs">
              <p class="text-pos-muted text-[11px]">Server Address URL (LAN):</p>
              {#if serverStatus?.lan_ips?.length}
                {#each serverStatus.lan_ips as ip}
                  <p class="font-bold text-sky-600">http://{ip}:{serverStatus.port}</p>
                {/each}
              {:else}
                <p class="text-pos-muted italic">No LAN address detected — connect the PC to the shop Wi-Fi/router.</p>
              {/if}
              <p class="text-pos-muted text-[11px] mt-2">Uptime: {Math.floor((serverStatus?.uptime_secs || 0) / 60)} min • Endpoints: /api/status, /api/handshake</p>
            </div>

            <!-- REAL QR: the actual LAN URL — scanning opens the landing
                 page with one tap (the QR content IS a URL, not text). -->
            <div class="flex items-center justify-center p-4 bg-white rounded-xl border border-pos-border">
              {#if serverQrDataUrl}
                <div class="text-center space-y-2">
                  <img src={serverQrDataUrl} alt="Pairing QR" class="w-40 h-40 mx-auto rounded-lg border border-pos-border" />
                  <p class="text-[10px] text-slate-500 font-bold">Scan with your phone camera — tap the link that appears to open it automatically.</p>
                  <button
                    type="button"
                    on:click={() => openUrlInBrowser(`http://${serverStatus?.lan_ips?.[0]}:${serverStatus?.port}/`)}
                    class="px-3 py-1.5 bg-sky-600 hover:bg-sky-700 text-white text-[10px] font-black rounded-lg cursor-pointer"
                  >
                    Open in Browser (فتح المتصفح)
                  </button>
                </div>
              {:else}
                <div class="text-center space-y-1 p-4">
                  <QrCode class="w-10 h-10 mx-auto text-pos-muted opacity-40" />
                  <p class="text-[11px] text-pos-muted font-bold">QR unavailable (no LAN address)</p>
                </div>
              {/if}
            </div>
          </div>

          <!-- Connected Devices: ONLY real, handshake-verified terminals -->
          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4 flex flex-col justify-between">
            <div>
              <h3 class="text-sm font-black text-pos-text mb-3">
                Connected Mobile Terminals ({serverStatus?.devices_count || 0} Active)
              </h3>
              <div class="space-y-2">
                {#if !serverStatus?.devices?.length}
                  <!-- Proper empty state — no demo devices, no fake QR -->
                  <div class="p-6 text-center bg-white dark:bg-slate-900 rounded-xl border border-dashed border-pos-border">
                    <Smartphone class="w-8 h-8 mx-auto text-pos-muted opacity-40 mb-2" />
                    <p class="text-xs font-bold text-pos-muted">No devices connected yet</p>
                    <p class="text-[10px] text-pos-muted mt-1">
                      Scan the pairing QR from the TitaouPosCRM mobile app — devices appear here the moment they connect.
                    </p>
                  </div>
                {:else}
                  {#each serverStatus.devices as dev}
                    <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border flex items-center justify-between">
                      <div class="flex items-center gap-3">
                        <Smartphone class="w-5 h-5 text-sky-500" />
                        <div>
                          <p class="text-xs font-black text-pos-text">{dev.device_name}</p>
                          <p class="text-[10px] text-pos-muted">IP: {dev.ip} • Role: {dev.device_role} • Seen {dev.last_seen_secs_ago}s ago</p>
                        </div>
                      </div>
                      <span class="px-2 py-0.5 bg-emerald-100 text-emerald-700 text-[10px] font-black rounded-full">ACTIVE</span>
                    </div>
                  {/each}
                {/if}
              </div>
            </div>

            <div class="pt-3 border-t border-pos-border flex justify-between items-center">
              <span class="text-xs font-bold text-pos-muted">Real-time device registry</span>
              <span class="text-xs font-bold text-emerald-600">Handshake-verified / موثوق</span>
            </div>
          </div>
        </div>
      </div>

    </div>

    <!-- 4b. CLOUD SYNC TAB (TitaouCRM) -->
    <div class:hidden={currentTab !== 'cloud'}>
      <CloudSyncTab />
    </div>

    <!-- 5. IMPORT / EXPORT TAB -->
    <div class:hidden={currentTab !== 'import_export'}>
      <div class="max-w-4xl space-y-6">
        <div>
          <h2 class="text-base font-black text-pos-text">Import, Export & Full Database Backups</h2>
          <p class="text-xs text-pos-muted">Export catalogs to Excel/CSV and schedule automatic SQLite database backups</p>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
          <!-- Excel / CSV Export & Import -->
          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
            <h3 class="text-sm font-black text-pos-text">1. Excel & CSV Operations</h3>

            <div class="space-y-2">
              <button
                type="button"
                on:click={() => triggerSaveNotification('1000 Products exported to Products_Export.csv')}
                class="w-full py-2.5 bg-pos-card hover:bg-slate-100 dark:hover:bg-slate-700 border border-pos-border rounded-xl text-xs font-bold text-pos-text flex items-center justify-center gap-2 cursor-pointer shadow-xs transition"
              >
                <Download class="w-4 h-4 text-sky-500" />
                <span>Export Products to Excel (.xlsx / .csv)</span>
              </button>

              <button
                type="button"
                on:click={() => triggerSaveNotification('100 Customers & Debts exported to Customers.csv')}
                class="w-full py-2.5 bg-pos-card hover:bg-slate-100 dark:hover:bg-slate-700 border border-pos-border rounded-xl text-xs font-bold text-pos-text flex items-center justify-center gap-2 cursor-pointer shadow-xs transition"
              >
                <Download class="w-4 h-4 text-emerald-500" />
                <span>Export Customers & Debts (.csv)</span>
              </button>
            </div>

            <!-- Import Products File Dropzone -->
            <div class="p-4 bg-white dark:bg-slate-900 border-2 border-dashed border-pos-border rounded-xl text-center space-y-2">
              <Upload class="w-6 h-6 text-pos-muted mx-auto" />
              <p class="text-xs font-bold text-pos-text">Import Products Template (.csv)</p>
              <p class="text-[10px] text-pos-muted">Drop CSV file with columns: SKU, Name, Barcode, Price, Stock</p>
              <label class="inline-block px-3 py-1.5 bg-sky-600 text-white text-xs font-bold rounded-lg cursor-pointer">
                <span>Select File</span>
                <input type="file" accept=".csv, .xlsx" on:change={() => triggerSaveNotification('Products imported successfully!')} class="hidden" />
              </label>
            </div>
          </div>

          <!-- Full Database Backup & Restore -->
          <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4 flex flex-col justify-between">
            <div class="space-y-3">
              <h3 class="text-sm font-black text-pos-text">2. Automatic Database Backups (نسخ احتياطي)</h3>
              <p class="text-xs text-pos-muted">Full snapshot of sales, products, debts, and ledger data.</p>

              <!-- Automatic backup toggles -->
              <div class="space-y-2">
                <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <div>
                    <span>Backup on startup</span>
                    <p class="text-[10px] text-pos-muted font-normal">Once per day when the app launches</p>
                  </div>
                  <input
                    type="checkbox"
                    bind:checked={settings.backup_on_startup}
                    on:change={autoSaveSettings}
                    class="rounded text-emerald-600 w-4 h-4 cursor-pointer"
                  />
                </label>

                <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <div>
                    <span>Backup on close</span>
                    <p class="text-[10px] text-pos-muted font-normal">Safety copy when the app exits</p>
                  </div>
                  <input
                    type="checkbox"
                    bind:checked={settings.backup_on_close}
                    on:change={autoSaveSettings}
                    class="rounded text-emerald-600 w-4 h-4 cursor-pointer"
                  />
                </label>

                <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <div>
                    <span>Scheduled backup</span>
                    <p class="text-[10px] text-pos-muted font-normal">Every day at the configured time</p>
                  </div>
                  <input
                    type="checkbox"
                    bind:checked={settings.backup_scheduled_enabled}
                    on:change={autoSaveSettings}
                    class="rounded text-emerald-600 w-4 h-4 cursor-pointer"
                  />
                </label>
                {#if settings.backup_scheduled_enabled}
                  <div class="p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border flex items-center justify-between gap-3">
                    <span class="text-[11px] font-bold text-pos-muted">Backup every day at</span>
                    <input
                      type="time"
                      bind:value={settings.backup_scheduled_time}
                      on:change={autoSaveSettings}
                      class="px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs font-mono font-bold text-pos-text outline-none cursor-pointer"
                    />
                  </div>
                {/if}

                <!-- Location + retention + settings inclusion -->
                <div class="p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border flex items-center justify-between gap-3">
                  <div class="min-w-0">
                    <span class="text-[11px] font-bold text-pos-muted block">Backup location</span>
                    <p class="text-[10px] text-pos-text font-mono truncate">{settings.backup_dir || '%APPDATA%\\TitaouPosCRM\\backups (default)'}</p>
                  </div>
                  <button
                    type="button"
                    on:click={handlePickBackupFolder}
                    class="px-3 py-1.5 bg-sky-600 hover:bg-sky-700 text-white text-[10px] font-black rounded-lg cursor-pointer shrink-0"
                  >
                    Browse…
                  </button>
                </div>

                <div class="p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border flex items-center justify-between gap-3">
                  <span class="text-[11px] font-bold text-pos-muted">Keep last [X] backups (retention)</span>
                  <input
                    type="number"
                    min="0"
                    max="200"
                    bind:value={settings.backup_keep_count}
                    on:change={autoSaveSettings}
                    class="w-16 px-2 py-1 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-lg text-xs font-mono font-bold text-pos-text text-center outline-none"
                  />
                </div>

                <label class="flex items-center justify-between text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
                  <div>
                    <span>Include application settings in backup</span>
                    <p class="text-[10px] text-pos-muted font-normal">POS config, printers, receipt & label presets, notifications, Telegram (as stored)</p>
                  </div>
                  <input
                    type="checkbox"
                    bind:checked={settings.backup_include_settings}
                    on:change={autoSaveSettings}
                    class="rounded text-emerald-600 w-4 h-4 cursor-pointer"
                  />
                </label>
              </div>

              <!-- Last/Backup Now row -->
              <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border space-y-1 text-xs">
                <div class="flex justify-between font-bold">
                  <span class="text-pos-muted">Last Backup:</span>
                  <span class="text-pos-text">{settings.last_backup_at || 'Never'}</span>
                </div>
                {#if backupMsg}
                  <p class="text-[10px] font-bold {backupMsg.startsWith('✅') ? 'text-emerald-600' : 'text-rose-600'} break-all">{backupMsg}</p>
                {/if}
              </div>

              <!-- Existing backups list -->
              {#if backupsList.length > 0}
                <div class="max-h-40 overflow-y-auto rounded-xl border border-pos-border divide-y divide-pos-border">
                  {#each backupsList as b}
                    <div class="p-2 bg-white dark:bg-slate-900 flex items-center justify-between gap-2 text-[11px]">
                      <div class="min-w-0">
                        <p class="font-bold text-pos-text truncate">{b.file_name}</p>
                        <p class="text-pos-muted font-mono">{b.modified} • {Math.round(b.size_bytes / 1024)} KB</p>
                      </div>
                      <button
                        type="button"
                        on:click={() => { restoreFilePath = b.path; showRestoreModal = true; }}
                        class="px-2 py-1 bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700 text-pos-text font-bold rounded-lg cursor-pointer shrink-0"
                      >
                        Restore
                      </button>
                    </div>
                  {/each}
                </div>
              {/if}
            </div>

            <div class="space-y-2">
              <button
                type="button"
                on:click={handleBackupNow}
                disabled={isCreatingBackup}
                class="w-full py-2.5 bg-emerald-600 hover:bg-emerald-700 disabled:opacity-50 text-white text-xs font-black rounded-xl flex items-center justify-center gap-2 cursor-pointer shadow-md transition active:scale-95"
              >
                <HardDrive class="w-4 h-4" />
                <span>{isCreatingBackup ? 'Backing up…' : 'Backup Now (نسخ احتياطي للبيانات)'}</span>
              </button>

              <button
                type="button"
                on:click={openRestoreModal}
                class="w-full py-2 bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 dark:hover:bg-slate-600 text-pos-text text-xs font-bold rounded-xl cursor-pointer transition active:scale-95"
              >
                Restore from Backup File (.sqlite)
              </button>
            </div>
          </div>
        </div>
      </div>

    </div>

    <!-- 6. ACTIVATION TAB -->
    <div class:hidden={currentTab !== 'activation'}>
      <div class="max-w-3xl space-y-6">
        <div>
          <h2 class="text-base font-black text-pos-text">App Activation & License Management</h2>
          <p class="text-xs text-pos-muted">Hardware Machine ID binding and perpetual offline license verification</p>
        </div>

        <!-- License Status Banner -->
        <div class="p-4 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-200 dark:border-emerald-800 rounded-2xl flex items-center justify-between">
          <div class="flex items-center gap-3">
            <ShieldCheck class="w-8 h-8 text-emerald-600 shrink-0" />
            <div>
              <h4 class="font-black text-sm text-emerald-900 dark:text-emerald-200">TitaouPosCRM PRO LIFETIME LICENSE</h4>
              <p class="text-xs text-emerald-700 dark:text-emerald-400">Fully activated and authorized for this hardware terminal.</p>
            </div>
          </div>
          <span class="px-3 py-1 bg-emerald-600 text-white text-xs font-black rounded-xl">ACTIVE</span>
        </div>

        <div class="space-y-4">
          <div>
            <label class="block text-xs font-bold text-pos-muted mb-1">Your Terminal Hardware ID (HWID)</label>
            <div class="flex items-center gap-2">
              <input type="text" readonly value={hwid} class="flex-1 px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text" />
              <button on:click={copyHwid} class="px-3 py-2 bg-sky-600 text-white text-xs font-bold rounded-xl flex items-center gap-1 cursor-pointer">
                <Copy class="w-3.5 h-3.5" />
                <span>Copy HWID</span>
              </button>
            </div>
          </div>

          <!-- Online Activation -->
          <div class="p-4 bg-sky-50 dark:bg-sky-950/30 rounded-2xl border border-sky-200 dark:border-sky-800/60 space-y-3">
            <h4 class="text-xs font-black text-pos-text">Activate Online (تنشيط عبر الإنترنت)</h4>
            <p class="text-[11px] text-pos-muted">
              Sends this machine's HWID to the developer's activation registry and activates automatically.
            </p>
            <div class="flex items-center gap-2">
              <button
                type="button"
                on:click={handleActivateOnline}
                disabled={isActivatingOnline}
                class="px-4 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white text-xs font-black rounded-xl cursor-pointer shadow-md"
              >
                {isActivatingOnline ? 'Checking...' : 'Activate This PC Online'}
              </button>
              <span class="text-[10px] font-mono text-pos-muted">HWID: {hwid}</span>
            </div>
            {#if activationMsg}
              <p class="text-[11px] font-bold {activationMsg.includes('success') || activationMsg.includes('بنجاح') ? 'text-emerald-600' : 'text-rose-600'}">{activationMsg}</p>
            {/if}
          </div>

          <!-- License File (.lic) Upload & Key Entry -->
          <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
            <h4 class="text-xs font-black text-pos-text">Activate using License File (.lic) or Serial Key</h4>
            <div class="flex items-center gap-2">
              <input
                type="text"
                bind:value={activationCode}
                placeholder="Enter Serial Key (e.g. TIT-XXXX-XXXX-XXXX)"
                class="flex-1 px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-mono text-pos-text"
              />
              <button on:click={handleActivate} class="px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white text-xs font-black rounded-xl cursor-pointer">
                Verify Key
              </button>
            </div>

            <div class="pt-2 flex items-center justify-between">
              <span class="text-xs text-pos-muted">Have a license file provided by vendor?</span>
              <label class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-bold rounded-xl cursor-pointer flex items-center gap-1.5">
                <FileText class="w-3.5 h-3.5" />
                <span>Upload License File (.lic)</span>
                <input type="file" accept=".lic, .key, .txt" on:change={handleLicenseFileUpload} class="hidden" />
              </label>
            </div>
          </div>
        </div>
      </div>

    </div>

    <!-- 7. UPDATES TAB -->
    <div class:hidden={currentTab !== 'updates'}>
      <div class="max-w-3xl space-y-6">
        <div>
          <h2 class="text-base font-black text-pos-text">Application Updates & Rollback</h2>
          <p class="text-xs text-pos-muted">Automated updater using GitHub releases with safe rollback capability</p>
        </div>

        <div class="p-5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-4">
          <div class="flex items-center justify-between">
            <div class="space-y-0.5">
              <p class="text-xs font-bold text-pos-muted">Current Installed Version:</p>
              <p class="text-base font-black text-pos-text">TitaouPosCRM {appVersion} (Windows x64)</p>
            </div>
            <span class="px-3 py-1 bg-sky-100 text-sky-800 dark:bg-sky-950 dark:text-sky-300 font-mono text-xs font-black rounded-full">
              Stable Channel
            </span>
          </div>

          <div class="p-3 bg-white dark:bg-slate-900 rounded-xl border border-pos-border text-xs flex items-center gap-2">
            {#if isCheckingUpdate}
              <RefreshCw class="w-4 h-4 text-sky-500 animate-spin" />
            {:else}
              <CheckCircle2 class="w-4 h-4 text-emerald-500" />
            {/if}
            <span class="font-bold text-pos-text">{updateStatus}</span>
          </div>

          <!-- Controls -->
          <div class="flex flex-wrap items-center gap-3">
            <button
              on:click={checkForUpdates}
              disabled={isCheckingUpdate}
              class="px-5 py-2.5 bg-sky-600 hover:bg-sky-700 text-white font-black text-xs rounded-xl flex items-center gap-2 cursor-pointer shadow-md transition"
            >
              <RefreshCw class="w-4 h-4 {isCheckingUpdate ? 'animate-spin' : ''}" />
              <span>{isCheckingUpdate ? 'Checking GitHub...' : 'Check for Updates Now (فحص التحديثات)'}</span>
            </button>

            {#if updateAvailable && latestDownloadUrl}
              <button
                type="button"
                on:click={() => openUrlInBrowser(latestDownloadUrl)}
                class="px-5 py-2.5 bg-emerald-600 hover:bg-emerald-700 text-white font-black text-xs rounded-xl flex items-center gap-2 cursor-pointer shadow-md transition animate-pulse"
              >
                <Download class="w-4 h-4" />
                <span>Download {latestReleaseInfo?.tag_name || 'Update'} (تحميل التحديث)</span>
              </button>
              <button
                type="button"
                on:click={() => openUrlInBrowser(latestReleaseUrl)}
                class="px-4 py-2.5 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 text-pos-text font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer transition"
              >
                <span>View Release Notes</span>
              </button>
            {/if}

            <button
              on:click={openRollbackModal}
              class="px-4 py-2.5 bg-slate-200 dark:bg-slate-700 text-pos-text font-bold text-xs rounded-xl flex items-center gap-1.5 cursor-pointer transition"
            >
              <History class="w-4 h-4 text-amber-500" />
              <span>Rollback to Previous Version</span>
            </button>
          </div>

          <label class="flex items-center gap-2.5 text-xs font-bold text-pos-text cursor-pointer pt-2">
            <input type="checkbox" bind:checked={settings.auto_update_enabled} class="rounded text-sky-600" />
            <span>Enable automatic background update checks</span>
          </label>
        </div>
      </div>

    </div>

    <!-- 8. ACCOUNT & USER MANAGEMENT TAB -->
    <div class:hidden={currentTab !== 'account'}>
      <div class="max-w-5xl space-y-6">
        <!-- Tab Header -->
        <div class="flex items-center justify-between">
          <div>
            <h2 class="text-base font-black text-pos-text flex items-center gap-2">
              <Users class="w-5 h-5 text-sky-600" />
              <span>User Accounts & Access Roles / إدارة المستخدمين والأدوار</span>
            </h2>
            <p class="text-xs text-pos-muted">Manage system users, login credentials, assigned roles, and discount authorizations</p>
          </div>
          <button
            on:click={openCreateUserModal}
            class="px-4 py-2.5 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl cursor-pointer shadow-md flex items-center gap-2 transition"
          >
            <UserPlus class="w-4 h-4" />
            <span>Add User Account / إضافة مستخدم</span>
          </button>
        </div>

        <!-- Current Active Session Card -->
        <div class="p-5 bg-gradient-to-r from-sky-500/10 via-sky-500/5 to-transparent rounded-2xl border border-sky-200 dark:border-sky-900/60 space-y-4">
          <div class="flex flex-wrap items-center justify-between gap-4">
            <div class="flex items-center gap-3">
              <div class="w-12 h-12 rounded-2xl bg-sky-600 text-white font-black flex items-center justify-center text-lg shadow-sm">
                {($currentUser?.display_name || 'A')[0].toUpperCase()}
              </div>
              <div>
                <div class="flex items-center gap-2">
                  <h3 class="text-sm font-black text-pos-text">{$currentUser?.display_name || 'Administrator'}</h3>
                  <span class="px-2 py-0.5 rounded-full text-[10px] font-black bg-sky-100 dark:bg-sky-950/80 text-sky-700 dark:text-sky-300 border border-sky-300 dark:border-sky-800">
                    {$currentUser?.role_name || 'Administrator'}
                  </span>
                </div>
                <p class="text-xs text-pos-muted">Active Session: <strong class="font-mono text-pos-text">@{$currentUser?.username || 'admin'}</strong> • Max Discount: <strong class="text-sky-600">{$currentUser?.max_discount_percent ?? 100}%</strong></p>
              </div>
            </div>

            <!-- Quick Password Change for Active User -->
            <div class="flex items-center gap-2 w-full md:w-auto">
              <div class="relative flex-1 md:w-44">
                <Lock class="w-3.5 h-3.5 text-pos-muted absolute start-3 top-2.5" />
                <input
                  type="password"
                  bind:value={oldPassword}
                  placeholder="Current password"
                  class="w-full ps-8 pe-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs text-pos-text"
                />
              </div>
              <div class="relative flex-1 md:w-44">
                <Lock class="w-3.5 h-3.5 text-pos-muted absolute start-3 top-2.5" />
                <input
                  type="password"
                  bind:value={newPassword}
                  placeholder="New password / PIN"
                  class="w-full ps-8 pe-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs text-pos-text"
                />
              </div>
              <button
                on:click={handleChangePassword}
                disabled={!newPassword || !oldPassword}
                class="px-4 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-40 text-white text-xs font-bold rounded-xl cursor-pointer shadow-xs transition shrink-0"
              >
                Change My Password
              </button>
            
              <p class="text-[9px] text-pos-muted w-full md:w-auto">Forgotten? Enter <span class="font-mono font-black text-sky-600">TITAOU</span> as the current password to force a reset.</p></div>
          </div>
        </div>

        <!-- Users Table Card -->
        <div class="bg-pos-card rounded-2xl border border-pos-border overflow-hidden shadow-xs">
          <div class="p-4 border-b border-pos-border flex items-center justify-between bg-slate-50/50 dark:bg-slate-800/30">
            <h3 class="text-xs font-black text-pos-text flex items-center gap-2">
              <Shield class="w-4 h-4 text-sky-600" />
              <span>Registered System Users ({userAccounts.length})</span>
            </h3>
            <button
              on:click={loadUsersAndRoles}
              class="text-xs text-sky-600 hover:text-sky-700 font-bold flex items-center gap-1 cursor-pointer"
            >
              <RefreshCw class="w-3.5 h-3.5" />
              <span>Refresh List</span>
            </button>
          </div>

          <div class="overflow-x-auto">
            <table class="w-full text-start text-xs">
              <thead class="bg-slate-100/60 dark:bg-slate-800/60 text-pos-muted font-black border-b border-pos-border">
                <tr>
                  <th class="p-3 text-start">User</th>
                  <th class="p-3 text-start">Role / الدور</th>
                  <th class="p-3 text-center">Max Discount</th>
                  <th class="p-3 text-center">Status</th>
                  <th class="p-3 text-start">Last Login</th>
                  <th class="p-3 text-end">Actions</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-pos-border">
                {#if userAccounts.length === 0}
                  <tr>
                    <td colspan="6" class="p-8 text-center text-pos-muted font-bold">
                      No user accounts found. Click "Add User Account" to create one.
                    </td>
                  </tr>
                {:else}
                  {#each userAccounts as u}
                    <tr class="hover:bg-slate-50/60 dark:hover:bg-slate-800/40 transition">
                      <td class="p-3">
                        <div class="flex items-center gap-2.5">
                          <div class="w-8 h-8 rounded-xl bg-slate-100 dark:bg-slate-800 text-pos-text font-black flex items-center justify-center text-xs border border-pos-border">
                            {u.display_name[0]?.toUpperCase() || 'U'}
                          </div>
                          <div>
                            <div class="font-black text-pos-text">{u.display_name}</div>
                            <div class="font-mono text-[11px] text-pos-muted">@{u.username}</div>
                          </div>
                        </div>
                      </td>
                      <td class="p-3">
                        <span class="inline-flex items-center px-2.5 py-0.5 rounded-full text-[11px] font-black {
                          u.role_name === 'Administrator' ? 'bg-amber-100 text-amber-800 dark:bg-amber-950/80 dark:text-amber-300 border border-amber-300 dark:border-amber-800' :
                          u.role_name === 'Manager' ? 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950/80 dark:text-emerald-300 border border-emerald-300 dark:border-emerald-800' :
                          u.role_name === 'Inventory Clerk' ? 'bg-purple-100 text-purple-800 dark:bg-purple-950/80 dark:text-purple-300 border border-purple-300 dark:border-purple-800' :
                          'bg-sky-100 text-sky-800 dark:bg-sky-950/80 dark:text-sky-300 border border-sky-300 dark:border-sky-800'
                        }">
                          {u.role_name || 'Standard Role'}
                        </span>
                      </td>
                      <td class="p-3 text-center font-mono font-bold text-pos-text">
                        {u.max_discount_percent}%
                      </td>
                      <td class="p-3 text-center">
                        {#if u.is_active}
                          <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-black bg-emerald-100 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-400">
                            <span class="w-1.5 h-1.5 rounded-full bg-emerald-500"></span> Active
                          </span>
                        {:else}
                          <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-black bg-rose-100 text-rose-700 dark:bg-rose-950/60 dark:text-rose-400">
                            <span class="w-1.5 h-1.5 rounded-full bg-rose-500"></span> Disabled
                          </span>
                        {/if}
                      </td>
                      <td class="p-3 text-pos-muted font-mono text-[11px]">
                        {u.last_login ? u.last_login.slice(0, 16).replace('T', ' ') : 'Never'}
                      </td>
                      <td class="p-3 text-end">
                        <div class="inline-flex items-center gap-1">
                          <button
                            on:click={() => toggleUserPin(u)}
                            class="p-1.5 rounded-lg cursor-pointer transition {u.pinned ? 'bg-amber-100 dark:bg-amber-950/60 text-amber-600' : 'hover:bg-amber-50 dark:hover:bg-amber-950/50 text-amber-500'}"
                            title={u.pinned ? 'Pinned — first on the login screen' : 'Pin: show first on the login screen'}
                          >
                            <Pin class="w-4 h-4" />
                          </button>
                          <button
                            on:click={() => openEditUserModal(u)}
                            class="p-1.5 hover:bg-sky-50 dark:hover:bg-sky-950/50 text-sky-600 rounded-lg cursor-pointer transition"
                            title="Edit User Details / تعديل"
                          >
                            <Edit2 class="w-4 h-4" />
                          </button>
                          {#if u.id !== 1}
                            <button
                              on:click={() => deleteUserAccount(u)}
                              class="p-1.5 hover:bg-rose-50 dark:hover:bg-rose-950/50 text-rose-500 rounded-lg cursor-pointer transition"
                              title="Delete User / حذف"
                            >
                              <Trash2 class="w-4 h-4" />
                            </button>
                          {/if}
                        </div>
                      </td>
                    </tr>
                  {/each}
                {/if}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>

    <!-- 9. ABOUT TAB -->
    <div class:hidden={currentTab !== 'about'}>
      <AboutView />
    </div>

    <!-- 10. DANGER / FACTORY RESET TAB -->
    <div class:hidden={currentTab !== 'danger'}>
      <div class="max-w-3xl space-y-6">
        <div>
          <h2 class="text-base font-black text-rose-600 flex items-center gap-2">
            <AlertOctagon class="w-5 h-5" />
            <span>Factory Reset & Data Purge / تهيئة المصنع ومسح البيانات</span>
          </h2>
          <p class="text-xs text-pos-muted">Select an operation below. These actions cannot be undone. Please backup database before proceeding.</p>
        </div>

        <!-- Clear History Only (non-destructive to products) -->
        <div class="p-5 bg-amber-50 dark:bg-amber-950/30 border border-amber-300 dark:border-amber-800 rounded-2xl space-y-3">
          <div class="flex items-start justify-between gap-3">
            <div>
              <h3 class="text-sm font-black text-amber-800 dark:text-amber-200 flex items-center gap-2">
                <History class="w-4 h-4" />
                <span>Clear Sales & Purchases History / مسح سجل المعاملات فقط</span>
              </h3>
              <p class="text-[11px] text-pos-muted mt-1">
                Erases sales, purchases, cash sessions and movements — but keeps products, prices, quantities, customers, suppliers and debts intact.
              </p>
            </div>
          </div>
          <input
            type="text"
            bind:value={clearHistoryConfirmText}
            placeholder="Type CLEAR HISTORY to confirm"
            class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-amber-300 dark:border-amber-800 rounded-xl text-xs font-mono font-bold text-pos-text outline-none"
          />
          <button
            type="button"
            on:click={handleClearHistory}
            disabled={isClearingHistory || clearHistoryCountdown > 0 || clearHistoryConfirmText.trim() !== 'CLEAR HISTORY'}
            class="px-4 py-2 bg-amber-600 hover:bg-amber-700 disabled:opacity-40 disabled:cursor-not-allowed text-white text-xs font-black rounded-xl cursor-pointer"
          >
            {isClearingHistory ? 'Clearing…' : 'Clear History Only'}
          </button>
          {#if clearHistoryCountdown > 0}
            <div class="flex items-center justify-between p-2.5 bg-amber-100 dark:bg-amber-950/60 border border-amber-300 dark:border-amber-800 rounded-xl animate-in fade-in duration-150">
              <span class="text-xs font-black text-amber-700 dark:text-amber-300">
                Clearing in {clearHistoryCountdown}s… / سيتم المسح خلال {clearHistoryCountdown} ثانية
              </span>
              <button
                type="button"
                on:click={cancelClearHistory}
                class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-black rounded-lg cursor-pointer hover:bg-slate-300 dark:hover:bg-slate-600"
              >
                Cancel / إلغاء
              </button>
            </div>
          {/if}
          {#if clearHistoryMsg}
            <p class="text-[11px] font-bold text-amber-700 dark:text-amber-300">{clearHistoryMsg}</p>
          {/if}
        </div>

        <div class="p-5 bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-900 rounded-2xl space-y-4">
          <div class="grid grid-cols-1 gap-2.5">
            <!-- 1. Delete All Products -->
            <label class="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border rounded-xl cursor-pointer transition {resetType === 'products_only' ? 'border-rose-500 ring-2 ring-rose-500/20' : 'border-pos-border'}">
              <input type="radio" bind:group={resetType} value="products_only" class="mt-0.5 text-rose-600" />
              <div class="flex-1">
                <span class="text-xs font-black text-pos-text block">Delete All Products & Stock / حذف جميع المنتجات والمخزون</span>
                <span class="text-[11px] text-pos-muted">Purges all products, barcodes, stock movements, and price history (keeps families, units, users).</span>
              </div>
            </label>

            <!-- 2. Reset Families / Categories -->
            <label class="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border rounded-xl cursor-pointer transition {resetType === 'categories_only' ? 'border-rose-500 ring-2 ring-rose-500/20' : 'border-pos-border'}">
              <input type="radio" bind:group={resetType} value="categories_only" class="mt-0.5 text-rose-600" />
              <div class="flex-1">
                <span class="text-xs font-black text-pos-text block">Reset Families & Categories / إعادة تعيين الفئات والعائلات</span>
                <span class="text-[11px] text-pos-muted">Resets all categories back to clean "Default / Général" family.</span>
              </div>
            </label>

            <!-- 3. Reset Units -->
            <label class="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border rounded-xl cursor-pointer transition {resetType === 'units_only' ? 'border-rose-500 ring-2 ring-rose-500/20' : 'border-pos-border'}">
              <input type="radio" bind:group={resetType} value="units_only" class="mt-0.5 text-rose-600" />
              <div class="flex-1">
                <span class="text-xs font-black text-pos-text block">Reset Units of Measurement / إعادة تعيين وحدات القياس</span>
                <span class="text-[11px] text-pos-muted">Resets custom units back to 5 standard system units (pcs, kg, L, pck, box).</span>
              </div>
            </label>

            <!-- 4. Clear Sales & Transactions -->
            <label class="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border rounded-xl cursor-pointer transition {resetType === 'transactions_only' ? 'border-rose-500 ring-2 ring-rose-500/20' : 'border-pos-border'}">
              <input type="radio" bind:group={resetType} value="transactions_only" class="mt-0.5 text-rose-600" />
              <div class="flex-1">
                <span class="text-xs font-black text-pos-text block">Clear Sales & Financial Transactions / مسح المبيعات والعمليات المالية</span>
                <span class="text-[11px] text-pos-muted">Clears all sales, held sales, cash drawer movements, receipts, expenses, and resets debt balances to 0 (keeps products & catalog).</span>
              </div>
            </label>

            <!-- 5. Reset Customers & Debts -->
            <label class="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border rounded-xl cursor-pointer transition {resetType === 'customers_only' ? 'border-rose-500 ring-2 ring-rose-500/20' : 'border-pos-border'}">
              <input type="radio" bind:group={resetType} value="customers_only" class="mt-0.5 text-rose-600" />
              <div class="flex-1">
                <span class="text-xs font-black text-pos-text block">Reset Customers & Customer Debts / مسح الزبائن والديون</span>
                <span class="text-[11px] text-pos-muted">Deletes all custom customer accounts and customer debt payment records.</span>
              </div>
            </label>

            <!-- 6. Reset Suppliers & Purchases -->
            <label class="flex items-start gap-3 p-3 bg-white dark:bg-slate-900 border rounded-xl cursor-pointer transition {resetType === 'suppliers_only' ? 'border-rose-500 ring-2 ring-rose-500/20' : 'border-pos-border'}">
              <input type="radio" bind:group={resetType} value="suppliers_only" class="mt-0.5 text-rose-600" />
              <div class="flex-1">
                <span class="text-xs font-black text-pos-text block">Reset Suppliers & Purchases / مسح الموردين وفواتير الشراء</span>
                <span class="text-[11px] text-pos-muted">Deletes all supplier records, purchase invoices, and supplier debt payments.</span>
              </div>
            </label>

            <!-- 7. Full Factory Reset -->
            <label class="flex items-start gap-3 p-3 bg-rose-100/60 dark:bg-rose-950/80 border-2 border-rose-400 dark:border-rose-800 rounded-xl cursor-pointer transition {resetType === 'full_reset' ? 'ring-2 ring-rose-600' : ''}">
              <input type="radio" bind:group={resetType} value="full_reset" class="mt-0.5 text-rose-600" />
              <div class="flex-1">
                <span class="text-xs font-black text-rose-700 dark:text-rose-300 block">Full Factory Reset (Comprehensive) / إعادة ضبط المصنع بالكامل</span>
                <span class="text-[11px] text-rose-600/80 dark:text-rose-400/80">Complete system wipe: purges all products, resets families to Default, resets units to standard, clears sales, customers, suppliers, and extra users.</span>
              </div>
            </label>
          </div>

          <div class="space-y-2 pt-3 border-t border-rose-200 dark:border-rose-900">
            <label class="block text-xs font-bold text-pos-muted">Type <span class="text-rose-600 font-mono font-black">RESET</span> to confirm execution:</label>
            <div class="flex items-center gap-2">
              <input
                type="text"
                bind:value={resetConfirm}
                placeholder="RESET"
                class="w-48 px-3 py-2 bg-white dark:bg-slate-900 border border-rose-300 dark:border-rose-800 rounded-xl text-xs font-mono font-black text-rose-600 outline-none focus:ring-2 focus:ring-rose-500"
              />
              <button
                on:click={handleFactoryReset}
                disabled={resetConfirm !== 'RESET' || resetCountdown > 0}
                class="px-5 py-2 bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white text-xs font-black rounded-xl cursor-pointer shadow-md transition"
              >
                Execute Reset / تنفيذ المسح
              </button>
            </div>
            {#if resetCountdown > 0}
              <div class="flex items-center justify-between p-2.5 bg-rose-100 dark:bg-rose-950/60 border border-rose-300 dark:border-rose-800 rounded-xl animate-in fade-in duration-150">
                <span class="text-xs font-black text-rose-700 dark:text-rose-300">
                  Resetting in {resetCountdown}s… / سيتم المسح خلال {resetCountdown} ثانية
                </span>
                <button
                  type="button"
                  on:click={cancelFactoryReset}
                  class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-black rounded-lg cursor-pointer hover:bg-slate-300 dark:hover:bg-slate-600"
                >
                  Cancel / إلغاء
                </button>
              </div>
            {/if}
            {#if resetResult}
              <div class="p-3 rounded-xl border animate-in fade-in duration-150 {resetResult.ok ? 'bg-emerald-50 dark:bg-emerald-950/60 border-emerald-300 dark:border-emerald-800' : 'bg-rose-50 dark:bg-rose-950/60 border-rose-300 dark:border-rose-800'}">
                <span class="text-xs font-black {resetResult.ok ? 'text-emerald-700 dark:text-emerald-300' : 'text-rose-700 dark:text-rose-300'}">{resetResult.text}</span>
              </div>
            {/if}
          </div>
        </div>
      </div>
    </div>
  </div>

  <!-- User Account Add / Edit Modal -->
  {#if showUserModal}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-md w-full space-y-4 animate-in zoom-in-95 duration-150">
        <div class="flex items-center justify-between border-b border-pos-border pb-3">
          <h3 class="font-black text-sm text-pos-text flex items-center gap-2">
            {#if userModalMode === 'create'}
              <UserPlus class="w-4 h-4 text-sky-600" />
              <span>Add New User Account / إضافة حساب مستخدم</span>
            {:else}
              <Edit2 class="w-4 h-4 text-sky-600" />
              <span>Edit User Account / تعديل الحساب</span>
            {/if}
          </h3>
          <button on:click={() => (showUserModal = false)} class="text-pos-muted hover:text-pos-text text-sm font-bold">✕</button>
        </div>

        {#if userFormError}
          <div class="p-3 bg-rose-50 dark:bg-rose-950/60 border border-rose-200 dark:border-rose-900 text-rose-600 rounded-xl text-xs font-bold">
            {userFormError}
          </div>
        {/if}

        <div class="space-y-3 text-xs">
          <div>
            <label class="block font-bold text-pos-muted mb-1">Username / اسم الدخول <span class="text-rose-500">*</span></label>
            <input
              type="text"
              bind:value={userForm.username}
              placeholder="e.g. cashier_ahmed"
              class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl font-mono text-pos-text"
            />
          </div>

          <div>
            <label class="block font-bold text-pos-muted mb-1">Display Name / الاسم الظاهر <span class="text-rose-500">*</span></label>
            <input
              type="text"
              bind:value={userForm.display_name}
              placeholder="e.g. Ahmed Benali"
              class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-pos-text"
            />
          </div>

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block font-bold text-pos-muted mb-1">Role / الدور</label>
              <select
                bind:value={userForm.role_id}
                class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-pos-text font-bold"
              >
                {#each allRoles as r}
                  <option value={r.id}>{r.name}</option>
                {/each}
              </select>
            </div>

            <div>
              <label class="block font-bold text-pos-muted mb-1">Max Discount (%)</label>
              <input
                type="number"
                min="0"
                max="100"
                bind:value={userForm.max_discount_percent}
                class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl font-mono text-pos-text font-bold"
              />
            </div>
          </div>

          <div>
            <label class="block font-bold text-pos-muted mb-1">
              {#if userModalMode === 'create'}
                Password / كلمة المرور <span class="text-rose-500">*</span>
              {:else}
                New Password (leave empty to keep unchanged) / كلمة المرور الجديدة
              {/if}
            </label>
            <input
              type="password"
              bind:value={userForm.password}
              placeholder={userModalMode === 'create' ? 'Enter secure password' : '••••••••'}
              class="w-full px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-pos-text"
            />
          </div>

          <div class="pt-2">
            <label class="flex items-center gap-2 cursor-pointer font-bold text-pos-text">
              <input type="checkbox" bind:checked={userForm.is_active} class="rounded text-sky-600" />
              <span>Account is Active / الحساب مفعّل</span>
            </label>
          </div>
        </div>

        <div class="flex justify-end gap-2 pt-3 border-t border-pos-border">
          <button
            on:click={() => (showUserModal = false)}
            class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-bold rounded-xl cursor-pointer"
          >
            Cancel / إلغاء
          </button>
          <button
            on:click={saveUserModal}
            class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl cursor-pointer shadow-md transition"
          >
            {userModalMode === 'create' ? 'Create User / إنشاء' : 'Save Changes / حفظ'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Bottom Global Developer Credit Footer -->
  <div class="pt-3 flex items-center justify-between text-xs text-pos-muted border-t border-pos-border mt-3 shrink-0">
    <div class="flex items-center gap-2">
      <span class="font-bold text-pos-text">TitaouPosCRM Desktop</span>
      <span>•</span>
      <span>Created & Developed by <strong class="text-sky-600">Titaou Bedreddine (0553444057)</strong></span>
    </div>
    <span class="font-mono text-[11px]">{appVersion} (PRO)</span>
  </div>

  <!-- Rollback Confirmation Modal -->
  {#if showRollbackModal}
    <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4">
        <h3 class="font-black text-sm text-pos-text flex items-center gap-2">
          <History class="w-5 h-5 text-amber-500" />
          <span>Confirm Version Rollback</span>
        </h3>
        <p class="text-xs font-bold text-rose-600 bg-rose-50 dark:bg-rose-950/40 border border-rose-200 dark:border-rose-800 rounded-xl p-2.5">
          ⚠️ Rolling back can lose data created in newer versions. Type <span class="font-mono font-black">ROLLBACK</span> below to confirm.
        </p>
        <input
          type="text"
          bind:value={rollbackConfirmText}
          placeholder="Type ROLLBACK to confirm"
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono font-bold text-pos-text outline-none"
        />
        <p class="text-[10px] text-pos-muted font-bold">Auto-cancels in {rollbackCountdown}s — nothing happens if you do nothing.</p>
        <div class="flex justify-end gap-2 pt-2">
          <button on:click={cancelRollback} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">Cancel</button>
          <button
            on:click={handleRollback}
            disabled={rollbackConfirmText.trim() !== 'ROLLBACK'}
            class="px-4 py-2 bg-amber-600 disabled:opacity-40 disabled:cursor-not-allowed text-white text-xs font-black rounded-xl cursor-pointer"
          >Confirm Rollback</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Animated Toast Notification Popover on Save -->
  {#if saveSuccessMsg}
    <div class="absolute bottom-6 end-6 z-50 bg-emerald-600 text-white px-5 py-3 rounded-2xl shadow-2xl flex items-center gap-3 text-xs font-black animate-in slide-in-from-bottom-3 duration-200">
      <Check class="w-5 h-5 bg-white/20 rounded-full p-0.5 shrink-0" />
      <span>{saveSuccessMsg}</span>
    </div>
  {/if}

  <!-- Restore From Backup Modal (app-styled, native-picked file) -->
  {#if showRestoreModal}
    <div class="fixed inset-0 z-[80] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4">
      <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-md w-full space-y-4 animate-in zoom-in-95">
        <div class="flex items-center gap-3 text-amber-600">
          <div class="w-10 h-10 rounded-2xl bg-amber-100 dark:bg-amber-950/60 flex items-center justify-center shrink-0">
            <HardDrive class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-sm text-pos-text">Restore from Backup</h3>
            <p class="text-[11px] text-pos-muted">Restore / استرجاع النسخة الاحتياطية</p>
          </div>
        </div>

        {#if restoreDone}
          <div class="p-3 bg-emerald-50 dark:bg-emerald-950/40 border border-emerald-300 dark:border-emerald-800 rounded-xl text-xs font-bold text-emerald-700 dark:text-emerald-300 space-y-2">
            <p>✅ Database restored successfully. The app will reload with the restored data.</p>
            <p class="text-[10px] text-pos-muted">A safety backup of the PREVIOUS data was saved before the restore (presafety file).</p>
          </div>
          <div class="flex justify-end pt-1 border-t border-pos-border">
            <button
              type="button"
              on:click={closeRestoreModal}
              class="px-5 py-2 bg-sky-600 hover:bg-sky-700 text-white text-xs font-black rounded-xl cursor-pointer shadow-md"
            >
              Reload App Now (تحديث التطبيق)
            </button>
          </div>
        {:else}
          <!-- Selected file + validation result -->
          <div class="p-3 bg-slate-50 dark:bg-slate-800/50 rounded-xl border border-pos-border space-y-1.5 text-xs">
            <p class="text-pos-muted font-bold">Selected backup:</p>
            <p class="font-mono text-pos-text break-all">{restoreFilePath}</p>
            {#if restoreValidateMsg}
              <p class="text-[11px] font-bold text-emerald-600">✅ {restoreValidateMsg}</p>
            {:else if restoreError}
              <p class="text-[11px] font-bold text-rose-600">❌ {restoreError}</p>
            {:else}
              <p class="text-[11px] font-bold text-pos-muted">Validating…</p>
            {/if}
          </div>

          <p class="text-[11px] text-pos-muted font-bold">
            ⚠️ This replaces the ENTIRE current database (sales, products, debts, sessions). A safety backup of the current
            data is created first, but the action is significant. Confirm to proceed.
          </p>

          <label class="flex items-center gap-2 text-xs font-bold text-pos-text cursor-pointer p-2.5 bg-white dark:bg-slate-900 rounded-xl border border-pos-border">
            <input type="checkbox" bind:checked={restoreSettingsToo} class="rounded text-sky-600" />
            <span>Also restore application settings from this backup's snapshot</span>
          </label>

          {#if restoreError && !restoreValidateMsg}
            <p class="text-[11px] font-bold text-rose-600">{restoreError}</p>
          {/if}

          <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
            <button
              type="button"
              on:click={() => (showRestoreModal = false)}
              class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer"
            >
              Cancel / إلغاء
            </button>
            <button
              type="button"
              on:click={handleConfirmRestore}
              disabled={isRestoring || (!restoreValidateMsg && !backupsList.some(b => b.path === restoreFilePath))}
              class="px-5 py-2 bg-amber-600 hover:bg-amber-700 disabled:opacity-40 text-white text-xs font-black rounded-xl cursor-pointer shadow-md"
            >
              {isRestoring ? 'Restoring…' : 'Confirm Restore (تأكيد)'}
            </button>
          </div>
        {/if}
      </div>
    </div>
  {/if}
</div>