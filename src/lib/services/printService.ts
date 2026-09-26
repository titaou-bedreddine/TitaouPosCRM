/**
 * CENTRALIZED PRINT SERVICE (v1.0.0).
 *
 * The single architectural entry point for all printing operations throughout
 * the entire TitaouPosCRM application.
 *
 * Routing Logic:
 *   POS Operation -> Save to SQLite -> PrintService -> Read Settings:
 *     - 'disabled' -> No print
 *     - 'thermal'  -> USB Thermal Backend (Existing GDI / RAW ESC/POS)
 *     - 'a4'       -> Native A4 Backend (GDI System Printer)
 *
 * Guarantees:
 * 1. Transactions are committed to SQLite BEFORE printing is invoked.
 * 2. Printer failure NEVER cancels or rolls back SQLite transactions.
 * 3. Individual Svelte views contain NO printer-specific logic.
 */

import { invoke } from '@tauri-apps/api/core';
import type { PrintableDocument, PrintableItem, PrintableParty, PrintablePayment } from '../printing/printableDocument';
import { buildA4DocumentHtml } from '../printing/a4Template';
import { buildUnifiedReceipt, printReceiptSmart, type UnifiedReceiptContext } from '../printing/unifiedReceipt';
import { entityQrDataUrl, entityQrPayload } from '../utils/printer';

export type PrintingMode = 'disabled' | 'thermal' | 'a4';

export interface PrintResult {
  ok: boolean;
  message: string;
  mode?: PrintingMode;
}

export interface PrintOptions {
  forceMode?: PrintingMode;
  copyLabel?: string;
  isCredit?: boolean;
}

class PrintService {
  private cachedSettings: Record<string, any> | null = null;
  private settingsTimestamp = 0;

  /**
   * Load current app settings from SQLite (cached for 2 seconds to avoid spamming DB during quick actions).
   */
  async getSettings(forceRefresh = false): Promise<Record<string, any>> {
    const now = Date.now();
    if (!forceRefresh && this.cachedSettings && now - this.settingsTimestamp < 2000) {
      return this.cachedSettings;
    }
    try {
      const s = await invoke<Record<string, any>>('get_all_settings');
      this.cachedSettings = s || {};
      this.settingsTimestamp = now;
      return this.cachedSettings;
    } catch (e) {
      console.warn('PrintService: Failed to fetch settings, falling back to cache/empty:', e);
      return this.cachedSettings || {};
    }
  }

  /**
   * Invalidate settings cache (e.g., after saving settings in SettingsView).
   */
  invalidateSettingsCache() {
    this.cachedSettings = null;
    this.settingsTimestamp = 0;
  }

  /**
   * Persist printing settings back to SQLite.
   */
  async saveSettings(settingsToSave: Record<string, string>): Promise<void> {
    await invoke('set_multiple_settings', { settings: settingsToSave });
    this.invalidateSettingsCache();
  }

  /**
   * Retrieve list of installed system printers (fast native EnumPrintersW).
   */
  async getSystemPrinters(): Promise<string[]> {
    try {
      const list = await invoke<string[]>('get_system_printers');
      return list || [];
    } catch {
      try {
        const fallback = await invoke<string[]>('list_printers');
        return fallback || [];
      } catch (e) {
        console.error('PrintService: Could not retrieve system printers:', e);
        return [];
      }
    }
  }

  /**
   * Core Print Dispatcher.
   * Decides between Disabled, USB Thermal, or Native A4 based on global settings.
   */
  async printDocument(doc: PrintableDocument, options?: PrintOptions): Promise<PrintResult> {
    try {
      const settings = await this.getSettings();
      const rawMode = options?.forceMode || settings['printing_mode'] || 'thermal';
      const mode: PrintingMode = rawMode === 'disabled' || rawMode === 'off' ? 'disabled' : rawMode === 'a4' ? 'a4' : 'thermal';

      if (mode === 'disabled') {
        console.log('[PrintService] Printing is DISABLED in settings. Operation completed without printing.');
        return { ok: true, message: 'Impression désactivée dans les paramètres', mode: 'disabled' };
      }

      if (mode === 'a4') {
        return await this.printA4(doc, settings, options);
      } else {
        return await this.printThermal(doc, settings, options);
      }
    } catch (e: any) {
      const errMsg = typeof e === 'string' ? e : e?.message || String(e);
      console.error('[PrintService] Unhandled print error:', errMsg);
      return { ok: false, message: errMsg };
    }
  }

