//! Native Windows GDI A4 multi-page document printing (v1.0.1).
//!
//! Provides silent, browser-dialog-free printing to any standard Windows system printer
//! (HP LaserJet, Canon, Brother, Epson, PDF printer, etc.) using DMPAPER_A4.
//!
//! Architecture:
//! 1. Open the target printer DC and read its REAL printable geometry
//!    (HORZRES/VERTRES device pixels at LOGPIXELSX/Y — e.g. 600 dpi laser).
//! 2. Rasterize each A4 HTML page at that exact device geometry using headless
//!    Chrome/Edge (one screenshot per page — never one giant surface, so long
//!    documents cannot hit browser raster limits).
//! 3. GDI print loop: StartDocW -> for each page (StartPage -> StretchBlt to the
//!    FULL printable area -> EndPage) -> EndDocW. The stretch is what keeps the
//!    document edge-to-edge on ANY printer resolution — a raw 1:1 BitBlt of a
//!    300 dpi raster onto a 600 dpi DC fills only a quarter of the sheet.
//! 4. Validates printer availability and returns structured, actionable errors.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct A4PrintRequest {
    pub html: String,
    pub title: String,
    pub printer: Option<String>,
    pub dpi: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct A4PrintDiagnostics {
    pub printer: String,
    pub page_count: u32,
    pub dpi: u32,
    pub width_mm: f64,
    pub height_mm: f64,
    pub raster_width_px: u32,
    pub raster_height_px: u32,
}

#[derive(Debug, Serialize)]
pub struct A4PrintResult {
    pub ok: bool,
    pub message: String,
    pub diagnostics: Option<A4PrintDiagnostics>,
}

const A4_WIDTH_MM: f64 = 210.0;
const A4_HEIGHT_MM: f64 = 297.0;

/// Hard cap on the raster DPI: at 1200 dpi a single A4 page decodes to a
/// ~560 MB BGRA buffer, so anything above 600 is downscaled before blitting
/// (the StretchBlt on the printer DC does the final upscale).
const MAX_RASTER_DPI: u32 = 600;

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
        .map_err(|e| format!("Failed to launch {}: {}", browser.display(), e))
}

fn headless_base_args(user_data: &std::path::Path) -> Vec<String> {
    vec![
        "--headless=new".to_string(),
        "--disable-gpu".to_string(),
        "--no-first-run".to_string(),
        "--disable-extensions".to_string(),
        "--disable-crash-reporter".to_string(),
        format!("--user-data-dir={}", user_data.to_string_lossy()),
    ]
}

/// Screenshot each A4 page of `html` at exactly `px_w` × `px_h` device pixels,
/// one browser run per page. Returns one PNG per page, in order.
fn rasterize_a4_pages(html: &str, px_w: i32, px_h: i32, dpi: u32, page_count: usize) -> Result<Vec<Vec<u8>>, String> {
    let tmp = std::env::temp_dir();
    let stamp = chrono::Local::now().timestamp_subsec_nanos();
    let html_path = tmp.join(format!("titaou_a4_{}.html", stamp));
    let user_data = tmp.join(format!("titaou_a4_profile_{}", stamp));
    let _ = std::fs::create_dir_all(&user_data);
    std::fs::write(&html_path, html).map_err(|e| e.to_string())?;

    let scale = dpi as f64 / 96.0;
    let css_w = (px_w as f64 / scale).round().max(1.0) as i32;
    let css_h = (px_h as f64 / scale).round().max(1.0) as i32;

    let url = format!("file:///{}", html_path.to_string_lossy().replace('\\', "/"));
    let browsers = browser_candidates();

    let mut pages: Vec<Vec<u8>> = Vec::new();
    let mut failures: Vec<String> = Vec::new();

    if browsers.is_empty() {
        let _ = std::fs::remove_file(&html_path);
        let _ = std::fs::remove_dir_all(&user_data);
        return Err("No Chrome or Edge installation found on this system for rasterization".into());
    }

    let cleanup = |html_path: &std::path::Path, user_data: &std::path::Path| {
        let _ = std::fs::remove_file(html_path);
        let _ = std::fs::remove_dir_all(user_data);
    };

    for page_idx in 0..page_count {
        let png_path = tmp.join(format!("titaou_a4_{}_p{}.png", stamp, page_idx));
        let mut args = headless_base_args(&user_data);
        args.push("--hide-scrollbars".to_string());
        args.push("--default-background-color=FFFFFFFF".to_string());
        args.push(format!("--window-size={},{}", css_w, css_h));
        args.push(format!("--force-device-scale-factor={}", scale));
        args.push(format!("--screenshot={}", png_path.to_string_lossy()));
        args.push(url.clone());

        let mut page_png: Option<Vec<u8>> = None;
        for browser in &browsers {
            let output = match run_headless(browser, &args) {
                Ok(o) => o,
                Err(e) => {
                    failures.push(e);
                    continue;
                }
            };
            if png_path.exists() {
                match std::fs::read(&png_path) {
                    Ok(png) => {
                        page_png = Some(png);
                        break;
                    }
                    Err(e) => failures.push(format!("{}: png read failed: {}", browser.display(), e)),
                }
            } else {
                let log = format!(
                    "{} stdout: {} | stderr: {}",
                    browser.display(),
                    String::from_utf8_lossy(&output.stdout).trim(),
                    String::from_utf8_lossy(&output.stderr).trim(),
                );
                failures.push(format!(
                    "{}: {} — {}",
                    browser.display(),
                    if output.status.success() { "exited ok but produced NO screenshot" } else { "exited with error" },
                    log
                ));
            }
        }
        let _ = std::fs::remove_file(&png_path);

        match page_png {
            Some(png) => pages.push(png),
            None => {
                cleanup(&html_path, &user_data);
                return Err(format!(
                    "Failed to render A4 page {} of {} with system browser: {}",
                    page_idx + 1,
                    page_count,
                    failures.join(" || ")
                ));
            }
        }
    }

    cleanup(&html_path, &user_data);
    Ok(pages)
}

