//! What a file's size is made of, and what Compress could remove from it (ADR 0027): the
//! Compress dialog shows it before anything is chosen.

use std::collections::HashSet;
use std::path::Path;

use mupdf::pdf::PdfObject;

use super::cleanup::{is_script, name_tree_len};
use crate::docinfo::{self, PdfA};
use crate::error::Result;

/// Bytes, and how many objects hold them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub bytes: u64,
    pub count: usize,
}

impl Usage {
    fn add(&mut self, bytes: u64) {
        self.bytes += bytes;
        self.count += 1;
    }
}

/// What the file has that Compress can remove; the dialog offers only these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Found {
    /// Pages with an embedded thumbnail.
    pub thumbnails: usize,
    /// Objects with other apps' private data (`/PieceInfo`).
    pub private_data: usize,
    /// Attached files.
    pub attachments: usize,
    /// JavaScript actions and document scripts.
    pub javascript: usize,
    /// It has a structure tree (tags).
    pub tags: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Analysis {
    pub file_size: u64,
    pub pages: usize,
    /// Image XObjects, soft masks included.
    pub images: Usage,
    /// Embedded font programs.
    pub fonts: Usage,
    /// Page contents and form XObjects.
    pub content: Usage,
    pub attachments: Usage,
    pub thumbnails: Usage,
    /// Streams only other apps' private data reaches.
    pub private_data: Usage,
    /// XMP metadata streams.
    pub metadata: Usage,
    /// Everything else: the structure, cross-reference tables, colour profiles, and so on.
    pub other: u64,
    pub found: Found,
    pub encrypted: bool,
    pub signed: bool,
    pub pdfa: Option<PdfA>,
}

