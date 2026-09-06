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
pub struct LabelPrintRequest {
    /// Complete HTML document for ONE label (repeated for every copy).
    pub html: String,
    pub printer: Option<String>,
    pub width_mm: f64,
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
/// rendered ticket, not a fixed A4/80mm form (which either clips long
/// tickets or pads short ones and feeds blank paper). This variant:
///   1. Rasterizes the full HTML at a low reference height first to MEASURE
///      the ticket's natural content height in CSS px,
///   2. re-rasterizes at the exact page height that content needs,
///   3. GDI-prints it on a DEVMODE locked to width × measured-height mm —
///      silent (direct DC), no Windows print dialog.
pub fn print_receipt_job(
    html: &str,
    width_mm: f64,
    printer: Option<&str>,
    dpi: u32,
    job_title: &str,
) -> LabelPrintResult {
    let dpi = dpi.clamp(96, 600);
    let scale = dpi as f64 / 96.0;

    // Pass 1: measure natural content height at the CSS-px scale.
    let css_w = ((width_mm * 96.0) / 25.4).round() as i32;
    let measured_css_h = match rasterize_html_measure(html, css_w) {
        Ok(h) => h,
        Err(e) => {
            return LabelPrintResult {
                ok: false,
                message: format!("Receipt measurement failed: {}", e),
                diagnostics: LabelPrintDiagnostics {
                    printer: printer.unwrap_or("Default printer").to_string(),
                    media_width_mm: width_mm,
                    media_height_mm: 0.0,
                    copies: 1,
                    print_width_mm: width_mm,
                    print_height_mm: 0.0,
                    page_count: 0,
                    dpi,
                    raster_width_px: 0,
                    raster_height_px: 0,
                    mode: "measure-failed".to_string(),
                },
            }
        }
    };

    // Pass 2: rasterize at the exact measured height (device px).
    let css_h = measured_css_h.max(1);
    let px_w = ((width_mm * dpi as f64) / 25.4).round() as i32;
    let px_h = (css_h as f64 * scale).round() as i32;
    let height_mm = (css_h as f64 * 25.4 / 96.0).round().max(10.0);

    let diag = LabelPrintDiagnostics {
        printer: printer.unwrap_or("Default printer").to_string(),
        media_width_mm: width_mm,
        media_height_mm: height_mm,
        copies: 1,
        print_width_mm: width_mm,
        print_height_mm: height_mm,
        page_count: 1,
        dpi,
        raster_width_px: px_w as u32,
        raster_height_px: px_h as u32,
        mode: String::new(),
    };
    let finish = |ok: bool, mode: &str, message: String| LabelPrintResult {
        ok,
        message,
        diagnostics: LabelPrintDiagnostics { mode: mode.to_string(), ..diag.clone() },
    };

    if px_w <= 0 || px_h <= 0 {
        return finish(false, "invalid-media", "Receipt content produced an empty page".into());
    }

    let png = match rasterize_html(html, px_w, px_h, dpi) {
        Ok(p) => p,
        Err(e) => return finish(false, "raster-failed", format!("Rasterization failed: {}", e)),
    };

    match gdi_print_pages(&png, width_mm, height_mm, 1, printer, job_title) {
        Ok(()) => finish(
            true,
            "gdi-dynamic-receipt",
            format!("Receipt printed silently: {}×{}mm ({} DPI)", width_mm, height_mm, dpi),
        ),
        Err(e) => finish(false, "gdi-failed", format!("GDI print failed: {}", e)),
    }
}

// ---------------------------------------------------------------------------
// HTML → PNG via headless Chromium screenshot (Edge or Chrome).
// ---------------------------------------------------------------------------

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

    let browser = find_browser()?;
    let url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));
    let screenshot_arg = format!("--screenshot={}", png_path.to_string_lossy());

    let output = std::process::Command::new(&browser)
        .args([
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--disable-extensions",
            "--disable-crash-reporter",
            "--no-pdf-header-footer",
            // CRITICAL: an isolated profile. Without it, a running Edge/Chrome
            // instance hijacks the launch — the flags are ignored, a visible
            // tab opens and no screenshot is ever produced.
            &format!("--user-data-dir={}", user_data.to_string_lossy()),
            &format!("--window-size={},{}", css_w, css_h),
            &format!("--force-device-scale-factor={}", scale),
            &screenshot_arg,
            &url,
        ])
        .output()
        .map_err(|e| format!("failed to launch browser: {}", e))?;
    let _ = std::fs::remove_file(&html_path);
    let _ = std::fs::remove_dir_all(&user_data);
    if !output.status.success() {
        let _ = std::fs::remove_file(&png_path);
        return Err("headless browser exited with an error".into());
    }
    if !png_path.exists() {
        return Err("headless browser produced no screenshot".into());
    }
    let png = std::fs::read(&png_path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&png_path);
    Ok(png)
}

fn find_browser() -> Result<std::path::PathBuf, String> {
    let candidates = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
        r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
    ];
    candidates
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.exists())
        .ok_or_else(|| "No Edge/Chrome found for label rasterization".to_string())
}

