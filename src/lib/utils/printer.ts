// Native silent printing utility (v0.5.17).
//
// There is exactly ONE print path in the app: the backend rasterizes the
// document with the OS-bundled headless Edge (isolated profile) and
// GDI-prints it on a Windows printer DC with a DEVMODE locked to the exact
// page. No browser print, no dialogs, no PDF viewers, no iframe tricks —
// failures are RETURNED to the caller as a real error, never a dialog.
export interface PrintPaper {
  widthMm: number;
  heightMm?: number;
}

// ---------------------------------------------------------------------------
// Exact-media label printing (thermal label printers, e.g. Xprinter
// XP-DT427B). The backend rasterizes the label at the printer's DPI and GDI-
// prints N pages on a DEVMODE locked to width×height mm — the driver's
// default A4/continuous stock is never used, so copies come out consecutive
// with zero blank gaps and no print dialog.
// ---------------------------------------------------------------------------

export interface LabelPrintDiagnostics {
  printer: string;
  media_width_mm: number;
  media_height_mm: number;
  copies: number;
  print_width_mm: number;
  print_height_mm: number;
  page_count: number;
  dpi: number;
  raster_width_px: number;
  raster_height_px: number;
  mode: string;
}

export interface LabelPrintOutcome {
  ok: boolean;
  message: string;
  diagnostics: LabelPrintDiagnostics;
}

export async function printLabelSilently(options: {
  /** ONE label's inner HTML (it is repeated for every copy). */
  html: string;
  label: string;
  widthMm: number;
  heightMm: number;
  copies: number;
  printer?: string;
  dpi?: number;
}): Promise<LabelPrintOutcome> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<LabelPrintOutcome>('print_label_job', {
    request: {
      html: wrapFullDocument(options.html, {
        widthMm: options.widthMm,
        heightMm: options.heightMm,
      }),
      label: options.label,
      printer: options.printer || null,
      widthMm: options.widthMm,
      heightMm: options.heightMm,
      copies: options.copies,
      dpi: options.dpi ?? 203,
    },
  });
}

// ---------------------------------------------------------------------------
// Silent document/receipt printing (the only document path).
// ---------------------------------------------------------------------------

export interface SilentPrintResult {
  ok: boolean;
  message: string;
}

/**
 * Silent native printing — the ONLY print path in the app (v0.5.17).
 * `paper` pins the physical page: widthMm + optional heightMm (omit for
 * dynamic-height receipts — the backend measures the natural content
 * height, so long tickets are never clipped).
 */
export async function printHtmlSilently(
  htmlContent: string,
  title = 'Thermal Receipt',
  paper?: PrintPaper
): Promise<SilentPrintResult> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('print_html_direct', {
      html: wrapFullDocument(htmlContent, paper),
      title,
      paper: paper
        ? { width_mm: paper.widthMm, height_mm: paper.heightMm ?? null }
        : null,
    });
    return { ok: true, message: 'Sent to printer' };
  } catch (e: any) {
    const message = typeof e === 'string' ? e : e?.message || String(e);
    console.error('Silent print failed:', message);
    return { ok: false, message };
  }
}