/// Decode a PNG into a BGRA top-down buffer.
fn decode_png_bgra(png_bytes: &[u8]) -> Result<(Vec<u8>, i32, i32), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("PNG parse error: {}", e))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG decode error: {}", e))?;
    let w = info.width as i32;
    let h = info.height as i32;
    let (color_type, _depth) = reader.output_color_type();
    let channels = match color_type {
        png::ColorType::Grayscale => 1usize,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err("Unexpected indexed PNG format".into());
        }
    };
    let px_count = (w as usize) * (h as usize);
    if buf.len() < px_count * channels {
        return Err(format!("PNG buffer underflow: {} bytes for {}x{}x{}", buf.len(), w, h, channels));
    }
    let mut bgra = Vec::with_capacity(px_count * 4);
    for px in buf[..px_count * channels].chunks_exact(channels) {
        let (r, g, b) = match channels {
            1 => (px[0], px[0], px[0]),
            2 => (px[0], px[0], px[0]),
            3 => (px[0], px[1], px[2]),
            _ => (px[0], px[1], px[2]),
        };
        bgra.push(b);
        bgra.push(g);
        bgra.push(r);
        bgra.push(255);
    }
    Ok((bgra, w, h))
}

/// Count how many `<div class="a4-page"` or page break indicators exist in the HTML.
fn estimate_a4_page_count(html: &str) -> usize {
    let count = html.matches("class=\"a4-page\"").count()
        + html.matches("class='a4-page'").count()
        + html.matches("a4-page-break").count();
    count.max(1)
}