  /**
   * A4 Printing Backend via Tauri native GDI layer.
   */
  private async printA4(doc: PrintableDocument, settings: Record<string, any>, options?: PrintOptions): Promise<PrintResult> {
    try {
      if (options?.copyLabel && !doc.copyLabel) {
        doc.copyLabel = options.copyLabel;
      }
      const a4Html = buildA4DocumentHtml(doc, settings);
      const a4Printer = (settings['a4_printer'] || '').trim();

      const result = await invoke<{ ok: boolean; message: string }>('print_a4_document', {
        request: {
          html: a4Html,
          title: `${doc.title} - ${doc.documentNumber}`,
          printer: a4Printer || null,
          dpi: 300,
        },
      });

      return {
        ok: result.ok,
        message: result.message,
        mode: 'a4',
      };
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      return { ok: false, message: `Erreur d'impression A4: ${msg}`, mode: 'a4' };
    }
  }

  /**
   * USB Thermal Receipt Backend (preserves existing USB thermal logic).
   */
  private async printThermal(doc: PrintableDocument, settings: Record<string, any>, options?: PrintOptions): Promise<PrintResult> {
    try {
      // Generate QR data URL if payload exists
      let qrUrl = doc.qrDataUrl;
      if (!qrUrl && doc.qrPayload) {
        qrUrl = await entityQrDataUrl(doc.qrPayload, 110);
      }

      // Convert items to UnifiedReceiptItem format
      const thermalItems = (doc.items || []).map((it) => ({
        name: it.name,
        quantity: it.quantity,
        unitPrice: it.unitPrice,
        totalPrice: it.totalPrice,
        discountPerUnit: it.discountPerUnit || 0,
        isRefund: it.isRefund || false,
      }));

      const context: UnifiedReceiptContext = {
        saleNumber: doc.documentNumber,
        saleDate: doc.date,
        cashierName: doc.cashierName || '',
        terminalName: doc.terminalName,
        customerName: doc.party?.name,
        items: thermalItems,
        subtotal: doc.subtotal,
        discount: doc.discountTotal,
        grandTotal: doc.grandTotal,
        amountPaid: doc.payment?.amountPaid ?? doc.grandTotal,
        change: doc.payment?.change ?? 0,
        paymentMethod: doc.payment?.method || 'ESPECES',
        settings,
        qrDataUrl: qrUrl,
        copyLabel: options?.copyLabel || doc.copyLabel,
        isCredit: options?.isCredit || doc.payment?.isCredit,
        versementPaid: doc.payment?.amountPaid,
        versementRemaining: doc.payment?.remainingDue,
      };

      const result = await printReceiptSmart(context);
      return {
        ok: result.ok,
        message: result.message,
        mode: 'thermal',
      };
    } catch (e: any) {
      const msg = typeof e === 'string' ? e : e?.message || String(e);
      return { ok: false, message: `Erreur d'impression thermique: ${msg}`, mode: 'thermal' };
    }
  }

  // -------------------------------------------------------------------------
  // CONVENIENCE BUSINESS METHODS
  // -------------------------------------------------------------------------

