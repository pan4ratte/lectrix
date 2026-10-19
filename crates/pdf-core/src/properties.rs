//! A document's properties (section 6.10): what its file declares, its description (title,
//! author, subject and keywords, which can be changed), and the fonts it uses.
//!
//! The description is written to the document information dictionary and, when the file
//! has XMP metadata, to the matching XMP properties, so that readers which prefer the XMP
//! show the same, and a PDF/A file keeps the two in agreement as the standard requires.
//! The rest of the XMP stays as it was.

use std::collections::HashSet;
use std::ops::Range;

use mupdf::Buffer;
use mupdf::pdf::{PdfDocument, PdfObject};

use crate::annot::meta::{parse_pdf_date, pdf_date_now};
use crate::docinfo::{
    self, XmpEncoding, decode_xmp, encode_xmp, find_xmp_property, is_xml_space, namespace_prefixes,
};
use crate::error::Result;
use crate::ffi::file_facts;
use crate::objects::{array_items, dict_entries, text_string};

/// What the Description tab lets the user change. An empty value is one the document does
/// not have.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Description {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
}

/// What the document's file declares about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Properties {
    pub description: Description,
    /// When the document was made and last changed, in milliseconds since the Unix epoch.
    pub created: Option<i64>,
    pub modified: Option<i64>,
    /// The application it was made with (`/Creator`).
    pub creator: Option<String>,
    /// What wrote the PDF (`/Producer`).
    pub producer: Option<String>,
    /// The PDF version it declares: the higher of the header's and the catalog's
    /// `/Version` ("1.7").
    pub version: String,
    /// It is a tagged PDF (`/MarkInfo /Marked true` and a structure tree).
    pub tagged: bool,
    /// It starts with a linearization dictionary ("fast web view").
    pub linearized: bool,
    /// How many signature fields hold a signature.
    pub signatures: u32,
}

const DC: &str = "http://purl.org/dc/elements/1.1/";
const PDF: &str = "http://ns.adobe.com/pdf/1.3/";
const XMP: &str = "http://ns.adobe.com/xap/1.0/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

pub fn read(doc: &PdfDocument) -> Result<Properties> {
    let info = doc
        .trailer()?
        .get_dict("Info")?
        .filter(|i| i.is_dict().unwrap_or(false));
    let info_text = |key: &str| -> Option<String> {
        let value = info.as_ref()?.get_dict(key).ok()??;
        if !value.is_string().ok()? {
            return None;
        }
        let text = value.as_string_lossy().ok()?;
        let text = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
        (!text.is_empty()).then(|| text.to_owned())
    };
    let xmp = docinfo::metadata_bytes(doc).map(|bytes| decode_xmp(&bytes).0);
    let from_xmp = |read: fn(&str) -> Option<String>| xmp.as_deref().and_then(read);
    let date = |key: &str, xmp_read: fn(&str) -> Option<String>| {
        info_text(key)
            .and_then(|d| parse_pdf_date(&d))
            .or_else(|| from_xmp(xmp_read).and_then(|d| parse_xmp_date(&d)))
    };

    let (header, linearized) = file_facts(doc)?;
    let catalog = doc.catalog()?;
    let declared = catalog
        .get_dict("Version")?
        .filter(|v| v.is_name().unwrap_or(false))
        .and_then(|v| v.as_name().ok())
        .and_then(|name| String::from_utf8(name).ok())
        .and_then(|name| name.parse::<f32>().ok())
        .map(|v| (v * 10.0).round() as i32)
        .unwrap_or(0);
    let version = header.max(declared);
    let tagged = catalog
        .get_dict("MarkInfo")?
        .and_then(|m| m.get_dict("Marked").ok().flatten())
        .is_some_and(|m| m.as_bool().unwrap_or(false))
        && catalog.get_dict("StructTreeRoot")?.is_some();

    Ok(Properties {
        description: Description {
            title: info_text("Title")
                .or_else(|| from_xmp(|x| xmp_list(x, DC, "title", false)))
                .unwrap_or_default(),
            author: info_text("Author")
                .or_else(|| from_xmp(|x| xmp_list(x, DC, "creator", true)))
                .unwrap_or_default(),
            subject: info_text("Subject")
                .or_else(|| from_xmp(|x| xmp_list(x, DC, "description", false)))
                .unwrap_or_default(),
            keywords: info_text("Keywords")
                .or_else(|| from_xmp(|x| xmp_simple(x, PDF, "Keywords")))
                .unwrap_or_default(),
        },
        created: date("CreationDate", |x| xmp_simple(x, XMP, "CreateDate")),
        modified: date("ModDate", |x| xmp_simple(x, XMP, "ModifyDate")),
        creator: info_text("Creator").or_else(|| from_xmp(|x| xmp_simple(x, XMP, "CreatorTool"))),
        producer: info_text("Producer").or_else(|| from_xmp(|x| xmp_simple(x, PDF, "Producer"))),
        version: format!("{}.{}", version / 10, version % 10),
        tagged,
        linearized,
        signatures: docinfo::signature_count(doc)?,
    })
}

