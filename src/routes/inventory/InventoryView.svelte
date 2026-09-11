<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { decomposeStock } from '../../lib/utils/stockUnits';
  import { normalizeBarcode } from '../../lib/utils/barcode';
  import type { Category, Product, Unit } from '../../lib/types';
  import { sortRows, clickSort } from '../../lib/utils/tableSort';
  import { localTodayISO } from '../../lib/utils/date';
  import { invalidations } from '../../lib/stores/invalidations';
  import ProductEditModal from '../../lib/components/ProductEditModal.svelte';
  import PrintLabelModal from '../../lib/components/PrintLabelModal.svelte';
  import BatchLabelPrintModal from '../../lib/components/BatchLabelPrintModal.svelte';
  import UniversalSearchBar from '../../lib/components/UniversalSearchBar.svelte';
  import {
  Printer,
    Package, Plus, Edit2, Trash2, QrCode,
    ArrowUpDown, AlertTriangle, Tag, LayoutGrid,
    List, DollarSign, TrendingUp, Boxes, Check, X,
    Pin, PinOff
  } from 'lucide-svelte';

  let products: Product[] = [];
  let isBatchLabelOpen = false;
  let batchLabelSettings: Record<string, string> = {};

  async function openBatchLabels() {
    try {
      batchLabelSettings = await invoke<Record<string, string>>('get_all_settings');
    } catch {
      batchLabelSettings = {};
    }
    isBatchLabelOpen = true;
  }
  // Three-state column sort: asc -> desc -> default (loadProducts order).
  let sortKey: string | null = null;
  let sortDir: 'asc' | 'desc' | null = null;
  function applySort(key: string) {
    const next = clickSort(key, sortKey, sortDir);
    sortKey = next.key;
    sortDir = next.dir;
  }
  function sortIndicator(key: string): string {
    if (sortKey !== key || !sortDir) return '';
    return sortDir === 'asc' ? '▲' : '▼';
  }
  $: sortedProducts = sortRows(products, sortKey, sortDir, products);

  // Pin/unpin a product: pinned products float to the top (same system as
  // the POS grid — shared toggle_product_pin backend).
  async function toggleProductPin(p: Product) {
    try {
      await invoke('toggle_product_pin', { productId: p.id, pinned: !p.pinned });
      await loadProducts();
    } catch (e) {
      console.warn('Pin failed:', e);
    }
  }
  let categories: Category[] = [];
  let units: Unit[] = [];

  let searchQuery = '';

  // Live cross-PC: stock edited on another terminal appears here instantly.
  $: if ($invalidations.products) {
    void loadProducts();
  }
  // AZERTY-normalized mirror of the search box (scanners may emit & é " ...).
  $: searchQueryN = normalizeBarcode(searchQuery).toLowerCase();
  let searchType: 'all' | 'name' | 'barcode' | 'price' | 'qr' = 'all';
  let selectedCategory: number | null = null;
  let sortBy: string = 'default';
  let viewMode: 'line' | 'card' = 'line';

  let isProductEditOpen = false;
  let editingProduct: Product | null = null;
  let initialBarcodeForNewProduct = '';

  // Print Label Modal
  let isPrintLabelOpen = false;
  let printLabelProduct: Product | null = null;
  let printLabelInitialType: 'barcode' | 'etiquette' = 'barcode';
  let printLabelInitialQty = 1;

  // Custom Delete Confirmation Modal
  let isDeleteModalOpen = false;
  let productToDelete: Product | null = null;
  let deleteConfirmText = '';
  let isDeleting = false;

  onMount(async () => {
    await loadCategories();
    await loadUnits();
    await loadProducts();
    await loadAllPacks();
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
      const catId = (selectedCategory !== null && selectedCategory !== undefined && String(selectedCategory) !== 'null' && String(selectedCategory) !== '')
        ? Number(selectedCategory)
        : null;

      const list = await invoke<Product[]>('search_products', {
        query: searchQueryN || '',
        categoryId: (catId && !isNaN(catId)) ? catId : null,
        searchType: searchType === 'qr' ? 'barcode' : (searchType || 'all'),
      });

      const today = localTodayISO();

      if (sortBy === 'name_asc') {
        list.sort((a, b) => (a.name_fr || a.name_ar).localeCompare(b.name_fr || b.name_ar));
      } else if (sortBy === 'name_desc') {
        list.sort((a, b) => (b.name_fr || b.name_ar).localeCompare(a.name_fr || a.name_ar));
      } else if (sortBy === 'price_asc') {
        list.sort((a, b) => a.sale_price - b.sale_price);
      } else if (sortBy === 'price_desc') {
        list.sort((a, b) => b.sale_price - a.sale_price);
      } else if (sortBy === 'stock_high') {
        list.sort((a, b) => b.current_stock - a.current_stock);
      } else if (sortBy === 'stock_low') {
        list.sort((a, b) => a.current_stock - b.current_stock);
      } else if (sortBy === 'newest') {
        list.sort((a, b) => b.id - a.id);
      } else if (sortBy === 'oldest') {
        list.sort((a, b) => a.id - b.id);
      } else if (sortBy === 'margin_high') {
        list.sort((a, b) => {
          const marginA = a.sale_price > 0 ? ((a.sale_price - a.purchase_price) / a.sale_price) : 0;
          const marginB = b.sale_price > 0 ? ((b.sale_price - b.purchase_price) / b.sale_price) : 0;
          return marginB - marginA;
        });
      } else if (sortBy === 'near_expiry') {
        list.sort((a, b) => {
          const aNear = isProductNearExpiry(a.expiry_date) ? 1 : 0;
          const bNear = isProductNearExpiry(b.expiry_date) ? 1 : 0;
          return bNear - aNear;
        });
      } else if (sortBy === 'expired') {
        list.sort((a, b) => {
          const aExp = a.expiry_date && a.expiry_date < today ? 1 : 0;
          const bExp = b.expiry_date && b.expiry_date < today ? 1 : 0;
          return bExp - aExp;
        });
      }

      products = list;
    } catch (e) {
      console.error(e);
    }
  }

  // Real Database Statistics Computed from Product List
  $: totalProductsCount = products.length;
  let allPacks: Array<{ product_id: number; name: string; units_per_package: number }> = [];

  $: stockLabelById = (() => {
    const map: Record<number, string> = {};
    for (const pk of allPacks) {
      (map[pk.product_id] ??= []).push(pk);
    }
    return map;
  })();

  function stockLabel(productId: number, stock: number): string | null {
    const packs = stockLabelById[productId];
    if (!packs || packs.length === 0) return null;
    return decomposeStock(stock, packs);
  }

  async function loadAllPacks() {
    try {
      allPacks = await invoke<any[]>('list_all_packagings');
    } catch {
      allPacks = [];
    }
  }

  $: totalStockQuantity = products.reduce((sum, p) => sum + (p.current_stock || 0), 0);
  $: positiveStockUnits = products.reduce((sum, p) => sum + (p.current_stock > 0 ? p.current_stock : 0), 0);
  $: negativeStockUnits = products.reduce((sum, p) => sum + (p.current_stock < 0 ? Math.abs(p.current_stock) : 0), 0);
  $: totalStockValueCost = products.reduce((sum, p) => sum + (p.current_stock > 0 ? (p.current_stock * p.purchase_price) : 0), 0);
  $: totalPotentialProfit = products.reduce((sum, p) => sum + (p.current_stock > 0 ? (p.current_stock * (p.sale_price - p.purchase_price)) : 0), 0);

  function isProductExpired(expiryDate?: string): boolean {
    if (!expiryDate) return false;
    const today = localTodayISO();
    return expiryDate < today;
  }

  function isProductNearExpiry(expiryDate?: string): boolean {
    if (!expiryDate) return false;
    const today = new Date();
    const exp = new Date(expiryDate);
    const diffDays = Math.ceil((exp.getTime() - today.getTime()) / (1000 * 60 * 60 * 24));
    return diffDays >= 0 && diffDays <= 30;
  }

  function openAddNew() {
    editingProduct = null;
    initialBarcodeForNewProduct = '';
    isProductEditOpen = true;
  }

  function openEdit(p: Product) {
    editingProduct = p;
    initialBarcodeForNewProduct = '';
    isProductEditOpen = true;
  }

  function promptDelete(p: Product, e?: Event) {
    if (e) e.stopPropagation();
    productToDelete = p;
    deleteConfirmText = '';
    isDeleteModalOpen = true;
  }

  async function confirmDelete() {
    if (!productToDelete) return;
    try {
      isDeleting = true;
      await invoke('delete_product', { productId: productToDelete.id });
      isDeleteModalOpen = false;
      productToDelete = null;
      await loadProducts();
    } catch (e: any) {
      alert('Failed to delete product: ' + (e.message || e));
    } finally {
      isDeleting = false;
    }
  }

  function openPrintSticker(p: Product, e?: Event) {
    if (e) e.stopPropagation();
    printLabelProduct = p;
    printLabelInitialType = 'barcode';
    printLabelInitialQty = p.current_stock > 0 ? p.current_stock : 1;
    isPrintLabelOpen = true;
  }

  function openPrintShelf(p: Product, e?: Event) {
    if (e) e.stopPropagation();
    printLabelProduct = p;
    printLabelInitialType = 'etiquette';
    printLabelInitialQty = 1;
    isPrintLabelOpen = true;
  }
