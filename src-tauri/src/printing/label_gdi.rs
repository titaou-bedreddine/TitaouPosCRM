//! Silent thermal-label printing with an EXACT custom media size.
//!
//! HTML `@page` hints only reach the browser's own print pipeline — the
//! Windows printer driver still paginates at its default stock (A4/Letter
//! or a big continuous roll). That is why multi-label jobs printed one
//! label per huge page with giant blank gaps.
//!
//! This pipeline instead:
//!   1. Rasterizes the HTML label to a PNG at the printer's real DPI with
//!      headless Edge/Chrome (`--screenshot`), sized exactly to the label
//!      media in device pixels.
//!   2. Prints that bitmap N times through GDI (StartDocW/StartPage/BitBlt)
//!      on a printer DC whose DEVMODE explicitly sets a custom paper form
//!      (dmPaperSize = DMPAPER_USER, dmPaperWidth/dmPaperLength in 0.1mm).
//!      Each page consumes exactly one 40×20mm label; the next page feeds
//!      the next label — zero blank pages, zero gaps.
//!   3. Prints silently: we drive the DC directly, so no Windows print
//!      dialog is ever shown.
//!
//! The Xprinter XP-DT427B driver accepts DMPAPER_USER custom forms and maps
//! them onto the loaded 40×20 stock.

use serde::{Deserialize, Serialize};

/// DEVMODE paper dimensions are specified in 0.1 mm units.
const MM_TO_TENTHS: f64 = 10.0;