/// Main entry point: print an A4 document silently to the given printer or default.
pub fn print_a4_job(req: &A4PrintRequest) -> A4PrintResult {
    let fallback_dpi = req.dpi.unwrap_or(300).clamp(150, MAX_RASTER_DPI);
    let page_count = estimate_a4_page_count(&req.html);

    let printer_name = match req.printer.as_deref() {
        Some(p) if !p.trim().is_empty() => p.trim().to_string(),
        _ => match get_default_printer() {
            Ok(d) => d,
            Err(e) => return A4PrintResult {
                ok: false,
                message: format!("No A4 printer specified and could not find default printer: {}", e),
                diagnostics: None,
            },
        },
    };

    #[cfg(windows)]
    {
        // Open the printer DC FIRST so the raster is built at the device's own
        // printable geometry (pixels AND dpi) — never at an assumed 300 dpi.
        let hdc = match unsafe { gdi::acquire_a4_dc(&printer_name) } {
            Ok(h) => h,
            Err(e) => return A4PrintResult {
                ok: false,
                message: format!("A4 print failed on \"{}\": {}", printer_name, e),
                diagnostics: None,
            },
        };

        let (dev_w, dev_h, dev_dpi) = gdi::printable_geometry(hdc);
        let use_device = dev_w > 0 && dev_h > 0 && dev_dpi > 0;

        let raster_dpi = if use_device { (dev_dpi as u32).clamp(150, MAX_RASTER_DPI) } else { fallback_dpi };
        let px_w = if use_device { dev_w } else { ((A4_WIDTH_MM * fallback_dpi as f64) / 25.4).round() as i32 };
        let page_px_h = if use_device { dev_h } else { ((A4_HEIGHT_MM * fallback_dpi as f64) / 25.4).round() as i32 };

        let outcome = (|| -> Result<A4PrintResult, String> {
            // 1. Rasterize each page separately.
            let page_pngs = rasterize_a4_pages(&req.html, px_w, page_px_h, raster_dpi, page_count)?;

            // 2. Decode every page into BGRA.
            let mut pages_bgra: Vec<(Vec<u8>, i32, i32)> = Vec::with_capacity(page_pngs.len());
            for png in &page_pngs {
                pages_bgra.push(decode_png_bgra(png)?);
            }

            // 3. Print via GDI, stretching each page over the full printable area.
            gdi::print_a4_pages_on_dc(hdc, &pages_bgra, &req.title)?;

            let first_h = pages_bgra.first().map(|(_, _, h)| *h).unwrap_or(0);
            Ok(A4PrintResult {
                ok: true,
                message: format!("A4 document printed successfully: {} page(s) on \"{}\"", pages_bgra.len(), printer_name),
                diagnostics: Some(A4PrintDiagnostics {
                    printer: printer_name.clone(),
                    page_count: pages_bgra.len() as u32,
                    dpi: raster_dpi,
                    width_mm: A4_WIDTH_MM,
                    height_mm: A4_HEIGHT_MM,
                    raster_width_px: pages_bgra.first().map(|(_, w, _)| *w).unwrap_or(0) as u32,
                    raster_height_px: first_h as u32,
                }),
            })
        })();

        gdi::delete_dc(hdc);

        match outcome {
            Ok(res) => res,
            Err(e) => A4PrintResult {
                ok: false,
                message: format!("A4 print failed on \"{}\": {}", printer_name, e),
                diagnostics: None,
            },
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (fallback_dpi, page_count, printer_name);
        A4PrintResult {
            ok: false,
            message: "A4 native printing is currently supported on Windows only".into(),
            diagnostics: None,
        }
    }
}

pub fn get_default_printer() -> Result<String, String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Printing::GetDefaultPrinterW;
        let mut len: u32 = 0;
        unsafe {
            let _ = GetDefaultPrinterW(std::ptr::null_mut(), &mut len);
            if len == 0 {
                return Err("No default printer configured in Windows".into());
            }
            let mut buf = vec![0u16; len as usize];
            if GetDefaultPrinterW(buf.as_mut_ptr(), &mut len) == 0 {
                return Err("Failed to query Windows default printer".into());
            }
            let l = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Ok(String::from_utf16_lossy(&buf[..l]))
        }
    }
    #[cfg(not(windows))]
    {
        Err("Unsupported operating system".into())
    }
}

