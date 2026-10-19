//! Document-level facts the UI needs before letting the user edit (AGENTS.md section 5.4).

use mupdf::Buffer;
use mupdf::pdf::{PdfDocument, PdfObject, Permission};

use crate::error::Result;
use crate::protection::Printing;

/// What the document allows and how it must be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocumentFlags {
    /// The file is encrypted (it keeps its encryption on save).
    pub encrypted: bool,
    /// At least one signature field holds a signature: warn before the first edit, and
    /// always save incrementally.
    pub signed: bool,
    /// MuPDF repaired a damaged cross-reference table on open, so the next save must be
    /// a full save.
    pub repaired: bool,
    /// Pages may be rotated, inserted or deleted (permission bit 4 or 11).
    pub can_assemble: bool,
    /// Annotations may be added or changed (permission bit 6).
    pub can_annotate: bool,
    /// Text may be copied (permission bit 5).
    pub can_copy: bool,
    /// What printing the file allows (permission bits 3 and 12, section 6.12).
    pub printing: Printing,
    /// The PDF/A level the file claims, if it claims one: such files open for reading
    /// only until the user enables editing (section 5.4).
    pub pdfa: Option<PdfA>,
}

/// The PDF/A level a document claims in its XMP metadata. A claim is all it is: nothing
/// here checks that the file complies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfA {
    /// `pdfaid:part`: 1 to 4 so far.
    pub part: u8,
    /// `pdfaid:conformance` in upper case (A, B, U, E or F). PDF/A-4 may have none.
    pub conformance: Option<char>,
}

impl std::fmt::Display for PdfA {
    /// As the level is usually written: "PDF/A-1b", "PDF/A-4".
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PDF/A-{}", self.part)?;
        match self.conformance {
            Some(c) => write!(f, "{}", c.to_ascii_lowercase()),
            None => Ok(()),
        }
    }
}

pub fn read_flags(doc: &PdfDocument) -> Result<DocumentFlags> {
    let encrypted = doc.trailer()?.get_dict("Encrypt")?.is_some();
    let permissions = if encrypted {
        doc.permissions()
    } else {
        Permission::all()
    };
    Ok(DocumentFlags {
        encrypted,
        signed: is_signed(doc)?,
        repaired: crate::ffi::was_repaired(doc)?,
        can_assemble: permissions.intersects(Permission::MODIFY | Permission::ASSEMBLE),
        can_annotate: permissions.contains(Permission::ANNOTATE),
        can_copy: permissions.contains(Permission::COPY),
        printing: if !permissions.contains(Permission::PRINT) {
            Printing::NotAllowed
        } else if permissions.contains(Permission::PRINT_HQ) {
            Printing::HighResolution
        } else {
            Printing::LowResolution
        },
        pdfa: read_pdfa(doc),
    })
}

/// The namespace of the PDF/A identification schema (ISO 19005), whose `part` and
/// `conformance` properties are how a file says it is PDF/A.
const PDFA_ID_NAMESPACE: &str = "http://www.aiim.org/pdfa/ns/id/";

/// The identification schema's properties: the level, and the amendment, corrigendum and
/// revision of the standard it refers to. They go together when the claim is removed.
const PDFA_ID_PROPERTIES: [&str; 5] = ["part", "conformance", "amd", "corr", "rev"];

/// The catalog's XMP `/Metadata` stream, if it has one.
pub(crate) fn metadata_stream(doc: &PdfDocument) -> Option<PdfObject> {
    let metadata = doc.catalog().ok()?.get_dict("Metadata").ok()??;
    metadata.is_stream().ok()?.then_some(metadata)
}

/// The PDF/A level the document claims in the catalog's XMP `/Metadata`, as Acrobat
/// decides to open a file in its PDF/A mode. Metadata that is missing or can't be read is
/// no claim: it never keeps a file from opening.
pub fn read_pdfa(doc: &PdfDocument) -> Option<PdfA> {
    pdfa_claim(&metadata_stream(doc)?.read_stream().ok()?)
}