/// Writes `description` to the document information dictionary (created if the document
/// has none) and to the XMP metadata, if the document has some. Call it inside a journal
/// operation.
pub fn write_description(doc: &mut PdfDocument, description: &Description) -> Result<()> {
    let mut info = match doc.trailer()?.get_dict("Info")? {
        Some(info) if info.is_dict()? => info,
        _ => {
            let created = doc.add_object(&doc.new_dict()?)?;
            // The trailer of the update the new object begins, not of the one before.
            let mut trailer = doc.trailer()?;
            trailer.dict_put("Info", created)?;
            trailer.get_dict("Info")?.ok_or_else(|| {
                crate::Error::InvalidArgument("could not create the information dictionary".into())
            })?
        }
    };
    for (key, value) in [
        ("Title", &description.title),
        ("Author", &description.author),
        ("Subject", &description.subject),
        ("Keywords", &description.keywords),
    ] {
        if value.is_empty() {
            info.dict_delete(key)?;
        } else {
            info.dict_put(key, text_string(doc, value)?)?;
        }
    }
    if let Some(mut metadata) = docinfo::metadata_stream(doc) {
        let xmp = metadata.read_stream()?;
        // XMP that can't be read as XMP is left as it was.
        if let Some(updated) = xmp_with_description(&xmp, description, &xmp_date_now()) {
            metadata.write_stream_buffer(&Buffer::from_copied_bytes(&updated)?)?;
        }
    }
    Ok(())
}

/// How an XMP property is written.
#[derive(Clone, Copy)]
enum Shape {
    /// A language alternative (`rdf:Alt`), written in the default language.
    Alt,
    /// An ordered list (`rdf:Seq`), written as one item.
    Seq,
    /// Plain text.
    Simple,
}

/// The packet with the description's properties, and the metadata date, written in place
/// of the ones it had (or added in a description of their own); an empty value takes its
/// property out. `None` if the packet has no RDF to write into, or its bytes can't carry
/// the text.
fn xmp_with_description(xmp: &[u8], d: &Description, now: &str) -> Option<Vec<u8>> {
    let (mut text, encoding) = decode_xmp(xmp);
    let non_ascii = [&d.title, &d.author, &d.subject, &d.keywords]
        .iter()
        .any(|v| !v.is_ascii());
    if encoding == XmpEncoding::Bytes && non_ascii {
        return None;
    }
    let rdf = namespace_prefixes(&text, RDF).first()?.to_string();
    let properties: [(&str, &str, &str, &str, Shape); 5] = [
        (DC, "dc", "title", &d.title, Shape::Alt),
        (DC, "dc", "creator", &d.author, Shape::Seq),
        (DC, "dc", "description", &d.subject, Shape::Alt),
        (PDF, "pdf", "Keywords", &d.keywords, Shape::Simple),
        (XMP, "xmp", "MetadataDate", now, Shape::Simple),
    ];
    let mut missing = String::new();
    let mut namespaces: Vec<(&str, &str)> = Vec::new();
    for (namespace, usual, name, value, shape) in properties {
        let prefixes: Vec<String> = namespace_prefixes(&text, namespace)
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut written = false;
        for prefix in &prefixes {
            written |= replace_property(&mut text, prefix, &rdf, name, value, shape, written);
        }
        if !written && !value.is_empty() {
            missing.push_str(&format!(
                "\n   {}",
                serialize(usual, &rdf, name, value, shape)
            ));
            if !namespaces.iter().any(|(p, _)| *p == usual) {
                namespaces.push((usual, namespace));
            }
        }
    }
    if !missing.is_empty() {
        let about = find_xmp_property(&text, &rdf, "about")
            .map(|found| text[found.value].to_owned())
            .unwrap_or_default();
        let declarations: String = namespaces
            .iter()
            .map(|(prefix, namespace)| format!(" xmlns:{prefix}=\"{namespace}\""))
            .collect();
        let block = format!(
            " <{rdf}:Description {rdf}:about=\"{}\"{declarations}>{missing}\n  </{rdf}:Description>\n ",
            escape(&about)
        );
        let at = text.find(&format!("</{rdf}:RDF"))?;
        text.insert_str(at, &block);
    }
    Some(encode_xmp(&text, encoding))
}

