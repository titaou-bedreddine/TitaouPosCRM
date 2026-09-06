/**
 * "80 mm – Professional Sales Receipt" built-in preset.
 *
 * Element-based flow layout (dynamic height — the receipt grows with the
 * number of products), all measurements in mm so the printed width is
 * exactly the configured paper width (80 mm standard, 58 mm compact)
 * regardless of DPI. Real offline QR (data URL provided by the caller)
 * and a real CODE128 invoice barcode — no reference artwork is used.
 *
 * Labels ship for Arabic / French / English; Arabic renders RTL while
 * amounts stay LTR and codes (QR/barcode) are never mirrored.
 */
import { barcodeSvgHtml } from './labelPresets';

export interface ProReceiptItem {
  name: string;
  quantity: number;
  unitPrice: number;
  totalPrice: number;
  discountPerUnit?: number;
  isRefund?: boolean;
}

export interface ProReceiptOptions {
  shopName: string;
  shopTagline?: string;
  shopAddress?: string;
  shopPhone?: string;
  shopWebsite?: string;
  shopLogoDataUrl?: string;
  shopRc?: string;
  shopNif?: string;
  invoiceNumber: string;
  invoiceBarcode?: string;
  dateStr: string;
  timeStr: string;
  cashierName?: string;
  customerName?: string;
  customerPhone?: string;
  paymentMethod: string;
  items: ProReceiptItem[];
  subtotal: number;
  discount: number;
  grandTotal: number;
  amountPaid?: number;
  change?: number;
  currency: string;
  qrDataUrl?: string;
  thankYou?: string;
  returnPolicy?: string;
  lang: 'ar' | 'fr' | 'en';
  paperWidthMm?: number;
  copyLabel?: string;
  versementPaid?: number;
  versementRemaining?: number;
  isCredit?: boolean;
  // Element toggles (default on) — mirror the receipt_show_* settings so
  // the single unified template honors every "Fields to Show" switch.
  showShopName?: boolean;
  showAddress?: boolean;
  showPhone?: boolean;
  showWebsite?: boolean;
  showRcNif?: boolean;
  showCashier?: boolean;
  showDate?: boolean;
  showFooter?: boolean;
  showQr?: boolean;
  showBarcode?: boolean;
}

type ReceiptLabels = Record<
  | 'invoiceNo' | 'date' | 'time' | 'cashier' | 'customer' | 'phone'
  | 'payment' | 'designation' | 'qty' | 'unitPrice' | 'total'
  | 'subtotal' | 'discount' | 'amountPaid' | 'change'
  | 'creditStrip' | 'verseNote',
  string
>;

const LABELS: Record<'ar' | 'fr' | 'en', ReceiptLabels> = {
  fr: {
    invoiceNo: 'FACTURE N°', date: 'DATE', time: 'HEURE', cashier: 'CAISSIER',
    customer: 'CLIENT', phone: 'TÉL', payment: 'MODE PAIEMENT',
    designation: 'DÉSIGNATION', qty: 'QTE', unitPrice: 'PU', total: 'TOTAL',
    subtotal: 'SOUS-TOTAL', discount: 'REMISE', amountPaid: 'MONTANT PAYÉ',
    change: 'MONNAIE',
    creditStrip: 'VENTE À CRÉDIT / دين',
    verseNote: 'BIENS CONSERVÉS AU MAGASIN / البضاعة تبقى في المحل',
  },
  en: {
    invoiceNo: 'INVOICE N°', date: 'DATE', time: 'TIME', cashier: 'CASHIER',
    customer: 'CUSTOMER', phone: 'PHONE', payment: 'PAYMENT METHOD',
    designation: 'DESCRIPTION', qty: 'QTY', unitPrice: 'UNIT PRICE', total: 'TOTAL',
    subtotal: 'SUBTOTAL', discount: 'DISCOUNT', amountPaid: 'AMOUNT PAID',
    change: 'CHANGE',
    creditStrip: 'CREDIT SALE / دين',
    verseNote: 'GOODS KEPT AT THE SHOP / البضاعة تبقى في المحل',
  },
  ar: {
    invoiceNo: 'رقم الفاتورة', date: 'التاريخ', time: 'الوقت', cashier: 'أمين الصندوق',
    customer: 'الزبون', phone: 'الهاتف', payment: 'طريقة الدفع',
    designation: 'البيان', qty: 'الكمية', unitPrice: 'سعر الوحدة', total: 'المجموع',
    subtotal: 'المجموع الفرعي', discount: 'الخصم', amountPaid: 'المبلغ المدفوع',
    change: 'الباقي',
    creditStrip: 'بيع بالدين / CREDIT',
    verseNote: 'البضاعة تبقى في المحل / BIENS CONSERVÉS AU MAGASIN',
  },
};