  /**
   * Print a completed Sale (Cash, Versement, or Credit).
   */
  async printSale(sale: any, items: any[] = [], options?: PrintOptions): Promise<PrintResult> {
    const isCredit = sale.payment_mode === 'credit' || options?.isCredit;
    const isVersement = sale.payment_mode === 'versement';

    let party: PrintableParty | undefined;
    if (sale.customer_name || sale.customer) {
      party = {
        name: sale.customer_name || sale.customer?.name || 'Client Comptoir',
        type: 'customer',
        phone: sale.customer_phone || sale.customer?.phone,
        address: sale.customer_address || sale.customer?.address,
      };
    }

    const printableItems: PrintableItem[] = items.map((it) => ({
      id: it.product_id || it.id,
      sku: it.sku || '',
      barcode: it.barcode || '',
      name: it.name_fr || it.name || it.product_name || 'Article',
      quantity: it.quantity || 1,
      unit: it.unit || it.sale_unit || '',
      unitPrice: it.unit_price || 0,
      discountPerUnit: it.discount_amount || 0,
      taxRate: it.tax_rate,
      taxAmount: it.tax_amount,
      totalPrice: it.total_price || (it.quantity || 1) * (it.unit_price || 0),
      isRefund: it.is_refund || false,
    }));

    const doc: PrintableDocument = {
      id: sale.id,
      documentNumber: sale.sale_number || `POS-${sale.id}`,
      documentType: isCredit ? 'sale_invoice' : 'sale_receipt',
      title: isCredit ? 'FACTURE DE VENTE' : isVersement ? 'BON DE VERSEMENT' : 'TICKET DE VENTE',
      date: sale.sale_date || new Date().toISOString().slice(0, 10),
      time: sale.sale_time || new Date().toLocaleTimeString('fr-FR'),
      cashierName: sale.cashier_name || sale.user_name,
      terminalName: sale.terminal_name,
      party,
      items: printableItems,
      subtotal: sale.subtotal ?? sale.total_amount ?? 0,
      discountTotal: sale.discount_amount ?? 0,
      taxTotal: sale.tax_amount ?? 0,
      grandTotal: sale.total_amount ?? 0,
      payment: {
        method: isVersement ? 'VERSEMENT' : isCredit ? 'CRÉDIT' : (sale.payment_mode || 'ESPECES').toUpperCase(),
        amountPaid: sale.paid_amount ?? sale.total_amount ?? 0,
        totalDue: sale.total_amount ?? 0,
        change: sale.change_amount ?? 0,
        remainingDue: sale.remaining_amount ?? (isCredit ? (sale.total_amount - (sale.paid_amount || 0)) : 0),
        isCredit,
      },
      notes: sale.notes,
      copyLabel: options?.copyLabel,
      qrPayload: entityQrPayload('SALE', sale.sale_number || String(sale.id)),
    };

    return await this.printDocument(doc, options);
  }

  /**
   * Print a Purchase invoice/receipt.
   */
  async printPurchase(purchase: any, items: any[] = [], options?: PrintOptions): Promise<PrintResult> {
    const printableItems: PrintableItem[] = items.map((it) => ({
      id: it.product_id || it.id,
      sku: it.sku || '',
      barcode: it.barcode || '',
      name: it.product_name || it.name_fr || it.name || 'Article',
      quantity: it.quantity || 1,
      unit: it.unit || '',
      unitPrice: it.unit_cost || it.unit_price || 0,
      discountPerUnit: it.discount_amount || 0,
      totalPrice: it.total_cost || it.total_price || (it.quantity || 1) * (it.unit_cost || 0),
    }));

    const doc: PrintableDocument = {
      id: purchase.id,
      documentNumber: purchase.invoice_number || `ACH-${purchase.id}`,
      documentType: 'purchase_invoice',
      title: "BON D'ACHAT / FACTURE FOURNISSEUR",
      date: purchase.purchase_date || new Date().toISOString().slice(0, 10),
      party: purchase.supplier_name ? {
        name: purchase.supplier_name,
        type: 'supplier',
        phone: purchase.supplier_phone,
      } : undefined,
      items: printableItems,
      subtotal: purchase.subtotal ?? purchase.total ?? 0,
      discountTotal: purchase.discount ?? 0,
      taxTotal: purchase.tax ?? 0,
      grandTotal: purchase.total ?? 0,
      payment: {
        method: purchase.payment_mode || 'ESPECES',
        amountPaid: purchase.paid_amount ?? purchase.total ?? 0,
        totalDue: purchase.total ?? 0,
        remainingDue: (purchase.total ?? 0) - (purchase.paid_amount ?? 0),
      },
      notes: purchase.notes,
      qrPayload: entityQrPayload('PUR', purchase.invoice_number || String(purchase.id)),
    };

    return await this.printDocument(doc, options);
  }