/// Writes `value` as the property `prefix:name` where the packet has it: the first time
/// in place (unless `already` was written under another prefix), every other time taken
/// out, as it is when `value` is empty. Returns whether it was written.
fn replace_property(
    text: &mut String,
    prefix: &str,
    rdf: &str,
    name: &str,
    value: &str,
    shape: Shape,
    already: bool,
) -> bool {
    let mut written = already;
    let mut from = 0;
    loop {
        // An element (its value plain text, or markup), or for plain text an attribute.
        let found = match shape {
            Shape::Simple => find_xmp_property(&text[from..], prefix, name)
                .map(|p| {
                    (
                        from + p.whole.start..from + p.whole.end,
                        true,
                        from + p.value.start..from + p.value.end,
                    )
                })
                .or_else(|| {
                    find_element(text, from, prefix, name)
                        .map(|(whole, content)| (whole, false, content))
                }),
            _ => find_element(text, from, prefix, name)
                .map(|(whole, content)| (whole, false, content)),
        };
        let Some((whole, simple, value_range)) = found else {
            return written;
        };
        let attribute = simple && !text[whole.clone()].starts_with('<');
        if !written && !value.is_empty() {
            let new = if attribute {
                text.replace_range(value_range.clone(), &escape(value));
                from = value_range.start + escape(value).len();
                written = true;
                continue;
            } else {
                serialize(prefix, rdf, name, value, shape)
            };
            text.replace_range(whole.clone(), &new);
            from = whole.start + new.len();
            written = true;
        } else {
            // Taken out with the white space before it.
            let start = text[..whole.start].trim_end_matches(is_xml_space).len();
            text.replace_range(start..whole.end, "");
            from = start;
        }
    }
}

/// The property as an element.
fn serialize(prefix: &str, rdf: &str, name: &str, value: &str, shape: Shape) -> String {
    let value = escape(value);
    match shape {
        Shape::Alt => format!(
            "<{prefix}:{name}><{rdf}:Alt><{rdf}:li xml:lang=\"x-default\">{value}</{rdf}:li></{rdf}:Alt></{prefix}:{name}>"
        ),
        Shape::Seq => format!(
            "<{prefix}:{name}><{rdf}:Seq><{rdf}:li>{value}</{rdf}:li></{rdf}:Seq></{prefix}:{name}>"
        ),
        Shape::Simple => format!("<{prefix}:{name}>{value}</{prefix}:{name}>"),
    }
}

