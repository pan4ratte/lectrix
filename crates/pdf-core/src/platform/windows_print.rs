//! Windows: printers through the print spooler and GDI (ADR 0028). The printers come from
//! `EnumPrintersW`, what each takes from `DeviceCapabilitiesW`, and its settings are a
//! `DEVMODEW` its driver fills in (`DocumentPropertiesW`, which also shows the driver's own
//! dialog). A job is a printer device context: each sheet's page is drawn by MuPDF in bands
//! at the printer's resolution ([`crate::print::raster`]) and sent with `StretchDIBits`.

use std::sync::Mutex;

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, COLORONCOLOR, CreateDCW, CreateICW, DEVMODEW,
    DIB_RGB_COLORS, DM_COLLATE, DM_COLOR, DM_COPIES, DM_DUPLEX, DM_IN_BUFFER, DM_IN_PROMPT,
    DM_ORIENTATION, DM_OUT_BUFFER, DM_PAPERSIZE, DMCOLLATE_FALSE, DMCOLLATE_TRUE, DMCOLOR_COLOR,
    DMCOLOR_MONOCHROME, DMDUP_HORIZONTAL, DMDUP_SIMPLEX, DMDUP_VERTICAL, DMORIENT_LANDSCAPE,
    DMORIENT_PORTRAIT, DeleteDC, GET_DEVICE_CAPS_INDEX, GetDeviceCaps, HDC, HORZRES, LOGPIXELSX,
    LOGPIXELSY, PHYSICALHEIGHT, PHYSICALOFFSETX, PHYSICALOFFSETY, PHYSICALWIDTH, RGBQUAD, SRCCOPY,
    SetStretchBltMode, StretchDIBits, VERTRES,
};
use windows::Win32::Graphics::Printing::{
    ClosePrinter, DocumentPropertiesW, EnumPrintersW, GetDefaultPrinterW, OpenPrinterW,
    PRINTER_ENUM_CONNECTIONS, PRINTER_ENUM_LOCAL, PRINTER_HANDLE, PRINTER_INFO_4W,
};
use windows::Win32::Storage::Xps::{
    AbortDoc, DC_COLLATE, DC_COLORDEVICE, DC_COPIES, DC_DUPLEX, DC_PAPERNAMES, DC_PAPERS,
    DC_PAPERSIZE, DOCINFOW, DeviceCapabilitiesW, EndDoc, EndPage, PRINTER_DEVICE_CAPABILITIES,
    StartDocW, StartPage,
};
use windows::core::{PCWSTR, PWSTR};

use super::{Paper, PrinterCapabilities, PrinterEntry, Printers};
use crate::error::{Error, Result};
use crate::print::raster::{BAND_BYTES, Resolution, draw_bands};
use crate::print::{Area, Duplex, PrintSetup, Sheet, Target};

/// The answer `DocumentPropertiesW` gives for OK in the driver's dialog.
const IDOK: i32 = 1;

pub struct Gdi {
    /// Each printer's settings as its driver's dialog left them, while Lectrix runs.
    settings: Mutex<Vec<(String, DevMode)>>,
}

pub static GDI: Gdi = Gdi {
    settings: Mutex::new(Vec::new()),
};

/// A `DEVMODEW` and the driver's private data after it, in a buffer aligned for it.
#[derive(Clone)]
struct DevMode {
    words: Vec<u64>,
}

impl DevMode {
    fn new(bytes: usize) -> DevMode {
        DevMode {
            words: vec![
                0;
                bytes
                    .div_ceil(8)
                    .max(std::mem::size_of::<DEVMODEW>().div_ceil(8))
            ],
        }
    }

    fn ptr(&self) -> *const DEVMODEW {
        self.words.as_ptr().cast()
    }

    fn ptr_mut(&mut self) -> *mut DEVMODEW {
        self.words.as_mut_ptr().cast()
    }

    fn get(&self) -> &DEVMODEW {
        // SAFETY: the buffer is at least a DEVMODEW long, 8-byte aligned, and was filled by
        // the driver (or zeroed), which is a valid DEVMODEW: it is plain data.
        unsafe { &*self.ptr() }
    }