#[derive(Debug, Clone, Serialize)]
pub struct LabelPrintDiagnostics {
    pub printer: String,
    pub media_width_mm: f64,
    pub media_height_mm: f64,
    pub copies: u32,
    pub print_width_mm: f64,
    pub print_height_mm: f64,
    pub page_count: u32,
    pub dpi: u32,
    pub raster_width_px: u32,
    pub raster_height_px: u32,
    pub mode: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelPrintRequest {
    /// Complete HTML document for ONE label (repeated for every copy).
    pub html: String,
    pub printer: Option<String>,
    #[serde(alias = "width_mm")]
    pub width_mm: f64,
    #[serde(alias = "height_mm")]
    pub height_mm: f64,
    pub copies: u32,
    pub dpi: Option<u32>,
    /// Human-readable preset name for the spooler job title.
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct LabelPrintResult {
    pub ok: bool,
    pub message: String,
    pub diagnostics: LabelPrintDiagnostics,
}

/// Print `copies` × (width × height) mm labels silently with exact media.
pub fn print_label_job(req: &LabelPrintRequest) -> LabelPrintResult {    let copies = req.copies.max(1);
    let dpi = req.dpi.unwrap_or(203).clamp(96, 600);

    let px_w = ((req.width_mm * dpi as f64) / 25.4).round() as i32;
    let px_h = ((req.height_mm * dpi as f64) / 25.4).round() as i32;

    let printer = match req.printer.as_deref() {
        Some(n) if !n.trim().is_empty() => n.to_string(),
        _ => "Default printer".to_string(),
    };

    let diagnostics = LabelPrintDiagnostics {
        printer,
        media_width_mm: req.width_mm,
        media_height_mm: req.height_mm,
        copies,
        print_width_mm: req.width_mm,
        print_height_mm: copies as f64 * req.height_mm,
        page_count: copies,
        dpi,
        raster_width_px: px_w as u32,
        raster_height_px: px_h as u32,
        mode: String::new(),
    };

    let finish = |ok: bool, mode: &str, message: String| LabelPrintResult {
        ok,
        message,
        diagnostics: LabelPrintDiagnostics {
            mode: mode.to_string(),
            ..diagnostics.clone()
        },
    };

    if px_w <= 0 || px_h <= 0 {
        return finish(false, "invalid-media", "Media size too small to rasterize".into());
    }

    // 1. Rasterize one label at the printer's DPI.
    let png = match rasterize_html(&req.html, px_w, px_h, dpi) {
        Ok(p) => p,
        Err(e) => return finish(false, "raster-failed", format!("Rasterization failed: {}", e)),
    };

    // 2. GDI print: one page per copy, custom DEVMODE media, silent.
    match gdi_print_pages(&png, req.width_mm, req.height_mm, copies, req.printer.as_deref(), &req.label) {
        Ok(()) => finish(
            true,
            "gdi-custom-media",
            format!(
                "Printed {} label(s) of exactly {}×{}mm ({} page(s), {} DPI)",
                copies, req.width_mm, req.height_mm, copies, dpi
            ),
        ),
        Err(e) => finish(false, "gdi-failed", format!("GDI print failed: {}", e)),
    }
}

/// Silent thermal-RECEIPT printing with a DYNAMIC page height.
///
/// Receipts are continuous-roll: the page must be exactly as tall as the
/// rendered ticket. Single-pass pipeline (v0.5.18 — the previous PDF-based
/// measurement step failed silently on machines where headless Edge is
/// broken):
///   1. ONE headless screenshot at the receipt width and a generous height,
///   2. decode the PNG and TRIM the trailing all-white rows to the natural
///      content height,
///   3. GDI-print the cropped bitmap on a DEVMODE locked to
///      width × content-height mm — silent, no dialogs, no PDF.
pub fn print_receipt_job(
    html: &str,
    width_mm: f64,
    printer: Option<&str>,
    dpi: u32,
    job_title: &str,
) -> LabelPrintResult {
    let dpi = dpi.clamp(96, 600);
    const MAX_HEIGHT_MM: f64 = 300.0;

    let px_w = ((width_mm * dpi as f64) / 25.4).round() as i32;
    let px_h = ((MAX_HEIGHT_MM * dpi as f64) / 25.4).round() as i32;

    let diag_base = LabelPrintDiagnostics {
        printer: printer.unwrap_or("Default printer").to_string(),
        media_width_mm: width_mm,
        media_height_mm: 0.0,
        copies: 1,
        print_width_mm: width_mm,
        print_height_mm: 0.0,
        page_count: 0,
        dpi,
        raster_width_px: px_w as u32,
        raster_height_px: 0,
        mode: String::new(),
    };
    let finish = |ok: bool, mode: &str, message: String, height_mm: f64, raster_h: i32| LabelPrintResult {
        ok,
        message,
        diagnostics: LabelPrintDiagnostics {
            mode: mode.to_string(),
            media_height_mm: height_mm,
            print_height_mm: height_mm,
            page_count: if ok { 1 } else { 0 },
            raster_height_px: raster_h as u32,
            ..diag_base.clone()
        },
    };

    if px_w <= 0 || px_h <= 0 {
        return finish(false, "invalid-media", "Receipt width too small to rasterize".into(), 0.0, 0);
    }

    // 1. Single screenshot at a generous page height.
    let png = match rasterize_html(html, px_w, px_h, dpi) {
        Ok(p) => p,
        Err(e) => return finish(false, "raster-failed", format!("Receipt rasterization failed: {}", e), 0.0, 0),
    };

    // 2. Decode + trim trailing all-white rows to the natural content height.
    let (bgra, img_w, img_h) = match decode_png_bgra_any(&png) {
        Ok(v) => v,
        Err(e) => return finish(false, "decode-failed", format!("PNG decode failed: {}", e), 0.0, 0),
    };
    let content_rows = last_non_white_row(&bgra, img_w, img_h).unwrap_or(img_h);
    // Small bottom padding so descenders/borders are never clipped.
    let content_px = (content_rows + 8).min(img_h);
    let height_mm = ((content_px as f64) * 25.4 / dpi as f64).ceil().max(10.0);

    // CRASH FIX (v0.5.19): the DIB is sized img_w × content_px but `bgra`
    // still holds the FULL generous-height page — copy_nonoverlapping copied
    // the whole oversized buffer into the smaller DIB = heap corruption =
    // the process died on checkout with auto-print ON. Crop the slice.
    let stride = (img_w as usize) * 4;
    let cropped = &bgra[..stride * content_px as usize];

    // 3. GDI print ONE page of exactly width × height mm.
    match gdi_print_bgra_pages(cropped, img_w, content_px, width_mm, height_mm, 1, printer, job_title) {
        Ok(()) => finish(
            true,
            "gdi-dynamic-receipt",
            format!("Receipt printed silently: {}×{}mm ({} DPI)", width_mm, height_mm, dpi),
            height_mm,
            content_px,
        ),
        Err(e) => finish(false, "gdi-failed", format!("GDI print failed: {}", e), height_mm, content_px),
    }
}

// ---------------------------------------------------------------------------
// HTML → PNG via headless Chromium screenshot.
//
// BROWSER ORDER MATTERS (v0.5.18): headless Edge 153 is broken on some
// machines (exits 0, produces NOTHING — verified in the field), so a working
// Chrome is tried FIRST and Edge is only the fallback. Each candidate is
// actually attempted: the first browser that produces output wins, and the
// combined stderr/stdout of every failure is surfaced in the error so a
// fully-broken machine is diagnosable instead of silent.
// ---------------------------------------------------------------------------

/// Headless-capable browser candidates, best-first.
fn browser_candidates() -> Vec<std::path::PathBuf> {
    [
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
    ]
    .iter()
    .map(std::path::PathBuf::from)
    .filter(|p| p.exists())
    .collect()
}

fn run_headless(browser: &std::path::Path, args: &[String]) -> Result<std::process::Output, String> {
    std::process::Command::new(browser)
        .args(args)
        .output()
        .map_err(|e| format!("failed to launch {}: {}", browser.display(), e))
}

fn headless_base_args(user_data: &std::path::Path) -> Vec<String> {
    vec![
        "--headless".to_string(),
        "--disable-gpu".to_string(),
        "--no-first-run".to_string(),
        "--disable-extensions".to_string(),
        "--disable-crash-reporter".to_string(),
        // CRITICAL: an isolated profile. Without it, a running Edge/Chrome
        // instance hijacks the launch — the flags are ignored, a visible
        // tab opens and no output is ever produced.
        format!("--user-data-dir={}", user_data.to_string_lossy()),
    ]
}

/// Screenshot `html` at exactly `px_w` × `px_h` device pixels.
fn rasterize_html(html: &str, px_w: i32, px_h: i32, dpi: u32) -> Result<Vec<u8>, String> {
    let tmp = std::env::temp_dir();
    let stamp = chrono::Local::now().timestamp_subsec_nanos();
    let html_path = tmp.join(format!("titaou_label_{}.html", stamp));
    let png_path = tmp.join(format!("titaou_label_{}.png", stamp));
    let user_data = tmp.join(format!("titaou_profile_{}", stamp));
    let _ = std::fs::create_dir_all(&user_data);
    std::fs::write(&html_path, html).map_err(|e| e.to_string())?;

    // The label HTML sizes everything in CSS mm (96 dpi reference). A device
    // pixel at `dpi` = scale × CSS px, so the viewport is raster/scale and
    // --force-device-scale-factor renders each CSS px as `scale` device px.
    let scale = dpi as f64 / 96.0;
    let css_w = (px_w as f64 / scale).round() as i32;
    let css_h = (px_h as f64 / scale).round() as i32;

    let url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));
    let mut args = headless_base_args(&user_data);
    args.push("--hide-scrollbars".to_string());
    args.push("--default-background-color=FFFFFFFF".to_string());
    args.push(format!("--window-size={},{}", css_w, css_h));
    args.push(format!("--force-device-scale-factor={}", scale));
    args.push(format!("--screenshot={}", png_path.to_string_lossy()));
    args.push(url);