function wrapFullDocument(htmlContent: string, paper?: PrintPaper): string {
  const widthMm = paper?.widthMm || 80;
  const heightMm = paper?.heightMm;
  // Dynamic height: the page rule pins the WIDTH only, so the backend's
  // measurement pass sees the ticket's natural height (never clipped).
  const pageRule = heightMm
    ? `@page { size: ${widthMm}mm ${heightMm}mm; margin: 0mm; }
       html, body { width: ${widthMm}mm; height: ${heightMm}mm; margin: 0; padding: 0; background: #fff; overflow: hidden; }`
    : `@page { size: ${widthMm}mm auto; margin: 0mm; }
       html, body { width: ${widthMm}mm; margin: 0; padding: 0; background: #fff; }`;

  return `<!DOCTYPE html><html><head><meta charset="utf-8" /><style>
    ${pageRule}
    * { box-sizing: border-box; margin: 0; padding: 0; font-family: 'Segoe UI', Tahoma, Geneva, Verdana, 'Noto Sans Arabic', Arial, sans-serif; color: #000; }
    .text-center { text-align: center; } .text-end { text-align: right; } .text-start { text-align: left; }
    .font-bold { font-weight: bold; } .font-black { font-weight: 900; }
    table { width: 100%; border-collapse: collapse; margin: 2px 0; }
    th { text-align: left; border-bottom: 1px dashed #000; font-size: 10px; padding: 1px 0; }
    td { padding: 1px 0; font-size: 10px; }
    .border-b-dashed { border-bottom: 1px dashed #000; } .border-t-dashed { border-top: 1px dashed #000; }
    .flex { display: flex; } .justify-between { justify-content: space-between; } .items-center { align-items: center; }
    .watermark {
      position: absolute; top: 40%; left: 50%; transform: translate(-50%, -50%) rotate(-30deg);
      font-size: 28px; font-weight: 900; color: rgba(0,0,0,0.18);
      border: 3px dashed rgba(0,0,0,0.25); padding: 6px 16px;
      pointer-events: none; text-transform: uppercase; letter-spacing: 2px;
    }
    .text-xxs { font-size: 9px; }
    .py-1 { padding-top: 3px; padding-bottom: 3px; }
    .py-2 { padding-top: 6px; padding-bottom: 6px; }
    .my-1 { margin-top: 3px; margin-bottom: 3px; }
    .space-y-1 > * + * { margin-top: 3px; }
    .mt-0\\.5 { margin-top: 2px; } .pt-2 { padding-top: 6px; } .pt-0\\.5 { padding-top: 2px; }
    .pb-2 { padding-bottom: 6px; } .pb-0\\.5 { padding-bottom: 2px; }
    .py-0\\.5 { padding-top: 2px; padding-bottom: 2px; } .py-3 { padding-top: 8px; padding-bottom: 8px; }
    .qr-box { width: 64px; height: 64px; margin: 6px auto 2px; }
    .bg-black { background: #000; } .text-white { color: #fff; } .px-1 { padding-left: 4px; padding-right: 4px; }
    .text-\\[8px\\] { font-size: 8px; } .text-gray-500 { color: #666; } .mt-1 { margin-top: 4px; }
    .font-mono { font-family: Consolas, 'Courier New', monospace; }
    .text-rose-600 { color: #dc2626; }
    .w-full { width: 100%; }
  </style></head><body>${htmlContent}</body></html>`;
}

/**
 * Offline-friendly entity QR payload. Same scheme the receipt uses, so a
 * scanner app (or the POS omni-search) maps a QR back to its record:
 *   SALE:POS-20260831..., PUR:ACH-..., EXP:EXP-..., CUST:CUST-001, EMP:EMP-01
 */
export function entityQrPayload(type: 'SALE' | 'PUR' | 'EXP' | 'CUST' | 'SUP' | 'EMP', code: string): string {
  return `${type}:${code}`;
}

// ---------------------------------------------------------------------------
// LOCAL QR GENERATION (offline — no api.qrserver.com round-trips).
// ---------------------------------------------------------------------------
import QRCode from 'qrcode';

let qrLibReady: typeof QRCode | null = null;

async function ensureQrLib(): Promise<typeof QRCode> {
  if (!qrLibReady) {
    qrLibReady = (await import('qrcode')).default as unknown as typeof QRCode;
  }
  return qrLibReady;
}

/**
 * Render a QR synchronously from cached SVG? qrcode is async — callers in
 * Svelte templates need a reactive wrapper instead. Use the dedicated
 * entityQrDataUrl store-friendly helper below.
 */
export async function entityQrDataUrl(payload: string, size = 120): Promise<string> {
  try {
    const lib = await ensureQrLib();
    return await lib.toDataURL(payload, {
      width: size,
      margin: 1,
      errorCorrectionLevel: 'M',
    });
  } catch {
    // No online fallback by design (offline-first POS): render nothing
    // rather than a broken image; callers show a placeholder instead.
    return '';
  }
}