    fn get_mut(&mut self) -> &mut DEVMODEW {
        // SAFETY: as in `get`; the borrow is unique.
        unsafe { &mut *self.ptr_mut() }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn from_wide(s: &[u16]) -> String {
    let end = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..end])
}

fn failed(printer: &str, what: &str) -> Error {
    Error::Printer {
        printer: printer.to_owned(),
        detail: format!("{what} failed: {}", windows::core::Error::from_thread()),
    }
}

/// An open printer, closed when dropped.
struct Opened {
    handle: PRINTER_HANDLE,
}

impl Opened {
    fn new(printer: &str, name: &[u16]) -> Result<Opened> {
        let mut handle = PRINTER_HANDLE::default();
        // SAFETY: `name` is NUL-terminated and outlives the call; `handle` is a live local.
        unsafe { OpenPrinterW(PCWSTR(name.as_ptr()), &mut handle, None) }.map_err(|e| {
            Error::Printer {
                printer: printer.to_owned(),
                detail: format!("OpenPrinterW failed: {e}"),
            }
        })?;
        Ok(Opened { handle })
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        // SAFETY: `handle` came from OpenPrinterW and is closed once.
        let _ = unsafe { ClosePrinter(self.handle) };
    }
}

impl Gdi {
    /// The settings jobs on `printer` start from: those its driver's dialog left, or the
    /// printer's own.
    fn devmode(&self, printer: &str) -> Result<DevMode> {
        if let Ok(settings) = self.settings.lock()
            && let Some((_, mode)) = settings.iter().find(|(p, _)| p == printer)
        {
            return Ok(mode.clone());
        }
        let name = wide(printer);
        let opened = Opened::new(printer, &name)?;
        // SAFETY: with no buffers, the call returns the size the settings need.
        let size = unsafe {
            DocumentPropertiesW(None, opened.handle, PCWSTR(name.as_ptr()), None, None, 0)
        };
        if size <= 0 {
            return Err(failed(printer, "DocumentPropertiesW"));
        }
        let mut mode = DevMode::new(size as usize);
        // SAFETY: `mode` holds at least `size` bytes, aligned for DEVMODEW; `name` is
        // NUL-terminated; both outlive the call.
        let rc = unsafe {
            DocumentPropertiesW(
                None,
                opened.handle,
                PCWSTR(name.as_ptr()),
                Some(mode.ptr_mut()),
                None,
                DM_OUT_BUFFER.0,
            )
        };
        if rc != IDOK {
            return Err(failed(printer, "DocumentPropertiesW"));
        }
        Ok(mode)
    }

    /// `printer`'s settings with `change` made, as its driver merges them in.
    fn devmode_with(&self, printer: &str, change: impl FnOnce(&mut DEVMODEW)) -> Result<DevMode> {
        let mut input = self.devmode(printer)?;
        change(input.get_mut());
        let name = wide(printer);
        let opened = Opened::new(printer, &name)?;
        let mut output = input.clone();
        // SAFETY: both buffers hold the driver's full settings size (they were made from
        // its own output), aligned for DEVMODEW; `name` is NUL-terminated; all outlive the
        // call.
        let rc = unsafe {
            DocumentPropertiesW(
                None,
                opened.handle,
                PCWSTR(name.as_ptr()),
                Some(output.ptr_mut()),
                Some(input.ptr()),
                DM_IN_BUFFER.0 | DM_OUT_BUFFER.0,
            )
        };
        if rc != IDOK {
            // The driver did not take the change: the settings as they were.
            return Ok(input);
        }
        Ok(output)
    }

    fn device_capability(
        printer: &[u16],
        what: PRINTER_DEVICE_CAPABILITIES,
        output: Option<PWSTR>,
        mode: &DevMode,
    ) -> i32 {
        // SAFETY: `printer` is NUL-terminated; `output`, when given, has room for what the
        // capability returns (sized by a call without it); `mode` is a full DEVMODEW.
        unsafe {
            DeviceCapabilitiesW(
                PCWSTR(printer.as_ptr()),
                PCWSTR::null(),
                what,
                output,
                Some(mode.ptr()),
            )
        }
    }
}