    let browsers = browser_candidates();
    if browsers.is_empty() {
        let _ = std::fs::remove_file(&html_path);
        let _ = std::fs::remove_dir_all(&user_data);
        return Err("No Chrome/Edge installation found for rasterization".into());
    }

    let mut failures: Vec<String> = Vec::new();
    let mut result: Result<Vec<u8>, String> = Err("no browser attempted".into());
    for browser in &browsers {
        match run_headless(browser, &args) {
            Ok(output) => {
                let log = format!(
                    "{} stdout: {} | stderr: {}",
                    browser.display(),
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim(),
                );
                if png_path.exists() {
                    match std::fs::read(&png_path) {
                        Ok(png) => {
                            let _ = std::fs::remove_file(&html_path);
                            let _ = std::fs::remove_file(&png_path);
                            let _ = std::fs::remove_dir_all(&user_data);
                            result = Ok(png);
                            break;
                        }
                        Err(e) => failures.push(format!("{}: png read failed: {}", browser.display(), e)),
                    }
                } else if !output.status.success() {
                    failures.push(format!("{}: exit {:?} — {}", browser.display(), output.status.code(), log));
                } else {
                    failures.push(format!("{}: exited ok but produced NO screenshot — {}", browser.display(), log));
                }
            }
            Err(e) => failures.push(e),
        }
    }
    let _ = std::fs::remove_file(&html_path);
    let _ = std::fs::remove_file(&png_path);
    let _ = std::fs::remove_dir_all(&user_data);

