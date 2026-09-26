/**
 * PRINTABLE DOCUMENT DOMAIN MODEL (v1.0.0).
 *
 * A unified, decoupled representation of any printable business operation in the POS:
 * Sales, Purchases, Returns, Payments, Vouchers, Customer/Supplier Debt Recaps,
 * and Register Shifts.
 *
 * Mappers convert transaction records from SQLite into this model, which is then
 * rendered by either the A4 Renderer or the USB Thermal Renderer.
 */

export type DocumentType =
  | 'sale_invoice'
  | 'sale_receipt'
  | 'sale_return'
  | 'purchase_invoice'
  | 'purchase_receipt'
  | 'purchase_return'
  | 'payment_receipt'
  | 'customer_recap'
  | 'supplier_recap'
  | 'stock_operation'
  | 'session_report';

export interface PrintableParty {
  id?: number | string;
  name: string;
  type?: 'customer' | 'supplier' | 'employee';
  phone?: string;
  address?: string;
  email?: string;
  taxNumber?: string; // NIF / RC / NIS
  currentBalance?: number;
}

export interface PrintableItem {
  id?: number | string;
  sku?: string;
  barcode?: string;
  name: string;
  quantity: number;
  unit?: string;
  unitPrice: number;
  discountPerUnit?: number;
  taxRate?: number;
  taxAmount?: number;
  totalPrice: number;
  isRefund?: boolean;
  notes?: string;
}

export interface PrintablePayment {
  method: string; // e.g. 'ESPECES', 'CARTE', 'CHEQUE', 'VIREMENT', 'CREDIT', 'VERSEMENT'
  amountPaid: number;
  totalDue: number;
  change?: number;
  remainingDue?: number;
  reference?: string;
  isCredit?: boolean;
}

export interface PrintableDocument {
  id?: number | string;
  documentNumber: string;
  documentType: DocumentType;
  title: string;
  date: string; // ISO string or formatted date
  time?: string;
  cashierName?: string;
  terminalName?: string;
  party?: PrintableParty;
  items: PrintableItem[];
  subtotal: number;
  discountTotal: number;
  taxTotal: number;
  grandTotal: number;
  payment?: PrintablePayment;
  notes?: string;
  footerNote?: string;
  copyLabel?: string;
  qrPayload?: string;
  qrDataUrl?: string;
  currency?: string;
}
