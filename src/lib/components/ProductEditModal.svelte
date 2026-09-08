<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { normalizeBarcode } from '../utils/barcode';
  import WebcamCapture from './WebcamCapture.svelte';
  import { t, currentLocale } from '../i18n';
  import { currentUser } from '../stores/auth';
  import type { Category, Product, ProductInput, Unit } from '../types';
  import PrintLabelModal from './PrintLabelModal.svelte';
  import {
    X, Check, Plus, Trash2, Edit2, Tag, Upload, Calendar,
    DollarSign, Package, History, Layers, AlertTriangle, Scale, RefreshCw, Send,
    Copy, Percent, Sparkles, FolderPlus, QrCode, Printer, Banknote
  } from 'lucide-svelte';

  export let isOpen = false;
  export let product: Product | null = null;
  export let categories: Category[] = [];
  export let units: Unit[] = [];
  export let initialBarcode: string = '';
  // Scanned barcode to attach to an EXISTING product (link flow): merged into
  // the token list on open so the user reviews and saves consciously.
  export let extraBarcode: string = '';
  export let onClose: () => void;
  export let onSaved: () => void;

  let activeTab: 'details' | 'scalable' | 'packaging' | 'history' = 'details';
  let applyMsg = '';
  let savedProductId: number | null = null;

  let sku = '';
  let nameAr = '';
  let nameFr = '';
  let nameEn = '';
  let categoryId: number | null = 1;
  let unitId: number | null = 1;
  let purchasePrice = 0;
  let salePrice = 0;
  let minSalePrice = 0;
  let taxRate = 19;
  let currentStock = 0;
  let minStock = 5;
  let imagePath = '';
  let isWebcamOpen = false;
  let expiryDate = '';
  let isBundle = false;

  // Scale Integration
  let isScalable = false;
  let scaleCode = '';
  let scalePlu = 1;
  let scaleBarcodeType = 97;
  let scaleDepartmentId = 1;
  let scaleSyncStatus = 'pending';
  let isSyncingScale = false;
  let scaleSyncMsg = '';

  // Units-in-packaging: carton 24 / fardeau 6 / palette 672 / plateau 30.
  // Each packaging multiplies the base-unit quantity at purchase/sale.
  interface PackagingRow {
    name: string;
    unitsPerPackage: number;
    salePrice: number;
    isDefault: boolean;
  }
  let packagingRows: PackagingRow[] = [];

  function addPackagingRow() {
    packagingRows = [...packagingRows, { name: '', unitsPerPackage: 2, salePrice: 0, isDefault: false }];
  }

  function removePackagingRow(idx: number) {
    packagingRows = packagingRows.filter((_, i) => i !== idx);
  }

  async function loadPackagings() {
    if (!product) {
      packagingRows = [];
      return;
    }
    try {
      const list = await invoke<any[]>('list_packagings', { productId: product.id });
      packagingRows = list.map((p) => ({
        name: p.name,
        unitsPerPackage: p.units_per_package,
        salePrice: p.sale_price,
        isDefault: !!p.is_default,
      }));
    } catch {
      packagingRows = [];
    }
  }

  // Save packagings right after the product save succeeds (needs the id).
  async function persistPackagings(savedProductId: number) {
    try {
      await invoke('save_packagings', {
        productId: savedProductId,
        inputs: packagingRows
          .filter((r) => r.name.trim() && r.unitsPerPackage > 1)
          .map((r) => ({
            name: r.name.trim(),
            units_per_package: r.unitsPerPackage,
            sale_price: r.salePrice || 0,
            is_default: r.isDefault,
          })),
      });
    } catch (e) {
      console.warn('Packagings save failed:', e);
    }
  }

  // Barcode tokenizer
  let barcodeTokens: string[] = [];
  let currentBarcodeTyped = '';
  let editingTokenIndex: number | null = null;

  // Live duplicate-barcode warning: checked while the cashier types so a
  // conflict surfaces BEFORE the save attempt, not as a rejected save.
  let duplicateBarcodeWarning = '';
  let barcodeCheckTimer: any = null;

  function checkBarcodeDuplicate() {
    clearTimeout(barcodeCheckTimer);
    const code = normalizeBarcode(currentBarcodeTyped).replace(/,/g, '');
    if (!code) {
      duplicateBarcodeWarning = '';
      return;
    }
    barcodeCheckTimer = setTimeout(async () => {
      try {
        const list = await invoke<any[]>('search_products', {
          query: code,
          categoryId: null,
          searchType: 'barcode',
        });
        const owner = list.find((p) => p.barcodes?.includes(code) || p.sku === code);
        // Editing a product: its own barcodes are of course fine.
        const ownId = product?.id ?? null;
        if (owner && owner.id !== ownId) {
          duplicateBarcodeWarning = `Barcode ${code} is already used by "${owner.name_fr || owner.name_ar}" — save will be rejected / الباركود مستعمل من قبل منتج آخر`;
        } else {
          duplicateBarcodeWarning = '';
        }
      } catch {
        duplicateBarcodeWarning = '';
      }
    }, 250);
  }

  // Profit / Margin Mode
  let marginMode: 'percent' | 'amount' = 'percent';
  let profitMarginPercent = 20;
  let profitMarginAmount = 0;

  // Settings-driven pricing defaults: rounding step (0/5/10...) and default
  // margin for NEW products (existing products reuse their own margin).
  let priceRoundStep = 5;
  let defaultMarginPercent = 20;

  // Stock fields are linked: current (already in shop) + new (being added now)
  // = total (what current_stock will become on save).
  let newStockQty = 0;
  let baselineStock = 0;

  async function loadPricingDefaults() {
    try {
      const s = await invoke<Record<string, string>>('get_all_settings');
      priceRoundStep = parseInt(s['price_round_step'] ?? '5', 10) || 0;
      defaultMarginPercent = parseFloat(s['default_margin_percent'] ?? '20') || 0;
    } catch (e) {
      console.warn('Could not load pricing defaults:', e);
    }
  }

  // Round to the configured step: 119 -> 120 (step 5), 116 -> 115.
  function roundToStep(value: number, step: number): number {
    if (!step || step <= 0) return Math.round(value);
    return Math.round(value / step) * step;
  }

  // Name autosuggest: live suggestions from existing catalog products.
  let nameSuggestions: { field: 'fr' | 'ar' | 'en'; value: string }[] = [];
  let showNameSuggestions = false;
  let nameSuggestTimeout: any = null;

  function refreshNameSuggestions(typed: string, field: 'fr' | 'ar' | 'en') {
    const q = typed.trim().toLowerCase();
    if (q.length < 2) {
      nameSuggestions = [];
      return;
    }
    clearTimeout(nameSuggestTimeout);
    nameSuggestTimeout = setTimeout(async () => {
      try {
        const list = await invoke<Product[]>('search_products', {
          query: typed.trim(),
          categoryId: null,
          searchType: 'name',
        });
        const seen = new Set<string>();
        const matches: { field: 'fr' | 'ar' | 'en'; value: string }[] = [];
        for (const p of list.slice(0, 8)) {
          for (const [f, v] of [['fr', p.name_fr], ['ar', p.name_ar], ['en', p.name_en]] as const) {
            const val = (v || '').trim();
            if (val && val.toLowerCase().includes(q) && !seen.has(val)) {
              seen.add(val);
              matches.push({ field: f as 'fr' | 'ar' | 'en', value: val });
              if (matches.length >= 6) break;
            }
          }
          if (matches.length >= 6) break;
        }
        nameSuggestions = matches;
        showNameSuggestions = matches.length > 0;
      } catch (e) {
        nameSuggestions = [];
      }
    }, 200);
  }

  function acceptNameSuggestion(s: { field: 'fr' | 'ar' | 'en'; value: string }) {
    if (s.field === 'fr') nameFr = s.value;
    else if (s.field === 'ar') nameAr = s.value;
    else nameEn = s.value;
    nameSuggestions = [];
    showNameSuggestions = false;
  }

  function handleNameKeydown(e: KeyboardEvent) {
    if (e.key === 'Tab' && showNameSuggestions && nameSuggestions.length > 0) {
      e.preventDefault();
      acceptNameSuggestion(nameSuggestions[0]);
    } else if (e.key === 'Escape') {
      showNameSuggestions = false;
    }
  }

  // Quick Add Family Modal
  let isQuickFamilyOpen = false;
  let quickFamilyNameFr = '';
  let quickFamilyNameAr = '';
  let quickFamilyColor = '#0284c7';
  let isSavingFamily = false;

  // Quick Add Unit Modal
  let isQuickUnitOpen = false;
  let quickUnitName = '';
  let quickUnitShort = '';
  let isSavingUnit = false;

  // Print Label Modal Integration
  let isPrintLabelOpen = false;
  let printLabelInitialType: 'barcode' | 'etiquette' = 'barcode';
  let printLabelInitialQty = 1;
  let printProductObj: Product | null = null;

  // Price history (persisted by the backend on every price change)
  interface PriceHistoryEntry {
    id: number;
    old_purchase_price: number;
    new_purchase_price: number;
    old_sale_price: number;
    new_sale_price: number;
    user_id?: number | null;
    created_at: string;
  }
  let priceHistory: PriceHistoryEntry[] = [];

  // Quantity history (every stock movement: sale, purchase, refund, manual
  // adjustment, broken — old → new with delta, user, timestamp).
  interface QuantityHistoryEntry {
    id: number;
    movement_type: string;
    old_quantity: number;
    new_quantity: number;
    difference: number;
    user_name?: string | null;
    created_at: string;
    notes?: string | null;
  }
  let quantityHistory: QuantityHistoryEntry[] = [];
  let historyView: 'price' | 'quantity' = 'price';

  async function loadPriceHistory() {
    if (!product || !product.id) {
      priceHistory = [];
      return;
    }
    try {
      priceHistory = await invoke<PriceHistoryEntry[]>('get_price_history', { productId: product.id });
    } catch (e) {
      console.warn('Could not load price history:', e);
      priceHistory = [];
    }
  }

  async function loadQuantityHistory() {
    if (!product || !product.id) {
      quantityHistory = [];
      return;
    }
    try {
      quantityHistory = await invoke<QuantityHistoryEntry[]>('get_quantity_history', { productId: product.id });
    } catch (e) {
      console.warn('Could not load quantity history:', e);
      quantityHistory = [];
    }
  }

  function movementTypeLabel(type: string): { label: string; cls: string } {
    switch (type) {
      case 'sale': return { label: t('qh_sale'), cls: 'text-rose-600 dark:text-rose-400' };
      case 'sale_refund': return { label: t('qh_sale_refund'), cls: 'text-amber-600 dark:text-amber-400' };
      case 'purchase': return { label: t('qh_purchase'), cls: 'text-emerald-600 dark:text-emerald-400' };
      case 'adjustment_inc': return { label: t('qh_adjustment_inc'), cls: 'text-sky-600 dark:text-sky-400' };
      case 'adjustment_dec': return { label: t('qh_adjustment_dec'), cls: 'text-orange-600 dark:text-orange-400' };
      case 'bundle_sale': return { label: t('qh_bundle_sale'), cls: 'text-rose-600 dark:text-rose-400' };
      case 'bundle_refund': return { label: t('qh_bundle_refund'), cls: 'text-amber-600 dark:text-amber-400' };
      default: return { label: type, cls: 'text-pos-muted' };
    }
  }

  // Load history whenever its tab is opened for an existing product.
  $: if (isOpen && activeTab === 'history' && product?.id) {
    loadPriceHistory();
    loadQuantityHistory();
  }

  // Enter moves focus down the pricing/stock chain: cost -> margin ->
  // sale price -> new qty -> current stock -> total (id listed on each
  // input via data-enter-next / id).
  function handlePricingEnter(e: KeyboardEvent) {
    if (e.key !== 'Enter') return;
    const el = e.currentTarget as HTMLElement;
    const nextId = el.getAttribute('data-enter-next');
    const target =
      (nextId && document.getElementById(nextId)) ||
      (el.id === 'pem-total' ? document.getElementById('pem-new-qty') : null);
    if (target) {
      e.preventDefault();
      (target as HTMLInputElement).focus();
      (target as HTMLInputElement).select();
    }
  }

  // Number inputs must not change when the user scrolls the modal.
  function noWheelScroll(e: WheelEvent) {
    (e.currentTarget as HTMLElement).blur();
  }

  function generateRandomSku() {
    sku = 'PRD-' + Math.floor(1000 + Math.random() * 9000);
  }

  function getTransientProduct(): Product {
    const bCodes = [...barcodeTokens];
    if (currentBarcodeTyped.trim() && !bCodes.includes(currentBarcodeTyped.trim())) {
      bCodes.push(currentBarcodeTyped.trim());
    }
    return {
      id: product ? product.id : 0,
      sku: sku || undefined,
      name_ar: nameAr.trim() || nameFr.trim() || 'Produit',
      name_fr: nameFr.trim() || nameAr.trim() || 'Produit',
      name_en: nameEn.trim() || nameFr.trim() || 'Product',
      category_id: categoryId,
      category_name: categories.find(c => c.id === categoryId)?.name_fr || '',
      unit_id: unitId,
      unit_name: units.find(u => u.id === unitId)?.name || '',
      purchase_price: Number(purchasePrice) || 0,
      sale_price: Number(salePrice) || 0,
      min_sale_price: Number(minSalePrice) || 0,
      tax_rate: Number(taxRate) || 19,
      current_stock: Number(currentStock) || 0,
      min_stock: Number(minStock) || 5,
      image_path: imagePath || undefined,
      expiry_date: expiryDate || undefined,
      is_scalable: isScalable,
      scale_code: scaleCode || undefined,
      scale_plu: isScalable ? scalePlu : undefined,
      scale_barcode_type: isScalable ? scaleBarcodeType : undefined,
      scale_department_id: isScalable ? scaleDepartmentId : undefined,
      scale_sync_status: scaleSyncStatus,
      is_bundle: isBundle,
      barcodes: bCodes,
    };
  }

  function openPrintSticker() {
    printProductObj = getTransientProduct();
    printLabelInitialType = 'barcode';
    printLabelInitialQty = Number(currentStock) > 0 ? Number(currentStock) : 1;
    isPrintLabelOpen = true;
  }

  function openPrintShelf() {
    printProductObj = getTransientProduct();
    printLabelInitialType = 'etiquette';
    printLabelInitialQty = 1;
    isPrintLabelOpen = true;
  }

  let isSaving = false;
  let errorMsg = '';
  let copyFeedback = '';

  function handlePurchasePriceChange() {
    if (marginMode === 'percent') {
      salePrice = roundToStep(purchasePrice * (1 + profitMarginPercent / 100), priceRoundStep);
      profitMarginAmount = salePrice - purchasePrice;
    } else {
      salePrice = roundToStep(purchasePrice + profitMarginAmount, priceRoundStep);
      if (purchasePrice > 0) {
        profitMarginPercent = Math.round((profitMarginAmount / purchasePrice) * 100);
      }
    }
  }

  function handleMarginPercentChange() {
    if (purchasePrice > 0) {
      salePrice = roundToStep(purchasePrice * (1 + profitMarginPercent / 100), priceRoundStep);
      profitMarginAmount = salePrice - purchasePrice;
    }
  }

  function handleMarginAmountChange() {
    salePrice = roundToStep(purchasePrice + profitMarginAmount, priceRoundStep);
    if (purchasePrice > 0) {
      profitMarginPercent = Math.round((profitMarginAmount / purchasePrice) * 100);
    }
  }

  function handleSalePriceChange() {
    if (purchasePrice > 0) {
      profitMarginAmount = salePrice - purchasePrice;
      profitMarginPercent = Math.round(((salePrice - purchasePrice) / purchasePrice) * 100);
    }
  }

  // Linked stock trio: baseline (already in shop) + new (added now) = total.
  function handleNewStockChange() {
    currentStock = Number(baselineStock) + Number(newStockQty || 0);
  }

  function handleTotalStockChange() {
    // Editing the total directly back-calculates the "new" quantity.
    newStockQty = Number(currentStock) - Number(baselineStock);
  }

  function handleBaselineStockChange() {
    // Editing what's already in stock keeps the added quantity intact.
    newStockQty = Number(currentStock) - Number(baselineStock);
  }

  function generateValidEan13() {
    // Prefix 613 (Algeria GS1) + 9 random digits = 12 digits
    let base = '613' + Math.floor(100000000 + Math.random() * 900000000).toString().slice(0, 9);
    // Calculate Modulo 10 Checksum
    let sum = 0;
    for (let i = 0; i < 12; i++) {
      const digit = parseInt(base[i]);
      sum += i % 2 === 0 ? digit * 1 : digit * 3;
    }
    const checksum = (10 - (sum % 10)) % 10;
    const ean13 = base + checksum.toString();
    if (!barcodeTokens.includes(ean13)) {
      barcodeTokens = [...barcodeTokens, ean13];
    }
    currentBarcodeTyped = '';
  }

  function generateScalePluCode() {
    scalePlu = Math.floor(100 + Math.random() * 899);
    scaleCode = '95' + Math.floor(1000 + Math.random() * 8999).toString();
  }

  function copyToClipboard(text: string) {
    navigator.clipboard.writeText(text);
    copyFeedback = 'Copied!';
    setTimeout(() => (copyFeedback = ''), 2000);
  }

  let lastIsOpen = false;
  let lastProductId: number | null | undefined = undefined;
  let lastInitialBarcode = '';
  let lastExtraBarcode = '';

  function initializeModal() {
    if (product) {
      sku = product.sku || '';
      nameAr = product.name_ar || '';
      nameFr = product.name_fr || '';
      nameEn = product.name_en || '';
      categoryId = product.category_id || (categories.length > 0 ? categories[0].id : 1);
      unitId = product.unit_id || (units.length > 0 ? units[0].id : 1);
      purchasePrice = product.purchase_price || 0;
      salePrice = product.sale_price || 0;
      minSalePrice = product.min_sale_price || 0;
      taxRate = product.tax_rate || 19;
      currentStock = product.current_stock || 0;
      minStock = product.min_stock || 5;
      imagePath = product.image_path || '';
      expiryDate = product.expiry_date || '';
      isBundle = product.is_bundle || false;
      isScalable = product.is_scalable || false;
      scaleCode = product.scale_code || '';
      scalePlu = product.scale_plu || (product.id || 1);
      scaleBarcodeType = product.scale_barcode_type || 97;
      scaleDepartmentId = product.scale_department_id || 1;
      scaleSyncStatus = product.scale_sync_status || 'pending';
      barcodeTokens = product.barcodes ? [...product.barcodes] : [];
      if (extraBarcode && extraBarcode.trim() && !barcodeTokens.includes(extraBarcode.trim())) {
        barcodeTokens = [...barcodeTokens, extraBarcode.trim()];
      }
      currentBarcodeTyped = '';
      editingTokenIndex = null;
      // An existing product keeps ITS OWN margin (derived from its prices).
      handleSalePriceChange();
      // Stock: current stock is the baseline, nothing added yet.
      baselineStock = product.current_stock || 0;
      currentStock = baselineStock;
      newStockQty = 0;
      activeTab = 'details';
    } else {
      sku = '';
      nameAr = '';
      nameFr = '';
      nameEn = '';
      categoryId = categories.length > 0 ? categories[0].id : 1;
      unitId = units.length > 0 ? units[0].id : 1;
      purchasePrice = 0;
      salePrice = 0;
      minSalePrice = 0;
      taxRate = 19;
      currentStock = 0;
      minStock = 5;
      imagePath = '';
      expiryDate = '';
      isBundle = false;
      isScalable = false;
      scaleCode = '';
      scalePlu = 1;
      scaleBarcodeType = 97;
      scaleDepartmentId = 1;
      scaleSyncStatus = 'pending';
      barcodeTokens = initialBarcode ? [initialBarcode.trim()] : [];
      currentBarcodeTyped = '';
      editingTokenIndex = null;
      // New product: settings default margin, no existing stock.
      loadPricingDefaults().then(() => {
        profitMarginPercent = defaultMarginPercent;
        profitMarginAmount = 0;
        handlePurchasePriceChange();
      });
      baselineStock = 0;
      newStockQty = 0;
      activeTab = 'details';
    }
    // Packaging editor: load existing definitions for this product.
    loadPackagings();

    // Common reset for suggestions on every open.
    nameSuggestions = [];
    showNameSuggestions = false;
    // A rejected save (e.g. duplicate name/barcode) must not haunt the next
    // product the cashier opens.
    errorMsg = '';
    duplicateBarcodeWarning = '';
    currentBarcodeTyped = '';
  }

  $: if (isOpen && (!lastIsOpen || (product ? product.id : null) !== lastProductId || initialBarcode !== lastInitialBarcode || extraBarcode !== lastExtraBarcode)) {
    lastIsOpen = true;
    lastProductId = product ? product.id : null;
    lastInitialBarcode = initialBarcode;
    lastExtraBarcode = extraBarcode;
    loadPricingDefaults();
    initializeModal();
  } else if (!isOpen) {
    lastIsOpen = false;
    lastProductId = undefined;
    lastInitialBarcode = '';
    lastExtraBarcode = '';
  }

  function addBarcodeToken() {
    const raw = normalizeBarcode(currentBarcodeTyped).replace(/,/g, '');
    if (!raw) return;
    if (editingTokenIndex !== null) {
      barcodeTokens[editingTokenIndex] = raw;
      editingTokenIndex = null;
    } else {
      if (!barcodeTokens.includes(raw)) {
        barcodeTokens = [...barcodeTokens, raw];
      }
    }
    currentBarcodeTyped = '';
    duplicateBarcodeWarning = '';
  }

  function handleBarcodeKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      addBarcodeToken();
    }
  }

  function removeBarcodeToken(index: number) {
    barcodeTokens = barcodeTokens.filter((_, i) => i !== index);
  }

  function startEditToken(index: number) {
    editingTokenIndex = index;
    currentBarcodeTyped = barcodeTokens[index];
  }

  function handleImageUpload(e: Event) {
    const target = e.target as HTMLInputElement;
    if (target.files && target.files[0]) {
      const file = target.files[0];
      const reader = new FileReader();
      reader.onload = (ev) => {
        imagePath = ev.target?.result as string;
      };
      reader.readAsDataURL(file);
    }
  }

  async function handleSaveQuickFamily() {
    if (!quickFamilyNameFr && !quickFamilyNameAr) return;
    try {
      isSavingFamily = true;
      const newId = await invoke<number>('save_category', {
        nameAr: quickFamilyNameAr || quickFamilyNameFr,
        nameFr: quickFamilyNameFr || quickFamilyNameAr,
        nameEn: quickFamilyNameFr || quickFamilyNameAr,
        color: quickFamilyColor || '#0284c7',
        categoryId: null,
      });
      categories = await invoke<Category[]>('get_categories');
      categoryId = newId;
      isQuickFamilyOpen = false;
      quickFamilyNameFr = '';
      quickFamilyNameAr = '';
    } catch (e: any) {
      alert('Failed to add category: ' + (e.message || e));
    } finally {
      isSavingFamily = false;
    }
  }

  async function handleSaveQuickUnit() {
    if (!quickUnitName) return;
    try {
      isSavingUnit = true;
      const newId = await invoke<number>('save_unit', {
        name: quickUnitName,
        shortName: quickUnitShort || quickUnitName.slice(0, 3),
        allowDecimals: false,
        unitId: null,
      });
      units = await invoke<Unit[]>('get_units');
      unitId = newId;
      isQuickUnitOpen = false;
      quickUnitName = '';
      quickUnitShort = '';
    } catch (e: any) {
      alert('Failed to add unit: ' + (e.message || e));
    } finally {
      isSavingUnit = false;
    }
  }

  async function handleDirectScaleSync() {
    if (!product?.id) {
      scaleSyncMsg = 'Save product first before synchronizing to scale';
      return;
    }
    try {
      isSyncingScale = true;
      scaleSyncMsg = 'Sending PLU data to ACLAS scale...';
      const settings = await invoke<Record<string, string>>('get_all_settings');
      const ip = settings['scale_ip'] || '192.168.1.87';
      const port = parseInt(settings['scale_port'] || '0');
      const protocol = parseInt(settings['scale_protocol'] || '0');

      await invoke('upload_product_to_scale', {
        productId: product.id,
        ip,
        port,
        protocolType: protocol,
        defaultDept: scaleDepartmentId,
        defaultBarcodeType: scaleBarcodeType,
        userName: 'admin',
      });
      scaleSyncStatus = 'synced';
      scaleSyncMsg = '✅ Synchronized successfully with scale!';
      setTimeout(() => (scaleSyncMsg = ''), 4000);
    } catch (e: any) {
      scaleSyncMsg = '❌ Scale error: ' + (typeof e === 'string' ? e : e.message);
    } finally {
      isSyncingScale = false;
    }
  }

  // Apply = save WITHOUT closing: persist now, keep editing. The saved
  // product id is remembered so a later Apply/Save UPDATES instead of
  // creating duplicates.
  async function handleApply() {
    await handleSave(true);
  }

  async function handleSave(keepOpen = false) {
    if (!nameFr.trim() && !nameAr.trim()) {
      errorMsg = 'Product name is required / اسم المنتج إجباري';
      return;
    }

    if (!purchasePrice || Number(purchasePrice) <= 0) {
      errorMsg = 'Purchase price is mandatory and must be > 0 / سعر الشراء إجباري وأكبر من الصفر';
      return;
    }

    if (!salePrice || Number(salePrice) <= 0) {
      errorMsg = 'Sale price is mandatory and must be > 0 / سعر البيع إجباري وأكبر من الصفر';
      return;
    }

    if (Number(salePrice) < Number(purchasePrice)) {
      errorMsg = 'Sale price cannot be less than purchase price / لا يمكن أن يكون سعر البيع أقل من سعر الشراء';
      return;
    }

    try {
      isSaving = true;
      errorMsg = '';

      if (currentBarcodeTyped.trim()) {
        addBarcodeToken();
      }

      const input: ProductInput = {
        sku: sku || undefined,
        name_ar: nameAr.trim() || nameFr.trim(),
        name_fr: nameFr.trim() || nameAr.trim(),
        name_en: nameEn.trim() || nameFr.trim(),
        category_id: categoryId,
        unit_id: unitId,
        purchase_price: Number(purchasePrice) || 0,
        sale_price: Number(salePrice) || 0,
        min_sale_price: Number(minSalePrice) || 0,
        tax_rate: Number(taxRate) || 19,
        current_stock: Number(currentStock) || 0,
        min_stock: Number(minStock) || 5,
        image_path: imagePath || undefined,
        expiry_date: expiryDate || undefined,
        is_scalable: isScalable,
        scale_code: scaleCode || undefined,
        scale_plu: isScalable ? scalePlu : undefined,
        scale_barcode_type: isScalable ? scaleBarcodeType : undefined,
        scale_department_id: isScalable ? scaleDepartmentId : undefined,
        scale_sync_status: scaleSyncStatus,
        is_bundle: isBundle,
        barcodes: barcodeTokens,
      };

      const savedId = await invoke<number>('save_product', {
        input,
        productId: (product ? product.id : null) ?? savedProductId,
        userId: $currentUser?.id,
      });

      // Units-in-packaging follow the product.
      await persistPackagings(savedId);

      // Auto-sync if enabled
      if (isScalable) {
        try {
          const settings = await invoke<Record<string, string>>('get_all_settings');
          if (settings['scale_auto_sync'] === 'true') {
            const ip = settings['scale_ip'] || '192.168.1.87';
            const port = parseInt(settings['scale_port'] || '0');
            const protocol = parseInt(settings['scale_protocol'] || '0');
            await invoke('upload_product_to_scale', {
              productId: savedId,
              ip,
              port,
              protocolType: protocol,
              defaultDept: scaleDepartmentId,
              defaultBarcodeType: scaleBarcodeType,
              userName: 'admin',
            });
          }
        } catch (scaleErr) {
          console.warn('Auto scale sync notice:', scaleErr);
        }
      }

      onSaved();
      if (keepOpen) {
        applyMsg = '✅ Applied / تم الحفظ';
        setTimeout(() => (applyMsg = ''), 2500);
        // Continue editing the JUST-SAVED product (no duplicates on the
        // next Apply): adopt its id.
        savedProductId = savedId;
      } else {
        onClose();
      }
    } catch (e: any) {
      errorMsg = typeof e === 'string' ? e : e.message || 'Failed to save product';
    } finally {
      isSaving = false;
    }
  }