  /**
   * Print a Sales or Purchase Return.
   */
  async printReturn(returnRecord: any, items: any[] = [], options?: PrintOptions): Promise<PrintResult> {
    const isPurchase = returnRecord.type === 'purchase';
    const printableItems: PrintableItem[] = items.map((it) => ({
      id: it.product_id || it.id,
      sku: it.sku || '',
      name: it.name || it.product_name || 'Article retourné',
      quantity: it.quantity || 1,
      unitPrice: it.unit_price || it.unit_cost || 0,
      totalPrice: it.total_price || (it.quantity || 1) * (it.unit_price || it.unit_cost || 0),
      isRefund: true,
      notes: it.reason,
    }));

    const total = printableItems.reduce((acc, x) => acc + x.totalPrice, 0);

    const doc: PrintableDocument = {
      id: returnRecord.id,
      documentNumber: returnRecord.return_number || `RET-${returnRecord.id || Date.now()}`,
      documentType: isPurchase ? 'purchase_return' : 'sale_return',
      title: isPurchase ? 'BON DE RETOUR FOURNISSEUR' : 'BON DE RETOUR / AVOIR',
      date: returnRecord.date || new Date().toISOString().slice(0, 10),
      party: returnRecord.party_name ? {
        name: returnRecord.party_name,
        type: isPurchase ? 'supplier' : 'customer',
      } : undefined,
      items: printableItems,
      subtotal: total,
      discountTotal: 0,
      taxTotal: 0,
      grandTotal: total,
      payment: {
        method: returnRecord.refund_method || 'REMBOURSEMENT',
        amountPaid: returnRecord.refunded_amount ?? total,
        totalDue: total,
      },
      notes: returnRecord.reason,
    };

    return await this.printDocument(doc, options);
  }

  /**
   * Print a Payment receipt / Expense voucher.
   */
  async printPayment(paymentData: {
    receiptNumber: string;
    date: string;
    title: string;
    partyName?: string;
    partyType?: 'customer' | 'supplier' | 'employee';
    amount: number;
    paymentMethod: string;
    notes?: string;
    remainingBalance?: number;
  }, options?: PrintOptions): Promise<PrintResult> {
    const doc: PrintableDocument = {
      documentNumber: paymentData.receiptNumber,
      documentType: 'payment_receipt',
      title: paymentData.title || 'REÇU DE RÈGLEMENT',
      date: paymentData.date || new Date().toISOString().slice(0, 10),
      party: paymentData.partyName ? {
        name: paymentData.partyName,
        type: paymentData.partyType || 'customer',
        currentBalance: paymentData.remainingBalance,
      } : undefined,
      items: [
        {
          name: paymentData.title,
          quantity: 1,
          unitPrice: paymentData.amount,
          totalPrice: paymentData.amount,
          notes: paymentData.notes,
        },
      ],
      subtotal: paymentData.amount,
      discountTotal: 0,
      taxTotal: 0,
      grandTotal: paymentData.amount,
      payment: {
        method: paymentData.paymentMethod || 'ESPECES',
        amountPaid: paymentData.amount,
        totalDue: paymentData.amount,
        remainingDue: paymentData.remainingBalance,
      },
      notes: paymentData.notes,
    };

    return await this.printDocument(doc, options);
  }

  /**
   * Print a Customer Debt Recap.
   */
  async printCustomerDebtRecap(customer: any, debts: any[]): Promise<PrintResult> {
    const items: PrintableItem[] = debts.map((d) => ({
      sku: d.sale_number || `Vente #${d.sale_id}`,
      name: d.date ? `Vente du ${d.date}` : `Facture ${d.sale_number || ''}`,
      quantity: 1,
      unitPrice: d.total_amount || d.amount || 0,
      totalPrice: d.remaining_amount || d.remaining || d.amount || 0,
      notes: d.notes || (d.paid_amount ? `Payé: ${d.paid_amount}` : undefined),
    }));

    const totalRemaining = debts.reduce((sum, d) => sum + (d.remaining_amount || d.remaining || d.amount || 0), 0);

    const doc: PrintableDocument = {
      documentNumber: `RECAP-${customer.id || Date.now()}`,
      documentType: 'customer_recap',
      title: 'RECAPITULATIF DETTE CLIENT',
      date: new Date().toISOString().slice(0, 10),
      party: {
        name: customer.name,
        type: 'customer',
        phone: customer.phone,
        address: customer.address,
        currentBalance: totalRemaining,
      },
      items,
      subtotal: totalRemaining,
      discountTotal: 0,
      taxTotal: 0,
      grandTotal: totalRemaining,
      notes: 'Ce relevé reprend l’ensemble des opérations restant dues à ce jour.',
    };

    return await this.printDocument(doc);
  }

