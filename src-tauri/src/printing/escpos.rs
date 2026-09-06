//! ESC/POS RAW spooling — the browser-free printing fallback (v0.5.20).
//!
//! On machines with NO Chrome/Edge the rasterization pipeline cannot run.
//! Thermal receipt printers are, however, ESC/POS text devices: we can send
//! a ready-made command stream straight to the Windows spooler as RAW data
//! (OpenPrinter → StartDocPrinter(datatype="RAW") → WritePrinter) — 100%
//! native, silent, fast, and completely independent of any browser.

/// Send a raw byte payload to a Windows printer via the spooler's RAW
/// datatype. No dialog is ever involved.
pub fn print_raw(payload: &str, printer: Option<&str>) -> Result<(), String> {
    let bytes = payload.as_bytes();
    print_raw_bytes(bytes, printer)
}

#[cfg(windows)]
pub fn print_raw_bytes(bytes: &[u8], printer: Option<&str>) -> Result<(), String> {
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::Graphics::Printing::{
        ClosePrinter, EndDocPrinter, EndPagePrinter, OpenPrinterW, StartDocPrinterW,
        StartPagePrinter, WritePrinter, PRINTER_ACCESS_USE, PRINTER_DEFAULTSW,
    };

    let name = match printer {
        Some(n) if !n.trim().is_empty() => n.to_string(),
        _ => default_printer()?,
    };
    let name16: Vec<u16> = name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut defaults: PRINTER_DEFAULTSW = std::mem::zeroed();
        defaults.DesiredAccess = PRINTER_ACCESS_USE;
        // "RAW" datatype = spooler passes the bytes to the printer untouched.
        let datatype: Vec<u16> = "RAW\0".encode_utf16().collect();
        defaults.pDatatype = datatype.as_ptr() as *mut u16 as *mut _;

        let mut hprinter: HANDLE = std::ptr::null_mut();
        if OpenPrinterW(name16.as_ptr(), &mut hprinter, &defaults) == 0 {
            return Err(format!("cannot open printer \"{}\" for RAW printing", name));
        }

        let doc_name: Vec<u16> = "TitaouPOS Receipt\0"
            .encode_utf16()
            .collect();
        let doc = windows_sys::Win32::Graphics::Printing::DOC_INFO_1W {
            pDocName: doc_name.as_ptr() as *mut _,
            pOutputFile: std::ptr::null_mut(),
            pDatatype: datatype.as_ptr() as *mut _,
        };

        if StartDocPrinterW(hprinter, 1, &doc) == 0 {
            ClosePrinter(hprinter);
            return Err("StartDocPrinter failed".into());
        }
        if StartPagePrinter(hprinter) == 0 {
            EndDocPrinter(hprinter);
            ClosePrinter(hprinter);
            return Err("StartPagePrinter failed".into());
        }
        let mut written: u32 = 0;
        let ok = WritePrinter(
            hprinter,
            bytes.as_ptr() as *const core::ffi::c_void,
            bytes.len() as u32,
            &mut written,
        );
        let _ = EndPagePrinter(hprinter);
        let _ = EndDocPrinter(hprinter);
        ClosePrinter(hprinter);

        if ok == 0 || written as usize != bytes.len() {
            return Err("WritePrinter failed (incomplete RAW job)".into());
        }
        Ok(())
    }
}

#[cfg(windows)]
fn default_printer() -> Result<String, String> {
    use windows_sys::Win32::Graphics::Printing::GetDefaultPrinterW;
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
        let l = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(String::from_utf16_lossy(&buf[..l]))
    }
}

#[cfg(not(windows))]
pub fn print_raw_bytes(_bytes: &[u8], _printer: Option<&str>) -> Result<(), String> {
    Err("RAW printing is Windows-only".into())
}