/// The catalog's XMP metadata as stored, for [`restore_metadata`].
pub fn metadata_bytes(doc: &PdfDocument) -> Option<Vec<u8>> {
    metadata_stream(doc)?.read_stream().ok()
}

/// Takes the PDF/A claim out of the document's XMP metadata, and returns the level it
/// claimed (`None`, and nothing written, if it claimed none). Everything else in the
/// metadata stays as it is, byte for byte. The stream is written without a filter, as
/// metadata should be, so that programs that do not read PDF can still find it.
///
/// This is how a file stops saying it is PDF/A once Lectrix has written something into it
/// that it cannot vouch for (section 5.4). Call it inside a journal operation.
pub fn remove_pdfa_claim(doc: &mut PdfDocument) -> Result<Option<PdfA>> {
    let Some(mut metadata) = metadata_stream(doc) else {
        return Ok(None);
    };
    let xmp = metadata.read_stream()?;
    let Some(claim) = pdfa_claim(&xmp) else {
        return Ok(None);
    };
    let Some(without) = without_pdfa_claim(&xmp) else {
        return Ok(None);
    };
    metadata.write_stream_buffer(&Buffer::from_copied_bytes(&without)?)?;
    Ok(Some(claim))
}

/// Puts back metadata read with [`metadata_bytes`] (after a save that failed, so that the
/// document in memory is as it was). Call it inside a journal operation.
pub fn restore_metadata(doc: &mut PdfDocument, xmp: &[u8]) -> Result<()> {
    if let Some(mut metadata) = metadata_stream(doc) {
        metadata.write_stream_buffer(&Buffer::from_copied_bytes(xmp)?)?;
    }
    Ok(())
}

/// The PDF/A level an XMP packet claims: the identification schema's `part`, with its
/// `conformance` if it has one, written as elements or as attributes, under whatever
/// prefix the packet gives the namespace.
fn pdfa_claim(xmp: &[u8]) -> Option<PdfA> {
    pdfa_claim_in(&decode_xmp(xmp).0)
}

fn pdfa_claim_in(text: &str) -> Option<PdfA> {
    let value = |prefix: &str, name: &str| {
        find_xmp_property(text, prefix, name).map(|found| text[found.value].trim())
    };
    namespace_prefixes(text, PDFA_ID_NAMESPACE)
        .into_iter()
        .find_map(|prefix| {
            let part = value(prefix, "part")?
                .parse::<u8>()
                .ok()
                .filter(|&part| part >= 1)?;
            let conformance = value(prefix, "conformance")
                .and_then(|value| value.chars().next())
                .filter(char::is_ascii_alphabetic)
                .map(|c| c.to_ascii_uppercase());
            Some(PdfA { part, conformance })
        })
}

/// The packet without the identification schema's properties, each removed with the
/// white space before it; `None` if it claims no level, or the claim can't be removed.
/// The namespace declaration stays: without a `part` it claims nothing.
fn without_pdfa_claim(xmp: &[u8]) -> Option<Vec<u8>> {
    let (mut text, encoding) = decode_xmp(xmp);
    pdfa_claim_in(&text)?;
    let prefixes: Vec<String> = namespace_prefixes(&text, PDFA_ID_NAMESPACE)
        .into_iter()
        .map(str::to_owned)
        .collect();
    for prefix in &prefixes {
        for name in PDFA_ID_PROPERTIES {
            while let Some(found) = find_xmp_property(&text, prefix, name) {
                let start = text[..found.whole.start]
                    .trim_end_matches(is_xml_space)
                    .len();
                text.replace_range(start..found.whole.end, "");
            }
        }
    }
    pdfa_claim_in(&text)
        .is_none()
        .then(|| encode_xmp(&text, encoding))
}