    match result {
        Ok(png) => Ok(png),
        Err(_) => Err(format!(
            "all headless browsers failed: {}",
            failures.join(" || ")
        )),
    }
}

// ---------------------------------------------------------------------------
// PNG decode + white-trim helpers (shared by labels and receipts).
// ---------------------------------------------------------------------------

/// Decode a PNG into a BGRA top-down buffer (alpha forced opaque — thermal
/// media has no transparency). Returns (bgra, width, height).
///
/// CRITICAL: Chrome/Edge screenshots may be RGBA8 **or RGB24** (3 bytes/px)
/// depending on version/background. The previous decoder assumed 4 bytes/px
/// unconditionally — RGB frames produced a short buffer (index panics) and
/// swapped colors on print. Expand + convert per actual color type.
fn decode_png_bgra_any(png_bytes: &[u8]) -> Result<(Vec<u8>, i32, i32), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    // Expand palettes/grayscale to 8-bit RGB(A) so only 4 shapes remain.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("png parse: {}", e))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("png decode: {}", e))?;
    let w = info.width as i32;
    let h = info.height as i32;
    let (color_type, _depth) = reader.output_color_type();
    let channels = match color_type {
        png::ColorType::Grayscale => 1usize,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err("unexpected indexed PNG after EXPAND".into());
        }
    };
    let px_count = (w as usize) * (h as usize);
    if buf.len() < px_count * channels {
        return Err(format!(
            "png buffer too short: {} bytes for {}×{} × {}ch",
            buf.len(), w, h, channels
        ));
    }
    // Convert to BGRA (the layout GDI DIBs expect).
    let mut bgra = Vec::with_capacity(px_count * 4);
    for px in buf[..px_count * channels].chunks_exact(channels) {
        let (r, g, b) = match channels {
            1 => (px[0], px[0], px[0]),
            2 => (px[0], px[0], px[0]), // gray + alpha, alpha dropped
            3 => (px[0], px[1], px[2]),
            _ => (px[0], px[1], px[2]),
        };
        bgra.push(b);
        bgra.push(g);
        bgra.push(r);
        bgra.push(255); // opaque: thermal media has no alpha
    }
    Ok((bgra, w, h))
}