/// Measure the natural (dynamic) content height of an HTML document in CSS
/// pixels, at a viewport of `css_w` CSS px wide. Uses headless Chromium's
/// PDF engine with `@page { size: Wmm auto }`: the produced PDF's page
/// height IS the laid-out ticket height, which maps 1:1 back to CSS px.
fn rasterize_html_measure(html: &str, _css_w: i32) -> Result<i32, String> {
    let tmp = std::env::temp_dir();
    let stamp = chrono::Local::now().timestamp_subsec_nanos();
    let html_path = tmp.join(format!("titaou_measure_{}.html", stamp));
    let pdf_path = tmp.join(format!("titaou_measure_{}.pdf", stamp));
    let user_data = tmp.join(format!("titaou_profile_{}", stamp));
    let _ = std::fs::create_dir_all(&user_data);
    std::fs::write(&html_path, html).map_err(|e| e.to_string())?;

    let browser = find_browser()?;
    let url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));
    let pdf_arg = format!("--print-to-pdf={}", pdf_path.to_string_lossy());

    let output = std::process::Command::new(&browser)
        .args([
            "--headless",
            "--disable-gpu",
            "--no-first-run",
            "--disable-extensions",
            "--disable-crash-reporter",
            "--print-to-pdf-no-header",
            "--no-pdf-header-footer",
            // CRITICAL: isolated profile — a running Edge would otherwise
            // hijack the launch (visible tab, no PDF) and measurement fails.
            &format!("--user-data-dir={}", user_data.to_string_lossy()),
            &pdf_arg,
            &url,
        ])
        .output()
        .map_err(|e| format!("failed to launch browser: {}", e))?;
    let _ = std::fs::remove_file(&html_path);
    let _ = std::fs::remove_dir_all(&user_data);
    if !output.status.success() || !pdf_path.exists() {
        let _ = std::fs::remove_file(&pdf_path);
        return Err("measurement render failed".into());
    }

    // PDF pages: width in points at index 3..5, height at 5..7 of each
    // /MediaBox [x0 y0 x1 y1]. One page is expected (auto height, no breaks).
    let pdf = std::fs::read(&pdf_path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&pdf_path);

    let text = String::from_utf8_lossy(&pdf).to_string();
    // Find the LAST /MediaBox — trailer pages reuse the same object; the
    // final one carries the page actually laid out. Values are like
    // "226.77 0 226.77 1417.32".
    let mut height_pt: f64 = 0.0;
    if let Some(pos) = text.rfind("/MediaBox") {
        let raw_slice: String = text[pos..(pos + 120).min(text.len())].to_string();
        let slice: String = raw_slice
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == ' ' || *c == '.' || *c == '-' || *c == '[' || *c == ']')
            .collect();
        let nums: Vec<f64> = slice
            .split(|c: char| c == '[' || c == ']' || c.is_whitespace())
            .filter_map(|t| t.parse::<f64>().ok())
            .collect();
        if nums.len() >= 4 {
            height_pt = nums[3] - nums[1];
        }
    }
    if height_pt <= 0.0 {
        return Err("could not read page height from PDF".into());
    }

    // PDF points (1/72 inch) → CSS px (1/96 inch).
    let css_h = (height_pt * 96.0 / 72.0).ceil() as i32;
    Ok(css_h.max(1))
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

    pub fn print_pages(
        png: &[u8],
        width_mm: f64,
        height_mm: f64,
        copies: u32,
        printer_name: Option<&str>,
        job_title: &str,
    ) -> Result<(), String> {
        // Decode PNG into BGRA top-down pixels.
        let (bgra, img_w, img_h) = decode_png_bgra(png)?;

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
                draw_label_page(hdc, &bgra, img_w, img_h);
                if EndPage(hdc) <= 0 {
                    failed = true;
                    break;
                }
            }

            if failed {
                // EndDoc flushes what GDI will accept; the spooler drops the rest.
                EndDoc(hdc);
                DeleteDC(hdc);
                return Err("a label page failed mid-job (check media/printer state)".into());
            }
            if EndDoc(hdc) <= 0 {
                DeleteDC(hdc);
                return Err("EndDoc failed".into());
            }
            DeleteDC(hdc);
            Ok(())
        }
    }

    /// Blit one label bitmap onto the current page, 1:1 with the page pixels:
    /// the DEVMODE media (40×20mm) equals the raster size, so no scaling.
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

        // Patch: custom paper form = exact label media.
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
            return Err("driver rejected the custom 40x20mm media".into());
        }
        ClosePrinter(hprinter);

        // Create the DC with "WINSPOOL" — the spooler routes by device name.
        let driver = utf16("WINSPOOL");
        let hdc = CreateDCW(driver.as_ptr(), name16.as_ptr(), std::ptr::null(), dm_ptr as *const _);
        if hdc.is_null() {
            return Err("CreateDCW failed for the label printer".into());
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

    /// Minimal PNG decode to BGRA (any bit depth the `png` crate outputs as
    /// 8-bit RGBA after transformations; palette/gray are expanded).
    fn decode_png_bgra(png_bytes: &[u8]) -> Result<(Vec<u8>, i32, i32), String> {
        let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
        let mut reader = decoder
            .read_info()
            .map_err(|e| format!("png parse: {}", e))?;
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader
            .next_frame(&mut buf)
            .map_err(|e| format!("png decode: {}", e))?;
        let w = info.width as i32;
        let h = info.height as i32;
        // Chrome screenshots are RGBA8; convert to the BGRA GDI expects.
        let mut bgra = Vec::with_capacity(buf.len());
        for px in buf.chunks_exact(4) {
            bgra.push(px[2]); // B
            bgra.push(px[1]); // G
            bgra.push(px[0]); // R
            bgra.push(255); // opaque: thermal media has no alpha
        }
        Ok((bgra, w, h))
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
    gdi::print_pages(png, width_mm, height_mm, copies, printer_name, job_title)
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