/// The element `prefix:name` from `from` on: all of it, and its content (empty for an
/// element that closes itself).
fn find_element(
    text: &str,
    from: usize,
    prefix: &str,
    name: &str,
) -> Option<(Range<usize>, Range<usize>)> {
    let open = format!("<{prefix}:{name}");
    let close = format!("</{prefix}:{name}");
    let mut at = from;
    loop {
        let start = at + text.get(at..)?.find(&open)?;
        let after = start + open.len();
        at = after;
        if !text[after..].starts_with(|c: char| c == '>' || c == '/' || is_xml_space(c)) {
            continue;
        }
        let tag_end = after + text[after..].find('>')?;
        if text[..tag_end].ends_with('/') {
            return Some((start..tag_end + 1, tag_end..tag_end));
        }
        let content = tag_end + 1;
        let close_at = content + text[content..].find(&close)?;
        let close_end = close_at + text[close_at..].find('>')? + 1;
        return Some((start..close_end, content..close_at));
    }
}

/// A list property's items (`rdf:li`), the first one or (`all`) all of them joined by "; "
/// as Acrobat shows several authors, under any prefix the namespace has.
fn xmp_list(text: &str, namespace: &str, name: &str, all: bool) -> Option<String> {
    let rdf = namespace_prefixes(text, RDF).first()?.to_string();
    namespace_prefixes(text, namespace)
        .into_iter()
        .find_map(|prefix| {
            let (_, content) = find_element(text, 0, prefix, name)?;
            let inner = &text[content];
            let mut items = Vec::new();
            let mut from = 0;
            while let Some((whole, item)) = find_element(inner, from, &rdf, "li") {
                // An item with markup in it is not text this reads.
                let raw = inner[item].trim();
                if !raw.is_empty() && !raw.contains('<') {
                    items.push(unescape(raw));
                }
                from = whole.end;
                if !all && !items.is_empty() {
                    break;
                }
            }
            (!items.is_empty()).then(|| items.join("; "))
        })
}

/// A plain text property, written as an element or an attribute.
fn xmp_simple(text: &str, namespace: &str, name: &str) -> Option<String> {
    namespace_prefixes(text, namespace)
        .into_iter()
        .find_map(|prefix| {
            let found = find_xmp_property(text, prefix, name)?;
            let value = unescape(text[found.value].trim());
            (!value.is_empty()).then_some(value)
        })
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let Some(end) = rest.find(';') else {
            break;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// An XMP date (ISO 8601: `2026-10-19`, `2026-10-19T14:30:00+02:00`, with seconds and
/// fractions optional) in milliseconds since the Unix epoch.
fn parse_xmp_date(text: &str) -> Option<i64> {
    let text = text.trim();
    let (date, time) = text.split_once('T').unwrap_or((text, ""));
    let mut parts = date.split('-');
    let year = digits(parts.next()?).filter(|y| y.len() == 4)?;
    let month = parts.next().and_then(digits).unwrap_or("01");
    let day = parts.next().and_then(digits).unwrap_or("01");
    let zone_at = time.find(['Z', '+', '-']).unwrap_or(time.len());
    let (clock, zone) = time.split_at(zone_at);
    let clock = clock.split('.').next().unwrap_or("");
    let mut fields = clock.split(':');
    let hour = fields.next().filter(|h| !h.is_empty()).unwrap_or("00");
    let minute = fields.next().unwrap_or("00");
    let second = fields.next().unwrap_or("00");
    let zone = match zone {
        "" | "Z" => "+00'00'".to_owned(),
        z => {
            let (h, m) = z[1..].split_once(':').unwrap_or((&z[1..], "00"));
            format!("{}{h}'{m}'", &z[..1])
        }
    };
    parse_pdf_date(&format!(
        "D:{year}{month:0>2}{day:0>2}{hour:0>2}{minute:0>2}{second:0>2}{zone}"
    ))
}

fn digits(s: &str) -> Option<&str> {
    s.chars().all(|c| c.is_ascii_digit()).then_some(s)
}

/// Now, as an XMP date in UTC: `2026-10-19T12:30:00Z`.
fn xmp_date_now() -> String {
    // "D:YYYYMMDDHHmmSS+00'00'"
    let d = pdf_date_now();
    format!(
        "{}-{}-{}T{}:{}:{}Z",
        &d[2..6],
        &d[6..8],
        &d[8..10],
        &d[10..12],
        &d[12..14],
        &d[14..16]
    )
}

/// The kind of a font, as the Fonts tab names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontKind {
    Type1,
    /// Type 1 in its compact form (`/FontFile3 /Subtype /Type1C`).
    Type1Compact,
    MultipleMaster,
    TrueType,
    OpenType,
    Type3,
    /// A composite font with CFF outlines (`CIDFontType0`).
    CidType0,
    /// A composite font with TrueType outlines (`CIDFontType2`).
    CidTrueType,
    Unknown,
}

/// A font a document uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsedFont {
    /// Its object number, which tells it from another of the same name (0 for a font
    /// written directly into its resources).
    pub id: u32,
    /// Its name, without a subset's prefix ("ABCDEF+").
    pub name: String,
    pub subset: bool,
    pub kind: FontKind,
    /// Its glyphs are in the file (a Type 3 font's always are).
    pub embedded: bool,
    /// One of the 14 standard fonts every reader has, not embedded.
    pub standard: bool,
    /// Its `/Encoding` name ("WinAnsiEncoding", "Identity-H"); "Custom" for a dictionary,
    /// `None` for the font's own.
    pub encoding: Option<String>,
    /// The first page (0-based) of those asked about that uses it.
    pub first_page: usize,
}