/// Index of the LAST row (0-based) containing any non-near-white pixel,
/// i.e. the natural content height of a screenshot. Returns None when the
/// whole image is blank (caller falls back to the full height).
fn last_non_white_row(bgra: &[u8], w: i32, h: i32) -> Option<i32> {
    const WHITE_THRESHOLD: u8 = 250;
    let stride = (w * 4) as usize;
    for row in (0..h).rev() {
        let row_start = row as usize * stride;
        let row_end = row_start + stride;
        let mut non_white = false;
        for px in bgra[row_start..row_end].chunks_exact(4) {
            if px[0] < WHITE_THRESHOLD || px[1] < WHITE_THRESHOLD || px[2] < WHITE_THRESHOLD {
                non_white = true;
                break;
            }
        }
        if non_white {
            return Some(row);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// GDI printing with a custom DEVMODE media size (Windows only).
// ---------------------------------------------------------------------------

#[cfg(windows)]
mod gdi {
    use super::*;
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleDC, CreateDCW, CreateDIBSection, DeleteDC, DeleteObject,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, SRCCOPY,
    };
    use windows_sys::Win32::Graphics::Printing::{
        ClosePrinter, DocumentPropertiesW, GetDefaultPrinterW, OpenPrinterW, PRINTER_ACCESS_USE,
        PRINTER_DEFAULTSW,
    };
    use windows_sys::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};

    const DMPAPER_USER: i16 = 256;
    const DMORIENT_PORTRAIT: i16 = 1;
    const DM_OUT_BUFFER: u32 = 2;
    const DM_IN_BUFFER: u32 = 8;
    const DM_ORIENTATION: u32 = 0x1;
    const DM_PAPERSIZE: u32 = 0x2;
    const DM_PAPERLENGTH: u32 = 0x4;
    const DM_PAPERWIDTH: u32 = 0x8;

    /// Print a decoded BGRA bitmap as `copies` pages of exactly
    /// width×height mm — silent, direct printer DC, no dialog.
    pub fn print_bgra_pages(
        bgra: &[u8],
        img_w: i32,
        img_h: i32,
        width_mm: f64,
        height_mm: f64,
        copies: u32,
        printer_name: Option<&str>,
        job_title: &str,
    ) -> Result<(), String> {
        unsafe {
            let hdc = acquire_label_dc(printer_name, width_mm, height_mm)?;
            if hdc.is_null() {
                return Err("could not create a printer device context".into());
            }

            let doc_name = utf16(job_title);
            let doc = DOCINFOW {
                cbSize: std::mem::size_of::<DOCINFOW>() as i32,
                lpszDocName: doc_name.as_ptr(),
                lpszOutput: std::ptr::null(),
                lpszDatatype: std::ptr::null(),
                fwType: 0,
            };
            if StartDocW(hdc, &doc) <= 0 {
                DeleteDC(hdc);
                return Err("StartDocW failed (printer refused the job)".into());
            }

            let mut failed = false;
            for _ in 0..copies {
                if StartPage(hdc) <= 0 {
                    failed = true;
                    break;
                }
                draw_label_page(hdc, bgra, img_w, img_h);
                if EndPage(hdc) <= 0 {
                    failed = true;
                    break;
                }
            }

            if failed {
                // EndDoc flushes what GDI will accept; the spooler drops the rest.
                EndDoc(hdc);
                DeleteDC(hdc);
                return Err("a page failed mid-job (check media/printer state)".into());
            }
            if EndDoc(hdc) <= 0 {
                DeleteDC(hdc);
                return Err("EndDoc failed".into());
            }
            DeleteDC(hdc);
            Ok(())
        }
    }

    /// Blit one bitmap onto the current page, 1:1 with the page pixels:
    /// the DEVMODE media equals the raster size, so no scaling.
    unsafe fn draw_label_page(hdc: HDC, bgra: &[u8], img_w: i32, img_h: i32) {
        let mem_dc = CreateCompatibleDC(hdc);
        if mem_dc.is_null() {
            return;
        }
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = img_w;
        bmi.bmiHeader.biHeight = -img_h; // top-down rows
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB;

        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(
            hdc,
            &bmi,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        if !dib.is_null() && !bits.is_null() {
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len());
            let old = SelectObject(mem_dc, dib);
            let _ = BitBlt(hdc, 0, 0, img_w, img_h, mem_dc, 0, 0, SRCCOPY);
            SelectObject(mem_dc, old);
        }
        if !dib.is_null() {
            DeleteObject(dib);
        }
        DeleteDC(mem_dc);
    }

    /// Create a printer DC whose DEVMODE carries a custom paper form of
    /// exactly width×height mm. The driver's default DEVMODE is fetched via
    /// DocumentPropertiesW, patched (DMPAPER_USER + dims), validated by the
    /// driver again, then handed to CreateDCW. No print dialog is involved.
    unsafe fn acquire_label_dc(
        printer_name: Option<&str>,
        width_mm: f64,
        height_mm: f64,
    ) -> Result<HDC, String> {
        let name = match printer_name {
            Some(n) if !n.trim().is_empty() => n.to_string(),
            _ => default_printer_name()?,
        };
        let name16 = utf16(&name);

        // Open the printer to query its driver defaults.
        let mut defaults: PRINTER_DEFAULTSW = std::mem::zeroed();
        defaults.DesiredAccess = PRINTER_ACCESS_USE;
        let mut hprinter: HANDLE = std::ptr::null_mut();
        if OpenPrinterW(name16.as_ptr(), &mut hprinter, &defaults) == 0 {
            return Err(format!("cannot open printer \"{}\"", name));
        }

        // Size probe for the driver's private DEVMODE storage.
        let needed = DocumentPropertiesW(
            std::ptr::null_mut(),   // no owner window → never pops UI
            hprinter,
            name16.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null(),
            DM_OUT_BUFFER,
        );
        if needed <= 0 {
            ClosePrinter(hprinter);
            return Err("DocumentPropertiesW size probe failed".into());
        }

        // Allocate DEVMODE + driver-private area, read the current settings.
        let mut dm_buf: Vec<u8> = vec![0u8; needed as usize];
        let dm_ptr = dm_buf.as_mut_ptr() as *mut windows_sys::Win32::Graphics::Gdi::DEVMODEW;
        if DocumentPropertiesW(
            std::ptr::null_mut(),
            hprinter,
            name16.as_ptr(),
            dm_ptr,
            std::ptr::null(),
            DM_OUT_BUFFER,
        ) <= 0
        {
            ClosePrinter(hprinter);
            return Err("could not read printer DEVMODE".into());
        }

        // Patch: custom paper form = exact media.
        (*dm_ptr).dmFields |= DM_ORIENTATION | DM_PAPERSIZE | DM_PAPERLENGTH | DM_PAPERWIDTH;
        (*dm_ptr).Anonymous1.Anonymous1.dmOrientation = DMORIENT_PORTRAIT;
        (*dm_ptr).Anonymous1.Anonymous1.dmPaperSize = DMPAPER_USER;
        (*dm_ptr).Anonymous1.Anonymous1.dmPaperWidth = (width_mm * MM_TO_TENTHS).round() as i16;
        (*dm_ptr).Anonymous1.Anonymous1.dmPaperLength = (height_mm * MM_TO_TENTHS).round() as i16;

        // Ask the driver to validate the patched DEVMODE (merges private data).
        if DocumentPropertiesW(
            std::ptr::null_mut(),
            hprinter,
            name16.as_ptr(),
            dm_ptr,
            dm_ptr,
            DM_IN_BUFFER | DM_OUT_BUFFER,
        ) <= 0
        {
            ClosePrinter(hprinter);
            return Err("driver rejected the custom media size".into());
        }
        ClosePrinter(hprinter);

        // Create the DC with "WINSPOOL" — the spooler routes by device name.
        let driver = utf16("WINSPOOL");
        let hdc = CreateDCW(driver.as_ptr(), name16.as_ptr(), std::ptr::null(), dm_ptr as *const _);
        if hdc.is_null() {
            return Err("CreateDCW failed for the printer".into());
        }
        Ok(hdc)
    }

    fn default_printer_name() -> Result<String, String> {
        let mut len: u32 = 0;
        unsafe {
            let _ = GetDefaultPrinterW(std::ptr::null_mut(), &mut len);
            if len == 0 {
                return Err("no default printer is configured".into());
            }
            let mut buf = vec![0u16; len as usize];
            if GetDefaultPrinterW(buf.as_mut_ptr(), &mut len) == 0 {
                return Err("GetDefaultPrinterW failed".into());
            }
            Ok(utf16_to_string(&buf))
        }
    }

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn utf16_to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }
}