/// Settings for a paper and orientation.
fn set_paper(mode: &mut DEVMODEW, paper: Option<i32>, landscape: bool) {
    if let Some(paper) = paper.and_then(|p| i16::try_from(p).ok()) {
        mode.Anonymous1.Anonymous1.dmPaperSize = paper;
        mode.dmFields |= DM_PAPERSIZE;
    }
    let orientation = if landscape {
        DMORIENT_LANDSCAPE
    } else {
        DMORIENT_PORTRAIT
    };
    mode.Anonymous1.Anonymous1.dmOrientation = orientation as i16;
    mode.dmFields |= DM_ORIENTATION;
}

/// The sheet a device context prints on, from what it says of itself.
fn sheet_of(dc: HDC) -> Option<(Sheet, f32, f32)> {
    // SAFETY: `dc` is a live printer device or information context.
    let cap = |i: GET_DEVICE_CAPS_INDEX| unsafe { GetDeviceCaps(Some(dc), i) } as f32;
    let (dpi_x, dpi_y) = (cap(LOGPIXELSX), cap(LOGPIXELSY));
    if dpi_x <= 0.0 || dpi_y <= 0.0 {
        return None;
    }
    let (to_x, to_y) = (72.0 / dpi_x, 72.0 / dpi_y);
    let sheet = Sheet {
        width: cap(PHYSICALWIDTH) * to_x,
        height: cap(PHYSICALHEIGHT) * to_y,
        printable: Area {
            x: cap(PHYSICALOFFSETX) * to_x,
            y: cap(PHYSICALOFFSETY) * to_y,
            width: cap(HORZRES) * to_x,
            height: cap(VERTRES) * to_y,
        },
    };
    (sheet.width > 0.0 && sheet.height > 0.0).then_some((sheet, dpi_x, dpi_y))
}

