/**
 * PROFESSIONAL MULTI-PAGE A4 DOCUMENT RENDERER (v1.0.0).
 *
 * Renders standard 210mm × 297mm multi-page documents (Invoices, Receipts,
 * Purchases, Returns, Vouchers, Account Recaps) that print natively via Windows GDI.
 *
 * Features:
 * - Multi-page pagination: large item lists split cleanly across pages
 * - Repeating document header & table column titles on every page
 * - Page number indicator: "Page X / Y"
 * - Full settings compliance: every `a4_show_*` toggle is respected
 * - Support for RTL/Arabic and LTR/French/English text
 */

import type { PrintableDocument, PrintableItem } from './printableDocument';

function bool(s: Record<string, any>, key: string, dflt = true): boolean {
  const v = s[key];
  if (v === undefined || v === null || v === '') return dflt;
  if (v === true || v === 'true' || v === '1' || v === 1) return true;
  if (v === false || v === 'false' || v === '0' || v === 0) return false;
  return dflt;
}

function formatMoney(amount: number | undefined | null, currency = 'DA'): string {
  const val = Number.isFinite(amount) ? (amount as number) : 0;
  return `${val.toLocaleString('fr-FR', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${currency}`;
}

export function buildA4DocumentHtml(doc: PrintableDocument, settings: Record<string, any>): string {
  const currency = doc.currency || settings['default_currency'] || 'DA';

  // Toggle settings
  const showLogo = bool(settings, 'a4_show_logo', true);
  const showStoreName = bool(settings, 'a4_show_store_name', true);
  const showAddress = bool(settings, 'a4_show_address', true);
  const showPhone = bool(settings, 'a4_show_phone', true);
  const showEmail = bool(settings, 'a4_show_email', true);
  const showTaxNumber = bool(settings, 'a4_show_tax_number', true);

  const showCustomer = bool(settings, 'a4_show_customer', true);
  const showSupplier = bool(settings, 'a4_show_supplier', true);

  const showSku = bool(settings, 'a4_show_sku', true);
  const showBarcode = bool(settings, 'a4_show_barcode', false);
  const showQuantity = bool(settings, 'a4_show_quantity', true);
  const showUnitPrice = bool(settings, 'a4_show_unit_price', true);
  const showDiscount = bool(settings, 'a4_show_discount', true);
  const showTax = bool(settings, 'a4_show_tax', false);

  const showPayment = bool(settings, 'a4_show_payment', true);
  const showAmountPaid = bool(settings, 'a4_show_amount_paid', true);
  const showRemaining = bool(settings, 'a4_show_remaining', true);

  const showNotes = bool(settings, 'a4_show_notes', true);
  const showFooter = bool(settings, 'a4_show_footer', true);

  // Store information
  const storeName = settings['shop_name_fr'] || settings['shop_name_ar'] || 'TITAOU POS';
  const storeLogo = settings['shop_logo_base64'] || '';
  const storeAddress = settings['shop_address'] || '';
  const storePhone = settings['shop_phone'] || '';
  const storeEmail = settings['shop_email'] || settings['shop_website'] || '';
  const storeRc = settings['shop_rc'] || '';
  const storeNif = settings['shop_nif'] || '';
  const storeNis = settings['shop_nis'] || '';
  const storeArt = settings['shop_art'] || '';

  // Pagination calculation:
  // On Page 1, header + parties take ~90mm. Table rows fit ~14 items.
  // On subsequent pages, only a mini-header is shown (~30mm), fitting ~24 items.
  // Last page needs ~60mm for totals & payment.
  const PAGE_1_MAX = 13;
  const SUBSEQUENT_PAGE_MAX = 22;

  const items = doc.items || [];
  const pages: PrintableItem[][] = [];

  if (items.length === 0) {
    pages.push([]);
  } else if (items.length <= PAGE_1_MAX) {
    pages.push(items);
  } else {
    pages.push(items.slice(0, PAGE_1_MAX));
    let remaining = items.slice(PAGE_1_MAX);
    while (remaining.length > 0) {
      pages.push(remaining.slice(0, SUBSEQUENT_PAGE_MAX));
      remaining = remaining.slice(SUBSEQUENT_PAGE_MAX);
    }
  }

  const totalPages = pages.length;

  // Render HTML for each page
  const renderedPages = pages.map((pageItems, pageIdx) => {
    const isFirstPage = pageIdx === 0;
    const isLastPage = pageIdx === totalPages - 1;
    const pageNum = pageIdx + 1;

    // Header section
    let headerHtml = '';
    if (isFirstPage) {
      headerHtml = `
        <div class="header-section">
          <div class="store-info">
            ${showLogo && storeLogo ? `<img src="${storeLogo}" class="store-logo" alt="Logo" />` : ''}
            <div>
              ${showStoreName ? `<div class="store-name">${storeName}</div>` : ''}
              ${showAddress && storeAddress ? `<div class="store-detail">${storeAddress}</div>` : ''}
              ${showPhone && storePhone ? `<div class="store-detail"><strong>Tél:</strong> ${storePhone}</div>` : ''}
              ${showEmail && storeEmail ? `<div class="store-detail"><strong>Web / Email:</strong> ${storeEmail}</div>` : ''}
              ${showTaxNumber && (storeRc || storeNif || storeNis || storeArt) ? `
                <div class="store-tax-info">
                  ${storeRc ? `<span>RC: ${storeRc}</span>` : ''}
                  ${storeNif ? `<span>NIF: ${storeNif}</span>` : ''}
                  ${storeNis ? `<span>NIS: ${storeNis}</span>` : ''}
                  ${storeArt ? `<span>ART: ${storeArt}</span>` : ''}
                </div>` : ''}
            </div>
          </div>

          <div class="doc-badge-box">
            <div class="doc-badge">${doc.title || 'FACTURE'}</div>
            <div class="doc-meta-row"><strong>N°:</strong> <span>${doc.documentNumber}</span></div>
            <div class="doc-meta-row"><strong>Date:</strong> <span>${doc.date} ${doc.time || ''}</span></div>
            ${doc.cashierName ? `<div class="doc-meta-row"><strong>Caissier:</strong> <span>${doc.cashierName}</span></div>` : ''}
            ${doc.copyLabel ? `<div class="copy-badge">${doc.copyLabel}</div>` : ''}
          </div>
        </div>

        ${((showCustomer || showSupplier) && doc.party) ? `
          <div class="party-card">
            <div class="party-title">${doc.party.type === 'supplier' ? 'FOURNISSEUR' : 'CLIENT'}</div>
            <div class="party-name">${doc.party.name}</div>
            <div class="party-details">
              ${doc.party.phone ? `<span><strong>Tél:</strong> ${doc.party.phone}</span>` : ''}
              ${doc.party.address ? `<span><strong>Adresse:</strong> ${doc.party.address}</span>` : ''}
              ${doc.party.taxNumber ? `<span><strong>NIF/RC:</strong> ${doc.party.taxNumber}</span>` : ''}
              ${doc.party.currentBalance !== undefined ? `<span><strong>Solde actuel:</strong> ${formatMoney(doc.party.currentBalance, currency)}</span>` : ''}
            </div>
          </div>` : ''}
      `;
    } else {
      headerHtml = `
        <div class="mini-header-section">
          <div>
            <span class="store-name-mini">${storeName}</span>
            <span class="doc-ref-mini">— ${doc.title} N° ${doc.documentNumber}</span>
          </div>
          <div class="page-counter-mini">Page ${pageNum} / ${totalPages}</div>
        </div>
      `;
    }

    // Items Table section
    let tableHtml = `
      <table class="items-table">
        <thead>
          <tr>
            <th class="col-num">#</th>
            ${showSku ? '<th class="col-sku">Réf / SKU</th>' : ''}
            ${showBarcode ? '<th class="col-barcode">Code-barres</th>' : ''}
            <th class="col-designation">Désignation</th>
            ${showQuantity ? '<th class="col-qty">Qté</th>' : ''}
            ${showUnitPrice ? '<th class="col-price">P.U.</th>' : ''}
            ${showDiscount ? '<th class="col-discount">Remise</th>' : ''}
            ${showTax ? '<th class="col-tax">TVA</th>' : ''}
            <th class="col-total">Total</th>
          </tr>
        </thead>
        <tbody>
    `;

    pageItems.forEach((item, itemIdx) => {
      const globalIdx = (pageIdx === 0 ? 0 : PAGE_1_MAX + (pageIdx - 1) * SUBSEQUENT_PAGE_MAX) + itemIdx + 1;
      const isRefund = item.isRefund;
      tableHtml += `
        <tr class="${isRefund ? 'refund-row' : ''}">
          <td class="col-num text-center">${globalIdx}</td>
          ${showSku ? `<td class="col-sku text-mono">${item.sku || '-'}</td>` : ''}
          ${showBarcode ? `<td class="col-barcode text-mono">${item.barcode || '-'}</td>` : ''}
          <td class="col-designation">
            <span class="item-name">${isRefund ? '[RETOUR] ' : ''}${escapeHtml(item.name)}</span>
            ${item.notes ? `<span class="item-note">(${escapeHtml(item.notes)})</span>` : ''}
          </td>
          ${showQuantity ? `<td class="col-qty text-center font-bold">${item.quantity} ${item.unit || ''}</td>` : ''}
          ${showUnitPrice ? `<td class="col-price text-end">${formatMoney(item.unitPrice, currency)}</td>` : ''}
          ${showDiscount ? `<td class="col-discount text-end">${item.discountPerUnit ? formatMoney(item.discountPerUnit, currency) : '-'}</td>` : ''}
          ${showTax ? `<td class="col-tax text-center">${item.taxRate ? `${item.taxRate}%` : '-'}</td>` : ''}
          <td class="col-total text-end font-bold">${formatMoney(item.totalPrice, currency)}</td>
        </tr>
      `;
    });

    tableHtml += `
        </tbody>
      </table>
    `;

    // Bottom section: Totals & Payment (only on the last page)
    let bottomHtml = '';
    if (isLastPage) {
      bottomHtml = `
        <div class="bottom-section">
          <div class="bottom-left">
            ${showPayment && doc.payment ? `
              <div class="payment-box">
                <div class="payment-title">RÈGLEMENT & PAIEMENT</div>
                <div class="payment-row">
                  <span>Mode de règlement:</span>
                  <strong>${doc.payment.method || 'ESPECES'}</strong>
                </div>
                ${showAmountPaid && doc.payment.amountPaid !== undefined ? `
                  <div class="payment-row">
                    <span>Montant versé:</span>
                    <strong>${formatMoney(doc.payment.amountPaid, currency)}</strong>
                  </div>` : ''}
                ${doc.payment.change ? `
                  <div class="payment-row">
                    <span>Monnaie rendue:</span>
                    <strong>${formatMoney(doc.payment.change, currency)}</strong>
                  </div>` : ''}
                ${showRemaining && doc.payment.remainingDue !== undefined && doc.payment.remainingDue > 0 ? `
                  <div class="payment-row remaining-due">
                    <span>Reste à payer (Dette):</span>
                    <strong>${formatMoney(doc.payment.remainingDue, currency)}</strong>
                  </div>` : ''}
              </div>` : ''}

            ${showNotes && doc.notes ? `
              <div class="notes-box">
                <strong>Observations:</strong>
                <div>${escapeHtml(doc.notes)}</div>
              </div>` : ''}
          </div>

          <div class="totals-box">
            ${doc.discountTotal > 0 ? `
              <div class="total-row">
                <span>Sous-total:</span>
                <span>${formatMoney(doc.subtotal, currency)}</span>
              </div>
              <div class="total-row text-rose">
                <span>Remise totale:</span>
                <span>-${formatMoney(doc.discountTotal, currency)}</span>
              </div>` : ''}

            ${doc.taxTotal > 0 ? `
              <div class="total-row">
                <span>Total TVA:</span>
                <span>${formatMoney(doc.taxTotal, currency)}</span>
              </div>` : ''}

            <div class="grand-total-row">
              <span>NET À PAYER:</span>
              <span class="grand-total-amount">${formatMoney(doc.grandTotal, currency)}</span>
            </div>
          </div>
        </div>
      `;
    }

    // Page footer
    const footerText = settings['receipt_thank_you'] || 'Merci pour votre confiance !';
    const policyText = settings['receipt_footer'] || '';
    const footerHtml = `
      <div class="page-footer">
        <div class="footer-left">
          ${showFooter ? `<span>${footerText}</span> ${policyText ? `&bull; <span>${policyText}</span>` : ''}` : ''}
        </div>
        <div class="footer-right">
          <span>Page ${pageNum} / ${totalPages}</span>
        </div>
      </div>
    `;

    return `
      <div class="a4-page">
        <div class="page-content">
          ${headerHtml}
          ${tableHtml}
          ${bottomHtml}
        </div>
        ${footerHtml}
      </div>
    `;
  }).join('\n');

  return `<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8" />
  <title>${escapeHtml(doc.title || 'Facture A4')} - ${escapeHtml(doc.documentNumber)}</title>
  <style>
    @page {
      size: 210mm 297mm;
      margin: 0;
    }
    * {
      box-sizing: border-box;
      margin: 0;
      padding: 0;
      font-family: 'Segoe UI', -apple-system, BlinkMacSystemFont, Roboto, Helvetica, Arial, sans-serif;
      color: #1e293b;
      -webkit-print-color-adjust: exact;
      print-color-adjust: exact;
    }
    html, body {
      background: #ffffff;
      width: 210mm;
      margin: 0;
      padding: 0;
    }
    .a4-page {
      width: 210mm;
      height: 297mm;
      max-height: 297mm;
      padding: 15mm 15mm 12mm 15mm;
      position: relative;
      background: #ffffff;
      page-break-after: always;
      break-after: page;
      overflow: hidden;
      display: flex;
      flex-direction: column;
      justify-content: space-between;
    }
    .page-content {
      flex: 1;
      display: flex;
      flex-direction: column;
    }
    /* Header */
    .header-section {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      border-bottom: 2px solid #0284c7;
      padding-bottom: 12px;
      margin-bottom: 14px;
    }
    .store-info {
      display: flex;
      align-items: flex-start;
      gap: 14px;
      max-width: 60%;
    }
    .store-logo {
      max-width: 75px;
      max-height: 75px;
      object-fit: contain;
      border-radius: 6px;
    }
    .store-name {
      font-size: 20px;
      font-weight: 900;
      color: #0f172a;
      letter-spacing: -0.5px;
      line-height: 1.2;
      margin-bottom: 3px;
    }
    .store-detail {
      font-size: 11px;
      color: #475569;
      line-height: 1.4;
    }
    .store-tax-info {
      font-size: 10px;
      color: #64748b;
      margin-top: 4px;
      display: flex;
      flex-wrap: wrap;
      gap: 8px;
    }
    .doc-badge-box {
      text-align: right;
      min-width: 170px;
    }
    .doc-badge {
      display: inline-block;
      background: #0284c7;
      color: #ffffff;
      font-size: 14px;
      font-weight: 900;
      padding: 4px 14px;
      border-radius: 6px;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      margin-bottom: 8px;
    }
    .doc-meta-row {
      font-size: 11px;
      color: #334155;
      margin-bottom: 2px;
    }
    .copy-badge {
      display: inline-block;
      font-size: 10px;
      font-weight: 800;
      color: #b91c1c;
      border: 1px dashed #b91c1c;
      padding: 2px 8px;
      border-radius: 4px;
      margin-top: 4px;
      text-transform: uppercase;
    }
    /* Mini header on pages 2+ */
    .mini-header-section {
      display: flex;
      justify-content: space-between;
      align-items: center;
      border-bottom: 1px solid #cbd5e1;
      padding-bottom: 8px;
      margin-bottom: 12px;
    }
    .store-name-mini {
      font-size: 13px;
      font-weight: 800;
      color: #0f172a;
    }
    .doc-ref-mini {
      font-size: 12px;
      color: #64748b;
      font-weight: 600;
    }
    .page-counter-mini {
      font-size: 11px;
      font-weight: 700;
      color: #0284c7;
    }
    /* Party Card */
    .party-card {
      background: #f8fafc;
      border: 1px solid #e2e8f0;
      border-left: 4px solid #0284c7;
      border-radius: 6px;
      padding: 10px 14px;
      margin-bottom: 14px;
    }
    .party-title {
      font-size: 10px;
      font-weight: 800;
      color: #0284c7;
      text-transform: uppercase;
      letter-spacing: 0.5px;
      margin-bottom: 2px;
    }
    .party-name {
      font-size: 14px;
      font-weight: 800;
      color: #0f172a;
      margin-bottom: 3px;
    }
    .party-details {
      display: flex;
      flex-wrap: wrap;
      gap: 14px;
      font-size: 11px;
      color: #475569;
    }
    /* Items Table */
    .items-table {
      width: 100%;
      border-collapse: collapse;
      margin-bottom: 14px;
    }
    .items-table th {
      background: #f1f5f9;
      color: #334155;
      font-size: 11px;
      font-weight: 800;
      text-transform: uppercase;
      padding: 7px 6px;
      border-top: 1px solid #cbd5e1;
      border-bottom: 1px solid #cbd5e1;
      text-align: left;
    }
    .items-table td {
      padding: 6px;
      font-size: 11px;
      border-bottom: 1px solid #e2e8f0;
      color: #1e293b;
    }
    .items-table tbody tr:nth-child(even) {
      background: #fafafa;
    }
    .col-num { width: 30px; text-align: center; }
    .col-sku { width: 75px; }
    .col-barcode { width: 90px; }
    .col-designation { }
    .col-qty { width: 65px; }
    .col-price { width: 90px; }
    .col-discount { width: 75px; }
    .col-tax { width: 50px; }
    .col-total { width: 95px; }

    .item-name { font-weight: 700; color: #0f172a; }
    .item-note { font-size: 10px; color: #64748b; margin-left: 4px; }
    .refund-row td { color: #dc2626; background: #fef2f2 !important; }
    /* Bottom Section */
    .bottom-section {
      display: flex;
      justify-content: space-between;
      align-items: flex-start;
      gap: 16px;
      margin-top: auto;
      padding-top: 8px;
    }
    .bottom-left {
      flex: 1;
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .payment-box {
      background: #f8fafc;
      border: 1px solid #e2e8f0;
      border-radius: 6px;
      padding: 10px 12px;
      font-size: 11px;
    }
    .payment-title {
      font-size: 10px;
      font-weight: 800;
      color: #64748b;
      text-transform: uppercase;
      margin-bottom: 6px;
      border-bottom: 1px solid #e2e8f0;
      padding-bottom: 3px;
    }
    .payment-row {
      display: flex;
      justify-content: space-between;
      margin-bottom: 3px;
      color: #334155;
    }
    .remaining-due {
      color: #dc2626;
      font-weight: 800;
      border-top: 1px dashed #cbd5e1;
      padding-top: 3px;
      margin-top: 3px;
    }
    .notes-box {
      background: #fff;
      border: 1px dashed #cbd5e1;
      border-radius: 6px;
      padding: 8px 12px;
      font-size: 10px;
      color: #475569;
    }
    .totals-box {
      width: 260px;
      background: #f8fafc;
      border: 1px solid #cbd5e1;
      border-radius: 8px;
      padding: 12px 14px;
    }
    .total-row {
      display: flex;
      justify-content: space-between;
      font-size: 12px;
      margin-bottom: 6px;
      color: #475569;
    }
    .grand-total-row {
      display: flex;
      justify-content: space-between;
      align-items: center;
      border-top: 2px solid #0284c7;
      padding-top: 8px;
      margin-top: 8px;
      font-size: 13px;
      font-weight: 900;
      color: #0f172a;
    }
    .grand-total-amount {
      font-size: 16px;
      color: #0284c7;
    }
    /* Page Footer */
    .page-footer {
      border-top: 1px solid #e2e8f0;
      padding-top: 8px;
      display: flex;
      justify-content: space-between;
      align-items: center;
      font-size: 10px;
      color: #94a3b8;
    }
    /* Utility */
    .text-center { text-align: center; }
    .text-end { text-align: right; }
    .text-mono { font-family: Consolas, 'Courier New', monospace; font-size: 10px; }
    .font-bold { font-weight: 700; }
    .text-rose { color: #dc2626; }
  </style>
</head>
<body>
  ${renderedPages}
</body>
</html>`;
}

function escapeHtml(str: string): string {
  if (!str) return '';
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#039;');
}
