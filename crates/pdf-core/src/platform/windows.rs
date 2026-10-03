//! Windows: installed fonts from DirectWrite's system font set (ADR 0005). The font set
//! reads names and properties from DirectWrite's font cache, so no font file is opened.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_PROPERTY_ID, DWRITE_FONT_PROPERTY_ID_POSTSCRIPT_NAME,
    DWRITE_FONT_PROPERTY_ID_STRETCH, DWRITE_FONT_PROPERTY_ID_STYLE, DWRITE_FONT_PROPERTY_ID_WEIGHT,
    DWRITE_FONT_PROPERTY_ID_WEIGHT_STRETCH_STYLE_FAMILY_NAME, DWriteCreateFactory, IDWriteFactory3,
    IDWriteFontSet, IDWriteLocalFontFileLoader, IDWriteLocalizedStrings,
};
use windows::core::{BOOL, Interface};

use super::SystemFonts;
use crate::fonts::index::{FontFace, Style};

pub struct DirectWrite;

impl SystemFonts for DirectWrite {
    fn installed_faces(&self) -> Result<Vec<FontFace>, String> {
        // SAFETY: DWriteCreateFactory has no preconditions; the shared factory is
        // thread-safe and needs no COM initialization.
        let factory: IDWriteFactory3 = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }
            .map_err(|e| format!("DirectWrite is not available: {e}"))?;
        // SAFETY: `factory` is a live interface.
        let set = unsafe { factory.GetSystemFontSet() }
            .map_err(|e| format!("no system font set: {e}"))?;
        // SAFETY: `set` is a live interface.
        let count = unsafe { set.GetFontCount() };
        let mut faces = Vec::with_capacity(count as usize);
        for i in 0..count {
            // A face that cannot be described (a font that is not a local file, say) is
            // left out; the rest of the index still works.
            if let Some(face) = describe(&set, i) {
                faces.push(face);
            }
        }
        Ok(faces)
    }
}

fn describe(set: &IDWriteFontSet, i: u32) -> Option<FontFace> {
    let mut families = strings(
        set,
        i,
        DWRITE_FONT_PROPERTY_ID_WEIGHT_STRETCH_STYLE_FAMILY_NAME,
    );
    // English first: it is the name MuPDF gets for the font, as with font-kit.
    families.sort_by_key(|(locale, _)| !locale.eq_ignore_ascii_case("en-us"));
    let families: Vec<String> = families.into_iter().map(|(_, name)| name).collect();
    if families.is_empty() {
        return None;
    }
    let number = |id| {
        strings(set, i, id)
            .first()
            .and_then(|(_, v)| v.trim().parse::<u16>().ok())
    };
    let weight = number(DWRITE_FONT_PROPERTY_ID_WEIGHT).unwrap_or(400);
    let stretch = number(DWRITE_FONT_PROPERTY_ID_STRETCH)
        .and_then(|v| u8::try_from(v).ok())
        .unwrap_or(5);
    let style = match number(DWRITE_FONT_PROPERTY_ID_STYLE) {
        Some(1) => Style::Oblique,
        Some(2) => Style::Italic,
        _ => Style::Normal,
    };
    let postscript_name = strings(set, i, DWRITE_FONT_PROPERTY_ID_POSTSCRIPT_NAME)
        .into_iter()
        .next()
        .map(|(_, name)| name);
    let (path, index) = local_file(set, i)?;
    Some(FontFace {
        postscript_name,
        families,
        weight,
        stretch,
        style,
        path,
        index,
    })
}

/// The values of one property of face `i`, with their locales.
fn strings(set: &IDWriteFontSet, i: u32, id: DWRITE_FONT_PROPERTY_ID) -> Vec<(String, String)> {
    let mut exists = BOOL(0);
    let mut values: Option<IDWriteLocalizedStrings> = None;
    // SAFETY: `i` is below the set's font count; `exists` and `values` are live locals
    // the call writes to.
    if unsafe { set.GetPropertyValues3(i, id, &mut exists, &mut values) }.is_err()
        || !exists.as_bool()
    {
        return Vec::new();
    }
    let Some(values) = values else {
        return Vec::new();
    };
    // SAFETY: `values` is a live interface; indices are below its count, and each buffer
    // has room for the reported length plus the terminating NUL.
    unsafe {
        let mut out = Vec::new();
        for k in 0..values.GetCount() {
            let read = |len: u32, get: &dyn Fn(&mut [u16]) -> windows::core::Result<()>| {
                let mut buf = vec![0u16; len as usize + 1];
                get(&mut buf).ok()?;
                Some(String::from_utf16_lossy(&buf[..len as usize]))
            };
            let Some(text) = values
                .GetStringLength(k)
                .ok()
                .and_then(|len| read(len, &|b| values.GetString(k, b)))
            else {
                continue;
            };
            let locale = values
                .GetLocaleNameLength(k)
                .ok()
                .and_then(|len| read(len, &|b| values.GetLocaleName(k, b)))
                .unwrap_or_default();
            out.push((locale, text));
        }
        out
    }
}

/// The file and face index of face `i`, if it is a local file.
fn local_file(set: &IDWriteFontSet, i: u32) -> Option<(PathBuf, u32)> {
    // SAFETY: `i` is below the set's font count. The reference key pointer returned by
    // GetReferenceKey stays valid while `file` is alive, which covers its use below, and
    // the path buffer has room for the reported length plus the terminating NUL.
    unsafe {
        let reference = set.GetFontFaceReference(i).ok()?;
        let index = reference.GetFontFaceIndex();
        let file = reference.GetFontFile().ok()?;
        let loader: IDWriteLocalFontFileLoader = file.GetLoader().ok()?.cast().ok()?;
        let mut key = std::ptr::null_mut();
        let mut key_size = 0u32;
        file.GetReferenceKey(&mut key, &mut key_size).ok()?;
        let len = loader.GetFilePathLengthFromKey(key, key_size).ok()?;
        let mut buf = vec![0u16; len as usize + 1];
        loader.GetFilePathFromKey(key, key_size, &mut buf).ok()?;
        Some((
            PathBuf::from(OsString::from_wide(&buf[..len as usize])),
            index,
        ))
    }
}