/// Reads `path`, opened with `password`, and says what its size is made of.
pub fn analyze(path: &Path, password: Option<&str>) -> Result<Analysis> {
    let file_size = std::fs::metadata(path)?.len();
    let doc = crate::unprotect::load(path, password)?;
    let pages = usize::try_from(doc.page_count()?).unwrap_or(0);
    let catalog = doc.catalog()?;

    let mut found = Found::default();
    let mut contents = HashSet::new();
    let mut thumbs = HashSet::new();
    for page in 0..pages {
        let page = doc.find_page(page as i32)?;
        if let Some(thumb) = page.get_dict("Thumb")? {
            found.thumbnails += 1;
            insert_ref(&mut thumbs, &thumb)?;
        }
        if let Some(c) = page.get_dict("Contents")? {
            if c.is_array()? {
                for i in 0..c.len()? {
                    if let Some(item) = c.get_array(i as i32)? {
                        insert_ref(&mut contents, &item)?;
                    }
                }
            } else {
                insert_ref(&mut contents, &c)?;
            }
        }
    }
    if let Some(names) = catalog.get_dict("Names")? {
        if let Some(files) = names.get_dict("EmbeddedFiles")? {
            found.attachments = name_tree_len(&files, 0)?;
        }
        if let Some(scripts) = names.get_dict("JavaScript")? {
            found.javascript += name_tree_len(&scripts, 0)?;
        }
    }
    found.tags = catalog.get_dict("StructTreeRoot")?.is_some();

    // Every object: what each stream is, and what reaches what.
    let len = i32::try_from(doc.xref_len()?).unwrap_or(i32::MAX);
    let mut streams: Vec<(i32, u64, Kind)> = Vec::new();
    let mut fonts = HashSet::new();
    let mut private = HashSet::new();
    for num in 1..len {
        let obj = doc.new_indirect(num, 0)?;
        if !obj.is_dict()? {
            continue;
        }
        if let Some(info) = obj.get_dict("PieceInfo")? {
            found.private_data += 1;
            reachable_streams(&info, &mut private, 0)?;
        }
        for key in ["OpenAction", "A"] {
            if let Some(action) = obj.get_dict(key)?
                && is_script(&action)?
            {
                found.javascript += 1;
            }
        }
        if let Some(actions) = obj.get_dict("AA")?
            && actions.is_dict()?
        {
            for i in 0..actions.dict_len()? {
                if let Some(action) = actions.get_dict_val(i as i32)?
                    && is_script(&action)?
                {
                    found.javascript += 1;
                }
            }
        }
        if name(&obj, "Type")?.as_deref() == Some("FontDescriptor") {
            for key in ["FontFile", "FontFile2", "FontFile3"] {
                if let Some(file) = obj.get_dict(key)? {
                    insert_ref(&mut fonts, &file)?;
                }
            }
        }
        if obj.is_stream()? {
            let length = match obj.get_dict("Length")? {
                Some(l) if l.is_number()? => l.as_float()?.max(0.0) as u64,
                _ => 0,
            };
            let kind = match (
                name(&obj, "Type")?.as_deref(),
                name(&obj, "Subtype")?.as_deref(),
            ) {
                (_, Some("Image")) => Kind::Image,
                (_, Some("Form")) => Kind::Content,
                (Some("EmbeddedFile"), _) => Kind::Attachment,
                (Some("Metadata"), _) => Kind::Metadata,
                _ => Kind::Unknown,
            };
            streams.push((num, length, kind));
        }
    }

    let mut analysis = Analysis {
        file_size,
        pages,
        images: Usage::default(),
        fonts: Usage::default(),
        content: Usage::default(),
        attachments: Usage::default(),
        thumbnails: Usage::default(),
        private_data: Usage::default(),
        metadata: Usage::default(),
        other: 0,
        found,
        encrypted: doc.trailer()?.get_dict("Encrypt")?.is_some(),
        signed: docinfo::is_signed(&doc)?,
        pdfa: docinfo::read_pdfa(&doc),
    };
    let mut counted = 0u64;
    for (num, length, kind) in streams {
        let usage = if thumbs.contains(&num) {
            &mut analysis.thumbnails
        } else if fonts.contains(&num) {
            &mut analysis.fonts
        } else if contents.contains(&num) || kind == Kind::Content {
            &mut analysis.content
        } else if kind == Kind::Image {
            &mut analysis.images
        } else if kind == Kind::Attachment {
            &mut analysis.attachments
        } else if kind == Kind::Metadata {
            &mut analysis.metadata
        } else if private.contains(&num) {
            &mut analysis.private_data
        } else {
            continue;
        };
        usage.add(length);
        counted += length;
    }
    analysis.other = file_size.saturating_sub(counted);
    Ok(analysis)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Image,
    Content,
    Attachment,
    Metadata,
    Unknown,
}

fn name(obj: &PdfObject, key: &str) -> Result<Option<String>> {
    match obj.get_dict(key)? {
        Some(v) if v.is_name()? => Ok(Some(String::from_utf8_lossy(&v.as_name()?).into_owned())),
        _ => Ok(None),
    }
}

fn insert_ref(set: &mut HashSet<i32>, obj: &PdfObject) -> Result<()> {
    if obj.is_indirect()? {
        set.insert(obj.as_indirect()?);
    }
    Ok(())
}

/// The streams reachable from `obj` (other apps' private data is dictionaries of
/// streams), a few levels down.
fn reachable_streams(obj: &PdfObject, into: &mut HashSet<i32>, depth: u8) -> Result<()> {
    if depth > 6 {
        return Ok(());
    }
    if obj.is_indirect()? {
        let num = obj.as_indirect()?;
        if obj.is_stream()? {
            into.insert(num);
            return Ok(());
        }
    }
    if obj.is_dict()? {
        for i in 0..obj.dict_len()? {
            if let Some(v) = obj.get_dict_val(i as i32)? {
                reachable_streams(&v, into, depth + 1)?;
            }
        }
    } else if obj.is_array()? {
        for i in 0..obj.len()? {
            if let Some(v) = obj.get_array(i as i32)? {
                reachable_streams(&v, into, depth + 1)?;
            }
        }
    }
    Ok(())
}
