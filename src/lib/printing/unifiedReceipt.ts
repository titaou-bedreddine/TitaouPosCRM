/**
 * UNIFIED RECEIPT PRESET SYSTEM (v0.5.17).
 *
 * One template, one builder, every receipt: POS auto-print, reprint from
 * Sales History, reopen-last-receipt, credit/versement copies, refund
 * tickets — all built here from the same app_settings keys, so a settings
 * change applies everywhere at once. The old "standard" monospace preset
 * and the per-page hardcoded formats are REMOVED; the professional graphic
 * template is the single receipt and it honors every receipt_show_* toggle.
 *
 * All inputs are plain data (sale + items + shop settings), so callers
 * never re-implement layout or option parsing.
 */
import { buildProfessionalReceiptHtml, type ProReceiptOptions } from './professionalReceipt';
import { getLanguage } from '../i18n';

export interface UnifiedReceiptItem {
  name: string;
  quantity: number;
  unitPrice: number;
  totalPrice: number;
  discountPerUnit?: number;
  isRefund?: boolean;
}

export interface UnifiedReceiptContext {
  saleNumber: string;
  saleDate: string;
  cashierName: string;
  customerName?: string;
  items: UnifiedReceiptItem[];
  subtotal: number;
  discount: number;
  grandTotal: number;
  amountPaid: number;
  change: number;
  paymentMethod: string;
  // shop + receipt settings (raw app_settings map; missing keys fall back
  // to the same defaults every other printer used before).
  settings: Record<string, string | undefined>;
  qrDataUrl?: string;
  copyLabel?: string;
  isCredit?: boolean;
  versementPaid?: number;
  versementRemaining?: number;
}

function bool(s: Record<string, string | undefined>, key: string, dflt = true): boolean {
  const v = s[key];
  if (v === undefined || v === null || v === '') return dflt;
  return v === 'true' || v === '1';
}

/** Shared settings-driven options for the single professional template. */
function proOptionsFromContext(c: UnifiedReceiptContext): ProReceiptOptions {
  const s = c.settings;
  const d = new Date(c.saleDate?.replace(' ', 'T') || Date.now());
  const valid = !isNaN(d.getTime()) ? d : new Date();
  const paperWidthMm = s['receipt_paper_width'] === '58mm' ? 58 : 80;

  return {
    shopName: s['shop_name_fr'] || s['shop_name_ar'] || 'TITAOU POS',
    shopTagline: s['receipt_header'] || '',
    shopAddress: s['shop_address'] || '',
    shopPhone: s['shop_phone'] || '',
    shopWebsite: s['shop_website'] || '',
    shopLogoDataUrl: s['shop_logo_base64'] || undefined,
    shopRc: s['shop_rc'] || undefined,
    shopNif: s['shop_nif'] || undefined,
    invoiceNumber: c.saleNumber,
    invoiceBarcode: c.saleNumber,
    dateStr: valid.toLocaleDateString('fr-FR'),
    timeStr: valid.toLocaleTimeString('fr-FR'),
    cashierName: c.cashierName,
    customerName: c.customerName,
    paymentMethod: c.paymentMethod,
    items: c.items,
    subtotal: c.subtotal,
    discount: c.discount,
    grandTotal: c.grandTotal,
    amountPaid: c.amountPaid,
    change: c.change,
    currency: s['default_currency'] || 'DA',
    qrDataUrl: c.qrDataUrl,
    // Every "Fields to Show" toggle from Settings → Printing is honored:
    showShopName: bool(s, 'receipt_show_shop_name'),
    showAddress: bool(s, 'receipt_show_address'),
    showPhone: bool(s, 'receipt_show_phone'),
    showRcNif: bool(s, 'receipt_show_rc_nif'),
    showCashier: bool(s, 'receipt_show_cashier'),
    showDate: bool(s, 'receipt_show_date'),
    showFooter: bool(s, 'receipt_show_footer'),
    showQr: bool(s, 'receipt_show_qr'),
    showBarcode: bool(s, 'receipt_show_barcode'),
    thankYou: s['receipt_thank_you'] || 'MERCI POUR VOTRE CONFIANCE !',
    returnPolicy: s['receipt_footer'] || '',
    lang: getLanguage(),
    paperWidthMm,
    copyLabel: c.copyLabel,
    isCredit: c.isCredit,
    versementPaid: c.versementPaid,
    versementRemaining: c.versementRemaining,
  };
}

export interface BuiltReceipt {
  html: string;
  title: string;
  paperWidthMm: number;
}

/**
 * Build ONE receipt (any kind) from the unified context. The returned
 * paper width drives the silent print job's DEVMODE.
 */
export function buildUnifiedReceipt(c: UnifiedReceiptContext): BuiltReceipt {
  const paperWidthMm = c.settings['receipt_paper_width'] === '58mm' ? 58 : 80;
  const title = `Receipt #${c.saleNumber}${c.copyLabel ? ' — ' + c.copyLabel : ''}`;

  return {
    html: buildProfessionalReceiptHtml(proOptionsFromContext(c)),
    title,
    paperWidthMm,
  };
}