</script>

<div class="h-full flex flex-col bg-pos-bg p-4 overflow-hidden select-none space-y-3">
  <!-- Top Header -->
  <div class="flex items-center justify-between pb-2 border-b border-pos-border shrink-0">
    <div class="flex items-center gap-3">
      <div class="w-10 h-10 rounded-2xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <Package class="w-5 h-5" />
      </div>
      <div>
        <h1 class="text-xl font-black text-pos-text tracking-tight">Products & Stock Catalog / إدارة المنتجات والمخزون</h1>
        <p class="text-xs text-pos-muted">Multi-barcode catalog, real-time inventory valuations, and scalable balance sync</p>
      </div>
    </div>

    <div class="flex items-center gap-2">
      <!-- Line vs Card View Mode Toggle -->
      <div class="flex items-center bg-pos-card border border-pos-border rounded-xl p-1 shadow-2xs">
        <button
          type="button"
          on:click={() => (viewMode = 'line')}
          class="p-1.5 rounded-lg text-xs font-bold transition {viewMode === 'line' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
          title="Line / Table View"
        >
          <List class="w-4 h-4" />
        </button>
        <button
          type="button"
          on:click={() => (viewMode = 'card')}
          class="p-1.5 rounded-lg text-xs font-bold transition {viewMode === 'card' ? 'bg-sky-600 text-white shadow-xs' : 'text-pos-muted hover:text-pos-text'}"
          title="Card / Grid View"
        >
          <LayoutGrid class="w-4 h-4" />
        </button>
      </div>

      <button
        type="button"
        on:click={openBatchLabels}
        class="px-4 py-2.5 bg-amber-600 hover:bg-amber-700 text-white text-xs font-black rounded-xl flex items-center gap-1.5 cursor-pointer shadow-xs transition active:scale-95"
      >
        <Printer class="w-4 h-4" />
        <span>Batch Labels (طباعة ملصقات متعددة)</span>
      </button>