function money(v: number): string {
  return (Number.isFinite(v) ? v : 0).toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

function esc(s: string): string {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// Small monochrome vector icons (print-safe, ~3mm).
const ICONS = {
  cart: (mmH: number) => `<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" style="height:${mmH}mm;width:${mmH * 1.1}mm;display:inline-block;vertical-align:middle;"><circle cx="9" cy="21" r="1.6" fill="#000" stroke="none"/><circle cx="19" cy="21" r="1.6" fill="#000" stroke="none"/><path d="M2.5 3h2l2.4 12.4a2 2 0 0 0 2 1.6h9.7a2 2 0 0 0 2-1.6L22 7H6"/></svg>`,
  pin: `<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" style="height:3mm;width:3mm;display:inline-block;vertical-align:-0.4mm;margin:0 0.8mm;"><path d="M20 10c0 6-8 12-8 12S4 16 4 10a8 8 0 1 1 16 0Z"/><circle cx="12" cy="10" r="3"/></svg>`,
  phone: `<svg viewBox="0 0 24 24" fill="#000" style="height:3mm;width:3mm;display:inline-block;vertical-align:-0.4mm;margin:0 0.8mm;"><path d="M6.6 10.8c1.4 2.8 3.8 5.1 6.6 6.6l2.2-2.2c.3-.3.7-.4 1-.2 1.1.4 2.3.6 3.6.6.6 0 1 .4 1 1V20c0 .6-.4 1-1 1C10.6 21 3 13.4 3 4c0-.6.4-1 1-1h3.5c.6 0 1 .4 1 1 0 1.2.2 2.4.6 3.6.1.3 0 .7-.2 1l-2.3 2.2Z"/></svg>`,
  globe: `<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" style="height:3mm;width:3mm;display:inline-block;vertical-align:-0.4mm;margin:0 0.8mm;"><circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a15 15 0 0 1 0 18 15 15 0 0 1 0-18Z"/></svg>`,
};

/**
 * Builds the complete self-contained receipt HTML (scoped <style> + content).
 * The same string feeds the modal preview and both print paths, so the
 * preview always matches the printed ticket.
 */
export function buildProfessionalReceiptHtml(o: ProReceiptOptions): string {
  const paperW = o.paperWidthMm || 80;
  const compact = paperW <= 60; // 58 mm: drop the PU column, smaller QR
  const t = LABELS[o.lang] || LABELS.fr;
  const rtl = o.lang === 'ar';
  const currency = (o.currency || 'DA').toUpperCase();
  const showUnitPrice = !compact;

  const font = `'Segoe UI',Arial,'Helvetica Neue',Tahoma,'Noto Sans Arabic',sans-serif`;
  const style = `<style>
    .tpRcp,.tpRcp *{box-sizing:border-box;margin:0;padding:0;font-family:${font};color:#000;}
    .tpRcp{width:${paperW}mm;background:#fff;font-size:2.6mm;line-height:1.4;padding:2mm 2.5mm 2.5mm;}
    .tpRcp .dash{border:none;border-top:0.35mm dashed #000;margin:1.8mm 0;}
    .tpRcp .ctr{text-align:center;}
    .tpRcp .info{display:flex;align-items:flex-start;}
    .tpRcp .info>div{flex:1;min-width:0;}
    .tpRcp .info p{white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}
    .tpRcp .info b{font-weight:800;}
    .tpRcp table{width:100%;border-collapse:collapse;}
    .tpRcp th{font-weight:800;font-size:2.6mm;text-align:left;border-bottom:0.3mm solid #000;padding:0 0 0.8mm;}
    .tpRcp td{font-size:2.6mm;padding:1.1mm 0;vertical-align:top;}
    .tpRcp .num{font-variant-numeric:tabular-nums;}
    .tpRcp .tr{display:flex;align-items:baseline;}
    .tpRcp .tr .lbl{flex:1;text-align:right;padding-right:3mm;font-weight:700;}
    .tpRcp .tr .val{min-width:22mm;text-align:right;font-weight:600;}
    .tpRcp .big .lbl{font-weight:900;}
    .tpRcp .big .val{font-weight:900;font-size:4.2mm;}
    .tpRcp .strip{border:0.35mm solid #000;padding:0.8mm 1mm;text-align:center;font-weight:800;margin-top:1.5mm;}
    .tpRcp .contact{font-size:2.5mm;margin-top:0.7mm;}
  </style>`;

  // ---- Header -------------------------------------------------------------
  const showShopName = o.showShopName !== false;
  const showAddress = o.showAddress !== false;
  const showPhone = o.showPhone !== false;
  const showWebsite = o.showWebsite !== false;
  const logoOrIcon = o.shopLogoDataUrl
    ? `<img src="${o.shopLogoDataUrl}" alt="" style="height:10mm;max-width:34mm;object-fit:contain;display:block;margin:0 auto 0.8mm;" />`
    : `<span style="margin-right:1.5mm;">${ICONS.cart(8)}</span>`;
  const rcNifLine = o.shopRc || o.shopNif
    ? `<div style="font-size:2.2mm;color:#333;margin-top:0.5mm;">${o.shopRc ? `RC: ${esc(o.shopRc)}` : ''}${o.shopRc && o.shopNif ? ' • ' : ''}${o.shopNif ? `NIF: ${esc(o.shopNif)}` : ''}</div>`
    : '';
  const header = `<div class="ctr">
      ${showShopName ? `<div style="display:flex;align-items:center;justify-content:center;">
        ${logoOrIcon}
        <span style="font-size:5.6mm;font-weight:900;letter-spacing:0.1mm;">${esc(o.shopName || 'TITAOU POS')}</span>
      </div>` : ''}
      ${showShopName && o.shopTagline ? `<div style="font-size:2.6mm;font-weight:700;margin-top:0.3mm;">${esc(o.shopTagline)}</div>` : ''}
      ${showAddress && o.shopAddress ? `<div class="contact">${ICONS.pin}${esc(o.shopAddress)}</div>` : ''}
      ${showPhone && o.shopPhone ? `<div class="contact">${ICONS.phone}<span dir="ltr">${esc(o.shopPhone)}</span></div>` : ''}
      ${showWebsite && o.shopWebsite ? `<div class="contact">${ICONS.globe}<span dir="ltr">${esc(o.shopWebsite)}</span></div>` : ''}
      ${o.showRcNif !== false ? rcNifLine : ''}
    </div>`;

  // ---- Invoice info (two columns) ------------------------------------------
  const showCashier = o.showCashier !== false;
  const showDate = o.showDate !== false;
  const infoRow = (label: string, value?: string) =>
    value ? `<p><b>${esc(label)}</b> : <span dir="ltr">${esc(value)}</span></p>` : '';
  const leftCol = [
    infoRow(t.invoiceNo, o.invoiceNumber),
    showDate ? infoRow(t.date, o.dateStr) : '',
    showDate ? infoRow(t.time, o.timeStr) : '',
    showCashier ? infoRow(t.cashier, o.cashierName) : '',
  ].join('');
  const rightCol = [
    infoRow(t.customer, o.customerName),
    infoRow(t.phone, o.customerPhone),
    infoRow(t.payment, o.paymentMethod),
  ].join('');
  const info = `<div class="info"><div>${leftCol}</div><div>${rightCol}</div></div>`;

  // ---- Products table -------------------------------------------------------
  const th = (w: string, label: string, align = 'left') =>
    `<th style="width:${w};text-align:${align};">${esc(label)}</th>`;
  const rows = o.items
    .map((i) => {
      const refund = i.isRefund ? ' <span style="font-size:2.1mm;font-weight:800;">[RETOUR]</span>' : '';
      const remise =
        i.discountPerUnit && i.discountPerUnit > 0
          ? `<div style="font-size:2.2mm;font-weight:400;">${t.discount} -${money(i.discountPerUnit)}/${esc(currency)}</div>`
          : '';
      return `<tr>
        <td>${esc(i.name)}${refund}${remise}</td>
        <td class="num" style="text-align:center;">${i.quantity}</td>
        ${showUnitPrice ? `<td class="num" style="text-align:right;"><span dir="ltr">${money(i.unitPrice)}</span></td>` : ''}
        <td class="num" style="text-align:right;font-weight:600;"><span dir="ltr">${i.isRefund ? '-' : ''}${money(i.totalPrice)}</span></td>
      </tr>`;
    })
    .join('');
  const table = `<table>
      <thead><tr>
        ${th('46%', t.designation)}
        ${th('12%', t.qty, 'center')}
        ${showUnitPrice ? th('20%', t.unitPrice, 'right') : ''}
        ${th(showUnitPrice ? '22%' : '42%', t.total, 'right')}
      </tr></thead>
      <tbody>${rows}</tbody>
    </table>`;

  // ---- Totals (+ QR two-column layout) ---------------------------------------
  const totalsRow = (label: string, value: string, big = false, currencySuffix = false) =>
    `<div class="tr${big ? ' big' : ''}" style="margin-top:${big ? 1.2 : 0.5}mm;">
      <span class="lbl">${esc(label)}</span>
      <span class="val"><span dir="ltr">${esc(value)}${currencySuffix ? ' ' + esc(currency) : ''}</span></span>
    </div>`;
  const totals = [
    totalsRow(t.subtotal, money(o.subtotal)),
    totalsRow(t.discount, money(o.discount)),
    totalsRow(t.total, money(o.grandTotal), true, true),
    o.amountPaid !== undefined ? totalsRow(t.amountPaid, money(o.amountPaid)) : '',
    o.change !== undefined ? totalsRow(t.change, money(o.change)) : '',
    o.versementPaid !== undefined
      ? totalsRow(
          o.lang === 'ar' ? 'المدفوع (تسبقة)' : o.lang === 'en' ? 'DEPOSIT PAID' : 'VERSÉ (TESPICA)',
          money(o.versementPaid)
        )
      : '',
    o.versementRemaining !== undefined
      ? totalsRow(
          o.lang === 'ar' ? 'الباقي' : o.lang === 'en' ? 'REMAINING' : 'RESTE À PAYER',
          money(o.versementRemaining)
        )
      : '',
  ].join('');
  const qrSize = compact ? 22 : 27;
  const showQr = o.showQr !== false && !!o.qrDataUrl;
  const totalsSection =
    showQr
      ? `<div style="display:flex;align-items:center;">
          <div style="width:${qrSize}mm;flex:0 0 ${qrSize}mm;"><img src="${o.qrDataUrl}" alt="QR" style="width:100%;display:block;" /></div>
          <div style="flex:1;min-width:0;">${totals}</div>
        </div>`
      : totals;

  // ---- Footer ----------------------------------------------------------------
  const showFooter = o.showFooter !== false;
  const footer = `<div class="ctr">
      ${showFooter && o.thankYou ? `<div style="font-size:3.2mm;font-weight:900;letter-spacing:0.1mm;">${esc(o.thankYou)}</div>` : ''}
      ${showFooter && o.returnPolicy ? `<div style="font-size:2.5mm;margin-top:0.8mm;white-space:pre-line;">${esc(o.returnPolicy)}</div>` : ''}
    </div>`;

  // ---- Invoice barcode ---------------------------------------------------------
  const showBarcode = o.showBarcode !== false;
  const bcValue = (o.invoiceBarcode || o.invoiceNumber || '').trim();
  // Pick the module size that fills the printable width without exceeding it
  // (0.19mm is the scanner-safe floor).
  const bcMaxW = paperW - 6;
  const bcUnits = 35 + bcValue.length * 11; // CODE128 modules (worst case)
  const bcModule = Math.max(0.19, Math.min(0.3, bcMaxW / bcUnits));
  const bcSvg = bcValue ? barcodeSvgHtml(bcValue, bcMaxW, 8, bcModule) : '';
  const barcode = showBarcode && bcValue
    ? `<div class="ctr" style="margin-top:2mm;">
        ${bcSvg ? `<div style="display:inline-block;">${bcSvg}</div>` : ''}
        <div class="num" style="font-size:2.4mm;letter-spacing:0.4mm;margin-top:0.6mm;"><span dir="ltr">${esc(bcValue)}</span></div>
      </div>`
    : '';

  const copyBanner = o.copyLabel
    ? `<div class="strip">[ ${esc(o.copyLabel)} ]</div>`
    : '';
  const creditStrip = o.isCredit ? `<div class="strip">${esc(t.creditStrip)}</div>` : '';
  const verseNote =
    o.versementPaid !== undefined
      ? `<div style="border:0.3mm dashed #000;padding:0.8mm;text-align:center;font-weight:700;font-size:2.3mm;margin-top:1.5mm;">${esc(t.verseNote)}</div>`
      : '';

  return `${style}
<div class="tpRcp"${rtl ? ' dir="rtl"' : ''}>
  ${header}
  ${copyBanner}${creditStrip}
  <hr class="dash" />
  ${info}
  <hr class="dash" />
  ${table}
  <hr class="dash" />
  ${totalsSection}
  ${verseNote}
  <hr class="dash" />
  ${footer}
  ${barcode}
</div>`;
}