</script>

{#if isOpen}
  <div class="fixed inset-0 z-50 bg-black/60 backdrop-blur-xs flex items-center justify-center p-4">
    <!-- Strict Fixed Modal Geometry to Prevent Any Jumps/Resizing on Tab Switches -->
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl w-full max-w-3xl h-[88vh] flex flex-col overflow-hidden animate-in zoom-in-95 duration-150 relative">
      <!-- Modal Header (Fixed Height) -->
      <div class="flex items-center justify-between px-6 py-4 border-b border-pos-border bg-slate-50 dark:bg-slate-800/60 shrink-0">
        <div class="flex items-center gap-3">
          <div class="w-10 h-10 rounded-2xl bg-sky-600/10 text-sky-600 flex items-center justify-center font-bold shrink-0">
            <Package class="w-5 h-5" />
          </div>
          <div>
            <h3 class="font-black text-base text-pos-text">
              {product ? `Edit Product: ${product.name_fr || product.name_ar}` : 'Add New Product (إضافة منتج جديد)'}
            </h3>
            <p class="text-xs text-pos-muted">Multi-barcode tokens, scalable ACLAS scale integration, and margin controls</p>
          </div>
        </div>

        <!-- Fixed Tab Bar -->
        <div class="flex items-center bg-slate-200 dark:bg-slate-700 p-1 rounded-xl gap-1 shrink-0">
          <button
            type="button"
            on:click={() => (activeTab = 'details')}
            class="px-3 py-1 rounded-lg text-xs font-bold transition {activeTab === 'details' ? 'bg-white dark:bg-slate-800 text-sky-600 shadow-xs' : 'text-pos-muted'}"
          >
            Details
          </button>
          <button
            type="button"
            on:click={() => (activeTab = 'scalable')}
            class="px-3 py-1 rounded-lg text-xs font-bold transition flex items-center gap-1 {activeTab === 'scalable' ? 'bg-white dark:bg-slate-800 text-sky-600 shadow-xs' : 'text-pos-muted'}"
          >
            <Scale class="w-3.5 h-3.5" />
            <span>Scale</span>
          </button>
          <button
            type="button"
            on:click={() => (activeTab = 'packaging')}
            class="px-3 py-1 rounded-lg text-xs font-bold transition flex items-center gap-1 {activeTab === 'packaging' ? 'bg-white dark:bg-slate-800 text-sky-600 shadow-xs' : 'text-pos-muted'}"
          >
            <Package class="w-3.5 h-3.5" />
            <span>Packaging</span>
          </button>
          {#if product}
            <button
              type="button"
              on:click={() => (activeTab = 'history')}
              class="px-3 py-1 rounded-lg text-xs font-bold transition {activeTab === 'history' ? 'bg-white dark:bg-slate-800 text-sky-600 shadow-xs' : 'text-pos-muted'}"
            >
              Price History
            </button>
          {/if}
          <button on:click={onClose} class="text-pos-muted hover:text-pos-text p-1.5 rounded-lg cursor-pointer">
            <X class="w-5 h-5" />
          </button>
        </div>
      </div>

      {#if errorMsg}
        <div class="mx-6 mt-3 p-3 bg-rose-100 dark:bg-rose-950 text-rose-800 dark:text-rose-200 text-xs font-bold rounded-xl border border-rose-300 shrink-0">
          {errorMsg}
        </div>
      {/if}

      <!-- Modal Body (Strictly Scrollable Interior) -->
      <div class="p-6 overflow-y-auto space-y-4 flex-1">
        {#if activeTab === 'details'}
          <!-- Images and Names Grid -->
          <div class="grid grid-cols-1 md:grid-cols-4 gap-4 items-start">
            <!-- Image upload box -->
            <div class="flex flex-col items-center">
              <div class="w-28 h-28 rounded-2xl border-2 border-dashed border-pos-border bg-slate-50 dark:bg-slate-800 flex flex-col items-center justify-center relative overflow-hidden group">
                {#if imagePath}
                  <img src={imagePath} alt="Product" class="w-full h-full object-cover" />
                {:else}
                  <Tag class="w-8 h-8 text-pos-muted mb-1" />
                  <span class="text-[10px] text-pos-muted font-bold">Photo</span>
                {/if}
                <label class="absolute inset-0 bg-black/40 text-white text-[10px] font-bold flex flex-col items-center justify-center opacity-0 group-hover:opacity-100 transition cursor-pointer">
                  <Upload class="w-4 h-4 mb-1" />
                  <span>Change</span>
                  <input type="file" accept="image/*" on:change={handleImageUpload} class="hidden" />
                </label>
              </div>
              <button
                type="button"
                on:click={() => (isWebcamOpen = true)}
                class="mt-1.5 px-2 py-1 bg-slate-100 dark:bg-slate-800 hover:bg-sky-100 dark:hover:bg-sky-950/50 text-pos-muted hover:text-sky-600 text-[10px] font-black rounded-lg cursor-pointer transition"
              >
                Camera (كاميرا)
              </button>
            </div>

            <!-- Names (with live DB autosuggest: Tab or click accepts) -->
            <div class="md:col-span-3 space-y-2.5">
              <div class="grid grid-cols-2 gap-3">
                <div class="relative">
                  <label class="block text-xs font-bold text-pos-muted mb-1">Product Name (Français) *</label>
                  <input
                    type="text"
                    bind:value={nameFr}
                    on:input={(e) => refreshNameSuggestions((e.target as HTMLInputElement).value, 'fr')}
                    on:focus={(e) => refreshNameSuggestions((e.target as HTMLInputElement).value, 'fr')}
                    on:keydown={handleNameKeydown}
                    on:blur={() => setTimeout(() => (showNameSuggestions = false), 150)}
                    placeholder="Ex: Lait Candia 1L Entier"
                    class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
                  />
                  {#if showNameSuggestions && nameSuggestions.length > 0}
                    <div class="absolute z-20 left-0 right-0 top-full mt-1 bg-pos-card border border-pos-border rounded-xl shadow-2xl overflow-hidden">
                      {#each nameSuggestions as s}
                        <button
                          type="button"
                          on:mousedown|preventDefault={() => acceptNameSuggestion(s)}
                          class="w-full text-start px-3 py-2 text-xs font-bold text-pos-text hover:bg-sky-100 dark:hover:bg-sky-950 cursor-pointer truncate"
                        >
                          {s.value}
                        </button>
                      {/each}
                      <p class="px-3 py-1 text-[9px] text-pos-muted font-bold border-t border-pos-border">Tab = accept first suggestion</p>
                    </div>
                  {/if}
                </div>
                <div>
                  <label class="block text-xs font-bold text-pos-muted mb-1">Product Name (العربية)</label>
                  <input
                    type="text"
                    bind:value={nameAr}
                    on:input={(e) => refreshNameSuggestions((e.target as HTMLInputElement).value, 'ar')}
                    on:keydown={handleNameKeydown}
                    placeholder="Ex: حليب كانديا 1 لتر كامل"
                    class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
                  />
                </div>
              </div>

              <div class="grid grid-cols-3 gap-3">
                <div>
                  <div class="flex items-center justify-between mb-1">
                    <label class="block text-xs font-bold text-pos-muted">SKU / Reference</label>
                    <button type="button" on:click={generateRandomSku} class="text-[10px] font-black text-sky-600 hover:underline flex items-center gap-0.5" title="Auto-generate SKU">
                      <Sparkles class="w-3 h-3" />
                      <span>Auto</span>
                    </button>
                  </div>
                  <input type="text" bind:value={sku} placeholder="Optional (PRD-001)" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500" />
                </div>

                <!-- Category / Family with Quick Add Trigger -->
                <div>
                  <div class="flex items-center justify-between mb-1">
                    <label class="block text-xs font-bold text-pos-muted">Family / الفئة</label>
                    <button type="button" on:click={() => (isQuickFamilyOpen = true)} class="text-[10px] font-black text-sky-600 hover:underline flex items-center gap-0.5">
                      <Plus class="w-3 h-3" />
                      <span>Add</span>
                    </button>
                  </div>
                  <select bind:value={categoryId} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none">
                    {#each categories as cat}
                      <option value={cat.id}>{cat.name_fr} / {cat.name_ar}</option>
                    {/each}
                  </select>
                </div>

                <!-- Unit with Quick Add Trigger -->
                <div>
                  <div class="flex items-center justify-between mb-1">
                    <label class="block text-xs font-bold text-pos-muted">Unit / الوحدة</label>
                    <button type="button" on:click={() => (isQuickUnitOpen = true)} class="text-[10px] font-black text-sky-600 hover:underline flex items-center gap-0.5">
                      <Plus class="w-3 h-3" />
                      <span>Add</span>
                    </button>
                  </div>
                  <select bind:value={unitId} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text outline-none">
                    {#each units as u}
                      <option value={u.id}>{u.name} ({u.short_name})</option>
                    {/each}
                  </select>
                </div>
              </div>
            </div>
          </div>

          <!-- Barcode Tokenizer Section & Generator -->
          <div class="bg-slate-50 dark:bg-slate-800/40 p-3.5 rounded-2xl border border-pos-border space-y-2">
            <div class="flex items-center justify-between">
              <label class="block text-xs font-bold text-pos-text">
                Barcodes & Tokens (الباركودات المتعددة)
                <span class="text-[10px] text-pos-muted font-normal block">Type barcode & press Enter or comma (,) to add multiple</span>
              </label>
              <button
                type="button"
                on:click={generateValidEan13}
                class="px-2.5 py-1 bg-sky-100 dark:bg-sky-950 text-sky-700 dark:text-sky-300 hover:bg-sky-200 border border-sky-300 dark:border-sky-800 text-[10px] font-bold rounded-lg flex items-center gap-1 cursor-pointer transition shadow-2xs"
              >
                <Sparkles class="w-3 h-3" />
                <span>+ Generate EAN-13 (توليد باركود)</span>
              </button>
            </div>

            <div class="flex flex-wrap gap-1.5 items-center p-2 bg-pos-card border border-pos-border rounded-xl min-h-[44px]">
              {#each barcodeTokens as token, idx}
                <div class="flex items-center gap-1 px-2.5 py-1 bg-sky-100 dark:bg-sky-950 text-sky-800 dark:text-sky-200 border border-sky-300 dark:border-sky-800 rounded-lg text-xs font-mono font-bold">
                  <span on:dblclick={() => startEditToken(idx)} class="cursor-pointer" title="Double click to edit">{token}</span>
                  <button type="button" on:click={() => copyToClipboard(token)} class="text-sky-600 hover:text-sky-900 ml-0.5" title="Copy"><Copy class="w-3 h-3" /></button>
                  <button type="button" on:click={() => removeBarcodeToken(idx)} class="text-rose-500 hover:text-rose-700 ml-0.5"><X class="w-3 h-3" /></button>
                </div>
              {/each}
              <input
                type="text"
                bind:value={currentBarcodeTyped}
                on:keydown={handleBarcodeKeyDown}
                on:input={checkBarcodeDuplicate}
                placeholder={barcodeTokens.length === 0 ? "Scan or type barcode & Enter..." : "+ Add barcode..."}
                class="flex-1 min-w-[140px] bg-transparent border-0 text-xs font-mono font-bold text-pos-text outline-none px-2"
              />
            </div>
            {#if duplicateBarcodeWarning}
              <p class="text-[10px] text-rose-600 font-bold flex items-center gap-1">
                <AlertTriangle class="w-3 h-3 shrink-0" />
                {duplicateBarcodeWarning}
              </p>
            {/if}
            {#if copyFeedback}
              <p class="text-[10px] text-emerald-600 font-bold">{copyFeedback}</p>
            {/if}
          </div>

          <!-- MONEY GROUP: purchase / margin / sale price -->
          <div class="p-3.5 bg-slate-50 dark:bg-slate-800/40 border border-pos-border rounded-2xl space-y-3">
            <div class="flex items-center gap-2">
              <Banknote class="w-4 h-4 text-emerald-600" />
              <span class="text-[11px] font-black text-pos-muted uppercase tracking-wider">{t('pem_money_pricing')}</span>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">
                  {t('pem_purchase_cost')} <span class="text-rose-500">*</span>
                </label>
                <input
                  type="number"
                  min="0"
                  data-enter-next="pem-margin"
                  bind:value={purchasePrice}
                  on:input={handlePurchasePriceChange}
                  on:keydown={handlePricingEnter}
                  on:wheel={noWheelScroll}
                  placeholder="0"
                  class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
                />
              </div>

              <!-- Margin Toggle: % vs DZD Amount -->
              <div>
                <div class="flex items-center justify-between mb-1">
                  <label class="block text-xs font-bold text-pos-muted">{t('pem_margin')} ({marginMode === 'percent' ? '%' : 'DZD'})</label>
                  <div class="flex items-center bg-slate-200 dark:bg-slate-700 rounded-md p-0.5 text-[9px] font-bold">
                    <button
                      type="button"
                      on:click={() => { marginMode = 'percent'; handleMarginPercentChange(); }}
                      class="px-1.5 py-0.5 rounded {marginMode === 'percent' ? 'bg-sky-600 text-white' : 'text-pos-muted'}"
                    >%</button>
                    <button
                      type="button"
                      on:click={() => { marginMode = 'amount'; handleMarginAmountChange(); }}
                      class="px-1.5 py-0.5 rounded {marginMode === 'amount' ? 'bg-sky-600 text-white' : 'text-pos-muted'}"
                    >DZD</button>
                  </div>
                </div>

                {#if marginMode === 'percent'}
                  <input
                    type="number"
                    id="pem-margin"
                    data-enter-next="pem-sale"
                    bind:value={profitMarginPercent}
                    on:input={handleMarginPercentChange}
                    on:keydown={handlePricingEnter}
                    on:wheel={noWheelScroll}
                    class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-emerald-600 outline-none"
                  />
                {:else}
                  <input
                    type="number"
                    id="pem-margin"
                    data-enter-next="pem-sale"
                    bind:value={profitMarginAmount}
                    on:input={handleMarginAmountChange}
                    on:keydown={handlePricingEnter}
                    on:wheel={noWheelScroll}
                    class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-emerald-600 outline-none"
                  />
                {/if}
                <p class="text-[9px] text-pos-muted font-bold mt-0.5">Sale price auto-rounds to {priceRoundStep > 0 ? `nearest ${priceRoundStep}` : 'whole DZD'}</p>
              </div>

              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">
                  {t('pem_sale_price')} <span class="text-rose-500">*</span>
                </label>
                <input
                  type="number"
                  min="0"
                  id="pem-sale"
                  data-enter-next="pem-new-qty"
                  bind:value={salePrice}
                  on:input={handleSalePriceChange}
                  on:keydown={handlePricingEnter}
                  on:wheel={noWheelScroll}
                  placeholder="0"
                  class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-black text-sky-600 outline-none focus:ring-2 focus:ring-sky-500"
                />
              </div>
            </div>

            {#if salePrice > 0 && purchasePrice > 0 && Number(salePrice) < Number(purchasePrice)}
              <div class="p-2.5 bg-rose-100 dark:bg-rose-950/60 border border-rose-300 dark:border-rose-800 text-rose-700 dark:text-rose-300 text-xs font-bold rounded-xl flex items-center gap-2">
                <AlertTriangle class="w-4 h-4 shrink-0 text-rose-600" />
                <span>Sale price ({salePrice} DZD) cannot be lower than purchase cost ({purchasePrice} DZD) / لا يمكن أن يكون سعر البيع أقل من سعر الشراء!</span>
              </div>
            {/if}
          </div>

          <!-- STOCK GROUP: linked current + new + total -->
          <div class="p-3.5 bg-slate-50 dark:bg-slate-800/40 border border-pos-border rounded-2xl space-y-3">
            <div class="flex items-center gap-2">
              <Package class="w-4 h-4 text-sky-600" />
              <span class="text-[11px] font-black text-pos-muted uppercase tracking-wider">{t('pem_stock')}</span>
            </div>
            <div class="grid grid-cols-1 md:grid-cols-3 gap-3">
              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">
                  {t('pem_new_qty')} <span class="text-emerald-600 font-black">+</span>
                </label>
                <input
                  type="number"
                  min="0"
                  id="pem-new-qty"
                  data-enter-next="pem-baseline"
                  bind:value={newStockQty}
                  on:input={handleNewStockChange}
                  on:keydown={handlePricingEnter}
                  on:wheel={noWheelScroll}
                  placeholder="0"
                  class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-black text-emerald-600 outline-none focus:ring-2 focus:ring-emerald-500"
                />
              </div>
              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">{t('pem_current_stock')}</label>
                <input
                  type="number"
                  min="0"
                  id="pem-baseline"
                  data-enter-next="pem-total"
                  bind:value={baselineStock}
                  on:input={handleBaselineStockChange}
                  on:keydown={handlePricingEnter}
                  on:wheel={noWheelScroll}
                  class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none focus:ring-2 focus:ring-sky-500"
                />
              </div>
              <div>
                <label class="block text-xs font-bold text-pos-muted mb-1">
                  {t('pem_total_qty')} <span class="text-sky-600 font-black">=</span>
                </label>
                <input
                  type="number"
                  min="0"
                  bind:value={currentStock}
                  on:input={handleTotalStockChange}
                  on:keydown={handlePricingEnter}
                  on:wheel={noWheelScroll}
                  id="pem-total"
                  class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-black text-sky-600 outline-none focus:ring-2 focus:ring-sky-500"
                />
              </div>
            </div>
            <p class="text-[9px] text-pos-muted font-bold">
              Current + New = Total — editing any field recalculates the others / تعديل أي خانة يحدّث الباقي
            </p>
          </div>

          <!-- Expiry Date & Min Stock -->
          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Expiry Date / تاريخ الصلاحية</label>
              <input type="date" bind:value={expiryDate} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
            </div>
            <div>
              <label class="block text-xs font-bold text-pos-muted mb-1">Min Stock Alert Level</label>
              <input type="number" min="1" bind:value={minStock} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text outline-none" />
            </div>
          </div>
        {:else if activeTab === 'packaging'}
          <!-- UNITS-IN-PACKAGING (carton 24 / fardeau 6 / palette 672) -->
          <div class="p-3.5 bg-slate-50 dark:bg-slate-800/40 border border-pos-border rounded-2xl space-y-3">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <Package class="w-4 h-4 text-emerald-600" />
                <span class="text-[11px] font-black text-pos-muted uppercase tracking-wider">Packaging / التغليف (carton, fardeau...)</span>
              </div>
              <button
                type="button"
                on:click={addPackagingRow}
                class="px-2.5 py-1 bg-emerald-600 hover:bg-emerald-700 text-white text-[10px] font-black rounded-lg cursor-pointer"
              >
                + Add Packaging
              </button>
            </div>
            <p class="text-[9px] text-pos-muted font-bold">
              Each packaging multiplies the base-unit quantity: 1 carton = 24 pcs, 1 palette = 672 pcs...
              Price per packaging is optional (defaults to unit price × units).
            </p>
            {#if packagingRows.length === 0}
              <p class="text-[10px] text-pos-muted text-center py-1.5">No packaging — sold by single units only.</p>
            {:else}
              <div class="space-y-2">
                {#each packagingRows as row, idx}
                  <div class="grid grid-cols-12 gap-1.5 items-center">
                    <input
                      type="text"
                      bind:value={row.name}
                      placeholder="Name (carton / fardeau...)"
                      class="col-span-4 px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-[11px] font-bold text-pos-text outline-none"
                    />
                    <div class="col-span-3 flex items-center gap-1">
                      <span class="text-[10px] font-bold text-pos-muted whitespace-nowrap">=</span>
                      <input
                        type="number"
                        min="2"
                        inputmode="numeric"
                        bind:value={row.unitsPerPackage}
                        title="Units per packaging"
                        class="w-full px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-[11px] font-mono font-black text-pos-text outline-none"
                      />
                      <span class="text-[10px] font-bold text-pos-muted">u</span>
                    </div>
                    <input
                      type="number"
                      min="0"
                      inputmode="numeric"
                      bind:value={row.salePrice}
                      placeholder="Price/pack"
                      title="Sale price for the whole packaging (optional)"
                      class="col-span-3 px-2 py-1.5 bg-white dark:bg-slate-900 border border-pos-border rounded-lg text-[11px] font-mono font-bold text-sky-600 outline-none"
                    />
                    <div class="col-span-2 flex items-center justify-end gap-1">
                      <button
                        type="button"
                        on:click={() => (packagingRows[idx].isDefault = !packagingRows[idx].isDefault)}
                        class="px-1.5 py-1 rounded-lg text-[9px] font-black cursor-pointer {row.isDefault ? 'bg-emerald-600 text-white' : 'bg-slate-200 dark:bg-slate-800 text-pos-muted'}"
                        title="Default packaging in POS"
                      >
                        DEF
                      </button>
                      <button
                        type="button"
                        on:click={() => removePackagingRow(idx)}
                        class="p-1 text-rose-500 hover:text-rose-700 rounded-lg cursor-pointer"
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <!-- Scale Tab -->
          <div class="space-y-4">
            <div class="flex items-center justify-between p-4 bg-sky-50 dark:bg-sky-950/40 border border-sky-200 dark:border-sky-800 rounded-2xl">
              <div class="flex items-center gap-3">
                <Scale class="w-6 h-6 text-sky-600" />
                <div>
                  <h4 class="font-black text-sm text-pos-text">Scalable Product (منتج يباع بالوزن على الميزان)</h4>
                  <p class="text-xs text-pos-muted">Enable to synchronize name and pricing to ACLAS electronic barcode scales</p>
                </div>
              </div>
              <label class="relative inline-flex items-center cursor-pointer">
                <input type="checkbox" bind:checked={isScalable} class="sr-only peer" />
                <div class="w-11 h-6 bg-gray-200 peer-focus:outline-none rounded-full peer dark:bg-gray-700 peer-checked:after:translate-x-full peer-checked:after:border-white after:content-[''] after:absolute after:top-[2px] after:left-[2px] after:bg-white after:border-gray-300 after:border after:rounded-full after:h-5 after:w-5 after:transition-all peer-checked:bg-sky-600"></div>
              </label>
            </div>

            {#if isScalable}
              <div class="grid grid-cols-2 md:grid-cols-3 gap-3 p-4 bg-pos-card border border-pos-border rounded-2xl">
                <div>
                  <div class="flex items-center justify-between mb-1">
                    <label class="block text-xs font-bold text-pos-muted">Scale PLU Number</label>
                    <button type="button" on:click={generateScalePluCode} class="text-[10px] text-sky-600 font-bold hover:underline">Regenerate</button>
                  </div>
                  <input type="number" min="1" max="999999" bind:value={scalePlu} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text" />
                </div>

                <div>
                  <label class="block text-xs font-bold text-pos-muted mb-1">Scale Item Code (6 digits)</label>
                  <input type="text" bind:value={scaleCode} placeholder="Ex: 950001" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text" />
                </div>

                <div>
                  <label class="block text-xs font-bold text-pos-muted mb-1">Scale Department ID</label>
                  <input type="number" min="1" max="99" bind:value={scaleDepartmentId} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-mono font-bold text-pos-text" />
                </div>

                <div class="md:col-span-2">
                  <label class="block text-xs font-bold text-pos-muted mb-1">Scale Barcode Type Format</label>
                  <select bind:value={scaleBarcodeType} class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border-0 rounded-xl text-xs font-bold text-pos-text">
                    <option value={97}>Type 97: 18-Code (Dept + ItemCode + Price + Weight + Checksum)</option>
                    <option value={2}>Type 02: EAN-13 Price Embedded (DD IIIII PPPPP C)</option>
                    <option value={7}>Type 07: EAN-13 Weight Embedded (DD IIIII WWWWW C)</option>
                    <option value={22}>Type 22: EAN-13 1-Digit Dept Price Embedded (D IIIIII PPPPP C)</option>
                    <option value={27}>Type 27: EAN-13 1-Digit Dept Weight Embedded (D IIIIII WWWWW C)</option>
                    <option value={12}>Type 12: Fixed Code 22 Price Embedded (22 IIIII PPPPP C)</option>
                    <option value={17}>Type 17: Fixed Code 27 Weight Embedded (27 IIIII WWWWW C)</option>
                  </select>
                </div>

                <div>
                  <label class="block text-xs font-bold text-pos-muted mb-1">Sync Status</label>
                  <div class="px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl text-xs font-bold capitalize text-sky-600">
                    {scaleSyncStatus}
                  </div>
                </div>
              </div>

              {#if scaleSyncMsg}
                <div class="p-3 bg-sky-100 dark:bg-sky-950 text-sky-800 dark:text-sky-200 text-xs font-bold rounded-xl">
                  {scaleSyncMsg}
                </div>
              {/if}

              {#if product}
                <div class="flex justify-end">
                  <button
                    type="button"
                    on:click={handleDirectScaleSync}
                    disabled={isSyncingScale}
                    class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white font-bold text-xs rounded-xl flex items-center gap-2 cursor-pointer shadow-xs"
                  >
                    <Send class="w-4 h-4" />
                    <span>{isSyncingScale ? 'Sending to Scale...' : 'Sync to ACLAS Scale Now (إرسال للميزان)'}</span>
                  </button>
                </div>
              {/if}
            {/if}
          </div>
        {:else if activeTab === 'history'}
          <div class="p-4 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-3">
            <div class="flex items-center justify-between">
              <h4 class="font-black text-sm text-pos-text">{t('history_tab_title')}</h4>
              <div class="flex items-center gap-2">
                <!-- Price / Quantity history sub-toggle -->
                <div class="flex bg-slate-200 dark:bg-slate-800 rounded-lg p-0.5">
                  <button
                    type="button"
                    on:click={() => (historyView = 'price')}
                    class="px-2.5 py-1 rounded-md text-[10px] font-black transition {historyView === 'price' ? 'bg-white dark:bg-slate-900 text-sky-600 shadow-xs' : 'text-pos-muted'}"
                  >
                    {t('history_price')}
                  </button>
                  <button
                    type="button"
                    on:click={() => (historyView = 'quantity')}
                    class="px-2.5 py-1 rounded-md text-[10px] font-black transition {historyView === 'quantity' ? 'bg-white dark:bg-slate-900 text-emerald-600 shadow-xs' : 'text-pos-muted'}"
                  >
                    {t('history_quantity')}
                  </button>
                </div>
                <button
                  type="button"
                  on:click={() => { historyView === 'price' ? loadPriceHistory() : loadQuantityHistory(); }}
                  class="text-[10px] font-black text-sky-600 hover:underline flex items-center gap-1 cursor-pointer"
                >
                  <RefreshCw class="w-3 h-3" />
                  <span>{t('refresh')}</span>
                </button>
              </div>
            </div>

            {#if !product || product.id === 0}
              <p class="text-xs text-pos-muted text-center py-4">{t('history_save_first')}</p>
            {:else if historyView === 'price'}
              {#if priceHistory.length === 0}
                <p class="text-xs text-pos-muted text-center py-4">{t('history_empty')}</p>
              {:else}
                <div class="overflow-x-auto">
                  <table class="w-full text-[10px]">
                    <thead>
                      <tr class="text-pos-muted border-b border-pos-border">
                        <th class="text-start py-1.5 font-black uppercase">{t('qh_date')}</th>
                        <th class="text-start py-1.5 font-black uppercase">{t('pem_purchase_cost')}</th>
                        <th class="text-start py-1.5 font-black uppercase">{t('pem_sale_price')}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each priceHistory as h (h.id)}
                        <tr class="border-b border-pos-border/50">
                          <td class="py-1.5 font-mono text-pos-muted">{h.created_at}</td>
                          <td class="py-1.5 font-mono">
                            {h.old_purchase_price.toLocaleString()} →
                            <span class="font-black text-emerald-600">{h.new_purchase_price.toLocaleString()}</span>
                          </td>
                          <td class="py-1.5 font-mono">
                            {h.old_sale_price.toLocaleString()} →
                            <span class="font-black text-sky-600">{h.new_sale_price.toLocaleString()}</span>
                          </td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              {/if}
            {:else}
              <!-- Quantity history: every stock movement with old/new, delta, type, user -->
              {#if quantityHistory.length === 0}
                <p class="text-xs text-pos-muted text-center py-4">{t('history_empty')}</p>
              {:else}
                <div class="overflow-x-auto max-h-72 overflow-y-auto">
                  <table class="w-full text-[10px]">
                    <thead class="sticky top-0 bg-slate-50 dark:bg-slate-800">
                      <tr class="text-pos-muted border-b border-pos-border">
                        <th class="text-start py-1.5 font-black uppercase">{t('qh_date')}</th>
                        <th class="text-start py-1.5 font-black uppercase">{t('qh_old')}</th>
                        <th class="text-start py-1.5 font-black uppercase">{t('qh_new')}</th>
                        <th class="text-start py-1.5 font-black uppercase">{t('qh_diff')}</th>
                        <th class="text-start py-1.5 font-black uppercase">{t('qh_type')}</th>
                        <th class="text-start py-1.5 font-black uppercase">{t('qh_user')}</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each quantityHistory as q (q.id)}
                        {@const mt = movementTypeLabel(q.movement_type)}
                        <tr class="border-b border-pos-border/50">
                          <td class="py-1.5 font-mono text-pos-muted whitespace-nowrap">{q.created_at}</td>
                          <td class="py-1.5 font-mono">{q.old_quantity.toLocaleString()}</td>
                          <td class="py-1.5 font-mono font-black">{q.new_quantity.toLocaleString()}</td>
                          <td class="py-1.5 font-mono font-black {q.difference > 0 ? 'text-emerald-600 dark:text-emerald-400' : q.difference < 0 ? 'text-rose-600 dark:text-rose-400' : ''}">
                            {q.difference > 0 ? '+' : ''}{q.difference.toLocaleString()}
                          </td>
                          <td class="py-1.5 font-black {mt.cls}">{mt.label}</td>
                          <td class="py-1.5 text-pos-muted">{q.user_name || '—'}</td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      </div>

      <!-- Modal Footer (Fixed Height) -->
      <div class="px-6 py-4 border-t border-pos-border bg-slate-50 dark:bg-slate-800/60 flex items-center justify-between shrink-0">
        <div class="flex items-center gap-2">
          <button on:click={onClose} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 hover:bg-slate-300 text-pos-text font-bold text-xs rounded-xl transition cursor-pointer">
            Cancel (إلغاء)
          </button>

          <button
            type="button"
            on:click={openPrintSticker}
            class="px-3 py-2 bg-sky-50 hover:bg-sky-100 dark:bg-sky-950/60 dark:hover:bg-sky-900/60 text-sky-700 dark:text-sky-300 border border-sky-200 dark:border-sky-800 font-bold text-xs rounded-xl transition cursor-pointer flex items-center gap-1.5 shadow-2xs"
            title="Print Product Sticker"
          >
            <QrCode class="w-3.5 h-3.5" />
            <span>Print Sticker ({Number(currentStock) > 0 ? currentStock : 1})</span>
          </button>

          <button
            type="button"
            on:click={openPrintShelf}
            class="px-3 py-2 bg-emerald-50 hover:bg-emerald-100 dark:bg-emerald-950/60 dark:hover:bg-emerald-900/60 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800 font-bold text-xs rounded-xl transition cursor-pointer flex items-center gap-1.5 shadow-2xs"
            title="Print Shelf Tag"
          >
            <Tag class="w-3.5 h-3.5" />
            <span>Shelf Tag (1)</span>
          </button>
        </div>

        <div class="flex items-center gap-2">
          {#if applyMsg}
            <span class="text-[11px] font-black text-emerald-600 dark:text-emerald-400">{applyMsg}</span>
          {/if}
          <button
            type="button"
            on:click={handleApply}
            disabled={isSaving}
            class="px-4 py-2 bg-emerald-50 hover:bg-emerald-100 dark:bg-emerald-950/60 dark:hover:bg-emerald-900/60 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800 font-black text-xs rounded-xl transition cursor-pointer shadow-2xs"
            title="Save now and keep editing — safe to print or leave anytime (يحفظ الآن ويبقى مفتوحاً)"
          >
            Apply (حفظ دون إغلاق)
          </button>
          <button on:click={() => handleSave(false)} disabled={isSaving} class="px-6 py-2 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white font-black text-xs rounded-xl shadow-md transition cursor-pointer flex items-center gap-1.5">
            <Check class="w-4 h-4" />
            <span>{isSaving ? 'Saving...' : 'Save Product (حفظ المنتج)'}</span>
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}

<!-- Embedded Print Label Modal -->
<PrintLabelModal
  isOpen={isPrintLabelOpen}
  product={printProductObj}
  initialType={printLabelInitialType}
  initialQty={printLabelInitialQty}
  onClose={() => (isPrintLabelOpen = false)}
/>

<!-- Sub-Modal: Quick Add Family -->
{#if isQuickFamilyOpen}
  <div class="fixed inset-0 z-[70] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95">
      <div class="flex items-center justify-between border-b border-pos-border pb-2">
        <h4 class="font-black text-xs text-pos-text flex items-center gap-1.5">
          <FolderPlus class="w-4 h-4 text-sky-600" />
          <span>Quick Add Family / إضافة فئة جديدة</span>
        </h4>
        <button on:click={() => (isQuickFamilyOpen = false)} class="text-pos-muted hover:text-pos-text"><X class="w-4 h-4" /></button>
      </div>

      <div class="space-y-3 text-xs">
        <div>
          <label class="block font-bold text-pos-muted mb-1">Family Name (Français) *</label>
          <input type="text" bind:value={quickFamilyNameFr} placeholder="Ex: Boissons" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl font-bold text-pos-text outline-none" />
        </div>
        <div>
          <label class="block font-bold text-pos-muted mb-1">Family Name (العربية)</label>
          <input type="text" bind:value={quickFamilyNameAr} placeholder="Ex: مشروبات" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl font-bold text-pos-text outline-none" />
        </div>
        <div>
          <label class="block font-bold text-pos-muted mb-1">Category Color</label>
          <div class="flex items-center gap-2">
            <input type="color" bind:value={quickFamilyColor} class="w-8 h-8 rounded-lg border-0 cursor-pointer" />
            <span class="font-mono text-pos-muted">{quickFamilyColor}</span>
          </div>
        </div>
      </div>

      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (isQuickFamilyOpen = false)} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-bold rounded-xl">Cancel</button>
        <button on:click={handleSaveQuickFamily} disabled={isSavingFamily} class="px-4 py-1.5 bg-sky-600 text-white text-xs font-black rounded-xl">Save & Select</button>
      </div>
    </div>
  </div>
{/if}

<!-- Sub-Modal: Quick Add Unit -->
{#if isQuickUnitOpen}
  <div class="fixed inset-0 z-[70] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-3xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95">
      <div class="flex items-center justify-between border-b border-pos-border pb-2">
        <h4 class="font-black text-xs text-pos-text flex items-center gap-1.5">
          <Plus class="w-4 h-4 text-sky-600" />
          <span>Quick Add Unit / إضافة وحدة جديدة</span>
        </h4>
        <button on:click={() => (isQuickUnitOpen = false)} class="text-pos-muted hover:text-pos-text"><X class="w-4 h-4" /></button>
      </div>

      <div class="space-y-3 text-xs">
        <div>
          <label class="block font-bold text-pos-muted mb-1">Unit Full Name *</label>
          <input type="text" bind:value={quickUnitName} placeholder="Ex: Kilogramme, Litre, Pièce" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl font-bold text-pos-text outline-none" />
        </div>
        <div>
          <label class="block font-bold text-pos-muted mb-1">Unit Short Code</label>
          <input type="text" bind:value={quickUnitShort} placeholder="Ex: kg, L, pcs" class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 rounded-xl font-mono font-bold text-pos-text outline-none" />
        </div>
      </div>

      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (isQuickUnitOpen = false)} class="px-3 py-1.5 bg-slate-200 dark:bg-slate-700 text-pos-text text-xs font-bold rounded-xl">Cancel</button>
        <button on:click={handleSaveQuickUnit} disabled={isSavingUnit} class="px-4 py-1.5 bg-sky-600 text-white text-xs font-black rounded-xl">Save & Select</button>
      </div>
    </div>
  </div>
{/if}

<!-- Webcam Capture -->
{#if isWebcamOpen}
  <WebcamCapture
    onCapture={(dataUrl) => (imagePath = dataUrl)}
    onClose={() => (isWebcamOpen = false)}
  />
{/if}