/// How an XMP packet's bytes became text, to turn the text back into the same bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum XmpEncoding {
    Utf8,
    /// Not valid UTF-8: each byte stands as the character with its number, so that
    /// whatever the bytes are, they come back as they were.
    Bytes,
    Utf16 {
        big_endian: bool,
    },
}

/// XMP in a PDF is UTF-8, but the format also allows UTF-16, which starts with a byte
/// order mark or with the packet's first `<`.
pub(crate) fn decode_xmp(bytes: &[u8]) -> (String, XmpEncoding) {
    let utf16 = |big_endian: bool| {
        let unit = if big_endian {
            u16::from_be_bytes
        } else {
            u16::from_le_bytes
        };
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| unit(pair))
            .collect();
        (
            String::from_utf16_lossy(&units),
            XmpEncoding::Utf16 { big_endian },
        )
    };
    match bytes {
        [0xFE, 0xFF, ..] | [0, b'<', ..] => utf16(true),
        [0xFF, 0xFE, ..] | [b'<', 0, ..] => utf16(false),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => (text.to_owned(), XmpEncoding::Utf8),
            Err(_) => (
                bytes.iter().map(|&b| char::from(b)).collect(),
                XmpEncoding::Bytes,
            ),
        },
    }
}

pub(crate) fn encode_xmp(text: &str, encoding: XmpEncoding) -> Vec<u8> {
    match encoding {
        XmpEncoding::Utf8 => text.as_bytes().to_vec(),
        // Every character came from one byte, and removing some of them made no new one.
        XmpEncoding::Bytes => text
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect(),
        XmpEncoding::Utf16 { big_endian } => text
            .encode_utf16()
            .flat_map(|unit| {
                if big_endian {
                    unit.to_be_bytes()
                } else {
                    unit.to_le_bytes()
                }
            })
            .collect(),
    }
}

/// White space as XML defines it (and nothing else that Unicode calls white space).
pub(crate) fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// The prefixes `text` declares for `namespace` (`xmlns:prefix="namespace"`).
pub(crate) fn namespace_prefixes<'a>(text: &'a str, namespace: &str) -> Vec<&'a str> {
    const DECLARATION: &str = "xmlns:";
    let mut prefixes = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(DECLARATION) {
        rest = &rest[at + DECLARATION.len()..];
        let Some(equals) = rest.find('=') else {
            break;
        };
        let prefix = rest[..equals].trim_end_matches(is_xml_space);
        let is_name = !prefix.is_empty()
            && prefix
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'));
        if is_name && quoted(&rest[equals + 1..]) == Some(namespace) && !prefixes.contains(&prefix)
        {
            prefixes.push(prefix);
        }
    }
    prefixes
}

/// The value of the quoted string `text` starts with, after any white space.
fn quoted(text: &str) -> Option<&str> {
    let text = text.trim_start_matches(is_xml_space);
    let quote = text.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let value = &text[1..];
    Some(&value[..value.find(quote)?])
}

/// Where a property is in an XMP packet: its value, and all of it (the element with its
/// tags, or the attribute with its name).
pub(crate) struct XmpProperty {
    pub(crate) value: std::ops::Range<usize>,
    pub(crate) whole: std::ops::Range<usize>,
}