#[cfg(windows)]
fn gdi_print_pages(
    png: &[u8],
    width_mm: f64,
    height_mm: f64,
    copies: u32,
    printer_name: Option<&str>,
    job_title: &str,
) -> Result<(), String> {
    let (bgra, w, h) = decode_png_bgra_any(png)?;
    gdi::print_bgra_pages(&bgra, w, h, width_mm, height_mm, copies, printer_name, job_title)
}

#[cfg(windows)]
fn gdi_print_bgra_pages(
    bgra: &[u8],
    img_w: i32,
    img_h: i32,
    width_mm: f64,
    height_mm: f64,
    copies: u32,
    printer_name: Option<&str>,
    job_title: &str,
) -> Result<(), String> {
    gdi::print_bgra_pages(bgra, img_w, img_h, width_mm, height_mm, copies, printer_name, job_title)
}

#[cfg(not(windows))]
fn gdi_print_pages(
    _png: &[u8],
    _width_mm: f64,
    _height_mm: f64,
    _copies: u32,
    _printer_name: Option<&str>,
    _job_title: &str,
) -> Result<(), String> {
    Err("label GDI printing is Windows-only".into())
}

#[cfg(not(windows))]
fn gdi_print_bgra_pages(
    _bgra: &[u8],
    _img_w: i32,
    _img_h: i32,
    _width_mm: f64,
    _height_mm: f64,
    _copies: u32,
    _printer_name: Option<&str>,
    _job_title: &str,
) -> Result<(), String> {
    Err("label GDI printing is Windows-only".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // The v0.5.17 regression: the frontend sends camelCase (`widthMm`) while
    // the struct had snake_case fields — "invalid args `request` for command
    // `print_label_job`: missing field `width_mm`" and label printing never
    // reached the printer. camelCase (and the snake_case alias) must both parse.
    #[test]
    fn label_request_accepts_camel_case_and_snake_case() {
        let camel: LabelPrintRequest = serde_json::from_str(
            r#"{"html":"<b>x</b>","printer":null,"widthMm":40.0,"heightMm":20.0,"copies":2,"dpi":203,"label":"T"}"#,
        )
        .expect("camelCase payload must deserialize");
        assert_eq!(camel.width_mm, 40.0);
        assert_eq!(camel.height_mm, 20.0);
        assert_eq!(camel.copies, 2);

        let snake: LabelPrintRequest = serde_json::from_str(
            r#"{"html":"<b>x</b>","printer":null,"width_mm":40.0,"height_mm":20.0,"copies":1,"dpi":203,"label":"T"}"#,
        )
        .expect("snake_case alias must deserialize");
        assert_eq!(snake.width_mm, 40.0);
    }

    // Receipt trimming: a mostly-white page with content at the top must
    // trim to the content height (+ padding), not the full page.
    #[test]
    fn last_non_white_row_trims_trailing_whitespace() {
        let w = 8usize;
        let h = 100usize;
        let mut bgra = vec![255u8; w * h * 4];
        // Row 40: one dark pixel.
        let row40 = 40 * w * 4;
        bgra[row40] = 0;
        assert_eq!(last_non_white_row(&bgra, w as i32, h as i32), Some(40));
        // All white → None.
        let all_white = vec![255u8; w * h * 4];
        assert_eq!(last_non_white_row(&all_white, w as i32, h as i32), None);
        // Content on the very last row → Some(h-1).
        let last = (h - 1) * w * 4;
        bgra[last] = 0;
        assert_eq!(last_non_white_row(&bgra, w as i32, h as i32), Some((h - 1) as i32));
    }

    // Rasterization must actually WORK on this machine: catches a broken
    // headless browser (e.g. Edge 153 producing nothing) regardless of which
    // browser is tried first.
    #[test]
    fn rasterize_produces_a_real_screenshot() {
        let html = "<!DOCTYPE html><html><body style='background:#fff;margin:0'><div style='width:50px;height:50px;background:#000'></div></body></html>";
        let png = rasterize_html(html, 100, 100, 96).expect("headless rasterization must succeed");
        let (bgra, w, h) = decode_png_bgra_any(&png).expect("png must decode");
        assert_eq!((w, h), (100, 100));
        // The 50px black square sits at the top of an otherwise-white page:
        // content ends at row 49. Proves RGB24 and RGBA8 frames both decode.
        assert_eq!(last_non_white_row(&bgra, w, h), Some(49));
    }
}