  /**
   * Print a Cash Register Session / Shift Report (Z-Report).
   */
  async printSessionReport(session: any, countedCash: number, closeNotes?: string): Promise<PrintResult> {
    const diff = countedCash - (session.expected_cash || 0);
    const items: PrintableItem[] = [
      { name: 'Solde d’ouverture', quantity: 1, unitPrice: session.opening_amount || 0, totalPrice: session.opening_amount || 0 },
      { name: 'Total Ventes (Cash)', quantity: 1, unitPrice: session.total_sales || 0, totalPrice: session.total_sales || 0 },
      { name: 'Total Sorties / Dépenses (Cash)', quantity: 1, unitPrice: session.total_expenses || 0, totalPrice: -(session.total_expenses || 0) },
      { name: 'Montant Attendu en Caisse', quantity: 1, unitPrice: session.expected_cash || 0, totalPrice: session.expected_cash || 0 },
      { name: 'Montant Réel Compté', quantity: 1, unitPrice: countedCash, totalPrice: countedCash },
      { name: 'Écart de Caisse', quantity: 1, unitPrice: diff, totalPrice: diff },
    ];

    const doc: PrintableDocument = {
      id: session.id,
      documentNumber: `CLOTURE-${session.id}`,
      documentType: 'session_report',
      title: 'RAPPORT DE CLÔTURE DE CAISSE (Z)',
      date: session.closed_at ? session.closed_at.slice(0, 10) : new Date().toISOString().slice(0, 10),
      time: session.closed_at ? session.closed_at.slice(11) : new Date().toLocaleTimeString('fr-FR'),
      cashierName: session.user_name || 'Caissier',
      items,
      subtotal: session.total_sales || 0,
      discountTotal: 0,
      taxTotal: 0,
      grandTotal: session.total_sales || 0,
      payment: {
        method: 'ESPECES',
        amountPaid: countedCash,
        totalDue: session.expected_cash || 0,
      },
      notes: [
        closeNotes ? `Notes: ${closeNotes}` : '',
        `Écart constaté: ${diff.toLocaleString()} DZD`,
      ].filter(Boolean).join('\n'),
      footerNote: 'Rapport officiel de clôture de caisse enregistré.',
    };

    return await this.printDocument(doc);
  }

  /**
   * Diagnostic Test: A4 Printer.
   */
  async testA4Printer(printerName?: string): Promise<PrintResult> {
    try {
      const s = await this.getSettings();
      const targetPrinter = printerName || s['a4_printer'] || null;
      const res = await invoke<{ ok: boolean; message: string }>('test_a4_printer', {
        printer: targetPrinter,
      });
      return { ok: res.ok, message: res.message, mode: 'a4' };
    } catch (e: any) {
      return { ok: false, message: `Test A4 échoué: ${e.message || e}`, mode: 'a4' };
    }
  }

  /**
   * Diagnostic Test: Thermal Printer.
   */
  async testThermalPrinter(printerName?: string): Promise<PrintResult> {
    const s = await this.getSettings();
    const testDoc: PrintableDocument = {
      documentNumber: 'TEST-001',
      documentType: 'sale_receipt',
      title: 'TEST THERMIQUE',
      date: new Date().toISOString().slice(0, 10),
      time: new Date().toLocaleTimeString('fr-FR'),
      items: [
        { name: 'Article de test #1', quantity: 1, unitPrice: 150, totalPrice: 150 },
        { name: 'Article de test #2', quantity: 2, unitPrice: 200, totalPrice: 400 },
      ],
      subtotal: 550,
      discountTotal: 50,
      taxTotal: 0,
      grandTotal: 500,
      payment: {
        method: 'ESPECES',
        amountPaid: 500,
        totalDue: 500,
      },
      footerNote: 'Si ce ticket est lisible, votre imprimante thermique est prête.',
    };

    return await this.printThermal(testDoc, s, { copyLabel: 'TEST HARDWARE' });
  }
}

export const printService = new PrintService();
export default printService;
