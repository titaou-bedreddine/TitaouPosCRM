<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import { invoke } from '@tauri-apps/api/core';
  import { t, currentLocale } from '../../lib/i18n';
  import { localTodayISO } from '../../lib/utils/date';
  import type { Category, Product, Supplier, Unit } from '../../lib/types';
  import { cartItems, cartGrandTotal, cartSubtotal, globalDiscountAmount, globalDiscountMode, globalDiscountValue, globalDiscountPercent, isRefundMode, addToCart, clearCart, cartItemOrder, qtyEditTarget, itemKey, stopQtyEdit, posMode, originSaleId, restoreActiveCart, holdCurrentSale, allowNegativeStock, saleTotalRoundingStep, recordSoldQuantities } from '../../lib/stores/cart';
  import { currentUser } from '../../lib/stores/auth';
  import { activeSession } from '../../lib/stores/session';
  import { printHtmlSilently, entityQrDataUrl } from '../../lib/utils/printer';
  import { buildProfessionalReceiptHtml } from '../../lib/printing/professionalReceipt';
  import { buildUnifiedReceipt, printReceiptSmart } from '../../lib/printing/unifiedReceipt';
  import { normalizeBarcode } from '../../lib/utils/barcode';
  import { networkStatus } from '../../lib/stores/network';
  import { invalidations } from '../../lib/stores/invalidations';
  import { getLanguage } from '../../lib/i18n';

  // Route navigation for F7 (products) / F8 (register) / F9 (sales).
  export let onNavigate: (route: string) => void = () => {};
  // Purchase QR scan (PUR:<invoice>) → App opens the purchases page.
  export let onOpenPurchase: ((invoiceNumber: string) => void) | null = null;
  // From notifications: open this product's editor as soon as the POS loads.
  export let initialOpenProductId: number | null = null;
  export let onProductOpened: () => void = () => {};

  import TopActionBar from '../../lib/components/TopActionBar.svelte';
  import UniversalSearchBar from '../../lib/components/UniversalSearchBar.svelte';
  import ProductCard from '../../lib/components/ProductCard.svelte';
  import CartItemCard from '../../lib/components/CartItemCard.svelte';
  import PaymentModal from '../../lib/components/PaymentModal.svelte';
  import CashDrawerModal from '../../lib/components/CashDrawerModal.svelte';
  import CategoryManagerModal from '../../lib/components/CategoryManagerModal.svelte';
  import HeldSalesModal from '../../lib/components/HeldSalesModal.svelte';
  import RemiseModal from '../../lib/components/RemiseModal.svelte';
  import PrintReceiptModal from '../../lib/components/PrintReceiptModal.svelte';
  import ProductEditModal from '../../lib/components/ProductEditModal.svelte';
  import QuickPurchaseModal from '../../lib/components/QuickPurchaseModal.svelte';
  import ReturnDamagedModal from '../../lib/components/ReturnDamagedModal.svelte';
  import CreditCustomerModal from '../../lib/components/CreditCustomerModal.svelte';
  import VersementCustomerModal from '../../lib/components/VersementCustomerModal.svelte';
  import CheckoutModal from '../../lib/components/CheckoutModal.svelte';
  import OtherArticleModal from '../../lib/components/OtherArticleModal.svelte';
  import UnknownBarcodeModal from '../../lib/components/UnknownBarcodeModal.svelte';
  import PurchasePriceModal from '../../lib/components/PurchasePriceModal.svelte';
  import LastReceiptModal from '../../lib/components/LastReceiptModal.svelte';

  import { customers, selectedCustomerId, selectedCustomer, refreshCustomers, DEFAULT_WALKIN_CUSTOMER_ID } from '../../lib/stores/customers';
  import { suppliers, selectedSupplierId, selectedSupplier, refreshSuppliers } from '../../lib/stores/suppliers';

  import {
    ShoppingBag, ArrowRight, CheckCircle2, Settings2, Plus,
    Store, Sparkles, AlertCircle, ArrowUpDown, Tag, Percent, UserRound, ChevronDown, Truck,
    Eye, EyeOff, TrendingUp
  } from 'lucide-svelte';

  let products: Product[] = [];
  let categories: Category[] = [];
  let units: Unit[] = [];

  let selectedCategory: number | null = null;
  let searchQuery = '';

  // Live cross-PC stock sync: when another terminal (or the server) sells,
  // saves or edits a product, this terminal's grid refreshes in place —
  // no navigation, no manual refresh (user-reported bug).
  $: if ($invalidations.products) {
    void loadProducts();
  }
  let searchType: 'all' | 'name' | 'barcode' | 'price' | 'qr' = 'all';
  let sortBy: 'name_asc' | 'name_desc' | 'price_asc' | 'price_desc' | 'stock' | 'best_sellers' | 'worst_sellers' = 'name_asc';

  let selectedPaymentMode: 'cash' | 'tpe' | 'credit' | 'versement' = 'cash';
  let autoPrintEnabled = true;
  let suppressNextPrint = false;
  let autoDrawerEnabled = true;
  let isFastCheckingOut = false;

  let isPaymentOpen = false;
  let isCashDrawerOpen = false;
  let isCategoryManagerOpen = false;
  let isHeldSalesOpen = false;
  let isRemiseOpen = false;
  let isPrintReceiptOpen = false;
  let isProductEditOpen = false;
  let editingProduct: Product | null = null;
  // Bumped on every product-modal open so the modal component fully remounts
  // — guarantees a virgin form (no leftover data from the previous popup,
  // including after the APPLY-without-closing flow).
  let productModalKey = 0;
  let isQuickPurchaseOpen = false;
  let isReturnDamagedOpen = false;
  let isCreditCustomerOpen = false;
  let isVersementOpen = false;
  let isCheckoutOpen = false;
  let isCustomerSelectorOpen = false;
  // In-dropdown search (customers / suppliers) so picking from a long list
  // is fast while adding a quick customer/supplier stays one click away.
  let customerSearchInCart = '';
  let supplierSearchInCart = '';
  $: filteredCartCustomers = customerSearchInCart.trim()
    ? $customers.filter(
        (c) =>
          (c.name || '').toLowerCase().includes(customerSearchInCart.trim().toLowerCase()) ||
          (c.phone || '').includes(customerSearchInCart.trim())
      )
    : $customers;
  $: filteredCartSuppliers = supplierSearchInCart.trim()
    ? $suppliers.filter(
        (x) =>
          (x.name || '').toLowerCase().includes(supplierSearchInCart.trim().toLowerCase()) ||
          (x.phone || '').includes(supplierSearchInCart.trim())
      )
    : $suppliers;
  let isQuickAddCustomerOpen = false;
  let quickAddName = '';
  let quickAddPhone = '';
  let isQuickSaving = false;

  // Quick-add a walk-in customer straight from the cart: saves, selects the
  // new customer and refreshes the global store so the dropdown updates.
  async function handleQuickAddCustomer() {
    if (!quickAddName.trim() || isQuickSaving) return;
    try {
      isQuickSaving = true;
      const savedId = await invoke<number>('save_customer', {
        name: quickAddName.trim(),
        phone: quickAddPhone.trim() || null,
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
      await refreshCustomers();
      $selectedCustomerId = savedId;
      isQuickAddCustomerOpen = false;
      quickAddName = '';
      quickAddPhone = '';
    } catch (e: any) {
      alert('Could not save customer: ' + (e?.message || e));
    } finally {
      isQuickSaving = false;
    }
  }

  let isQuickAddSupplierOpen = false;
  let quickSupplierName = '';
  let quickSupplierPhone = '';

  async function handleQuickAddSupplier() {
    if (!quickSupplierName.trim() || isQuickSaving) return;
    try {
      isQuickSaving = true;
      const newSupplierId = await invoke<number>('save_supplier', {
        name: quickSupplierName.trim(),
        contactPerson: quickSupplierPhone.trim() || null,
        phone: quickSupplierPhone.trim() || null,
        email: null,
        address: null,
        rc: null,
        nif: null,
        nis: null,
        ai: null,
        notes: null,
        supplierId: null,
      });
      $selectedSupplierId = newSupplierId;
      await refreshSuppliers();
      isQuickAddSupplierOpen = false;
      quickSupplierName = '';
      quickSupplierPhone = '';
    } catch (e: any) {
      alert('Could not save supplier: ' + (e?.message || e));
    } finally {
      isQuickSaving = false;
    }
  }
  let isSupplierSelectorOpen = false;
  let isOtherArticleOpen = false;

  let isUnknownBarcodeModalOpen = false;
  // Last-receipt reopen/reprint modal (top bar button).
  let isLastReceiptOpen = false;
  // Banner for receipt-QR scan results (SALE: payload lookup).
  let qrSaleNotice = '';

  // EMPLOYEE SCAN (RFID badge / EMP-QR tag): sets the employee's linked
  // auto-customer as the cart customer so the sale lands on their history;
  // debt then rides their salary. Shows remaining salary + a quick
  // Absent/Today-Present control.
  let employeeBanner: {
    employeeId: number;
    name: string;
    code: string;
    customerId: number;
    remaining: number;
  } | null = null;

  async function refreshEmployeeBanner(employeeId: number, customerId: number, name: string, code: string) {
    const today = new Date();
    const monthStart = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-01`;
    const [advances, absences, customers] = await Promise.all([
      invoke<any[]>('list_employee_advances', { employeeId, month: null }).catch(() => []),
      invoke<any[]>('list_employee_absences', { employeeId, month: null }).catch(() => []),
      invoke<any[]>('list_customers').catch(() => []),
    ]);
    const advancesTotal = (advances || [])
      .filter((a: any) => (a.date || '') >= monthStart)
      .reduce((sum: number, a: any) => sum + (a.amount || 0), 0);
    const daysAbsent = (absences || [])
      .filter((a: any) => (a[4] || '') >= monthStart)
      .reduce((sum: number, a: any) => sum + (a[2] || 0), 0);
    let debt = 0;
    try {
      debt = Math.max(0, (customers as any[]).find((c: any) => c.id === customerId)?.balance || 0);
    } catch { debt = 0; }
    void today;
    return { employeeId, name, code, customerId, advances: advancesTotal, absences: daysAbsent, debt, remaining: 0 };
  }

  async function handleEmployeeScan(rawTag: string): Promise<boolean> {
    const tag = rawTag.trim();
    const up = tag.toUpperCase();
    if (!up.startsWith('RFID-') && !up.startsWith('EMP-QR-') && !up.startsWith('EMP_')) {
      return false;
    }
    try {
      const emp = await invoke<any | null>('find_employee_by_rfid', { rfid: tag });
      if (!emp) {
        qrSaleNotice = '❌ ' + (t('emp_card_unknown') || 'Unknown card');
        setTimeout(() => (qrSaleNotice = ''), 4000);
        return true;
      }
      // Resolve the employee's linked customer (auto-created on save; the
      // fallback creates it for employees added before this feature).
      let customerId = emp.customer_id as number | null;
      if (!customerId) {
        const customers = await invoke<any[]>('list_customers');
        const linked = (customers as any[]).find(
          (c: any) => c.qr_code === `EMP-${emp.employee_code}` || c.name === emp.full_name
        );
        if (linked) {
          customerId = linked.id;
        } else {
          customerId = await invoke<number>('save_customer', {
            name: emp.full_name,
            phone: emp.phone || null,
            email: null, address: null, rc: null, nif: null, nis: null, ai: null,
            initialDebt: 0, notes: `Auto-created for employee ${emp.employee_code}`,
            customerId: null,
          });
        }
      }
      // Make the employee the cart customer.
      if (customerId === null || customerId === undefined) {
        qrSaleNotice = '❌ ' + (t('emp_card_unknown') || 'Unknown card');
        setTimeout(() => (qrSaleNotice = ''), 4000);
        return true;
      }
      $selectedCustomerId = customerId;
      const today = new Date();
      const monthStart = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, '0')}-01`;
      const [advances, absences, customers] = await Promise.all([
        invoke<any[]>('list_employee_advances', { employeeId: emp.id, month: null }).catch(() => []),
        invoke<any[]>('list_employee_absences', { employeeId: emp.id, month: null }).catch(() => []),
        invoke<any[]>('list_customers').catch(() => []),
      ]);
      const advancesTotal = (advances || [])
        .filter((a: any) => (a.date || '') >= monthStart)
        .reduce((sum: number, a: any) => sum + (a.amount || 0), 0);
      const daysAbsent = (absences || [])
        .filter((a: any) => (a[4] || '') >= monthStart)
        .reduce((sum: number, a: any) => sum + (a[2] || 0), 0);
      const debt = Math.max(0, (customers as any[]).find((c: any) => c.id === customerId)?.balance || 0);
      const dailyRate = Math.round((emp.base_salary || 0) / 30);
      const remaining = Math.max(
        0,
        (emp.base_salary || 0) - advancesTotal - daysAbsent * dailyRate - debt
      );
      employeeBanner = {
        employeeId: emp.id,
        name: emp.full_name,
        code: emp.employee_code,
        customerId,
        remaining,
      };
    } catch (e: any) {
      console.error('Employee scan failed:', e);
      qrSaleNotice = '❌ ' + (typeof e === 'string' ? e : e?.message || 'Scan failed');
      setTimeout(() => (qrSaleNotice = ''), 4000);
    }
    return true;
  }

  async function markEmployeeAbsent(days: number) {
    if (!employeeBanner) return;
    try {
      await invoke('record_employee_absence', {
        employeeId: employeeBanner.employeeId,
        days,
        reason: 'POS quick-absence / غياب سريع',
        date: localTodayISO(),
      });
      qrSaleNotice = '✅ ' + (t('emp_absence_recorded') || 'Absence recorded');
      setTimeout(() => (qrSaleNotice = ''), 4000);
    } catch (e: any) {
      console.error('absence record failed', e);
      qrSaleNotice = '❌ ' + (typeof e === 'string' ? e : e?.message || 'Failed');
      setTimeout(() => (qrSaleNotice = ''), 4000);
    }
    closeEmployeeBanner();
  }

  function closeEmployeeBanner() {
    employeeBanner = null;
  }
  // Purchase mode: the product waiting for its new supplier price.
  let purchasePriceTarget: Product | null = null;
  let unknownScannedBarcode = '';
  let initialBarcodeForNewProduct = '';
  let editingProductWithExtraBarcode = '';

  let lastSaleSuccessNumber = '';
  // Eye toggle: show the cart's purchase cost under the total.
  let showCartCost = false;
  // Cart cost = Σ purchase_price × qty (falls back to unit_price in
  // purchase mode, where unit_price IS the cost).
  // Line count vs summed quantities: "2 lines · 13 units".
  $: totalUnits = $cartItems.reduce((sum, i) => sum + i.quantity, 0);
  $: cartCost = $cartItems.reduce(
    (sum, i) => sum + Math.round((i.purchase_price ?? (i as any).unit_cost ?? i.unit_price) * i.quantity),
    0
  );
  // Purchase mode: estimated sale value of the cart. Priority per line:
  // 1. the sale price ENTERED in the price dialog (sale_price_est),
  // 2. the product's current stored sale price,
  // 3. cost × 1.2 as a last-resort guess (unit_price is the COST here).
  $: estSaleTotal = $cartItems.reduce((sum, i) => {
    const live = products.find((pp) => pp.id === i.product_id);
    const estPerUnit =
      (i as any).sale_price_est ?? (live ? live.sale_price : i.unit_price * 1.2);
    return sum + Math.round(estPerUnit * i.quantity);
  }, 0);
  // Estimated profit of the buy: est. sale value − invoice cost.
  $: estProfit = estSaleTotal - cartCost;

  let barcodeBuffer = '';
  let lastKeyTime = 0;

  let currentShopName = 'TitaouPosCRM';
  let currentTime = new Date().toLocaleTimeString();
  let currentDate = new Date().toLocaleDateString();
  let timeInterval: any;
  // POS rule: idle seconds before the search bar re-steals focus (0 = off).
  let autofocusTimerSeconds = 0;

  // Rebindable shortcuts (Settings > Shortcuts persists these).
  const DEFAULT_SHORTCUTS: Record<string, string> = {
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
  let shortcuts: Record<string, string> = { ...DEFAULT_SHORTCUTS };
  function isKey(action: string, e: KeyboardEvent): boolean {
    return shortcuts[action] === e.key;
  }

  let cartContainerEl: HTMLDivElement;

  // Auto-scroll ONLY when a new item is actually added (not on every cart
  // mutation like qty edits, which caused the jumpy scrolling). The new line
  // scrolls into view at the top or bottom per the configured order.
  let lastCartCount = 0;
  $: if ($cartItems.length !== lastCartCount) {
    const grew = $cartItems.length > lastCartCount;
    lastCartCount = $cartItems.length;
    if (grew && cartContainerEl) {
      requestAnimationFrame(() => {
        if (!cartContainerEl) return;
        if ($cartItemOrder === 'top') {
          cartContainerEl.scrollTop = 0;
        } else {
          cartContainerEl.scrollTop = cartContainerEl.scrollHeight;
        }
      });
    }
  }

  onDestroy(() => {
    clearInterval(timeInterval);
    window.removeEventListener('keydown', handleGlobalKeyDown);
    window.removeEventListener('mousedown', handlePageMouseDown);
  });

  async function loadCategories() {
    try {
      categories = await invoke<Category[]>('get_categories');
    } catch (e) {
      console.error(e);
    }
  }

  async function loadUnits() {
    try {
      units = await invoke<Unit[]>('get_units');
    } catch (e) {
      console.error(e);
    }
  }


  async function loadProducts() {
    try {
      const list = await invoke<Product[]>('search_products', {
        query: searchQuery,
        categoryId: selectedCategory,
        searchType: searchType === 'qr' ? 'barcode' : searchType,
      });

      // Sort products
      // Pinned products ALWAYS float to the top, in their manual order,
      // whatever the selected sort — the backend already returns them
      // first, but the client sort below would have re-buried them.
      const pinned = list.filter(p => p.pinned).sort((a, b) => (a.pin_order || 0) - (b.pin_order || 0));

      if (sortBy === 'name_asc') {
        list.sort((a, b) => ((a.name_fr || a.name_ar || '') as string).localeCompare((b.name_fr || b.name_ar || '') as string));
      } else if (sortBy === 'name_desc') {
        list.sort((a, b) => ((b.name_fr || b.name_ar || '') as string).localeCompare((a.name_fr || a.name_ar || '') as string));
      } else if (sortBy === 'price_asc') {
        list.sort((a, b) => a.sale_price - b.sale_price);
      } else if (sortBy === 'price_desc') {
        list.sort((a, b) => b.sale_price - a.sale_price);
      } else if (sortBy === 'stock') {
        list.sort((a, b) => b.current_stock - a.current_stock);
      } else if (sortBy === 'best_sellers') {
        list.sort((a, b) => (b.total_sold || 0) - (a.total_sold || 0));
      } else if (sortBy === 'worst_sellers') {
        list.sort((a, b) => (a.total_sold || 0) - (b.total_sold || 0));
      }

      products = [...pinned, ...list.filter(p => !p.pinned)];

      // Scanned-barcode auto-add is handled ONLY by the window-level Enter
      // handler (rapid-input detection): adding here too made one scan fire
      // twice — once mid-typing (input event) and once on Enter — so the
      // quantity jumped by 2 per scan. Live search just filters.
    } catch (e) {
      console.error(e);
    }
  }

  async function handleScannedBarcode(rawBarcode: string) {
    const code = normalizeBarcode(rawBarcode).trim();
    if (!code) return;

    // Receipt QR payloads (SALE:<number>) open the sale for edit/reprint —
    // they are NOT product barcodes.
    if (code.toUpperCase().startsWith('SALE:')) {
      await handleSaleQrScan(code);
      searchQuery = '';
      return;
    }

    // Employee RFID / QR tags set the employee as the cart customer.
    if (await handleEmployeeScan(code)) {
      searchQuery = '';
      return;
    }

    // ACLAS scale barcodes (price/weight embedded): resolve the scalable
    // product and add it with the scanned WEIGHT.
    if (code.length >= 13 && /^\d{13}$/.test(code)) {
      try {
        const scan = await invoke<any | null>('resolve_scale_scan', { code });
        if (scan && scan.product_id) {
          const prod = await invoke<Product[]>('search_products', {
            query: String(scan.product_id),
            categoryId: null,
            searchType: 'all',
          }).catch(() => [] as Product[]);
          let product = prod.find((p) => p.id === scan.product_id);
          if (!product) {
            const all = await invoke<Product[]>('search_products', { query: '', categoryId: null, searchType: 'all' });
            product = (all as Product[]).find((p) => p.id === scan.product_id);
          }
          if (product) {
            const weight = scan.weight > 0 ? scan.weight : 1;
            const item = { ...product, sale_price: scan.unit_price || product.sale_price };
            addToCart(item, weight, $isRefundMode);
            searchQuery = '';
            return;
          }
        }
      } catch (e) {
        console.warn('scale scan lookup failed:', e);
      }
    }

    try {
      const list = await invoke<Product[]>('search_products', {
        query: code,
        categoryId: null,
        searchType: 'barcode',
      });

      const matched = list.find(p => p.barcodes?.includes(code) || p.sku === code);
      if (matched) {
        await addProductToCart(matched);
        searchQuery = '';
        await loadProducts();
      } else {
        unknownScannedBarcode = code;
        isUnknownBarcodeModalOpen = true;
        searchQuery = '';
      }
    } catch (e) {
      console.error('Error looking up scanned barcode:', e);
    }
  }

  // Adding a product to the cart: in PURCHASE mode the cashier is buying
  // stock, so a small dialog first asks the new purchase cost (prefilled
  // with the product's current one); sale mode adds at the shelf price.
  async function addProductToCart(product: Product) {
    if ($posMode === 'purchase') {
      purchasePriceTarget = product;
      return;
    }
    addToCart(product, 1, $isRefundMode);
  }

  function handlePurchasePriceConfirm(price: number, salePriceEntered: number, qty = 1) {
    if (!purchasePriceTarget) return;
    // Buy at the entered cost (unit_price mirrors sale_price = cost here).
    // The packaging picker can multiply the quantity (1 carton = 24 u).
    // sale_price_est carries the entered sale price for the footer's
    // estimated-sale-value and profit readout.
    addToCart(
      {
        ...purchasePriceTarget,
        sale_price: price,
        sale_price_est: salePriceEntered || purchasePriceTarget.sale_price,
      } as any,
      qty,
      $isRefundMode
    );
    purchasePriceTarget = null;
  }

  function handleCategoryClick(catId: number | null) {
    selectedCategory = catId;
    loadProducts();
  }

  function handleOpenEdit(p: Product) {
    editingProduct = p;
    initialBarcodeForNewProduct = '';
    editingProductWithExtraBarcode = '';
    productModalKey += 1;
    isProductEditOpen = true;
  }

  // Link flow: open the editor for an existing product with the scanned
  // barcode pre-attached for review before saving.
  function handleEditProductWithBarcode(p: Product, newBarcode: string) {
    editingProduct = p;
    initialBarcodeForNewProduct = '';
    editingProductWithExtraBarcode = newBarcode;
    productModalKey += 1;
    isProductEditOpen = true;
  }

  function handleOpenNewProduct() {
    editingProduct = null;
    initialBarcodeForNewProduct = '';
    editingProductWithExtraBarcode = '';
    productModalKey += 1;
    isProductEditOpen = true;
  }

  async function handleProductSaved() {
    await loadProducts();
    // After creating a product from a scanned barcode (or linking that
    // barcode to an existing one), add it straight to the cart.
    const pendingBarcode = initialBarcodeForNewProduct || editingProductWithExtraBarcode;
    if (pendingBarcode) {
      try {
        const list = await invoke<Product[]>('search_products', {
          query: pendingBarcode,
          categoryId: null,
          searchType: 'barcode',
        });
        const created = list.find(p => p.barcodes?.includes(pendingBarcode));
        if (created) {
          addToCart(created, 1, $isRefundMode);
        }
      } catch (e) {
        console.warn('Auto add created product error:', e);
      }
      initialBarcodeForNewProduct = '';
      editingProductWithExtraBarcode = '';
    } else {
      // Product created via the + card: do NOT add it to the cart; just
      // make sure the search bar no longer holds the half-typed query so
      // the next scan starts clean.
      searchQuery = '';
      await loadProducts();
    }
  }

  // Fast Checkout Action
  async function handleFastCheckout() {
    if ($cartItems.length === 0) return;

    if (!$activeSession) {
      isCashDrawerOpen = true;
      return;
    }

    // Purchase / Broken modes have their own checkout flows.
    if ($posMode === 'purchase') {
      await executeModeCheckout('purchase');
      return;
    }
    if ($posMode === 'broken') {
      await executeModeCheckout('broken');
      return;
    }

    if (selectedPaymentMode === 'credit') {
      isCreditCustomerOpen = true;
      return;
    }

    if (selectedPaymentMode === 'versement') {
      isVersementOpen = true;
      return;
    }

    // Cash or TPE Checkout directly
    await executeCheckout(null, undefined);
  }

  // Mode switch with cart parking: each mode keeps its own held cart,
  // tagged MODE:sale / MODE:purchase / MODE:broken in the held list.
  async function switchPosMode() {
    const next = $posMode === 'sale' ? 'purchase' : $posMode === 'purchase' ? 'broken' : 'sale';
    if ($cartItems.length > 0) {
      const ok = await holdCurrentSale(`[${$posMode.toUpperCase()} MODE]`);
      if (!ok) return; // hold failed: stay in the current mode, keep the cart
    }
    $posMode = next;
  }

  // Cycle the payment mode: Cash -> TPE -> Credit -> Versement (F11).
  function cyclePaymentMode() {
    if (selectedPaymentMode === 'cash') selectedPaymentMode = 'tpe';
    else if (selectedPaymentMode === 'tpe') selectedPaymentMode = 'credit';
    else if (selectedPaymentMode === 'credit') selectedPaymentMode = 'versement';
    else selectedPaymentMode = 'cash';
  }

  // Purchase & Broken mode checkouts: purchases add stock from the
  // supplier; broken writes quantity off and books an expense.
  async function executeModeCheckout(mode: 'purchase' | 'broken') {
    if ($cartItems.length === 0) return;
    try {
      const stamp = Date.now().toString().slice(-8);
      if (mode === 'purchase') {
        // Stock-in purchase from the selected supplier at cart line prices.
        const total = $cartItems.reduce((s, i) => s + Math.round(i.unit_price * i.quantity), 0);
        await invoke('create_purchase', {
          input: {
            invoice_number: 'ACH-' + stamp,
            supplier_id: $selectedSupplierId ?? 1,
            user_id: $currentUser?.id || 1,
            date: localTodayISO(),
            subtotal: total,
            discount: 0,
            tax: 0,
            total,
            paid_amount: total,
            payment_method: 'cash',
            items: $cartItems.map((i) => ({
              product_id: i.product_id,
              quantity: i.quantity,
              unit_cost: Math.round(i.unit_price),
              discount: 0,
              tax: 0,
              total: Math.round(i.unit_price * i.quantity),
            })),
            notes: 'POS Purchase Mode (شراء)',
          },
        });

        // The purchase is paid cash from the drawer: book the supplier
        // payment as a cash movement so the register and stats reflect the
        // money leaving (previously the drawer never moved for POS buys —
        // "cash out on purchase mode didn't work").
        if ($activeSession?.id) {
          await invoke('record_supplier_debt_payment', {
            input: {
              supplier_id: $selectedSupplierId ?? 1,
              amount: total,
              payment_method: 'cash',
              reference: 'ACH-' + stamp,
              session_id: $activeSession.id,
              user_id: $currentUser?.id || 1,
              notes: 'POS purchase paid cash / دفع شراء نقدي من الصندوق',
            },
          }).catch(() => {});
        }
      } else {
        // Broken: quantity leaves stock and the value becomes an expense
        // per line (negative-quantity purchase = write-off movement).
        const total = $cartItems.reduce((s, i) => s + Math.round(i.unit_price * i.quantity), 0);
        await invoke('create_purchase', {
          input: {
            invoice_number: 'BRK-' + stamp,
            supplier_id: 1,
            user_id: $currentUser?.id || 1,
            date: localTodayISO(),
            subtotal: total,
            discount: 0,
            tax: 0,
            total: 0,
            paid_amount: 0,
            payment_method: 'cash',
            items: $cartItems.map((i) => ({
              product_id: i.product_id,
              quantity: -i.quantity,
              unit_cost: Math.round(i.unit_price),
              discount: 0,
              tax: 0,
              total: Math.round(i.unit_price * i.quantity),
            })),
            notes: `Broken/Damaged write-off (تالف): ${$cartItems.map((i) => `${i.name_fr || i.name_ar} x${i.quantity}`).join(', ')}`,
          },
        });
      }
      // Purchase/broken checkouts empty the cart: the buy is recorded, the
      // drawer movement booked — the lines must not linger on screen.
      clearCart();
      if (mode === 'purchase') refreshSuppliers().catch(() => {});
      await loadProducts();
      setTimeout(() => (lastSaleSuccessNumber = ''), 4000);
    } catch (err: any) {
      console.error('Mode checkout failed:', err);
      alert('Checkout Failed: ' + (err?.message || err));
    }
  }

  // Clicking anywhere outside the customer/supplier dropdowns closes them.
  function handlePageMouseDown(e: MouseEvent) {
    const el = e.target as HTMLElement;
    if (!el.closest('[data-customer-selector]')) isCustomerSelectorOpen = false;
    if (!el.closest('[data-supplier-selector]')) isSupplierSelectorOpen = false;
  }

  // "Edit in POS" from the last-receipt modal or a scanned receipt QR:
  // the CURRENT cart is HELD first (never silently lost), then the sale's
  // items load; the existing originSaleId mechanism updates the sale in
  // place on checkout (no duplicate row, same receipt number).
  async function handleEditLastSaleInPos(sale: any) {
    try {
      // Park the active cart before replacing it (editing a receipt with
      // items in the cart must HOLD it, never discard).
      if ($cartItems.length > 0) {
        await holdCurrentSale(`[${$posMode.toUpperCase()} MODE]`);
      }
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
      clearCart();
      $cartItems = mapped;
      if (sale.customer_id) $selectedCustomerId = sale.customer_id;
      originSaleId.set(sale.id);
    } catch (e) {
      console.error('Failed to load last sale for editing:', e);
    }
  }

  // Receipt QR scan (payload `SALE:<sale_number>`, printed on every
  // receipt): opens that sale in the POS for reprint/edit. The current
  // cart is held first; the sale loads in EDIT mode (originSaleId) so
  // checkout updates it in place. Returns true when the payload was a
  // sale QR (consumed, do not treat it as a product barcode).
  async function handleSaleQrScan(rawPayload: string): Promise<boolean> {
    const payload = rawPayload.trim();
    if (!payload.toUpperCase().startsWith('SALE:')) return false;
    const saleNumber = payload.slice(5).trim();
    if (!saleNumber) return true;
    try {
      const res = await invoke<any | null>('get_sale_by_number', { saleNumber });
      if (!res) {
        qrSaleNotice = `❌ ${t('sale_qr_not_found')}: #${saleNumber}`;
      } else {
        await handleEditLastSaleInPos(res.sale);
        qrSaleNotice = `🧾 ${t('sale_loaded_edit')}: #${res.sale.sale_number}`;
      }
    } catch (e: any) {
      console.error('Sale QR lookup failed:', e);
      qrSaleNotice = '❌ ' + (typeof e === 'string' ? e : e?.message || 'Lookup failed');
    }
    setTimeout(() => (qrSaleNotice = ''), 5000);
    return true;
  }

  onMount(async () => {
    try {
      const s = await invoke<Record<string, string>>('get_all_settings');
      currentShopName = s['shop_name_fr'] || s['shop_name_ar'] || 'TitaouPosCRM';
      if (s['cart_item_order'] === 'top' || s['cart_item_order'] === 'bottom') {
        $cartItemOrder = s['cart_item_order'];
      }
      // Auto-print persistence: stays as last left (on/off), across restarts.
      autoPrintEnabled = s['pos_autoprint'] !== 'false';
      // Auto-drawer kick on cash checkout: same persistence rule.
      autoDrawerEnabled = s['open_drawer_on_sale'] !== 'false';
      // Auto-focus search: enabled flag + idle timer in seconds.
      if (s['pos_autofocus_search'] === 'true') {
        autofocusTimerSeconds = parseInt(s['pos_autofocus_timer_seconds'] || '10', 10) || 10;
      }
      $allowNegativeStock = s['allow_negative_stock'] === 'true';
      // Checkout total rounding: OFF (default) / nearest 50 / nearest 100 DZD.
      $saleTotalRoundingStep = s['sale_price_rounding'] === '50' ? 50 : s['sale_price_rounding'] === '100' ? 100 : 0;
    } catch (e) {
      console.warn(e);
    }

    timeInterval = setInterval(() => {
      currentTime = new Date().toLocaleTimeString();
      currentDate = new Date().toLocaleDateString();
    }, 1000);

    try {
      const raw = await invoke<string | null>('get_setting', { key: 'pos_shortcuts' });
      if (raw) {
        const parsed = JSON.parse(raw);
        if (parsed && typeof parsed === 'object') {
          shortcuts = { ...shortcuts, ...parsed };
        }
      }
    } catch {
      // Defaults stand.
    }

    // Catalog: families + products must load on mount — losing this call
    // emptied the whole POS grid until a manual category switch.
    await loadCategories();
    await loadUnits();
    await loadProducts();
    await refreshCustomers();
    await refreshSuppliers();

    // Recover an interrupted sale (shutdown/crash) before it was checked out.
    await restoreActiveCart();

    // Notification deep-link: open the product's editor.
    if (initialOpenProductId) {
      const target = products.find((p) => p.id === initialOpenProductId);
      if (target) {
        handleOpenEdit(target);
        onProductOpened();
      }
    }

    window.addEventListener('keydown', handleGlobalKeyDown);
    window.addEventListener('mousedown', handlePageMouseDown);
  });

  // Unified Checkout dialog (TopBar Checkout / F2): customer + amount +
  // reste/change; amount < total lets the cashier pick credit vs versement.
  function handleOpenCheckout() {
    if ($cartItems.length === 0) return;
    if (!$activeSession) {
      isCashDrawerOpen = true;
      return;
    }
    isCheckoutOpen = true;
  }

  function handleCheckoutConfirm(result: {
    customerId: number;
    customerName: string;
    paidAmount: number;
    mode: 'direct' | 'credit' | 'versement';
  }) {
    $selectedCustomerId = result.customerId;
    const change = Math.max(0, result.paidAmount - $cartGrandTotal);
    executeCheckout(result.customerId, result.customerName, {
      mode: result.mode,
      paidAmount: result.paidAmount,
      changeAmount: change,
    });
  }

  async function executeCheckout(
    customerId: number | null,
    customerName?: string,
    options?: {
      mode?: 'direct' | 'credit' | 'versement';
      paidAmount?: number;
      changeAmount?: number;
    }
  ) {
    try {
      const saleNumber = 'VTE-' + Date.now().toString().slice(-6);
      const saleDate = new Date().toLocaleString();
      const cashier = $currentUser?.display_name || 'Admin';

      const mode = options?.mode || 'direct';
      const paid = mode === 'direct' ? $cartGrandTotal : (options?.paidAmount ?? 0);
      const change = mode === 'direct' ? 0 : (options?.changeAmount ?? 0);
      const reste = Math.max(0, $cartGrandTotal - paid);
      const effectiveMethod = mode === 'versement' ? 'versement' : mode === 'credit' ? 'credit' : selectedPaymentMode;

      const saleInput = {
        session_id: $activeSession?.id || 1,
        customer_id: customerId,
        user_id: $currentUser?.id || 1,
        subtotal: $cartSubtotal,
        total_amount: $cartGrandTotal,
        paid_amount: paid,
        change_amount: change,
        tax_amount: 0,
        discount_amount: $globalDiscountAmount,
        discount_percentage: $globalDiscountPercent,
        payment_method: effectiveMethod,
        is_refund: $isRefundMode,
        notes:
          mode === 'versement'
            ? `Versement Sale to ${customerName} — Paid ${paid} DZD, Reste ${reste} DZD (goods at shop)`
            : mode === 'credit'
              ? `Credit Sale to ${customerName} — Paid ${paid} DZD, Reste ${reste} DZD`
              : customerName
                ? `Direct Sale to ${customerName}`
                : undefined,
        skip_stock: mode === 'versement',
        payments: [
          {
            // Record only what was actually handed over now; the remainder is
            // tracked on the customer's account for credit AND versement.
            payment_method: effectiveMethod,
            amount: paid,
            reference_code: null,
          }
        ],
        items: $cartItems.map((i) => ({
          product_id: i.product_id,
          sku: i.sku || '',
          barcode: i.barcode || '',
          name_fr: i.name_fr || '',
          name_ar: i.name_ar || '',
          name_en: i.name_en || '',
          quantity: i.quantity,
          unit_price: i.unit_price,
          discount_amount: i.discount_amount || 0,
          tax_amount: i.tax_amount || 0,
          total_price: i.total_price,
          is_refund: i.is_refund || false,
        })),
      };

      // Editing a sale loaded from history? Update it in place (tagged
      // MODIFIED, no duplicate) instead of creating a new row.
      if (get(originSaleId)) {
        await invoke('replace_sale', { originalSaleId: get(originSaleId), input: saleInput });
      } else {
        await invoke('create_sale', { input: saleInput });
      }

      // Offline receipt QR (local data URL; no network needed).
      const receiptQrDataUrl = await entityQrDataUrl(
        `SALE:${saleNumber}`,
        100
      ).catch(() => undefined);

      // Silent Auto-Print (F12 quick checkout skips it for this sale).
      const printThisSale = autoPrintEnabled && !suppressNextPrint;
      suppressNextPrint = false;
      if (printThisSale) {
        let appSettings: Record<string, string> = {};
        try {
          appSettings = await invoke<Record<string, string>>('get_all_settings');
        } catch (e) {
          console.warn('Could not load settings for receipt:', e);
        }

        const terminalName = $networkStatus?.pc_name || undefined;

        const receiptItems = $cartItems.map((i) => ({
          name: i.name_fr || i.name_ar,
          quantity: i.quantity,
          unitPrice: i.unit_price,
          totalPrice: i.total_price,
          discountPerUnit: i.discount_amount || 0,
          isRefund: i.is_refund || false,
        }));

        // ONE unified receipt preset system (v0.5.16): every sale kind and
        // every print location builds from the same settings + template
        // choice — no more a hardcoded format per page.
        const effectiveMethod = mode === 'versement' ? 'VERSEMENT (تسبقة)' : mode === 'credit' ? 'CREDIT (دين)' : selectedPaymentMode.toUpperCase();
        if (mode === 'credit') {
          // Store copy + client copy, one page each — break on the FIRST
          // receipt's own node (an empty separator div printed a blank page).
          const storeCopy = buildUnifiedReceipt({
            saleNumber, saleDate, cashierName: cashier, terminalName,
            customerName: customerName || 'Client Crédit',
            items: receiptItems,
            subtotal: $cartSubtotal,
            discount: $globalDiscountAmount,
            grandTotal: $cartGrandTotal,
            amountPaid: paid,
            change,
            paymentMethod: effectiveMethod,
            settings: appSettings,
            qrDataUrl: receiptQrDataUrl,
            isCredit: true,
            copyLabel: 'COPIE MAGASIN / STORE COPY',
          });
          const clientCopy = buildUnifiedReceipt({
            saleNumber, saleDate, cashierName: cashier, terminalName,
            customerName: customerName || 'Client Crédit',
            items: receiptItems,
            subtotal: $cartSubtotal,
            discount: $globalDiscountAmount,
            grandTotal: $cartGrandTotal,
            amountPaid: paid,
            change,
            paymentMethod: effectiveMethod,
            settings: appSettings,
            qrDataUrl: receiptQrDataUrl,
            isCredit: true,
            copyLabel: 'COPIE CLIENT / CUSTOMER COPY',
          });
          // Smart print: native raster → ESC/POS RAW fallback.
          void printHtmlSilently(
            `<div style="page-break-after:always;break-after:page;">${storeCopy.html}</div>` + clientCopy.html,
            'Credit Receipts',
            { widthMm: storeCopy.paperWidthMm }
          ).then((r) => { if (!r.ok) console.warn('credit receipts print:', r.message); });
        } else {
          const receipt = buildUnifiedReceipt({
            saleNumber, saleDate, cashierName: cashier, terminalName,
            customerName: customerName || undefined,
            items: receiptItems,
            subtotal: $cartSubtotal,
            discount: $globalDiscountAmount,
            grandTotal: $cartGrandTotal,
            amountPaid: mode === 'direct' ? $cartGrandTotal : paid,
            change,
            paymentMethod: effectiveMethod,
            settings: appSettings,
            qrDataUrl: receiptQrDataUrl,
            copyLabel: mode === 'versement' ? 'VERSEMENT / تسبقة' : undefined,
            versementPaid: mode === 'versement' ? paid : undefined,
            versementRemaining: mode === 'versement' ? reste : undefined,
          });
          // Smart print: native raster first; when the machine has NO
          // browser at all, fall back to ESC/POS RAW text automatically.
          printReceiptSmart({
            saleNumber, saleDate, cashierName: cashier, terminalName,
            customerName: customerName || undefined,
            items: receiptItems,
            subtotal: $cartSubtotal,
            discount: $globalDiscountAmount,
            grandTotal: $cartGrandTotal,
            amountPaid: mode === 'direct' ? $cartGrandTotal : paid,
            change,
            paymentMethod: effectiveMethod,
            settings: appSettings,
            qrDataUrl: receiptQrDataUrl,
            copyLabel: mode === 'versement' ? 'VERSEMENT / تسبقة' : undefined,
            versementPaid: mode === 'versement' ? paid : undefined,
            versementRemaining: mode === 'versement' ? reste : undefined,
          }).then((r) => { if (!r.ok) console.warn('receipt print failed:', r.message); });
        }
      }

      // Auto-kick cash drawer
      if (autoDrawerEnabled && mode === 'direct' && effectiveMethod === 'cash') {
        try {
          const settings = await invoke<Record<string, string>>('get_all_settings');
          const port = parseInt(settings['drawer_com_port'] || '1');
          const baud = parseInt(settings['drawer_baud_rate'] || '9600');
          await invoke('open_serial_cash_drawer', { comPort: port, baudRate: baud });
        } catch (drawerErr) {
          console.warn('Native drawer checkout trigger notice:', drawerErr);
        }
      }

      // Learn the sold quantities so scalable products get smart
      // weight-suggestion chips next time (1kg, 0.5, eggs 30…).
      recordSoldQuantities($cartItems.map((i) => ({ product_id: i.product_id, quantity: i.quantity })));

      lastSaleSuccessNumber = saleNumber;
      clearCart();
      await loadProducts();

      setTimeout(() => {
        lastSaleSuccessNumber = '';
      }, 4000);
    } catch (err: any) {
      console.error('Failed to complete sale:', err);
      alert('Checkout Failed: ' + (err.message || err));
    }
  }

  // Quantity-edit mode navigation: start on the first cart line (F6), then
  // Enter jumps to the next line, ESC leaves the mode entirely.
  function advanceQtyEdit() {
    const keys = $cartItems.map(itemKey);
    if (keys.length === 0) return;
    const currentIdx = keys.indexOf($qtyEditTarget);
    const nextIdx = currentIdx < 0 ? 0 : currentIdx + 1;
    if (nextIdx < keys.length) {
      qtyEditTarget.set(keys[nextIdx]);
    } else {
      // Past the last line: exit edit mode.
      stopQtyEdit();
    }
  }

  // Global Keyboard Shortcuts
  async function handleGlobalKeyDown(e: KeyboardEvent) {
    // While editing quantities, Enter advances to the next cart line and
    // ESC exits the mode. Swallow both so they never trigger other actions.
    if ($qtyEditTarget) {
      if (e.key === 'Enter') {
        e.preventDefault();
        e.stopPropagation();
        advanceQtyEdit();
        return;
      }
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        stopQtyEdit();
        return;
      }
    }

    // Barcode scanners type into whatever field has focus. When that field
    // is a modal input (product editor, checkout, etc.) — anything that is
    // NOT the POS search bar — the characters belong to that field: don't
    // accumulate them in the scanner buffer, or Enter in the modal would
    // pop the unknown-barcode dialog over it.
    const active = document.activeElement as HTMLElement | null;
    if (
      active &&
      (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA') &&
      !active.hasAttribute('data-scanner-input')
    ) {
      return;
    }

    // Barcode scanner rapid input detection
    const now = Date.now();
    if (now - lastKeyTime > 100) {
      barcodeBuffer = '';
    }
    lastKeyTime = now;

    if (e.key === 'Enter') {
      const normalizedCode = normalizeBarcode(barcodeBuffer);
      if (normalizedCode.length >= 4 || barcodeBuffer.length >= 4) {
        e.preventDefault();
        const code = normalizedCode || barcodeBuffer;
        barcodeBuffer = '';
        handleScannedBarcode(code);
        return;
      }
      // Enter with a short/typed query: if the live search narrowed the
      // catalog to exactly one product, add it straight to the cart.
      if (searchQuery.trim() && products.length === 1) {
        e.preventDefault();
        addToCart(products[0], 1, $isRefundMode);
        searchQuery = '';
        loadProducts();
        return;
      }
    } else if (e.key.length === 1) {
      barcodeBuffer += e.key;
    }

    // Function keys shortcuts (defaults; rebindable in Settings > Shortcuts).
    if (isKey('new_sale', e)) {
      // New sale: HOLD the current cart first (never silently lose it),
      // then clear for the next customer.
      e.preventDefault();
      if ($cartItems.length > 0) {
        holdCurrentSale();
      } else {
        clearCart();
      }
    } else if (isKey('checkout_print', e)) {
      // Checkout + print.
      e.preventDefault();
      handleFastCheckout();
    } else if (isKey('hold_cart', e)) {
      // Hold the current cart (park it for the customer to come back).
      e.preventDefault();
      if ($cartItems.length > 0) {
        holdCurrentSale();
      } else {
        isHeldSalesOpen = true;
      }
    } else if (isKey('remise', e)) {
      e.preventDefault();
      isRemiseOpen = true;
    } else if (isKey('returns', e)) {
      // Returns / refunds dialog.
      e.preventDefault();
      isReturnDamagedOpen = true;
    } else if (isKey('edit_qty', e)) {
      e.preventDefault();
      // Enter quantity-edit mode on the first cart line (or the next one if
      // already editing).
      if ($cartItems.length === 0) return;
      if ($qtyEditTarget) {
        advanceQtyEdit();
      } else {
        qtyEditTarget.set(itemKey($cartItems[0]));
      }
    } else if (isKey('toggle_products', e)) {
      // Toggle products page (inventory).
      e.preventDefault();
      onNavigate('inventory');
    } else if (isKey('toggle_register', e)) {
      // Toggle the cash register page.
      e.preventDefault();
      onNavigate('cash');
    } else if (isKey('toggle_sales', e)) {
      // Toggle the sales history page.
      e.preventDefault();
      onNavigate('sales');
    } else if (isKey('cycle_mode', e)) {
      // Cycle the POS mode: Sale -> Purchase -> Broken. Switching away with
      // items in the cart parks it as a HELD cart tagged for that mode, so
      // nothing is lost between sale/purchase/broken work.
      e.preventDefault();
      await switchPosMode();
    } else if (isKey('cycle_payment', e)) {
      // Cycle the payment mode: Cash -> TPE -> Credit -> Versement.
      e.preventDefault();
      cyclePaymentMode();
    } else if (isKey('quick_checkout', e)) {
      // Quick checkout WITHOUT printing the receipt.
      e.preventDefault();
      suppressNextPrint = true;
      handleFastCheckout();
    } else if (isKey('open_drawer', e)) {
      // Kick the cash drawer open.
      e.preventDefault();
      try {
        invoke('open_serial_cash_drawer', { comPort: 1, baudRate: 9600 });
      } catch (err) {
        console.warn(err);
      }
    }
  }
</script>

<div class="flex flex-col h-full bg-pos-bg select-none">
  <!-- Top Persistent Grouped Action Cards Bar -->
  <TopActionBar
    bind:selectedPaymentMode
    bind:autoPrintEnabled
    bind:autoDrawerEnabled
    onOpenPayment={handleFastCheckout}
    onCheckout={handleOpenCheckout}
    onCycleMode={switchPosMode}
    onOpenCashDrawer={() => (isCashDrawerOpen = true)}
    onOpenRemise={() => (isRemiseOpen = true)}
    onOpenHeldSales={() => (isHeldSalesOpen = true)}
    onPrintReceipt={() => (isPrintReceiptOpen = true)}
    onOpenOtherArticle={() => (isOtherArticleOpen = true)}
    onReopenLastReceipt={() => (isLastReceiptOpen = true)}
  />

  {#if lastSaleSuccessNumber}
    <div class="bg-emerald-600 text-white px-4 py-2 text-xs font-bold flex items-center justify-between shadow-md animate-in slide-in-from-top-2">
      <div class="flex items-center gap-2">
        <CheckCircle2 class="w-4 h-4" />
        <span>{t('pos_sale_completed')}: #{lastSaleSuccessNumber}</span>
      </div>
      <button on:click={() => (lastSaleSuccessNumber = '')} class="underline cursor-pointer">{t('pos_dismiss')}</button>
    </div>
  {/if}

  {#if qrSaleNotice}
    <div class="bg-indigo-600 text-white px-4 py-2 text-xs font-bold flex items-center justify-between shadow-md animate-in slide-in-from-top-2">
      <span>{qrSaleNotice}</span>
      <button on:click={() => (qrSaleNotice = '')} class="underline cursor-pointer">{t('pos_dismiss')}</button>
    </div>
  {/if}

  {#if employeeBanner}
    <div class="bg-gradient-to-r from-violet-600 to-indigo-600 text-white px-4 py-2.5 text-xs font-bold flex flex-wrap items-center justify-between gap-2 shadow-md animate-in slide-in-from-top-2">
      <div class="flex items-center gap-2">
        <span class="px-2 py-0.5 bg-white/25 rounded-lg font-black uppercase">{employeeBanner.code}</span>
        <span>{employeeBanner.name}</span>
        <span class="opacity-90">• {t('emp_remaining_salary')}: <strong class="font-mono">{employeeBanner.remaining.toLocaleString()} DZD</strong></span>
      </div>
      <div class="flex items-center gap-1.5">
        <button
          type="button"
          on:click={() => markEmployeeAbsent(1)}
          class="px-3 py-1.5 bg-rose-500 hover:bg-rose-600 text-white font-black text-[11px] rounded-lg cursor-pointer active:scale-95 transition"
        >{t('emp_absent_today')}</button>
        <button
          type="button"
          on:click={closeEmployeeBanner}
          class="px-3 py-1.5 bg-white/20 hover:bg-white/30 text-white font-black text-[11px] rounded-lg cursor-pointer active:scale-95 transition"
        >{t('emp_present')}</button>
        <button on:click={closeEmployeeBanner} class="underline cursor-pointer ms-1">{t('pos_dismiss')}</button>
      </div>
    </div>
  {/if}

  <!-- Main POS Workspace -->
  <div class="flex-1 flex overflow-hidden p-2.5 gap-2.5">
    <!-- LEFT PANEL: Search, Category Pills, Catalog Grid -->
    <div class="flex-1 flex flex-col min-w-0 bg-pos-card border border-pos-border rounded-2xl p-3 shadow-xs">
      <!-- Search & Sort Row -->
      <div class="flex items-center gap-2 mb-2">
        <div class="flex-1">
          <UniversalSearchBar
            bind:query={searchQuery}
            bind:searchType
            onSearch={loadProducts}
            autofocusSeconds={autofocusTimerSeconds}
            onSaleQrScan={handleSaleQrScan}
            onPurchaseQrScan={(payload) => onOpenPurchase?.(payload)}
            onEmployeeScan={handleEmployeeScan}
          />
        </div>

        <!-- Sort dropdown -->
        <div class="flex items-center gap-1 bg-slate-100 dark:bg-slate-800 px-2.5 py-1.5 rounded-xl border border-pos-border text-xs">
          <ArrowUpDown class="w-3.5 h-3.5 text-pos-muted" />
          <select bind:value={sortBy} on:change={loadProducts} class="bg-transparent text-pos-text font-bold outline-none cursor-pointer">
            <option value="default">{t('sort_default')}</option>
            <option value="name_asc">{t('sort_name_asc')}</option>
            <option value="name_desc">{t('sort_name_desc')}</option>
            <option value="price_asc">{t('sort_price_asc')}</option>
            <option value="price_desc">{t('sort_price_desc')}</option>
            <option value="stock">{t('sort_stock')}</option>
            <option value="best_sellers">{t('sort_best')}</option>
            <option value="worst_sellers">{t('sort_worst')}</option>
          </select>
        </div>
      </div>

      <!-- Categories Pills with Wrap -->
      <div class="flex items-center gap-1.5 flex-wrap pb-1.5 mb-1.5 shrink-0">
        <button
          type="button"
          on:click={() => handleCategoryClick(null)}
          class="px-3 py-1.5 rounded-xl text-xs font-bold shrink-0 transition cursor-pointer {selectedCategory === null ? 'bg-sky-600 text-white shadow-xs' : 'bg-slate-100 dark:bg-slate-800 text-pos-muted hover:bg-slate-200'}"
        >
          {t('filter_all')}
        </button>

        {#each categories as cat}
          <button
            type="button"
            on:click={() => handleCategoryClick(cat.id)}
            class="px-3 py-1.5 rounded-xl text-xs font-bold shrink-0 transition flex items-center gap-1.5 cursor-pointer {selectedCategory === cat.id ? 'bg-sky-600 text-white shadow-xs' : 'bg-slate-100 dark:bg-slate-800 text-pos-muted hover:bg-slate-200'}"
          >
            <span class="w-2.5 h-2.5 rounded-full" style="background-color: {cat.color || '#0284c7'}"></span>
            <span>{cat.name_ar || cat.name_fr}</span>
          </button>
        {/each}

        <!-- Add / Manage Categories -->
        <button
          type="button"
          on:click={() => (isCategoryManagerOpen = true)}
          class="p-1.5 rounded-xl text-xs font-bold bg-slate-100 dark:bg-slate-800 text-pos-muted hover:text-sky-600 hover:bg-slate-200 shrink-0 transition cursor-pointer"
          title="Manage Groups"
        >
          <Settings2 class="w-4 h-4" />
        </button>
      </div>

      <!-- Products Grid -->
      <div class="flex-1 overflow-y-auto pr-1">
        <div class="grid grid-cols-2 md:grid-cols-3 xl:grid-cols-4 gap-2.5">
          <!-- CARD 1: Interactive "+ Quick Add Product" Card -->
          <button
            type="button"
            on:click={handleOpenNewProduct}
            class="flex flex-col items-center justify-center p-4 bg-sky-50/50 dark:bg-sky-950/20 border-2 border-dashed border-sky-400 hover:border-sky-600 rounded-2xl text-sky-600 dark:text-sky-400 cursor-pointer transition hover:scale-[1.02] active:scale-95 group shadow-xs min-h-[160px]"
          >
            <div class="w-12 h-12 rounded-2xl bg-sky-100 dark:bg-sky-900/60 flex items-center justify-center group-hover:rotate-90 transition duration-200 mb-2">
              <Plus class="w-6 h-6" />
            </div>
            <span class="font-black text-xs text-center">{t('add_product_hint')}</span>
            <span class="text-[10px] text-pos-muted font-bold">　</span>
          </button>

          <!-- Product Catalog Cards with Category Borders and Pen Icon -->
          {#each products as product (product.id)}
            {@const cat = categories.find(c => c.id === product.category_id)}
            <ProductCard
              {product}
              categoryColor={cat?.color || '#0284c7'}
              onEditProduct={handleOpenEdit}
              onPinned={loadProducts}
              onAddToCart={addProductToCart}
              pinnedIds={products.filter(p => p.pinned).map(p => p.id)}
            />
          {/each}
        </div>
      </div>
    </div>

    <!-- RIGHT PANEL: Shopping Cart with Animated Store Title -->
    <div class="w-[410px] flex flex-col shrink-0 bg-pos-card border rounded-2xl shadow-xs overflow-hidden transition-colors {$posMode === 'purchase' ? 'border-amber-400 ring-1 ring-amber-400/50' : $posMode === 'broken' ? 'border-rose-400 ring-1 ring-rose-400/50' : 'border-pos-border'}">
      <!-- Big Animated Store Header (color = current POS mode) -->
      <div class="p-3 bg-gradient-to-r text-white shadow-xs transition-colors duration-300 {$posMode === 'purchase'
        ? 'from-amber-600 via-orange-600 to-amber-700'
        : $posMode === 'broken'
        ? 'from-rose-700 via-red-700 to-rose-800'
        : 'from-sky-600 via-indigo-600 to-sky-700'}">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2.5">
            <div class="w-10 h-10 rounded-2xl bg-white p-1 flex items-center justify-center shadow-md shrink-0">
              <img src="/logo.png" alt="Logo" class="w-full h-full object-contain" />
            </div>
            <div>
              <h2 class="font-black text-base tracking-tight leading-none text-white drop-shadow-xs truncate max-w-[210px]">
                {currentShopName}
              </h2>
              <p class="text-[10px] text-sky-200 font-bold leading-none mt-1">
                {currentDate} • {currentTime}
              </p>
            </div>
          </div>
          <div class="flex items-center gap-1.5">
            <span class="px-2 py-0.5 rounded-full text-[10px] font-black uppercase tracking-wider {$posMode === 'purchase' ? 'bg-amber-300 text-amber-900' : $posMode === 'broken' ? 'bg-rose-300 text-rose-900' : 'bg-emerald-300 text-emerald-900'}">
              {$posMode === 'purchase' ? 'شراء / ACHAT' : $posMode === 'broken' ? 'تالف / CASSÉ' : 'بيع / VENTE'}
            </span>
            <div class="flex items-center gap-1 bg-white/20 px-2 py-0.5 rounded-full text-[10px] font-bold">
              <span class="w-2 h-2 rounded-full bg-emerald-400 animate-ping"></span>
              <span>Live</span>
            </div>
          </div>
        </div>
      </div>

      <!-- Closed Cash Register Warning Alert Banner -->
      {#if !$activeSession}
        <div class="p-2.5 bg-amber-500/15 border-b border-amber-500/30 flex items-center justify-between text-amber-800 dark:text-amber-300 text-xs">
          <div class="flex items-center gap-1.5 font-bold">
            <AlertCircle class="w-4 h-4 text-amber-500 shrink-0" />
            <span>{t('pos_session_closed')}</span>
          </div>
          <button
            on:click={() => (isCashDrawerOpen = true)}
            class="px-2.5 py-1 bg-amber-600 hover:bg-amber-700 text-white font-black text-[11px] rounded-lg cursor-pointer"
          >
            {t('pos_open_session')}
          </button>
        </div>
      {/if}

      <!-- Cart Header -->
      <div class="p-3 border-b border-pos-border bg-slate-50 dark:bg-slate-800/40 space-y-2">
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <ShoppingBag class="w-4 h-4 text-sky-600" />
            <span class="font-extrabold text-xs text-pos-text">{t('pos_shopping_cart')}</span>
            <span class="text-[11px] font-bold bg-sky-100 dark:bg-sky-950 text-sky-700 dark:text-sky-300 px-2 py-0.2 rounded-full font-mono">
              {$cartItems.length} {t('pos_items')}
            </span>
          </div>
          {#if $cartItems.length > 0}
            <button
              on:click={clearCart}
              class="text-[11px] text-rose-500 hover:text-rose-700 font-bold cursor-pointer"
            >
              {t('btn_delete_cart')}
            </button>
          {/if}
        </div>

        <!-- Customer Selector (defaults to Walk-in / Client Comptoir).
             In purchase mode the supplier selector replaces it. -->
        {#if $posMode !== 'purchase'}
        <div class="relative" class:z-30={isCustomerSelectorOpen} data-customer-selector>
          <button
            type="button"
            on:click={() => {
              if (!isCustomerSelectorOpen) {
                isQuickAddCustomerOpen = false;
                quickAddName = '';
                quickAddPhone = '';
              }
              isCustomerSelectorOpen = !isCustomerSelectorOpen;
            }}
            class="w-full flex items-center justify-between gap-2 px-3 py-2 bg-white dark:bg-slate-900 border border-pos-border rounded-xl text-xs font-bold cursor-pointer hover:border-sky-400 transition {isCustomerSelectorOpen ? 'ring-2 ring-sky-400 border-sky-400' : ''}"
            title={t('pos_select_customer')}
          >
            <span class="flex items-center gap-2 min-w-0">
              <UserRound class="w-3.5 h-3.5 {$selectedCustomer?.id === DEFAULT_WALKIN_CUSTOMER_ID ? 'text-pos-muted' : 'text-sky-600'} shrink-0" />
              <span class="truncate text-pos-text">{$selectedCustomer?.name || t('pos_walkin')}</span>
              {#if $selectedCustomer && $selectedCustomer.balance > 0}
                <span class="text-[9px] font-mono font-black text-rose-600 bg-rose-50 dark:bg-rose-950/50 px-1.5 py-0.5 rounded-full shrink-0">
                  {$selectedCustomer.balance.toLocaleString()} DZD {t('pos_debt')}
                </span>
              {/if}
            </span>
            <ChevronDown class="w-3.5 h-3.5 text-pos-muted shrink-0 transition-transform {isCustomerSelectorOpen ? 'rotate-180' : ''}" />
          </button>

          {#if isCustomerSelectorOpen}
            <div class="absolute left-0 right-0 top-full mt-1 bg-pos-card border border-pos-border rounded-xl shadow-2xl overflow-hidden z-40 animate-in fade-in duration-100">
              <div class="p-1.5 pb-0">
                <input
                  type="text"
                  bind:value={customerSearchInCart}
                  placeholder="Search… / بحث…"
                  class="w-full px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-[11px] font-bold text-pos-text outline-none"
                />
              </div>
              <div class="max-h-56 overflow-y-auto p-1.5 space-y-1">
                {#each filteredCartCustomers as c}
                  <button
                    type="button"
                    on:click={() => { $selectedCustomerId = c.id; isCustomerSelectorOpen = false; }}
                    class="w-full flex items-center justify-between gap-2 px-2.5 py-2 rounded-lg text-start transition cursor-pointer {$selectedCustomerId === c.id ? 'bg-sky-100 dark:bg-sky-950 text-sky-700 dark:text-sky-300' : 'hover:bg-slate-100 dark:hover:bg-slate-800 text-pos-text'}"
                  >
                    <span class="min-w-0">
                      <span class="text-[11px] font-black block truncate">{c.name}</span>
                      <span class="text-[9px] text-pos-muted block truncate">{c.phone || t('pos_no_phone')}</span>
                    </span>
                    {#if c.balance > 0}
                      <span class="text-[9px] font-mono font-black text-rose-600 shrink-0">{c.balance.toLocaleString()}</span>
                    {/if}
                  </button>
                {/each}
                <!-- Quick Add Customer -->
                <div class="border-t border-pos-border pt-1.5 mt-1 space-y-1">
                  {#if isQuickAddCustomerOpen}
                    <input
                      type="text"
                      bind:value={quickAddName}
                      placeholder={t('customer_name') || 'Name'}
                      class="w-full px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-[11px] font-bold text-pos-text outline-none"
                    />
                    <input
                      type="text"
                      bind:value={quickAddPhone}
                      placeholder="Phone / هاتف"
                      class="w-full px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-[11px] font-bold text-pos-text outline-none"
                    />
                    <button
                      type="button"
                      on:click={handleQuickAddCustomer}
                      disabled={!quickAddName.trim() || isQuickSaving}
                      class="w-full py-1.5 bg-emerald-600 hover:bg-emerald-700 disabled:opacity-40 text-white text-[10px] font-black rounded-lg cursor-pointer"
                    >
                      {t('cart_add_customer')}
                    </button>
                  {:else}
                    <button
                      type="button"
                      on:click={() => (isQuickAddCustomerOpen = true)}
                      class="w-full flex items-center justify-center gap-1 py-1.5 bg-slate-100 dark:bg-slate-800 hover:bg-emerald-50 dark:hover:bg-emerald-950/40 text-emerald-600 text-[10px] font-black rounded-lg cursor-pointer"
                    >
                      <Plus class="w-3 h-3" /> {t('cart_add_customer')}
                    </button>
                  {/if}
                </div>
              </div>
            </div>
          {/if}
        </div>
        {/if}
        <!-- Supplier Selector: purchase mode picks the source supplier -->
        {#if $posMode === 'purchase'}
          <div class="relative" class:z-30={isSupplierSelectorOpen} data-supplier-selector>
            <button
              type="button"
              on:click={() => {
                if (!isSupplierSelectorOpen) {
                  isQuickAddSupplierOpen = false;
                  quickSupplierName = '';
                  quickSupplierPhone = '';
                }
                isSupplierSelectorOpen = !isSupplierSelectorOpen;
              }}
              class="w-full flex items-center justify-between gap-2 px-3 py-2 bg-white dark:bg-slate-900 border border-sky-400 rounded-xl text-xs font-bold cursor-pointer transition {isSupplierSelectorOpen ? 'ring-2 ring-sky-400' : ''}"
              title={t('pos_select_supplier')}
            >
              <span class="flex items-center gap-2 min-w-0">
                <Truck class="w-3.5 h-3.5 text-sky-600 shrink-0" />
                <span class="truncate text-pos-text">{$selectedSupplier?.name || t('pos_select_supplier')}</span>
                {#if $selectedSupplier && $selectedSupplier.balance > 0}
                  <span class="text-[9px] font-mono font-black text-amber-600 bg-amber-50 dark:bg-amber-950/50 px-1.5 py-0.5 rounded-full shrink-0">
                    {$selectedSupplier.balance.toLocaleString()} DZD due
                  </span>
                {/if}
              </span>
              <ChevronDown class="w-3.5 h-3.5 text-pos-muted shrink-0 transition-transform {isSupplierSelectorOpen ? 'rotate-180' : ''}" />
            </button>

            {#if isSupplierSelectorOpen}
              <div class="absolute left-0 right-0 top-full mt-1 bg-pos-card border border-pos-border rounded-xl shadow-2xl overflow-hidden z-40 animate-in fade-in duration-100">
                <div class="p-1.5 pb-0">
                  <input
                    type="text"
                    bind:value={supplierSearchInCart}
                    placeholder="Search… / بحث…"
                    class="w-full px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-[11px] font-bold text-pos-text outline-none"
                  />
                </div>
                <div class="max-h-56 overflow-y-auto p-1.5 space-y-1">
                  {#each filteredCartSuppliers as s}
                    <button
                      type="button"
                      on:click={() => { $selectedSupplierId = s.id; isSupplierSelectorOpen = false; }}
                      class="w-full flex items-center justify-between gap-2 px-2.5 py-2 rounded-lg text-start transition cursor-pointer {$selectedSupplierId === s.id ? 'bg-sky-100 dark:bg-sky-950 text-sky-700 dark:text-sky-300' : 'hover:bg-slate-100 dark:hover:bg-slate-800 text-pos-text'}"
                    >
                      <span class="min-w-0">
                        <span class="text-[11px] font-black block truncate">{s.name}</span>
                        <span class="text-[9px] text-pos-muted block truncate">{s.phone || 'No phone'}</span>
                      </span>
                      {#if s.balance > 0}
                        <span class="text-[9px] font-mono font-black text-amber-600 shrink-0">{s.balance.toLocaleString()}</span>
                      {/if}
                    </button>
                  {/each}
                  <!-- Quick Add Supplier -->
                  <div class="border-t border-pos-border pt-1.5 mt-1 space-y-1">
                    {#if isQuickAddSupplierOpen}
                      <input
                        type="text"
                        bind:value={quickSupplierName}
                        placeholder="Supplier name / اسم المورد"
                        class="w-full px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-[11px] font-bold text-pos-text outline-none"
                      />
                      <input
                        type="text"
                        bind:value={quickSupplierPhone}
                        placeholder="Phone / هاتف"
                        class="w-full px-2.5 py-1.5 bg-slate-100 dark:bg-slate-800 border-0 rounded-lg text-[11px] font-bold text-pos-text outline-none"
                      />
                      <button
                        type="button"
                        on:click={handleQuickAddSupplier}
                        disabled={!quickSupplierName.trim() || isQuickSaving}
                        class="w-full py-1.5 bg-emerald-600 hover:bg-emerald-700 disabled:opacity-40 text-white text-[10px] font-black rounded-lg cursor-pointer"
                      >
                        {t('cart_add_supplier')}
                      </button>
                    {:else}
                      <button
                        type="button"
                        on:click={() => (isQuickAddSupplierOpen = true)}
                        class="w-full flex items-center justify-center gap-1 py-1.5 bg-slate-100 dark:bg-slate-800 hover:bg-emerald-50 dark:hover:bg-emerald-950/40 text-emerald-600 text-[10px] font-black rounded-lg cursor-pointer"
                      >
                        <Plus class="w-3 h-3" /> {t('cart_add_supplier')}
                      </button>
                    {/if}
                  </div>
                </div>
              </div>
            {/if}
          </div>
        {/if}
      </div>
      <div bind:this={cartContainerEl} class="flex-1 overflow-y-auto p-2.5 space-y-2">
        {#if $cartItems.length === 0}
          <div class="h-full flex flex-col items-center justify-center text-pos-muted p-4 text-center">
            <ShoppingBag class="w-10 h-10 stroke-1 mb-2 opacity-20" />
            <p class="text-xs font-medium">{t('cart_empty')}</p>
          </div>
        {:else}
          {#each $cartItems as item (item.product_id + (item.is_refund ? '_ref' : ''))}
            <CartItemCard
              {item}
              baseSalePrice={products.find((pp) => pp.id === item.product_id)?.sale_price ?? 0}
              purchaseCost={item.purchase_price ?? products.find((pp) => pp.id === item.product_id)?.purchase_price ?? null}
              onEdit={() => {
                const live = products.find((pp) => pp.id === item.product_id);
                if (live) handleOpenEdit(live);
              }}
            />
          {/each}
        {/if}
      </div>

      <!-- Totals & Checkout Footer -->
      <div class="p-3.5 border-t border-pos-border bg-slate-50 dark:bg-slate-800/40 space-y-3">
        {#if $globalDiscountAmount > 0}
          <!-- Applied Remise row, above the total -->
          <div class="flex items-center justify-between p-2.5 bg-purple-50/70 dark:bg-purple-950/30 rounded-xl border border-purple-200/70 dark:border-purple-800/60">
            <div class="flex items-center gap-1.5">
              <Percent class="w-3.5 h-3.5 text-purple-600" />
              <span class="text-[10px] font-black text-pos-muted uppercase tracking-wider">
                {t('pos_remise_applied')}{$globalDiscountMode === 'percent' ? ` (${$globalDiscountValue}%)` : ''}
              </span>
            </div>
            <div class="text-end">
              <span class="text-base font-black font-mono text-purple-600 dark:text-purple-400">
                -{$globalDiscountAmount.toLocaleString()} <span class="text-[10px] font-bold">DZD</span>
              </span>
              <span class="text-[10px] font-bold text-pos-muted block">
                {t('subtotal')}: {$cartSubtotal.toLocaleString()} DZD
              </span>
            </div>
          </div>
        {/if}

        <div class="p-2.5 bg-sky-50/50 dark:bg-sky-950/30 rounded-xl border border-sky-200/60 dark:border-sky-800/60 space-y-1">
          <div class="flex items-center justify-between">
            <div>
              <span class="text-[10px] font-black text-pos-muted uppercase tracking-wider block">{t('total_payable')}</span>
              <span class="text-[11px] font-bold text-sky-600">
                {$cartItems.length} {t('pos_lines')} · {totalUnits} {t('pos_units')}
              </span>
            </div>
            <div class="flex items-center gap-2">
              <button
                type="button"
                on:click={() => (showCartCost = !showCartCost)}
                class="p-1.5 rounded-lg bg-slate-200/70 dark:bg-slate-800 hover:bg-slate-300/70 dark:hover:bg-slate-700 text-pos-muted hover:text-pos-text transition cursor-pointer"
                title={t('cart_show_cost')}
              >
                {#if showCartCost}<Eye class="w-3.5 h-3.5" />{:else}<EyeOff class="w-3.5 h-3.5" />{/if}
              </button>
              <span class="text-3xl lg:text-4xl font-black font-mono tracking-tight transition-all duration-200 hover:scale-105 {$cartGrandTotal < 0 ? 'text-amber-600' : 'text-sky-600 dark:text-sky-400'}">
                {$cartGrandTotal.toLocaleString()} <span class="text-sm font-bold">DZD</span>
              </span>
            </div>
          </div>
          {#if showCartCost && $posMode === 'sale'}
            <!-- Small digits: what the cart costs the shop -->
            <p class="text-[10px] font-bold text-pos-muted text-end">
              {t('pem_purchase_cost')}: <span class="font-mono text-emerald-600 dark:text-emerald-400">{cartCost.toLocaleString()} DZD</span>
              {#if cartCost > 0 && $cartGrandTotal > 0}
                • +{($cartGrandTotal - cartCost).toLocaleString()} DZD
              {/if}
            </p>
          {/if}
          {#if $posMode === 'purchase' && estSaleTotal > 0}
            <!-- Purchase mode: estimated sale value of this buy -->
            <p class="text-[10px] font-bold text-pos-muted text-end flex items-center justify-end gap-1">
              <TrendingUp class="w-3 h-3 text-emerald-500" />
              {t('est_sale_price')}: <span class="font-mono text-emerald-600 dark:text-emerald-400">{estSaleTotal.toLocaleString()} DZD</span>
              {#if estProfit > 0}
                <span class="font-mono text-sky-600 dark:text-sky-400">(+{estProfit.toLocaleString()})</span>
              {/if}
            </p>
          {/if}
        </div>

        <button
          type="button"
          on:click={handleFastCheckout}
          disabled={$cartItems.length === 0}
          class="w-full py-3.5 px-4 bg-emerald-600 hover:bg-emerald-700 disabled:opacity-40 text-white font-black text-sm rounded-xl transition shadow-md flex items-center justify-center gap-2 cursor-pointer active:scale-98"
        >
          <span>{t('pos_checkout_cash')} [{selectedPaymentMode.toUpperCase()}]</span>
          <ArrowRight class="w-4 h-4" />
        </button>
      </div>
    </div>
  </div>

  <!-- Modals -->
  <PaymentModal
    isOpen={isPaymentOpen}
    onClose={() => (isPaymentOpen = false)}
    onSaleSuccess={(num) => { lastSaleSuccessNumber = num; loadProducts(); }}
  />

  <CashDrawerModal
    isOpen={isCashDrawerOpen}
    onClose={() => (isCashDrawerOpen = false)}
  />

  <CategoryManagerModal
    isOpen={isCategoryManagerOpen}
    onClose={() => (isCategoryManagerOpen = false)}
    onCategoryChanged={() => { loadCategories(); loadProducts(); }}
  />

  <HeldSalesModal
    isOpen={isHeldSalesOpen}
    onClose={() => (isHeldSalesOpen = false)}
  />

  <RemiseModal
    isOpen={isRemiseOpen}
    onClose={() => (isRemiseOpen = false)}
  />

  <PrintReceiptModal
    isOpen={isPrintReceiptOpen}
    onClose={() => (isPrintReceiptOpen = false)}
  />

  {#key productModalKey}
  <ProductEditModal
    isOpen={isProductEditOpen}
    product={editingProduct}
    categories={categories}
    units={units}
    initialBarcode={initialBarcodeForNewProduct}
    extraBarcode={editingProductWithExtraBarcode}
    onClose={() => {
      isProductEditOpen = false;
      // Cancelling the editor never adds to the cart; also drop any
      // half-typed search so the next scan starts clean.
      if (!initialBarcodeForNewProduct && !editingProductWithExtraBarcode && searchQuery.trim()) {
        searchQuery = '';
        loadProducts();
      }
    }}
    onSaved={handleProductSaved}
  />
  {/key}

  <UnknownBarcodeModal
    isOpen={isUnknownBarcodeModalOpen}
    barcode={unknownScannedBarcode}
    onClose={() => { isUnknownBarcodeModalOpen = false; searchQuery = ''; }}
    onAddNewWithBarcode={(bc) => {
      editingProduct = null;
      initialBarcodeForNewProduct = bc;
      editingProductWithExtraBarcode = '';
      isProductEditOpen = true;
    }}
    onEditProductWithBarcode={handleEditProductWithBarcode}
    onLinkedToProduct={(p) => {
      addToCart(p, 1, $isRefundMode);
      loadProducts();
    }}
  />

  <QuickPurchaseModal
    isOpen={isQuickPurchaseOpen}
    suppliers={$suppliers}
    onClose={() => (isQuickPurchaseOpen = false)}
    onPurchaseCompleted={loadProducts}
  />

  <ReturnDamagedModal
    isOpen={isReturnDamagedOpen}
    suppliers={$suppliers}
    onClose={() => (isReturnDamagedOpen = false)}
    onReturnCompleted={loadProducts}
  />

  <CreditCustomerModal
    isOpen={isCreditCustomerOpen}
    totalAmount={$cartGrandTotal}
    onClose={() => (isCreditCustomerOpen = false)}
    onConfirmCredit={(cId, cName, paid, remaining) => {
      $selectedCustomerId = cId;
      executeCheckout(cId, cName, { mode: 'credit', paidAmount: paid, changeAmount: 0, });
    }}
  />

  <VersementCustomerModal
    isOpen={isVersementOpen}
    totalAmount={$cartGrandTotal}
    onClose={() => (isVersementOpen = false)}
    onConfirmVersement={(cId, cName, paid, remaining) => {
      // Versement reserves the cart for the customer: their selection follows
      // the sale so the remainder is tracked on their account.
      $selectedCustomerId = cId;
      executeCheckout(cId, cName, { mode: 'versement', paidAmount: paid, changeAmount: 0 });
    }}
  />

  <CheckoutModal
    isOpen={isCheckoutOpen}
    totalAmount={$cartGrandTotal}
    onClose={() => (isCheckoutOpen = false)}
    onConfirmCheckout={handleCheckoutConfirm}
  />

  <OtherArticleModal
    isOpen={isOtherArticleOpen}
    onClose={() => (isOtherArticleOpen = false)}
  />

  <PurchasePriceModal
    isOpen={purchasePriceTarget !== null}
    product={purchasePriceTarget}
    onClose={() => (purchasePriceTarget = null)}
    onConfirm={handlePurchasePriceConfirm}
  />

  <LastReceiptModal
    isOpen={isLastReceiptOpen}
    onClose={() => (isLastReceiptOpen = false)}
    onEditInPos={handleEditLastSaleInPos}
  />
</div>