/// The first occurrence of the property `prefix:name` in an XMP packet, written as an
/// element with nothing but its value in it (`<pdfaid:part>1</pdfaid:part>`) or as an
/// attribute (`pdfaid:part="1"`).
pub(crate) fn find_xmp_property(text: &str, prefix: &str, name: &str) -> Option<XmpProperty> {
    let qualified = format!("{prefix}:{name}");
    let closing = format!("</{qualified}");
    let mut from = 0;
    while let Some(at) = text[from..].find(&qualified) {
        let start = from + at;
        let end = start + qualified.len();
        from = end;
        let after = &text[end..];
        match text[..start].chars().next_back() {
            Some('<') if after.starts_with(|c: char| c == '>' || is_xml_space(c)) => {
                let Some(value_start) = after.find('>').map(|tag_end| end + tag_end + 1) else {
                    continue;
                };
                let Some(value_end) = text[value_start..].find('<').map(|len| value_start + len)
                else {
                    continue;
                };
                // The closing tag must follow the value: anything else in the element
                // (markup, a comment) is not a value this reads, or removes.
                let Some(tail) = text[value_end..].strip_prefix(closing.as_str()) else {
                    continue;
                };
                let tail = tail.trim_start_matches(is_xml_space);
                if tail.starts_with('>') {
                    return Some(XmpProperty {
                        value: value_start..value_end,
                        whole: start - 1..text.len() - tail.len() + 1,
                    });
                }
            }
            Some(c) if is_xml_space(c) => {
                let Some(value) = after
                    .trim_start_matches(is_xml_space)
                    .strip_prefix('=')
                    .map(|value| value.trim_start_matches(is_xml_space))
                else {
                    continue;
                };
                let Some(quote) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
                    continue;
                };
                let value_start = text.len() - value.len() + 1;
                if let Some(len) = text[value_start..].find(quote) {
                    return Some(XmpProperty {
                        value: value_start..value_start + len,
                        whole: start..value_start + len + 1,
                    });
                }
            }
            _ => {}
        }
    }
    None
}

/// How the document asks to be laid out when opened: the catalog's `/PageLayout`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLayout {
    /// One page at a time.
    SinglePage,
    /// One column, scrolling continuously.
    OneColumn,
    /// Two columns, odd-numbered pages on the left.
    TwoColumnLeft,
    /// Two columns, odd-numbered pages on the right (the first page alone).
    TwoColumnRight,
    /// Two pages at a time, odd-numbered pages on the left.
    TwoPageLeft,
    /// Two pages at a time, odd-numbered pages on the right.
    TwoPageRight,
}

/// The catalog's `/PageLayout`, or `None` when it is missing or not one of the six names.
pub fn read_page_layout(doc: &PdfDocument) -> Result<Option<PageLayout>> {
    let Some(value) = doc.catalog()?.get_dict("PageLayout")? else {
        return Ok(None);
    };
    if !value.is_name()? {
        return Ok(None);
    }
    Ok(match value.as_name()?.as_slice() {
        b"SinglePage" => Some(PageLayout::SinglePage),
        b"OneColumn" => Some(PageLayout::OneColumn),
        b"TwoColumnLeft" => Some(PageLayout::TwoColumnLeft),
        b"TwoColumnRight" => Some(PageLayout::TwoColumnRight),
        b"TwoPageLeft" => Some(PageLayout::TwoPageLeft),
        b"TwoPageRight" => Some(PageLayout::TwoPageRight),
        _ => None,
    })
}

/// True if any form field of type `/Sig` (possibly inherited) has a value.
pub fn is_signed(doc: &PdfDocument) -> Result<bool> {
    Ok(signature_count(doc)? > 0)
}

/// How many form fields of type `/Sig` (possibly inherited) have a value.
pub fn signature_count(doc: &PdfDocument) -> Result<u32> {
    let Some(form) = doc.catalog()?.get_dict("AcroForm")? else {
        return Ok(0);
    };
    let Some(fields) = form.get_dict("Fields")? else {
        return Ok(0);
    };
    count_signed(&fields, false, 0)
}