impl Printers for Gdi {
    fn list(&self) -> Result<Vec<PrinterEntry>> {
        let flags = PRINTER_ENUM_LOCAL | PRINTER_ENUM_CONNECTIONS;
        let (mut needed, mut count) = (0u32, 0u32);
        // SAFETY: without a buffer, the call returns the size it needs in `needed`.
        let _ = unsafe { EnumPrintersW(flags, PCWSTR::null(), 4, None, &mut needed, &mut count) };
        let mut entries = Vec::new();
        if needed > 0 {
            let mut words = vec![0u64; (needed as usize).div_ceil(8)];
            // SAFETY: the byte view covers `words` exactly, which outlives it.
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(words.as_mut_ptr().cast::<u8>(), words.len() * 8)
            };
            // SAFETY: `bytes` has the room the first call asked for.
            unsafe {
                EnumPrintersW(
                    flags,
                    PCWSTR::null(),
                    4,
                    Some(bytes),
                    &mut needed,
                    &mut count,
                )
            }
            .map_err(|e| Error::Printer {
                printer: String::new(),
                detail: format!("EnumPrintersW failed: {e}"),
            })?;
            // SAFETY: the buffer starts with `count` PRINTER_INFO_4W, aligned (8 bytes),
            // whose strings point into the same buffer, which lives until the end of the
            // block.
            let infos = unsafe {
                std::slice::from_raw_parts(words.as_ptr().cast::<PRINTER_INFO_4W>(), count as usize)
            };
            for info in infos {
                // SAFETY: the spooler's strings are NUL-terminated.
                if let Ok(name) = unsafe { info.pPrinterName.to_string() } {
                    entries.push(PrinterEntry {
                        name,
                        default: false,
                    });
                }
            }
        }
        let mut size = 0u32;
        // SAFETY: without a buffer, the call returns the size it needs in `size`.
        let _ = unsafe { GetDefaultPrinterW(None, &mut size) };
        if size > 0 {
            let mut buffer = vec![0u16; size as usize];
            // SAFETY: `buffer` has `size` characters of room.
            if unsafe { GetDefaultPrinterW(Some(PWSTR(buffer.as_mut_ptr())), &mut size) }.as_bool()
            {
                let default = from_wide(&buffer);
                for entry in &mut entries {
                    entry.default = entry.name == default;
                }
            }
        }
        Ok(entries)
    }

    fn capabilities(&self, printer: &str) -> Result<PrinterCapabilities> {
        let mode = self.devmode(printer)?;
        let name = wide(printer);
        let count = Gdi::device_capability(&name, DC_PAPERS, None, &mode);
        let mut papers = Vec::new();
        if count > 0 {
            let n = count as usize;
            let mut ids = vec![0u16; n];
            let mut names = vec![0u16; n * 64];
            // Tenths of a millimetre, width and height.
            let mut sizes = vec![0i32; n * 2];
            let got_ids =
                Gdi::device_capability(&name, DC_PAPERS, Some(PWSTR(ids.as_mut_ptr())), &mode);
            let got_names = Gdi::device_capability(
                &name,
                DC_PAPERNAMES,
                Some(PWSTR(names.as_mut_ptr())),
                &mode,
            );
            let got_sizes = Gdi::device_capability(
                &name,
                DC_PAPERSIZE,
                Some(PWSTR(sizes.as_mut_ptr().cast())),
                &mode,
            );
            if got_ids == count && got_names == count && got_sizes == count {
                for i in 0..n {
                    let (w, h) = (sizes[2 * i] as f32, sizes[2 * i + 1] as f32);
                    let name = from_wide(&names[i * 64..(i + 1) * 64]);
                    if w > 0.0 && h > 0.0 && !name.is_empty() {
                        // Tenths of a millimetre to points; portrait.
                        let (w, h) = (w * 72.0 / 254.0, h * 72.0 / 254.0);
                        papers.push(Paper {
                            id: i32::from(ids[i]),
                            name,
                            width: w.min(h),
                            height: w.max(h),
                        });
                    }
                }
            }
        }
        let m = mode.get();
        // SAFETY: reading plain integers of the union the driver filled.
        let paper = unsafe { m.Anonymous1.Anonymous1.dmPaperSize };
        let paper = (m.dmFields.0 & DM_PAPERSIZE.0 != 0 && paper > 0).then_some(i32::from(paper));
        Ok(PrinterCapabilities {
            papers,
            paper,
            duplex: Gdi::device_capability(&name, DC_DUPLEX, None, &mode) == 1,
            colour: Gdi::device_capability(&name, DC_COLORDEVICE, None, &mode) == 1,
            duplex_on: m.dmFields.0 & DM_DUPLEX.0 != 0
                && (m.dmDuplex == DMDUP_VERTICAL || m.dmDuplex == DMDUP_HORIZONTAL),
        })
    }

    fn sheet(&self, printer: &str, paper: Option<i32>, landscape: bool) -> Result<Sheet> {
        let mode = self.devmode_with(printer, |m| set_paper(m, paper, landscape))?;
        let name = wide(printer);
        // SAFETY: `name` is NUL-terminated and `mode` a full DEVMODEW; both outlive the call.
        let ic = unsafe {
            CreateICW(
                PCWSTR::null(),
                PCWSTR(name.as_ptr()),
                PCWSTR::null(),
                Some(mode.ptr()),
            )
        };
        if ic.is_invalid() {
            return Err(failed(printer, "CreateICW"));
        }
        let sheet = sheet_of(ic);
        // SAFETY: `ic` came from CreateICW and is deleted once.
        let _ = unsafe { DeleteDC(ic) };
        sheet
            .map(|(sheet, _, _)| sheet)
            .ok_or_else(|| failed(printer, "GetDeviceCaps"))
    }

    fn has_settings_dialog(&self) -> bool {
        true
    }

    fn settings_dialog(&self, printer: &str, parent: Option<isize>) -> Result<bool> {
        let input = self.devmode(printer)?;
        let mut output = input.clone();
        let name = wide(printer);
        let opened = Opened::new(printer, &name)?;
        let parent = parent.map(|h| HWND(h as *mut std::ffi::c_void));
        // SAFETY: both buffers hold the driver's full settings, aligned for DEVMODEW;
        // `name` is NUL-terminated; `parent` is the app's window or none. The dialog is
        // modal and returns once closed.
        let rc = unsafe {
            DocumentPropertiesW(
                parent,
                opened.handle,
                PCWSTR(name.as_ptr()),
                Some(output.ptr_mut()),
                Some(input.ptr()),
                DM_IN_BUFFER.0 | DM_OUT_BUFFER.0 | DM_IN_PROMPT.0,
            )
        };
        if rc != IDOK {
            return Ok(false);
        }
        if let Ok(mut settings) = self.settings.lock() {
            settings.retain(|(p, _)| p != printer);
            settings.push((printer.to_owned(), output));
        }
        Ok(true)
    }

    fn start(&self, printer: &str, setup: &PrintSetup) -> Result<Box<dyn Target>> {
        let name = wide(printer);
        let base = self.devmode(printer)?;
        let max_copies = Gdi::device_capability(&name, DC_COPIES, None, &base);
        let collates = Gdi::device_capability(&name, DC_COLLATE, None, &base) == 1;
        let copies = setup.copies.max(1);
        // The printer makes the copies where it can make them all, collated if asked.
        let makes_copies =
            copies == 1 || (max_copies >= i32::from(copies) && (collates || !setup.collate));
        let mode = self.devmode_with(printer, |m| {
            set_paper(m, setup.paper, setup.landscape);
            m.dmDuplex = match setup.duplex {
                Duplex::OneSided => DMDUP_SIMPLEX,
                // Long edge of a portrait sheet is its vertical one.
                Duplex::LongEdge if setup.landscape => DMDUP_HORIZONTAL,
                Duplex::LongEdge => DMDUP_VERTICAL,
                Duplex::ShortEdge if setup.landscape => DMDUP_VERTICAL,
                Duplex::ShortEdge => DMDUP_HORIZONTAL,
            };
            m.dmFields |= DM_DUPLEX;
            m.dmColor = if setup.grey {
                DMCOLOR_MONOCHROME
            } else {
                DMCOLOR_COLOR
            };
            m.dmFields |= DM_COLOR;
            let made = if makes_copies { copies } else { 1 };
            m.Anonymous1.Anonymous1.dmCopies = i16::try_from(made).unwrap_or(1);
            m.dmCollate = if setup.collate {
                DMCOLLATE_TRUE
            } else {
                DMCOLLATE_FALSE
            };
            m.dmFields |= DM_COPIES | DM_COLLATE;
        })?;
        // SAFETY: `name` is NUL-terminated and `mode` a full DEVMODEW; both outlive the call.
        let dc = unsafe {
            CreateDCW(
                PCWSTR::null(),
                PCWSTR(name.as_ptr()),
                PCWSTR::null(),
                Some(mode.ptr()),
            )
        };
        if dc.is_invalid() {
            return Err(failed(printer, "CreateDCW"));
        }
        let Some((sheet, dpi_x, dpi_y)) = sheet_of(dc) else {
            // SAFETY: `dc` came from CreateDCW and is deleted once.
            let _ = unsafe { DeleteDC(dc) };
            return Err(failed(printer, "GetDeviceCaps"));
        };
        let title = wide(&setup.title);
        let output = setup.output.as_ref().map(|p| wide(&p.to_string_lossy()));
        let info = DOCINFOW {
            cbSize: std::mem::size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(title.as_ptr()),
            lpszOutput: output
                .as_ref()
                .map_or(PCWSTR::null(), |o| PCWSTR(o.as_ptr())),
            lpszDatatype: PCWSTR::null(),
            fwType: 0,
        };
        // SAFETY: `dc` is a live printer context; `info` and the strings it points to
        // outlive the call.
        if unsafe { StartDocW(dc, &info) } <= 0 {
            let error = failed(printer, "StartDocW");
            // SAFETY: as above.
            let _ = unsafe { DeleteDC(dc) };
            return Err(error);
        }
        // SAFETY: `dc` is a live printer context.
        unsafe { SetStretchBltMode(dc, COLORONCOLOR) };
        Ok(Box::new(GdiJob {
            dc,
            printer: printer.to_owned(),
            sheet,
            resolution: Resolution::new(
                dpi_x,
                dpi_y,
                (sheet.printable.x, sheet.printable.y),
                setup.max_dpi,
            ),
            makes_copies,
        }))
    }
}

