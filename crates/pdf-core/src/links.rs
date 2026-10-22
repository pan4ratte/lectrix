//! Links on a page (link annotations, AGENTS.md section 6.1): where each one is and what
//! it does when clicked. A push button of a form, which has no value to fill and only an
//! action to run when clicked, is a link too: books made for screens use them for their
//! contents and their "next page" buttons. Targets are read as a bookmark's are
//! (`outline::Target`): a place in this document resolves to a page and a point, and
//! anything else (a web address, another file, an action) is only described, since Lectrix
//! opens and runs none of them.
//!
//! Reading is lenient, as for other apps' annotations (section 5.2): a link without a
//! usable `/Rect` or without a destination or action is left out.

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::geometry::{PageGeometry, read_page_boxes};
use crate::objects;
use crate::outline::Target;
use crate::outline::tree::Resolver;

/// Most links read on one page (a guard against runaway files; an index page has a few
/// hundred).
const MAX_LINKS: usize = 10_000;

/// Annotation flags of a link that is not shown on screen: Hidden (bit 2) and NoView
/// (bit 6).
const NOT_SHOWN: i32 = 2 | 32;

/// The Pushbutton flag of a button field's `/Ff` (bit 17).
const PUSH_BUTTON: i32 = 1 << 16;

/// One link.
#[derive(Debug, Clone, PartialEq)]
pub struct PageLink {
    /// Where it is: `[x0, y0, x1, y1]` in view space at zoom 1, as text is (`text.rs`).
    pub rect: [f32; 4],
    pub target: Target,
}

/// The links of `page`, in the order of the page's `/Annots` (later ones are drawn over
/// earlier ones).
pub fn page_links(doc: &PdfDocument, page: usize) -> Result<Vec<PageLink>> {
    let page_no = i32::try_from(page).map_err(|_| Error::PageOutOfRange(page))?;
    if page_no >= doc.page_count()? {
        return Err(Error::PageOutOfRange(page));
    }
    let page_obj = doc.find_page(page_no)?;
    let Some(annots) = page_obj.get_dict("Annots")? else {
        return Ok(Vec::new());
    };
    let items = objects::array_items(&annots)?;
    if items.is_empty() {
        return Ok(Vec::new());
    }
    let geometry = PageGeometry::new(&read_page_boxes(&page_obj)?);
    let mut resolver = Resolver::for_a_few(doc);
    let mut out = Vec::new();
    for item in items {
        if out.len() >= MAX_LINKS {
            break;
        }
        if let Some(link) = read_link(&item, &geometry, &mut resolver)? {
            out.push(link);
        }
    }
    Ok(out)
}

fn read_link(
    obj: &PdfObject,
    geometry: &PageGeometry,
    resolver: &mut Resolver,
) -> Result<Option<PageLink>> {
    if !obj.is_dict()? {
        return Ok(None);
    }
    let subtype = match obj.get_dict("Subtype")? {
        Some(s) if s.is_name()? => s.as_name()?,
        _ => return Ok(None),
    };
    let button = match subtype.as_slice() {
        b"Link" => false,
        b"Widget" if is_push_button(obj)? => true,
        _ => return Ok(None),
    };
    let flags = match obj.get_dict("F")? {
        Some(f) if f.is_int()? => f.as_int()?,
        _ => 0,
    };
    if flags & NOT_SHOWN != 0 {
        return Ok(None);
    }
    let Some(rect) = (match obj.get_dict("Rect")? {
        Some(r) => objects::rect(&r)?,
        None => None,
    }) else {
        return Ok(None);
    };
    // The part of it on the visible page.
    let view = geometry.user_rect_to_view(rect).normalized();
    let (x0, y0) = (view.x0.max(0.0), view.y0.max(0.0));
    let (x1, y1) = (view.x1.min(geometry.width), view.y1.min(geometry.height));
    if !(x1 > x0 && y1 > y0) {
        return Ok(None);
    }
    let target = if button {
        match button_action(obj)? {
            Some(action) => resolver.action(&action)?,
            None => Target::None,
        }
    } else {
        resolver.target(obj)?
    };
    if target == Target::None {
        return Ok(None);
    }
    Ok(Some(PageLink {
        rect: [x0 as f32, y0 as f32, x1 as f32, y1 as f32],
        target,
    }))
}

/// Whether a widget is a push button: a button field (`/FT /Btn`) with the Pushbutton flag
/// (bit 17 of `/Ff`), both of which the widget may inherit from its field.
fn is_push_button(widget: &PdfObject) -> Result<bool> {
    let is_button = match widget.get_dict_inheritable("FT")? {
        Some(t) if t.is_name()? => t.as_name()? == b"Btn",
        _ => false,
    };
    if !is_button {
        return Ok(false);
    }
    Ok(match widget.get_dict_inheritable("Ff")? {
        Some(f) if f.is_int()? => f.as_int()? & PUSH_BUTTON != 0,
        _ => false,
    })
}

/// What a push button does when clicked: its `/A`, else its mouse-up action (`/AA /U`).
fn button_action(widget: &PdfObject) -> Result<Option<PdfObject>> {
    if let Some(action) = widget.get_dict("A")? {
        return Ok(Some(action));
    }
    match widget.get_dict("AA")? {
        Some(aa) if aa.is_dict()? => Ok(aa.get_dict("U")?),
        _ => Ok(None),
    }
}
