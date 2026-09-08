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
  /** LAN: PC name of the terminal that recorded the sale. */
  terminalName?: string;
  customerName?: string;
  items: UnifiedReceiptItem[];
  subtotal: number;
  discount: number;
  grandTotal: number;
  amountPaid: number;
  change: number;
  paymentMethod: string;
  // shop + receipt settings (raw app_settings map; missing keys fall back
  // to the same defaults every other printer used before). Values may be
  // DB strings ('true') or pre-normalized booleans from the Settings UI.
  settings: Record<string, any>;
  qrDataUrl?: string;
  copyLabel?: string;
  isCredit?: boolean;
  versementPaid?: number;
  versementRemaining?: number;
}

function bool(s: Record<string, any>, key: string, dflt = true): boolean {
  const v = s[key];
  if (v === undefined || v === null || v === '') return dflt;
  // SettingsView normalizes toggles to real booleans; the DB returns
  // 'true'/'false' strings — accept BOTH (the old string-only check made
  // the live settings preview ignore every toggle).
  if (v === true || v === 'true' || v === '1') return true;
  if (v === false || v === 'false' || v === '0') return false;
  return dflt;
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
    terminalName: c.terminalName,
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

// ---------------------------------------------------------------------------
// Browser-free fallback (v0.5.20): on machines with NO Chrome/Edge the
// raster pipeline cannot run. Thermal receipt printers are ESC/POS text
// devices, so we can render the SAME receipt as a plain-text command stream
// and spool it RAW — 100% native, silent, fast, no browser needed.
// ---------------------------------------------------------------------------

const ESC = '\x1B';
const GS = '\x1D';

function escposMoney(v: number): string {
  return (Number.isFinite(v) ? v : 0).toLocaleString('en-US', { maximumFractionDigits: 2 });
}

/** Two-column line: label left, value right-aligned at `width` columns. */
function escposRow(label: string, value: string, width: number): string {
  const space = Math.max(1, width - label.length - value.length);
  return label + ' '.repeat(space) + value + '\n';
}

function escposCenter(text: string, width: number): string {
  const pad = Math.max(0, Math.floor((width - text.length) / 2));
  return ' '.repeat(pad) + text + '\n';
}

/** Build the complete ESC/POS payload for one receipt (cut included). */
export function buildEscposReceipt(c: UnifiedReceiptContext, width: 32 | 42 | 48 = 42): string {
  const s = c.settings;
  const b = (key: string, dflt = true) => {
    const v = s[key];
    if (v === undefined || v === null || v === '') return dflt;
    if (v === true || v === 'true' || v === '1') return true;
    if (v === false || v === 'false' || v === '0') return false;
    return dflt;
  };
  let out = '';
  out += ESC + '@'; // init
  out += ESC + 'a' + '\x01'; // center
  out += ESC + '!\x30'; // double size + bold
  out += (c.settings['shop_name_fr'] || c.settings['shop_name_ar'] || 'TITAOU POS') + '\n';
  out += ESC + '!\x00'; // normal
  if (b('receipt_show_address') && s['shop_address']) out += escposCenter(String(s['shop_address']).slice(0, width), width);
  if (b('receipt_show_phone') && s['shop_phone']) out += escposCenter('Tel: ' + s['shop_phone'], width);
  if (b('receipt_show_rc_nif') && (s['shop_rc'] || s['shop_nif'])) {
    out += escposCenter(`RC: ${s['shop_rc'] || '-'} NIF: ${s['shop_nif'] || '-'}`.slice(0, width), width);
  }
  out += '-'.repeat(width) + '\n';
  out += ESC + 'a' + '\x00'; // left
  const info = `#${c.saleNumber}  ${c.saleDate || ''}`;
  out += escposCenter(info.slice(0, width), width);
  if (b('receipt_show_cashier') && c.cashierName) out += escposRow('Cashier', c.cashierName, width);
  if (c.terminalName) out += escposRow('Terminal', c.terminalName.slice(0, width - 10), width);
  if (c.customerName) out += escposRow('Client', c.customerName.slice(0, width - 8), width);
  out += escposRow('Payment', c.paymentMethod, width);
  out += '-'.repeat(width) + '\n';
  for (const i of c.items) {
    const name = (i.isRefund ? '[R] ' : '') + i.name.slice(0, width - 1);
    out += name + '\n';
    out += escposRow(`  ${i.quantity} x ${escposMoney(i.unitPrice)}`, escposMoney(i.totalPrice), width);
  }
  out += '-'.repeat(width) + '\n';
  if (b('receipt_show_address') === false) { /* noop */ }
  if (c.discount > 0) {
    out += escposRow('SUBTOTAL', escposMoney(c.subtotal), width);
    out += escposRow('DISCOUNT', '-' + escposMoney(c.discount), width);
  }
  out += ESC + '!\x30'; // big total
  out += escposRow('TOTAL', escposMoney(c.grandTotal), width);
  out += ESC + '!\x00';
  if (c.amountPaid !== undefined && c.amountPaid > 0) out += escposRow('PAID', escposMoney(c.amountPaid), width);
  if (c.change) out += escposRow('CHANGE', escposMoney(c.change), width);
  if (c.versementPaid !== undefined) out += escposRow('DEPOSIT', escposMoney(c.versementPaid), width);
  if (c.versementRemaining !== undefined) out += escposRow('REMAINING', escposMoney(c.versementRemaining), width);
  out += '-'.repeat(width) + '\n';
  const footer = b('receipt_show_footer')
    ? `${s['receipt_thank_you'] || 'MERCI POUR VOTRE CONFIANCE !'}`
    : '';
  if (footer) out += escposCenter(footer.slice(0, width), width);
  if (b('receipt_show_footer') && s['receipt_footer']) out += escposCenter(String(s['receipt_footer']).slice(0, width), width);
  out += '\n\n\n';
  out += GS + 'V' + '\x42\x00'; // partial cut
  return out;
}

/**
 * ONE smart receipt print: native raster first; if the machine has no
 * browser at all, fall back to ESC/POS RAW text printing automatically.
 */
export async function printReceiptSmart(c: UnifiedReceiptContext): Promise<{ ok: boolean; message: string }> {
  const { printHtmlSilently } = await import('../utils/printer');
  const built = buildUnifiedReceipt(c);
  const r = await printHtmlSilently(built.html, built.title, { widthMm: built.paperWidthMm });
  if (r.ok) return r;
  const browserMissing = /no chrome|no edge|browser/i.test(r.message);
  if (!browserMissing) return r;
  const { invoke } = await import('@tauri-apps/api/core');
  const esc = buildEscposReceipt(c, c.settings['receipt_paper_width'] === '58mm' ? 32 : 42);
  await invoke('print_escpos_raw', {
    payload: esc,
    printer: c.settings['invoice_printer_name'] || null,
  });
  return { ok: true, message: 'Printed via ESC/POS (no browser on this machine)' };
}