const STANDARD_FONTS: [&str; 14] = [
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

/// The fonts `pages` use, in their page resources, the forms those draw and their
/// annotations' appearances, each once, in the order they are first met.
pub fn fonts(doc: &PdfDocument, pages: Range<usize>) -> Result<Vec<UsedFont>> {
    let mut walk = FontWalk::default();
    for page in pages {
        walk.page = page;
        let Ok(number) = i32::try_from(page) else {
            break;
        };
        let page_obj = doc.find_page(number)?;
        if let Some(resources) = inherited(&page_obj, "Resources")? {
            walk.resources(&resources, 0)?;
        }
        if let Some(annots) = page_obj.get_dict("Annots")? {
            for annot in array_items(&annots)? {
                let Some(ap) = annot.get_dict("AP")? else {
                    continue;
                };
                for key in ["N", "R", "D"] {
                    let Some(appearance) = ap.get_dict(key)? else {
                        continue;
                    };
                    if appearance.is_stream()? {
                        walk.form(&appearance, 0)?;
                    } else if appearance.is_dict()? {
                        for (_, state) in dict_entries(&appearance)? {
                            if state.is_stream()? {
                                walk.form(&state, 0)?;
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(walk.found)
}

#[derive(Default)]
struct FontWalk {
    page: usize,
    found: Vec<UsedFont>,
    fonts: HashSet<u32>,
    /// Direct fonts, by name.
    direct: HashSet<String>,
    /// Resource dictionaries and forms gone through, by object number.
    visited: HashSet<u32>,
}

impl FontWalk {
    /// False if `obj` is an indirect object already gone through.
    fn first_visit(&mut self, obj: &PdfObject) -> Result<bool> {
        if !obj.is_indirect()? {
            return Ok(true);
        }
        Ok(self
            .visited
            .insert(u32::try_from(obj.as_indirect()?).unwrap_or(0)))
    }

    fn resources(&mut self, resources: &PdfObject, depth: u32) -> Result<()> {
        // Guards against forms that draw themselves in malformed files.
        if depth > 16 || !resources.is_dict()? || !self.first_visit(resources)? {
            return Ok(());
        }
        if let Some(fonts) = resources.get_dict("Font")? {
            for (_, font) in dict_entries(&fonts)? {
                self.font(&font)?;
            }
        }
        if let Some(xobjects) = resources.get_dict("XObject")? {
            for (_, xobject) in dict_entries(&xobjects)? {
                let form = xobject.is_stream()?
                    && xobject
                        .get_dict("Subtype")?
                        .is_some_and(|s| s.as_name().is_ok_and(|n| n == b"Form"));
                if form {
                    self.form(&xobject, depth + 1)?;
                }
            }
        }
        Ok(())
    }

    fn form(&mut self, form: &PdfObject, depth: u32) -> Result<()> {
        if !self.first_visit(form)? {
            return Ok(());
        }
        if let Some(resources) = form.get_dict("Resources")? {
            self.resources(&resources, depth + 1)?;
        }
        Ok(())
    }

    fn font(&mut self, font: &PdfObject) -> Result<()> {
        if !font.is_dict()? {
            return Ok(());
        }
        let id = if font.is_indirect()? {
            u32::try_from(font.as_indirect()?).unwrap_or(0)
        } else {
            0
        };
        let used = describe(font, id, self.page)?;
        let new = if id == 0 {
            self.direct.insert(used.name.clone())
        } else {
            self.fonts.insert(id)
        };
        if new {
            self.found.push(used);
        }
        Ok(())
    }
}

fn name_of(obj: &PdfObject, key: &str) -> Result<Option<String>> {
    Ok(match obj.get_dict(key)? {
        Some(v) if v.is_name()? => Some(String::from_utf8_lossy(&v.as_name()?).into_owned()),
        _ => None,
    })
}

fn describe(font: &PdfObject, id: u32, page: usize) -> Result<UsedFont> {
    let subtype = name_of(font, "Subtype")?.unwrap_or_default();
    let base = name_of(font, "BaseFont")?
        .or(name_of(font, "Name")?)
        .unwrap_or_else(|| "Unnamed font".into());
    let subset = base.len() > 7
        && base.as_bytes()[6] == b'+'
        && base.as_bytes()[..6].iter().all(u8::is_ascii_uppercase);
    let name = if subset { base[7..].to_owned() } else { base };
    // A composite font's outlines and descriptor are its descendant's.
    let descendant = match font.get_dict("DescendantFonts")? {
        Some(list) => array_items(&list)?.into_iter().next(),
        None => None,
    };
    let outlines = descendant.as_ref().unwrap_or(font);
    let descriptor = outlines.get_dict("FontDescriptor")?;
    let mut file3 = None;
    let mut embedded = subtype == "Type3";
    if let Some(descriptor) = &descriptor {
        embedded |= descriptor.get_dict("FontFile")?.is_some()
            || descriptor.get_dict("FontFile2")?.is_some();
        if let Some(file) = descriptor.get_dict("FontFile3")? {
            embedded = true;
            file3 = name_of(&file, "Subtype")?;
        }
    }
    let opentype = file3.as_deref() == Some("OpenType");
    let kind = match subtype.as_str() {
        "Type1" if opentype => FontKind::OpenType,
        "Type1" if file3.as_deref() == Some("Type1C") => FontKind::Type1Compact,
        "Type1" => FontKind::Type1,
        "MMType1" => FontKind::MultipleMaster,
        "TrueType" if opentype => FontKind::OpenType,
        "TrueType" => FontKind::TrueType,
        "Type3" => FontKind::Type3,
        "Type0" => match descendant
            .as_ref()
            .map(|d| name_of(d, "Subtype"))
            .transpose()?
            .flatten()
            .as_deref()
        {
            _ if opentype => FontKind::OpenType,
            Some("CIDFontType0") => FontKind::CidType0,
            Some("CIDFontType2") => FontKind::CidTrueType,
            _ => FontKind::Unknown,
        },
        _ => FontKind::Unknown,
    };
    let encoding = match font.get_dict("Encoding")? {
        Some(e) if e.is_name()? => Some(String::from_utf8_lossy(&e.as_name()?).into_owned()),
        Some(e) if e.is_dict()? || e.is_stream()? => Some("Custom".to_owned()),
        _ => None,
    };
    Ok(UsedFont {
        id,
        standard: !embedded && STANDARD_FONTS.contains(&name.as_str()),
        name,
        subset,
        kind,
        embedded,
        encoding,
        first_page: page,
    })
}

/// `key` of a page or of the first of its ancestors that has it.
fn inherited(page: &PdfObject, key: &str) -> Result<Option<PdfObject>> {
    let mut node = Some(page.resolve()?.unwrap_or(page.try_clone()?));
    for _ in 0..32 {
        let Some(current) = node else {
            return Ok(None);
        };
        if let Some(value) = current.get_dict(key)? {
            return Ok(Some(value));
        }
        node = current.get_dict("Parent")?;
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PACKET: &str = concat!(
        "<?xpacket begin=\"\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n",
        "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n",
        " <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n",
        "  <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"\n",
        "    xmlns:pdf=\"http://ns.adobe.com/pdf/1.3/\" pdf:Producer=\"Writer\" pdf:Keywords=\"old, words\">\n",
        "   <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">Old &amp; dusty</rdf:li></rdf:Alt></dc:title>\n",
        "   <dc:creator><rdf:Seq><rdf:li>Ann</rdf:li><rdf:li>Bob</rdf:li></rdf:Seq></dc:creator>\n",
        "   <dc:format>application/pdf</dc:format>\n",
        "  </rdf:Description>\n",
        " </rdf:RDF>\n",
        "</x:xmpmeta>\n",
        "<?xpacket end=\"w\"?>"
    );

    #[test]
    fn reads_xmp_lists_and_plain_text() {
        assert_eq!(
            xmp_list(PACKET, DC, "title", false).as_deref(),
            Some("Old & dusty")
        );
        assert_eq!(
            xmp_list(PACKET, DC, "creator", true).as_deref(),
            Some("Ann; Bob")
        );
        assert_eq!(
            xmp_simple(PACKET, PDF, "Keywords").as_deref(),
            Some("old, words")
        );
        assert_eq!(
            xmp_simple(PACKET, PDF, "Producer").as_deref(),
            Some("Writer")
        );
        assert_eq!(xmp_list(PACKET, DC, "description", false), None);
    }

    #[test]
    fn writes_the_description_in_place_and_adds_what_is_missing() {
        let d = Description {
            title: "New <title>".into(),
            author: "Zoë".into(),
            subject: "About it".into(),
            keywords: String::new(),
        };
        let out = xmp_with_description(PACKET.as_bytes(), &d, "2026-10-19T12:00:00Z").unwrap();
        let out = String::from_utf8(out).unwrap();
        assert_eq!(
            xmp_list(&out, DC, "title", false).as_deref(),
            Some("New <title>")
        );
        assert_eq!(xmp_list(&out, DC, "creator", true).as_deref(), Some("Zoë"));
        assert_eq!(
            xmp_list(&out, DC, "description", false).as_deref(),
            Some("About it")
        );
        assert_eq!(xmp_simple(&out, PDF, "Keywords"), None, "{out}");
        assert_eq!(
            xmp_simple(&out, XMP, "MetadataDate").as_deref(),
            Some("2026-10-19T12:00:00Z")
        );
        // The rest is as it was.
        assert!(out.contains("<dc:format>application/pdf</dc:format>"));
        assert!(out.contains("pdf:Producer=\"Writer\""));
        assert!(out.ends_with("<?xpacket end=\"w\"?>"));
        // Written once each.
        assert_eq!(out.matches("<dc:title>").count(), 1);
        // Written again with the same values, it stays as it is.
        let again = xmp_with_description(out.as_bytes(), &d, "2026-10-19T12:00:00Z").unwrap();
        assert_eq!(String::from_utf8(again).unwrap(), out);
    }

    #[test]
    fn a_packet_without_rdf_is_left_alone() {
        assert_eq!(
            xmp_with_description(b"<x:xmpmeta/>", &Description::default(), "now"),
            None
        );
    }

    #[test]
    fn xmp_dates_read_as_pdf_dates_do() {
        assert_eq!(parse_xmp_date("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_xmp_date("1970-01-01T02:00:00+02:00"), Some(0));
        assert_eq!(parse_xmp_date("1970-01-01T00:00:01.250-00:00"), Some(1000));
        assert_eq!(parse_xmp_date("1970-01-02"), Some(86_400_000));
        assert_eq!(parse_xmp_date("garbage"), None);
        let now = xmp_date_now();
        assert!(parse_xmp_date(&now).is_some(), "{now}");
    }
}