fn count_signed(fields: &PdfObject, parent_is_sig: bool, depth: u32) -> Result<u32> {
    // Guards against reference cycles in malformed files.
    if depth > 32 || !fields.is_array()? {
        return Ok(0);
    }
    let mut count = 0;
    for i in 0..i32::try_from(fields.len()?).unwrap_or(i32::MAX) {
        let Some(field) = fields.get_array(i)? else {
            continue;
        };
        if !field.is_dict()? {
            continue;
        }
        let is_sig = match field.get_dict("FT")? {
            Some(ft) if ft.is_name()? => ft.as_name()? == b"Sig",
            _ => parent_is_sig,
        };
        if is_sig
            && field
                .get_dict("V")?
                .is_some_and(|v| v.is_dict().unwrap_or(false))
        {
            count += 1;
        }
        if let Some(kids) = field.get_dict("Kids")? {
            count += count_signed(&kids, is_sig, depth + 1)?;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testgen::{SampleSpec, sample_document};

    fn add_field(doc: &mut PdfDocument, ft_on_parent: bool, with_value: bool) {
        let mut kid = doc.new_dict().unwrap();
        kid.dict_put("T", PdfObject::new_string("kid").unwrap())
            .unwrap();
        if !ft_on_parent {
            kid.dict_put("FT", PdfObject::new_name("Sig").unwrap())
                .unwrap();
        }
        if with_value {
            let mut v = doc.new_dict().unwrap();
            v.dict_put("Type", PdfObject::new_name("Sig").unwrap())
                .unwrap();
            kid.dict_put("V", v).unwrap();
        }
        let mut kids = doc.new_array().unwrap();
        kids.array_push(doc.add_object(&kid).unwrap()).unwrap();
        let mut parent = doc.new_dict().unwrap();
        if ft_on_parent {
            parent
                .dict_put("FT", PdfObject::new_name("Sig").unwrap())
                .unwrap();
        }
        parent.dict_put("Kids", kids).unwrap();
        let mut fields = doc.new_array().unwrap();
        fields.array_push(doc.add_object(&parent).unwrap()).unwrap();
        let mut form = doc.new_dict().unwrap();
        form.dict_put("Fields", fields).unwrap();
        doc.catalog().unwrap().dict_put("AcroForm", form).unwrap();
    }

    #[test]
    fn plain_document_is_unsigned_and_editable() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let flags = read_flags(&doc).unwrap();
        assert_eq!(
            flags,
            DocumentFlags {
                encrypted: false,
                signed: false,
                repaired: false,
                can_assemble: true,
                can_annotate: true,
                can_copy: true,
                printing: Printing::HighResolution,
                pdfa: None,
            }
        );
    }

    fn level(part: u8, conformance: Option<char>) -> Option<PdfA> {
        Some(PdfA { part, conformance })
    }

    #[test]
    fn reads_the_pdfa_claim_of_generated_documents() {
        for (claim, name) in [
            (level(1, Some('B')), "PDF/A-1b"),
            (level(2, Some('U')), "PDF/A-2u"),
            (level(4, None), "PDF/A-4"),
        ] {
            let doc = sample_document(&SampleSpec {
                pdfa: claim,
                ..SampleSpec::default()
            })
            .unwrap();
            assert_eq!(read_flags(&doc).unwrap().pdfa, claim);
            assert_eq!(claim.unwrap().to_string(), name);
        }
    }

    #[test]
    fn metadata_that_is_not_a_stream_is_no_claim() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        doc.catalog()
            .unwrap()
            .dict_put("Metadata", PdfObject::new_string("pdfaid:part").unwrap())
            .unwrap();
        assert_eq!(read_pdfa(&doc), None);
    }

    #[test]
    fn finds_the_claim_however_the_packet_writes_it() {
        const NS: &str = "xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"";
        // Elements, as most writers do.
        let elements = format!(
            "<rdf:Description rdf:about=\"\" {NS}>\n <pdfaid:part>2</pdfaid:part>\n <pdfaid:conformance>b</pdfaid:conformance>\n</rdf:Description>"
        );
        assert_eq!(pdfa_claim(elements.as_bytes()), level(2, Some('B')));
        // Attributes, in either kind of quotes, with spaces around the equals sign.
        let attributes =
            format!("<rdf:Description {NS} pdfaid:part = '3' pdfaid:conformance=\"A\"/>");
        assert_eq!(pdfa_claim(attributes.as_bytes()), level(3, Some('A')));
        // Another prefix for the same namespace.
        let prefixed = "<rdf:Description xmlns:id='http://www.aiim.org/pdfa/ns/id/'><id:part> 1 </id:part><id:conformance>B</id:conformance></rdf:Description>";
        assert_eq!(pdfa_claim(prefixed.as_bytes()), level(1, Some('B')));
        // PDF/A-4 without a conformance level.
        let four = format!("<rdf:Description {NS}><pdfaid:part>4</pdfaid:part></rdf:Description>");
        assert_eq!(pdfa_claim(four.as_bytes()), level(4, None));
        // UTF-16, with and without a byte order mark.
        let units: Vec<u16> = elements.encode_utf16().collect();
        let mut big_endian = vec![0xFE, 0xFF];
        big_endian.extend(units.iter().flat_map(|u| u.to_be_bytes()));
        assert_eq!(pdfa_claim(&big_endian), level(2, Some('B')));
        let little_endian: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(pdfa_claim(&little_endian), level(2, Some('B')));
    }

    #[test]
    fn other_metadata_is_no_claim() {
        for xmp in [
            "",
            "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\"><dc:title>pdfaid:part</dc:title></x:xmpmeta>",
            // The namespace declared, but no part.
            "<rdf:Description xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"/>",
            // A part under a prefix that is another namespace's.
            "<rdf:Description xmlns:pdfaid=\"http://example.com/\"><pdfaid:part>1</pdfaid:part></rdf:Description>",
            // Not a part number, an empty element, another property that starts the same.
            "<rdf:Description xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"><pdfaid:part>one</pdfaid:part></rdf:Description>",
            "<rdf:Description xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"><pdfaid:part/></rdf:Description>",
            "<rdf:Description xmlns:pdfaid=\"http://www.aiim.org/pdfa/ns/id/\"><pdfaid:partial>1</pdfaid:partial></rdf:Description>",
            // PDF/X and PDF/UA are other standards.
            "<rdf:Description xmlns:pdfuaid=\"http://www.aiim.org/pdfua/ns/id/\"><pdfuaid:part>1</pdfuaid:part></rdf:Description>",
        ] {
            assert_eq!(pdfa_claim(xmp.as_bytes()), None, "{xmp}");
        }
    }

    #[test]
    fn detects_signatures_including_inherited_field_type() {
        for ft_on_parent in [false, true] {
            let mut doc = sample_document(&SampleSpec::default()).unwrap();
            add_field(&mut doc, ft_on_parent, true);
            assert!(is_signed(&doc).unwrap(), "ft_on_parent: {ft_on_parent}");
        }
    }

    #[test]
    fn empty_signature_field_is_not_signed() {
        let mut doc = sample_document(&SampleSpec::default()).unwrap();
        add_field(&mut doc, false, false);
        assert!(!is_signed(&doc).unwrap());
    }

    fn without_claim(xmp: &[u8]) -> Option<String> {
        without_pdfa_claim(xmp).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
    }

    #[test]
    fn removes_the_claim_and_nothing_else() {
        // Elements: they go with the white space before them, and the rest stays.
        let elements = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:format>application/pdf</dc:format>
  <pdfaid:part>2</pdfaid:part>
  <pdfaid:conformance>B</pdfaid:conformance>
  <pdfaid:rev>2011</pdfaid:rev>
  <dc:title>Füße — 日本</dc:title>
 </rdf:Description>
</x:xmpmeta>"#;
        let expected = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:Description rdf:about="" xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:format>application/pdf</dc:format>
  <dc:title>Füße — 日本</dc:title>
 </rdf:Description>
</x:xmpmeta>"#;
        assert_eq!(
            without_claim(elements.as_bytes()).as_deref(),
            Some(expected)
        );
        assert_eq!(pdfa_claim(expected.as_bytes()), None);

        // Attributes, among others.
        let attributes = r#"<rdf:Description rdf:about='' xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" pdfaid:part='3' dc:format="application/pdf" pdfaid:conformance = "A"/>"#;
        assert_eq!(
            without_claim(attributes.as_bytes()).as_deref(),
            Some(
                r#"<rdf:Description rdf:about='' xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/" dc:format="application/pdf"/>"#
            )
        );

        // UTF-16 comes back as UTF-16, byte order mark included.
        let mut utf16 = vec![0xFE, 0xFF];
        utf16.extend(elements.encode_utf16().flat_map(u16::to_be_bytes));
        let mut expected_utf16 = vec![0xFE, 0xFF];
        expected_utf16.extend(expected.encode_utf16().flat_map(u16::to_be_bytes));
        assert_eq!(without_pdfa_claim(&utf16), Some(expected_utf16));

        // Bytes that are not UTF-8 stay as they are, also right before the claim.
        let mut broken = b"<rdf:Description xmlns:pdfaid='http://www.aiim.org/pdfa/ns/id/'><dc:title>caf\xE9 \xA0</dc:title>\xC3\xA0 <pdfaid:part>1</pdfaid:part>".to_vec();
        broken.extend(b"</rdf:Description>");
        assert_eq!(
            without_pdfa_claim(&broken).as_deref(),
            Some(
                &b"<rdf:Description xmlns:pdfaid='http://www.aiim.org/pdfa/ns/id/'><dc:title>caf\xE9 \xA0</dc:title>\xC3\xA0</rdf:Description>"[..]
            )
        );

        // Nothing to remove.
        assert_eq!(without_claim(b"<x:xmpmeta/>"), None);
    }

    #[test]
    fn removes_the_claim_from_a_document_and_puts_it_back() {
        let claim = level(1, Some('B'));
        let mut doc = sample_document(&SampleSpec {
            pdfa: claim,
            ..SampleSpec::default()
        })
        .unwrap();
        let before = metadata_bytes(&doc).unwrap();
        assert_eq!(remove_pdfa_claim(&mut doc).unwrap(), claim);
        assert_eq!(read_pdfa(&doc), None);
        let after = String::from_utf8(metadata_bytes(&doc).unwrap()).unwrap();
        assert!(
            after.contains("<x:xmpmeta") && after.contains("xpacket end"),
            "{after}"
        );
        assert!(!after.contains("pdfaid:part"), "{after}");
        // Once is enough.
        assert_eq!(remove_pdfa_claim(&mut doc).unwrap(), None);

        restore_metadata(&mut doc, &before).unwrap();
        assert_eq!(read_pdfa(&doc), claim);
        assert_eq!(metadata_bytes(&doc).unwrap(), before);

        // A document without metadata has nothing to remove.
        let mut plain = sample_document(&SampleSpec::default()).unwrap();
        assert_eq!(remove_pdfa_claim(&mut plain).unwrap(), None);
    }

    #[test]
    fn reads_the_page_layout_from_the_catalog() {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        assert_eq!(read_page_layout(&doc).unwrap(), None);
        for (name, layout) in [
            ("SinglePage", Some(PageLayout::SinglePage)),
            ("OneColumn", Some(PageLayout::OneColumn)),
            ("TwoColumnLeft", Some(PageLayout::TwoColumnLeft)),
            ("TwoColumnRight", Some(PageLayout::TwoColumnRight)),
            ("TwoPageLeft", Some(PageLayout::TwoPageLeft)),
            ("TwoPageRight", Some(PageLayout::TwoPageRight)),
            ("Unknown", None),
        ] {
            doc.catalog()
                .unwrap()
                .dict_put("PageLayout", PdfObject::new_name(name).unwrap())
                .unwrap();
            assert_eq!(read_page_layout(&doc).unwrap(), layout, "{name}");
        }
        // A string instead of a name is ignored.
        doc.catalog()
            .unwrap()
            .dict_put("PageLayout", PdfObject::new_string("TwoPageLeft").unwrap())
            .unwrap();
        assert_eq!(read_page_layout(&doc).unwrap(), None);
    }
}