/// A job: a printer device context with a document started.
struct GdiJob {
    dc: HDC,
    printer: String,
    sheet: Sheet,
    resolution: Resolution,
    makes_copies: bool,
}

/// A bitmap header with the grey palette an 8-bit bitmap needs.
#[repr(C)]
struct GreyInfo {
    header: BITMAPINFOHEADER,
    colours: [RGBQUAD; 256],
}

impl Target for GdiJob {
    fn sheet(&self) -> Sheet {
        self.sheet
    }

    fn makes_copies(&self) -> bool {
        self.makes_copies
    }

    fn begin_sheet(&mut self) -> Result<()> {
        // SAFETY: `dc` is a live printer context with a document started.
        if unsafe { StartPage(self.dc) } <= 0 {
            return Err(failed(&self.printer, "StartPage"));
        }
        Ok(())
    }

    fn draw(
        &mut self,
        list: &mupdf::DisplayList,
        ctm: &mupdf::Matrix,
        clip: Area,
        grey: bool,
    ) -> Result<()> {
        let dc = self.dc;
        let printer = self.printer.clone();
        draw_bands(
            list,
            ctm,
            clip,
            grey,
            &self.resolution,
            BAND_BYTES,
            &mut |band| {
                let mut info = GreyInfo {
                    header: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: band.width as i32,
                        // Negative: rows from the top down.
                        biHeight: -(band.height as i32),
                        biPlanes: 1,
                        biBitCount: if band.grey { 8 } else { 24 },
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    colours: [RGBQUAD::default(); 256],
                };
                if band.grey {
                    for (i, c) in info.colours.iter_mut().enumerate() {
                        let v = i as u8;
                        *c = RGBQUAD {
                            rgbBlue: v,
                            rgbGreen: v,
                            rgbRed: v,
                            rgbReserved: 0,
                        };
                    }
                    info.header.biClrUsed = 256;
                }
                let (x, y, w, h) = band.dest;
                // SAFETY: `dc` is a live printer context with a page started; `band.samples`
                // holds `height` rows of `stride` bytes (a multiple of 4, as the header
                // implies); `info` starts with the header, followed by the palette an 8-bit
                // bitmap reads; all outlive the call.
                let lines = unsafe {
                    StretchDIBits(
                        dc,
                        x,
                        y,
                        w,
                        h,
                        0,
                        0,
                        band.width as i32,
                        band.height as i32,
                        Some(band.samples.as_ptr().cast()),
                        (&info as *const GreyInfo).cast::<BITMAPINFO>(),
                        DIB_RGB_COLORS,
                        SRCCOPY,
                    )
                };
                if lines == 0 {
                    return Err(failed(&printer, "StretchDIBits"));
                }
                Ok(())
            },
        )
    }

    fn end_sheet(&mut self) -> Result<()> {
        // SAFETY: `dc` is a live printer context with a page started.
        if unsafe { EndPage(self.dc) } <= 0 {
            return Err(failed(&self.printer, "EndPage"));
        }
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<()> {
        // SAFETY: `dc` is a live printer context with a document started; it is deleted
        // once, here.
        let ended = unsafe { EndDoc(self.dc) };
        let error = (ended <= 0).then(|| failed(&self.printer, "EndDoc"));
        // SAFETY: as above.
        let _ = unsafe { DeleteDC(self.dc) };
        error.map_or(Ok(()), Err)
    }

    fn abort(self: Box<Self>) {
        // SAFETY: `dc` is a live printer context with a document started; it is deleted
        // once, here.
        unsafe {
            AbortDoc(self.dc);
            let _ = DeleteDC(self.dc);
        }
    }
}