/// Retrieve all available system printers directly using Windows Spooler EnumPrintersW API.
pub fn get_system_printers_list() -> Result<Vec<String>, String> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Graphics::Printing::{
            EnumPrintersW, PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_INFO_4W,
        };

        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let mut needed: u32 = 0;
        let mut count: u32 = 0;

        unsafe {
            // First call to determine required buffer size
            let _ = EnumPrintersW(flags, std::ptr::null_mut(), 4, std::ptr::null_mut(), 0, &mut needed, &mut count);
            if needed == 0 {
                return Ok(Vec::new());
            }

            let mut buf = vec![0u8; needed as usize];
            if EnumPrintersW(
                flags,
                std::ptr::null_mut(),
                4,
                buf.as_mut_ptr(),
                needed,
                &mut needed,
                &mut count,
            ) == 0 {
                return Err("EnumPrintersW failed to list system printers".into());
            }

            let pinfo = buf.as_ptr() as *const PRINTER_INFO_4W;
            let mut list = Vec::new();
            for i in 0..count {
                let info = *pinfo.add(i as usize);
                if !info.pPrinterName.is_null() {
                    let mut len = 0;
                    while *info.pPrinterName.add(len) != 0 {
                        len += 1;
                    }
                    let slice = std::slice::from_raw_parts(info.pPrinterName, len);
                    let name = String::from_utf16_lossy(slice);
                    if !name.trim().is_empty() {
                        list.push(name);
                    }
                }
            }
            list.sort();
            Ok(list)
        }
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[cfg(windows)]
mod gdi {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDCW, CreateDIBSection, DeleteDC, DeleteObject, GetDeviceCaps,
        SelectObject, SetBrushOrgEx, SetStretchBltMode, StretchBlt,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HALFTONE, HDC, HORZRES, LOGPIXELSX,
        SRCCOPY, VERTRES,
    };
    use windows_sys::Win32::Graphics::Printing::{
        ClosePrinter, DocumentPropertiesW, OpenPrinterW, PRINTER_ACCESS_USE, PRINTER_DEFAULTSW,
    };
    use windows_sys::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};

    const DMPAPER_A4: i16 = 9;
    const DMORIENT_PORTRAIT: i16 = 1;
    const DM_OUT_BUFFER: u32 = 2;
    const DM_IN_BUFFER: u32 = 8;
    const DM_ORIENTATION: u32 = 0x1;
    const DM_PAPERSIZE: u32 = 0x2;

    /// The printer's printable area in device pixels + native resolution.
    pub fn printable_geometry(hdc: HDC) -> (i32, i32, i32) {
        unsafe {
            let w = GetDeviceCaps(hdc, HORZRES as i32);
            let h = GetDeviceCaps(hdc, VERTRES as i32);
            let dpi = GetDeviceCaps(hdc, LOGPIXELSX as i32);
            (w, h, dpi)
        }
    }

    pub fn delete_dc(hdc: HDC) {
        unsafe {
            DeleteDC(hdc);
        }
    }

    /// Print pre-decoded BGRA pages, each stretched to the printer's FULL
    /// printable area. Stretching (not a 1:1 BitBlt) is what guarantees the
    /// document covers the whole sheet regardless of the printer's dpi.
    pub fn print_a4_pages_on_dc(
        hdc: HDC,
        pages: &[(Vec<u8>, i32, i32)],
        job_title: &str,
    ) -> Result<(), String> {
        let (dst_w, dst_h) = unsafe { (GetDeviceCaps(hdc, HORZRES as i32), GetDeviceCaps(hdc, VERTRES as i32)) };
        if dst_w <= 0 || dst_h <= 0 {
            return Err("Printer reported an invalid printable area".into());
        }

        unsafe {
            let doc_name: Vec<u16> = job_title.encode_utf16().chain(std::iter::once(0)).collect();
            let doc = DOCINFOW {
                cbSize: std::mem::size_of::<DOCINFOW>() as i32,
                lpszDocName: doc_name.as_ptr(),
                lpszOutput: std::ptr::null(),
                lpszDatatype: std::ptr::null(),
                fwType: 0,
            };

            if StartDocW(hdc, &doc) <= 0 {
                return Err("StartDocW failed — printer rejected the A4 print job".into());
            }

            let mut failed: Option<String> = None;
            for (idx, (bgra, img_w, img_h)) in pages.iter().enumerate() {
                if StartPage(hdc) <= 0 {
                    failed = Some(format!("StartPage failed on page {}", idx + 1));
                    break;
                }
                draw_page_full_bleed(hdc, bgra, *img_w, *img_h, dst_w, dst_h);
                if EndPage(hdc) <= 0 {
                    failed = Some(format!("EndPage failed on page {}", idx + 1));
                    break;
                }
            }

            if let Some(err) = failed {
                EndDoc(hdc);
                return Err(err);
            }

            if EndDoc(hdc) <= 0 {
                return Err("EndDoc failed on A4 printer".into());
            }

            Ok(())
        }
    }

    /// Stretch one BGRA page over the printer's entire printable area.
    unsafe fn draw_page_full_bleed(
        hdc: HDC,
        bgra: &[u8],
        img_w: i32,
        img_h: i32,
        dst_w: i32,
        dst_h: i32,
    ) {
        if img_w <= 0 || img_h <= 0 {
            return;
        }
        let mem_dc = CreateCompatibleDC(hdc);
        if mem_dc.is_null() {
            return;
        }

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = img_w;
        bmi.bmiHeader.biHeight = -img_h; // Top-down rows
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
            let copy_len = bgra.len().min((img_w as usize) * (img_h as usize) * 4);
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, copy_len);
            let old = SelectObject(mem_dc, dib);

            // HALFTONE keeps gradients/text readable under scaling; the brush
            // origin reset afterwards is required by the GDI docs.
            SetStretchBltMode(hdc, HALFTONE);
            SetBrushOrgEx(hdc, 0, 0, std::ptr::null_mut());
            StretchBlt(hdc, 0, 0, dst_w, dst_h, mem_dc, 0, 0, img_w, img_h, SRCCOPY);

            SelectObject(mem_dc, old);
        }

        if !dib.is_null() {
            DeleteObject(dib);
        }
        DeleteDC(mem_dc);
    }

    /// Open the printer DC configured for A4 portrait. The caller owns the
    /// returned DC and must release it with `delete_dc`.
    pub(super) unsafe fn acquire_a4_dc(printer_name: &str) -> Result<HDC, String> {
        let name16: Vec<u16> = printer_name.encode_utf16().chain(std::iter::once(0)).collect();

        let mut defaults: PRINTER_DEFAULTSW = std::mem::zeroed();
        defaults.DesiredAccess = PRINTER_ACCESS_USE;
        let mut hprinter: HANDLE = std::ptr::null_mut();

        if OpenPrinterW(name16.as_ptr(), &mut hprinter, &defaults) == 0 {
            return Err(format!("Cannot open printer \"{}\". Is it plugged in and turned on?", printer_name));
        }

        let needed = DocumentPropertiesW(
            std::ptr::null_mut(),
            hprinter,
            name16.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null(),
            DM_OUT_BUFFER,
        );

        if needed <= 0 {
            ClosePrinter(hprinter);
            return Err(format!("DocumentPropertiesW size probe failed for printer \"{}\"", printer_name));
        }

        let mut dm_buf: Vec<u8> = vec![0u8; needed as usize];
        let dm_ptr = dm_buf.as_mut_ptr() as *mut windows_sys::Win32::Graphics::Gdi::DEVMODEW;

        if DocumentPropertiesW(
            std::ptr::null_mut(),
            hprinter,
            name16.as_ptr(),
            dm_ptr,
            std::ptr::null(),
            DM_OUT_BUFFER,
        ) <= 0 {
            ClosePrinter(hprinter);
            return Err(format!("Could not read DEVMODE configuration for printer \"{}\"", printer_name));
        }

        // Configure standard A4 portrait form
        (*dm_ptr).dmFields |= DM_ORIENTATION | DM_PAPERSIZE;
        (*dm_ptr).Anonymous1.Anonymous1.dmOrientation = DMORIENT_PORTRAIT;
        (*dm_ptr).Anonymous1.Anonymous1.dmPaperSize = DMPAPER_A4;

        if DocumentPropertiesW(
            std::ptr::null_mut(),
            hprinter,
            name16.as_ptr(),
            dm_ptr,
            dm_ptr,
            DM_IN_BUFFER | DM_OUT_BUFFER,
        ) <= 0 {
            ClosePrinter(hprinter);
            return Err(format!("Printer driver \"{}\" rejected A4 configuration", printer_name));
        }

        ClosePrinter(hprinter);

        let driver: Vec<u16> = "WINSPOOL\0".encode_utf16().collect();
        let hdc = CreateDCW(driver.as_ptr(), name16.as_ptr(), std::ptr::null(), dm_ptr as *const _);
        if hdc.is_null() {
            return Err(format!("Failed to create GDI DC for printer \"{}\"", printer_name));
        }

        Ok(hdc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_system_printers_non_empty() {
        let printers = get_system_printers_list().expect("Should query system printers");
        println!("Discovered printers: {:?}", printers);
        assert!(!printers.is_empty(), "Should discover installed system printers");
    }

    #[test]
    fn test_get_default_printer() {
        let def = get_default_printer();
        println!("Default system printer: {:?}", def);
        assert!(def.is_ok(), "Should query default system printer");
    }

    #[test]
    fn test_rasterize_page_dimensions() {
        // One A4 page at 300 dpi must come back as ~2480×3508 device pixels —
        // this validates the headless screenshot geometry the GDI blit relies on.
        let html = r#"<!DOCTYPE html><html><head><style>
            @page { size: 210mm 297mm; margin: 0; }
            .a4-page { width: 210mm; height: 297mm; background: #fff; }
        </style></head><body><div class="a4-page"><h1>Test</h1></div></body></html>"#;
        let dpi = 300u32;
        let px_w = ((A4_WIDTH_MM * dpi as f64) / 25.4).round() as i32;
        let px_h = ((A4_HEIGHT_MM * dpi as f64) / 25.4).round() as i32;
        let pages = rasterize_a4_pages(html, px_w, px_h, dpi, 1).expect("rasterize");
        assert_eq!(pages.len(), 1);
        let (bgra, w, h) = decode_png_bgra(&pages[0]).expect("decode");
        // The browser rounds CSS px, so allow a ±2 device-pixel slop; the GDI
        // stretch absorbs it. What matters is the raster is the FULL page.
        assert!((w - px_w).abs() <= 2, "raster width {} != device width {}", w, px_w);
        assert!((h - px_h).abs() <= 2, "raster height {} != device height {}", h, px_h);
        assert_eq!(bgra.len(), (w as usize) * (h as usize) * 4);
    }
}