<button
        type="button"
        on:click={openAddNew}
        class="px-4 py-2 bg-sky-600 hover:bg-sky-700 text-white rounded-xl text-xs font-black transition shadow-xs flex items-center gap-2 cursor-pointer active:scale-95"
      >
        <Plus class="w-4 h-4" />
        <span>Add Product (إضافة منتج جديد)</span>
      </button>
    </div>
  </div>

  <!-- Real Statistics Cards Bar -->
  <div class="grid grid-cols-2 md:grid-cols-5 gap-3 shrink-0">
    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-sky-50 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <Package class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">Total Products</p>
        <p class="text-base font-black font-mono text-pos-text">{totalProductsCount.toLocaleString()}</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-emerald-50 dark:bg-emerald-950 text-emerald-600 flex items-center justify-center font-bold">
        <Boxes class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-emerald-600 dark:text-emerald-400 uppercase">Positive Stock (+)</p>
        <p class="text-base font-black font-mono text-emerald-600">{positiveStockUnits.toLocaleString()} pcs</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3 {negativeStockUnits > 0 ? 'border-rose-300 dark:border-rose-900 bg-rose-50/20' : ''}">
      <div class="w-9 h-9 rounded-xl {negativeStockUnits > 0 ? 'bg-rose-100 dark:bg-rose-950 text-rose-600' : 'bg-slate-100 dark:bg-slate-800 text-pos-muted'} flex items-center justify-center font-bold">
        <AlertTriangle class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold {negativeStockUnits > 0 ? 'text-rose-600 dark:text-rose-400' : 'text-pos-muted'} uppercase">Negative Stock (-)</p>
        <p class="text-base font-black font-mono {negativeStockUnits > 0 ? 'text-rose-600' : 'text-pos-muted'}">{negativeStockUnits > 0 ? `-${negativeStockUnits.toLocaleString()}` : '0'} pcs</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-sky-50 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
        <DollarSign class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">Stock Cost Value</p>
        <p class="text-base font-black font-mono text-sky-600">{totalStockValueCost.toLocaleString()} DZD</p>
      </div>
    </div>

    <div class="bg-pos-card border border-pos-border p-3 rounded-2xl shadow-xs flex items-center gap-3">
      <div class="w-9 h-9 rounded-xl bg-amber-50 dark:bg-amber-950 text-amber-600 flex items-center justify-center font-bold">
        <TrendingUp class="w-4 h-4" />
      </div>
      <div>
        <p class="text-[10px] font-bold text-pos-muted uppercase">Potential Profit</p>
        <p class="text-base font-black font-mono text-amber-600">+{totalPotentialProfit.toLocaleString()} DZD</p>
      </div>
    </div>
  </div>

  <!-- Filter & Sort Row -->
  <div class="flex items-center gap-2 shrink-0">
    <div class="flex-1">
      <UniversalSearchBar
        bind:query={searchQuery}
        bind:searchType
        onSearch={loadProducts}
      />
    </div>

    <!-- Category selector with Fix -->
    <div class="bg-pos-card border border-pos-border rounded-xl px-3 py-1.5 text-xs font-bold text-pos-text">
      <select
        bind:value={selectedCategory}
        on:change={loadProducts}
        class="bg-transparent outline-none cursor-pointer"
      >
        <option value="">All Families (كل الفئات)</option>
        {#each categories as cat}
          <option value={cat.id}>{cat.name_fr} / {cat.name_ar}</option>
        {/each}
      </select>
    </div>

    <!-- Sort selector -->
    <div class="flex items-center gap-1 bg-pos-card border border-pos-border rounded-xl px-3 py-1.5 text-xs font-bold text-pos-text">
      <ArrowUpDown class="w-3.5 h-3.5 text-pos-muted" />
      <select bind:value={sortBy} on:change={loadProducts} class="bg-transparent outline-none cursor-pointer">
        <option value="default">Default Sort</option>
        <option value="newest">Newest Added (الأحدث إضافة)</option>
        <option value="oldest">Oldest Added (الأقدم)</option>
        <option value="name_asc">Name (A-Z)</option>
        <option value="name_desc">Name (Z-A)</option>
        <option value="price_asc">Price (Low → High)</option>
        <option value="price_desc">Price (High → Low)</option>
        <option value="margin_high">Highest Margin % (الأعلى هامش ربح)</option>
        <option value="stock_high">Highest Stock (الأكثر مخزوناً)</option>
        <option value="stock_low">Lowest Stock (الأقل مخزوناً)</option>
        <option value="near_expiry">Near Expiry &lt; 30 Days (قريبة الانتهاء)</option>
        <option value="expired">Expired First (المنتهية أولاً)</option>
      </select>
    </div>
  </div>

  <!-- Content Area: Line View or Card View -->
  {#if viewMode === 'line'}
    <div class="flex-1 overflow-y-auto bg-pos-card border border-pos-border rounded-2xl shadow-xs">
      <table class="w-full text-start text-xs border-collapse">
        <thead class="bg-slate-50 dark:bg-slate-800/60 border-b border-pos-border text-pos-muted font-bold sticky top-0 z-10">
          <tr>
            <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('sku')}>SKU {sortIndicator('sku')}</th>
            <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('name_fr')}>Product Name {sortIndicator('name_fr')}</th>
            <th class="p-3 text-start cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('category_name')}>Family {sortIndicator('category_name')}</th>
            <th class="p-3 text-start">Barcodes</th>
            <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('purchase_price')}>Cost {sortIndicator('purchase_price')}</th>
            <th class="p-3 text-end cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('sale_price')}>Sale Price {sortIndicator('sale_price')}</th>
            <th class="p-3 text-center cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('current_stock')}>Stock {sortIndicator('current_stock')}</th>
            <th class="p-3 text-center cursor-pointer select-none hover:text-pos-text" on:click={() => applySort('expiry_date')}>Expiry {sortIndicator('expiry_date')}</th>
            <th class="p-3 text-end">Actions</th>
          </tr>
        </thead>
        <tbody class="divide-y divide-pos-border/40">
          {#if products.length === 0}
            <tr>
              <td colspan="9" class="p-8 text-center text-pos-muted">No products found matching filters.</td>
            </tr>
          {:else}
            {#each sortedProducts as p}
              {@const expired = isProductExpired(p.expiry_date)}
              {@const nearExp = isProductNearExpiry(p.expiry_date)}
              {@const isBeingDeleted = isDeleteModalOpen && productToDelete?.id === p.id}
              <tr
                on:click={() => openEdit(p)}
                class="transition cursor-pointer hover:bg-slate-50 dark:hover:bg-slate-800/40 {isBeingDeleted ? 'ring-2 ring-rose-500 bg-rose-50/80 dark:bg-rose-950/50 relative z-10' : expired ? 'bg-rose-50/60 dark:bg-rose-950/20 border-s-4 border-s-rose-500' : nearExp ? 'bg-amber-50/40 dark:bg-amber-950/10 border-s-4 border-s-amber-500' : ''}"
              >
                <td class="p-3 font-mono font-bold text-sky-600">{p.sku || '—'}</td>
                <td class="p-3 font-bold text-pos-text">
                  <div class="flex items-center gap-1.5">
                    {#if expired}
                      <span class="w-2 h-2 rounded-full bg-rose-600 animate-pulse"></span>
                    {/if}
                    <p class="truncate max-w-xs">{p.name_fr || p.name_ar}</p>
                  </div>
                  {#if p.name_ar && p.name_fr}
                    <p class="text-[10px] text-pos-muted truncate max-w-xs">{p.name_ar}</p>
                  {/if}
                </td>
                <td class="p-3 text-pos-muted">{p.category_name || 'Général'}</td>
                <td class="p-3">
                  {#if p.barcodes && p.barcodes.length > 0}
                    <span class="px-2 py-0.5 bg-slate-100 dark:bg-slate-800 font-mono text-[10px] rounded font-bold">{p.barcodes[0]}</span>
                    {#if p.barcodes.length > 1}
                      <span class="text-[10px] text-sky-600 font-bold ml-1">+{p.barcodes.length - 1}</span>
                    {/if}
                  {:else}
                    <span class="text-pos-muted font-mono">—</span>
                  {/if}
                </td>
                <td class="p-3 text-end font-mono font-bold text-pos-muted">{p.purchase_price.toLocaleString()} DZD</td>
                <td class="p-3 text-end font-mono font-black text-sky-600">{p.sale_price.toLocaleString()} DZD</td>
                <td class="p-3 text-center font-mono font-bold {p.current_stock <= p.min_stock ? 'text-rose-600' : 'text-pos-text'}">
                  {p.current_stock}
                  {#if stockLabel(p.id, p.current_stock)}
                    <p class="text-[9px] font-sans font-bold text-pos-muted">{stockLabel(p.id, p.current_stock)}</p>
                  {/if}
                </td>
                <td class="p-3 text-center font-mono text-[10px]">
                  {#if expired}
                    <span class="px-2 py-0.5 bg-rose-100 text-rose-800 dark:bg-rose-950 dark:text-rose-300 font-bold rounded-full">
                      Expired ({p.expiry_date})
                    </span>
                  {:else if nearExp}
                    <span class="px-2 py-0.5 bg-amber-100 text-amber-800 dark:bg-amber-950 dark:text-amber-300 font-bold rounded-full">
                      {p.expiry_date}
                    </span>
                  {:else}
                    <span class="text-pos-muted">{p.expiry_date || '—'}</span>
                  {/if}
                </td>
                <td class="p-3 text-end">
                  <div class="flex items-center justify-end gap-1">
                    <button
                      type="button"
                      on:click={(e) => openPrintSticker(p, e)}
                      class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer transition"
                      title="Print Product Sticker (ملصق باركود)"
                    >
                      <QrCode class="w-4 h-4" />
                    </button>
                    <button
                      type="button"
                      on:click={(e) => openPrintShelf(p, e)}
                      class="p-1.5 text-pos-muted hover:text-emerald-600 rounded-lg cursor-pointer transition"
                      title="Print Shelf Tag (بطاقة رف)"
                    >
                      <Tag class="w-4 h-4" />
                    </button>
                    <button
                      type="button"
                      on:click={(e) => { e.stopPropagation(); openEdit(p); }}
                      class="p-1.5 text-pos-muted hover:text-sky-600 rounded-lg cursor-pointer transition"
                      title="Edit Product"
                    >
                      <Edit2 class="w-4 h-4" />
                    </button>
                    <button
                      type="button"
                      on:click={(e) => promptDelete(p, e)}
                      class="p-1.5 text-pos-muted hover:text-rose-600 rounded-lg cursor-pointer transition"
                      title="Delete Product"
                    >
                      <X class="w-4 h-4" />
                    </button>
                  </div>
                </td>
              </tr>
            {/each}
          {/if}
        </tbody>
      </table>
    </div>
  {:else}
    <!-- Card / Grid View -->
    <div class="flex-1 overflow-y-auto p-2">
      {#if products.length === 0}
        <div class="p-12 text-center text-pos-muted bg-pos-card border border-pos-border rounded-2xl">
          <Package class="w-12 h-12 mx-auto text-pos-muted/40 mb-2" />
          <p class="font-bold text-sm">No products found</p>
          <p class="text-xs mt-1">Try adjusting your search query or family filter.</p>
        </div>
      {:else}
        <div class="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 lg:grid-cols-5 xl:grid-cols-6 gap-4 items-stretch pb-6">
          {#each sortedProducts as p}
            {@const expired = isProductExpired(p.expiry_date)}
            {@const nearExp = isProductNearExpiry(p.expiry_date)}
            {@const isBeingDeleted = isDeleteModalOpen && productToDelete?.id === p.id}
            <div
              class="bg-pos-card border rounded-2xl p-3.5 flex flex-col justify-between text-start transition relative overflow-hidden group shadow-xs hover:shadow-lg hover:border-sky-500 min-h-[260px] {isBeingDeleted ? 'ring-2 ring-rose-500 bg-rose-50 dark:bg-rose-950/40' : expired ? 'border-rose-500 bg-rose-50/40 dark:bg-rose-950/20' : nearExp ? 'border-amber-400 bg-amber-50/20 dark:bg-amber-950/10' : 'border-pos-border'}"
            >
              <!-- Top Badges & Actions -->
              <div class="flex items-center justify-between gap-1 mb-2.5">
                <span class="font-mono text-[11px] font-bold text-sky-600 bg-sky-50 dark:bg-sky-950 px-2 py-0.5 rounded-lg border border-sky-200 dark:border-sky-800 truncate max-w-[100px]">
                  {p.sku || '—'}
                </span>

                <div class="flex items-center gap-1">
                  {#if expired}
                    <span class="px-1.5 py-0.5 bg-rose-600 text-white text-[9px] font-black rounded-md uppercase animate-pulse">
                      Expired
                    </span>
                  {:else if nearExp}
                    <span class="px-1.5 py-0.5 bg-amber-500 text-white text-[9px] font-black rounded-md">
                      Near Exp
                    </span>
                  {/if}
                  <button
                    type="button"
                    on:click={() => openEdit(p)}
                    class="p-1.5 rounded-lg bg-slate-100 hover:bg-sky-100 dark:bg-slate-800 dark:hover:bg-sky-950 text-pos-muted hover:text-sky-600 transition cursor-pointer"
                    title="Edit Product"
                  >
                    <Edit2 class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    on:click={(e) => openPrintSticker(p, e)}
                    class="p-1.5 rounded-lg bg-slate-100 hover:bg-sky-100 dark:bg-slate-800 dark:hover:bg-sky-950 text-pos-muted hover:text-sky-600 transition cursor-pointer"
                    title="Print Product Sticker"
                  >
                    <QrCode class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    on:click={(e) => openPrintShelf(p, e)}
                    class="p-1.5 rounded-lg bg-slate-100 hover:bg-emerald-100 dark:bg-slate-800 dark:hover:bg-emerald-950 text-pos-muted hover:text-emerald-600 transition cursor-pointer"
                    title="Print Shelf Tag"
                  >
                    <Tag class="w-3.5 h-3.5" />
                  </button>
                  <button
                    type="button"
                    on:click={() => toggleProductPin(p)}
                    class="p-1.5 rounded-lg transition cursor-pointer {p.pinned ? 'bg-amber-500 text-white' : 'bg-slate-100 dark:bg-slate-800 text-pos-muted hover:text-amber-500'}"
                    title={p.pinned ? 'Unpin (إزالة التثبيت)' : 'Pin to top (تثبيت)'}
                  >
                    {#if p.pinned}
                      <PinOff class="w-3.5 h-3.5" />
                    {:else}
                      <Pin class="w-3.5 h-3.5" />
                    {/if}
                  </button>
                </div>
              </div>

              <!-- Product Image / Icon Box -->
              <div
                role="button"
                tabindex="0"
                on:click={() => openEdit(p)}
                on:keydown={(e) => { if (e.key === 'Enter') openEdit(p); }}
                class="w-full h-28 bg-slate-100 dark:bg-slate-800/80 rounded-xl mb-3 flex items-center justify-center overflow-hidden cursor-pointer group-hover:scale-[1.02] transition shrink-0"
              >
                {#if p.image_path}
                  <img src={p.image_path} alt={p.name_fr || p.name_ar} class="w-full h-full object-cover" />
                {:else}
                  <div class="w-12 h-12 rounded-xl bg-sky-50 dark:bg-sky-950/60 text-sky-600 dark:text-sky-400 flex items-center justify-center">
                    <Package class="w-7 h-7" />
                  </div>
                {/if}
              </div>

              <!-- Product Info -->
              <div
                role="button"
                tabindex="0"
                on:click={() => openEdit(p)}
                on:keydown={(e) => { if (e.key === 'Enter') openEdit(p); }}
                class="flex-1 cursor-pointer space-y-1 mb-3"
              >
                <h3 class="text-xs font-black text-pos-text line-clamp-2 leading-tight group-hover:text-sky-600 transition min-h-[32px]">
                  {p.name_fr || p.name_ar}
                </h3>
                {#if p.name_ar && p.name_fr}
                  <p class="text-[10px] text-pos-muted truncate font-medium">{p.name_ar}</p>
                {/if}
                <div class="flex items-center gap-1 text-[10px] text-pos-muted font-mono pt-1">
                  <Tag class="w-3 h-3 text-pos-muted/60 shrink-0" />
                  <span class="truncate">{p.barcodes?.[0] || 'No barcode'}</span>
                </div>
              </div>

              <!-- Price & Stock Footer -->
              <div class="pt-2.5 border-t border-pos-border flex items-center justify-between shrink-0">
                <div>
                  <p class="text-[9px] text-pos-muted uppercase font-bold">Prix Vente</p>
                  <p class="text-xs font-mono font-black text-sky-600 dark:text-sky-400">
                    {p.sale_price.toLocaleString()} DZD
                  </p>
                </div>
                <span class="text-[10px] font-black font-mono px-2 py-1 rounded-lg border {p.current_stock <= p.min_stock ? 'bg-rose-100 text-rose-800 dark:bg-rose-950 dark:text-rose-300 border-rose-300' : 'bg-emerald-50 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300 border-emerald-200'}">
                  Qté: {p.current_stock}
                  {#if stockLabel(p.id, p.current_stock)}
                    <p class="text-[9px] font-sans font-bold text-pos-muted">{stockLabel(p.id, p.current_stock)}</p>
                  {/if}
                </span>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}
</div>

<!-- Product Add / Edit Modal -->
<ProductEditModal
  isOpen={isProductEditOpen}
  product={editingProduct}
  categories={categories}
  units={units}
  initialBarcode={initialBarcodeForNewProduct}
  onClose={() => (isProductEditOpen = false)}
  onSaved={loadProducts}
/>

<!-- Print Label Modal -->
<PrintLabelModal
  isOpen={isPrintLabelOpen}
  product={printLabelProduct}
  initialType={printLabelInitialType}
  initialQty={printLabelInitialQty}
  onClose={() => (isPrintLabelOpen = false)}
/>

<!-- Batch Label Printing (scan many products, print once) -->
<BatchLabelPrintModal
  isOpen={isBatchLabelOpen}
  products={products}
  settings={batchLabelSettings}
  onClose={() => (isBatchLabelOpen = false)}
/>

<!-- Delete Confirmation Modal (Requires typing DELETE) -->
{#if isDeleteModalOpen && productToDelete}
  <div class="fixed inset-0 z-[9999] bg-black/70 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-pos-card border border-pos-border rounded-2xl shadow-2xl p-6 max-w-sm w-full space-y-4 animate-in zoom-in-95">
      <div class="flex items-center gap-3 text-rose-600">
        <AlertTriangle class="w-6 h-6 shrink-0" />
        <h3 class="font-black text-sm text-pos-text">Confirm Product Deletion / تأكيد حذف المنتج</h3>
      </div>
      <p class="text-xs text-pos-muted">
        Are you sure you want to permanently delete <strong class="text-pos-text">{productToDelete.name_fr || productToDelete.name_ar}</strong>? This action cannot be undone.
      </p>

      <div class="space-y-1">
        <label class="block text-xs font-bold text-pos-muted">
          Type <span class="text-rose-600 font-mono font-black">DELETE</span> to confirm:
        </label>
        <input
          type="text"
          bind:value={deleteConfirmText}
          placeholder="DELETE"
          class="w-full px-3 py-2 bg-slate-100 dark:bg-slate-800 border border-pos-border rounded-xl text-xs font-mono font-black text-rose-600 outline-none focus:ring-2 focus:ring-rose-500"
        />
      </div>

      <div class="flex justify-end gap-2 pt-2 border-t border-pos-border">
        <button on:click={() => (isDeleteModalOpen = false)} class="px-4 py-2 bg-slate-200 dark:bg-slate-700 text-xs font-bold rounded-xl cursor-pointer">
          Cancel / إلغاء
        </button>
        <button
          on:click={confirmDelete}
          disabled={isDeleting || (deleteConfirmText.trim().toUpperCase() !== 'DELETE' && deleteConfirmText.trim() !== 'حذف')}
          class="px-4 py-2 bg-rose-600 hover:bg-rose-700 disabled:opacity-40 text-white text-xs font-black rounded-xl cursor-pointer shadow-md transition"
        >
          {isDeleting ? 'Deleting...' : 'Delete Product / حذف'}
        </button>
      </div>
    </div>
  </div>
{/if}
