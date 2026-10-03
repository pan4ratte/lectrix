//! The outline as the bookmarks panel shows it: a tree of items with stable ids (their
//! object numbers) and targets resolved to a page and a point in view space.
//!
//! Reading is lenient (AGENTS.md section 5.2 applies the same idea to annotations): cycles,
//! shared items, items that are not indirect objects and absurd depths are cut off and
//! shown as far as they go, and the outline is marked damaged so edits are refused.

use std::collections::{HashMap, HashSet};

use mupdf::pdf::{PdfDocument, PdfObject};

use super::names::{lookup_dest, name_bytes};
use crate::error::Result;
use crate::geometry::{PageGeometry, Point, read_page_boxes};
use crate::objects;

/// Deepest nesting read; deeper items are left out and the outline is marked damaged.
const MAX_DEPTH: usize = 64;
/// Most items read (a guard against runaway files; the largest real outline seen has
/// about 5,000).
const MAX_ITEMS: usize = 200_000;

/// One bookmark.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    /// The item's object number: stable while the document is open, so the panel can
    /// refer to the item in edits. 0 for an item that is not an indirect object (only in
    /// damaged outlines, which cannot be edited).
    pub id: u32,
    pub title: String,
    /// Expanded in the bookmarks panel (positive `/Count`).
    pub open: bool,
    pub target: Target,
    /// `/F` flags and `/C` color: some files style their bookmarks.
    pub bold: bool,
    pub italic: bool,
    pub color: Option<[f32; 3]>,
    pub children: Vec<Bookmark>,
}

/// What a bookmark does when clicked.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A heading with no destination or action.
    None,
    /// A place in this document. `x` and `y` are the point to show at the top-left of the
    /// view, in view space (points from the top-left of the visible page); `None` keeps
    /// the current horizontal or vertical position. `named` is the destination's name
    /// when it is a named destination.
    Page {
        page: usize,
        x: Option<f32>,
        y: Option<f32>,
        named: Option<String>,
    },
    /// A destination that does not lead to a page of this document.
    Broken { named: Option<String> },
    /// A web link (URI action).
    Uri(String),
    /// Another file (GoToR, GoToE and Launch actions).
    File(String),
    /// Any other action, by its `/S` type (JavaScript, Named, ...).
    Action(String),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outline {
    pub items: Vec<Bookmark>,
    /// The outline is malformed in a way that edits could make worse (see the module
    /// documentation). It is shown as far as it can be read, but not edited.
    pub damaged: bool,
}

impl Outline {
    /// Visits every bookmark, depth first.
    pub fn for_each(&self, mut f: impl FnMut(&Bookmark)) {
        fn walk(items: &[Bookmark], f: &mut impl FnMut(&Bookmark)) {
            for item in items {
                f(item);
                walk(&item.children, f);
            }
        }
        walk(&self.items, &mut f);
    }

    /// Overrides the open state of the listed items (bookmarks expanded or collapsed in
    /// the panel since the last save).
    pub fn with_open_states(mut self, states: &HashMap<u32, bool>) -> Outline {
        fn walk(items: &mut [Bookmark], states: &HashMap<u32, bool>) {
            for item in items {
                if let Some(&open) = states.get(&item.id) {
                    item.open = open;
                }
                walk(&mut item.children, states);
            }
        }
        if !states.is_empty() {
            walk(&mut self.items, states);
        }
        self
    }
}

/// One item while walking the tree.
pub(crate) struct Node {
    /// The item, as referenced from its parent or previous sibling (an indirect reference
    /// unless the outline is damaged).
    pub obj: PdfObject,
    /// Object number, 0 for a direct object.
    pub num: i32,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub open: bool,
}

/// The outline's structure, as an arena of items in document order.
pub(crate) struct Tree {
    /// The outline dictionary (`/Outlines` in the catalog), if any.
    pub root: Option<PdfObject>,
    pub nodes: Vec<Node>,
    pub top: Vec<usize>,
    pub damaged: bool,
}

impl Tree {
    pub fn load(doc: &PdfDocument) -> Result<Tree> {
        let mut tree = Tree {
            root: None,
            nodes: Vec::new(),
            top: Vec::new(),
            damaged: false,
        };
        let catalog = doc.catalog()?;
        let Some(root) = catalog.get_dict("Outlines")? else {
            return Ok(tree);
        };
        if !root.is_dict()? {
            return Ok(tree);
        }
        // Items point at the outline dictionary with /Parent, so it must be an indirect
        // object for edits to work.
        if !root.is_indirect()? {
            tree.damaged = true;
        }
        let mut seen = HashSet::new();
        tree.read_level(root.get_dict("First")?, None, 0, &mut seen)?;
        tree.root = Some(root);
        Ok(tree)
    }

    fn read_level(
        &mut self,
        mut next: Option<PdfObject>,
        parent: Option<usize>,
        depth: usize,
        seen: &mut HashSet<i32>,
    ) -> Result<()> {
        while let Some(obj) = next {
            if !obj.is_dict()? {
                // A broken reference or a non-dictionary: the chain ends here.
                self.damaged = true;
                break;
            }
            let num = if obj.is_indirect()? {
                obj.as_indirect()?
            } else {
                0
            };
            if num == 0 {
                self.damaged = true;
            } else if !seen.insert(num) {
                // Reached twice: a cycle, or an item shared by two parents.
                self.damaged = true;
                break;
            }
            if self.nodes.len() >= MAX_ITEMS {
                self.damaged = true;
                break;
            }
            let open = match obj.get_dict("Count")? {
                Some(c) if c.is_number()? => c.as_int()? > 0,
                _ => false,
            };
            let index = self.nodes.len();
            let first = obj.get_dict("First")?;
            next = obj.get_dict("Next")?;
            self.nodes.push(Node {
                obj,
                num,
                parent,
                children: Vec::new(),
                open,
            });
            match parent {
                Some(p) => self.nodes[p].children.push(index),
                None => self.top.push(index),
            }
            if first.is_some() {
                if depth + 1 >= MAX_DEPTH {
                    self.damaged = true;
                } else {
                    self.read_level(first, Some(index), depth + 1, seen)?;
                }
            }
        }
        Ok(())
    }

    /// The node with object number `id`.
    pub fn find(&self, id: u32) -> Option<usize> {
        let id = i32::try_from(id).ok().filter(|&n| n > 0)?;
        self.nodes.iter().position(|n| n.num == id)
    }

    pub fn children(&self, parent: Option<usize>) -> &[usize] {
        match parent {
            Some(p) => &self.nodes[p].children,
            None => &self.top,
        }
    }

    pub fn children_mut(&mut self, parent: Option<usize>) -> &mut Vec<usize> {
        match parent {
            Some(p) => &mut self.nodes[p].children,
            None => &mut self.top,
        }
    }

    /// True if `node` is `ancestor` or lies below it.
    pub fn is_within(&self, node: usize, ancestor: usize) -> bool {
        let mut current = Some(node);
        while let Some(n) = current {
            if n == ancestor {
                return true;
            }
            current = self.nodes[n].parent;
        }
        false
    }
}

/// Reads the outline with ids and resolved targets.
pub fn read_bookmarks(doc: &PdfDocument) -> Result<Outline> {
    let tree = Tree::load(doc)?;
    let mut resolver = Resolver::new(doc);
    let mut items = Vec::with_capacity(tree.top.len());
    for &i in &tree.top {
        items.push(bookmark(&tree, i, &mut resolver)?);
    }
    Ok(Outline {
        items,
        damaged: tree.damaged,
    })
}

fn bookmark(tree: &Tree, index: usize, resolver: &mut Resolver) -> Result<Bookmark> {
    let node = &tree.nodes[index];
    let obj = &node.obj;
    let title = match obj.get_dict("Title")? {
        Some(t) if t.is_string()? => t.as_string_lossy()?,
        _ => String::new(),
    };
    let flags = match obj.get_dict("F")? {
        Some(f) if f.is_int()? => f.as_int()?,
        _ => 0,
    };
    let color = match obj.get_dict("C")? {
        Some(c) => match objects::numbers(&c)?.as_deref() {
            Some(&[r, g, b]) => Some([r as f32, g as f32, b as f32]),
            _ => None,
        },
        None => None,
    };
    let mut children = Vec::with_capacity(node.children.len());
    for &c in &node.children {
        children.push(bookmark(tree, c, resolver)?);
    }
    Ok(Bookmark {
        id: u32::try_from(node.num).unwrap_or(0),
        title,
        open: node.open,
        target: resolver.target(obj)?,
        bold: flags & 2 != 0,
        italic: flags & 1 != 0,
        color,
        children,
    })
}

/// Resolves destinations to pages, caching page numbers and geometry.
struct Resolver<'a> {
    doc: &'a PdfDocument,
    /// Page object number to page index, built on first use.
    pages: Option<HashMap<i32, usize>>,
    geometry: HashMap<usize, PageGeometry>,
    page_count: usize,
}

impl<'a> Resolver<'a> {
    fn new(doc: &'a PdfDocument) -> Self {
        Resolver {
            doc,
            pages: None,
            geometry: HashMap::new(),
            page_count: doc
                .page_count()
                .ok()
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0),
        }
    }

    fn target(&mut self, item: &PdfObject) -> Result<Target> {
        if let Some(dest) = item.get_dict("Dest")? {
            return self.destination(&dest);
        }
        let Some(action) = item.get_dict("A")? else {
            return Ok(Target::None);
        };
        if !action.is_dict()? {
            return Ok(Target::None);
        }
        let kind = match action.get_dict("S")? {
            Some(s) if s.is_name()? => String::from_utf8_lossy(&s.as_name()?).into_owned(),
            _ => String::new(),
        };
        Ok(match kind.as_str() {
            "GoTo" => match action.get_dict("D")? {
                Some(d) => self.destination(&d)?,
                None => Target::Broken { named: None },
            },
            "URI" => match action.get_dict("URI")? {
                // URIs are 7-bit ASCII byte strings, not text strings.
                Some(u) if u.is_string()? => {
                    Target::Uri(String::from_utf8_lossy(&u.as_bytes()?).into_owned())
                }
                _ => Target::Uri(String::new()),
            },
            "GoToR" | "GoToE" | "Launch" => Target::File(file_name(&action)?),
            _ => Target::Action(kind),
        })
    }

    fn destination(&mut self, dest: &PdfObject) -> Result<Target> {
        if dest.is_array()? {
            return self.explicit(dest, None);
        }
        let Some(name) = name_bytes(dest)? else {
            return Ok(Target::Broken { named: None });
        };
        let named = Some(String::from_utf8_lossy(&name).into_owned());
        match lookup_dest(self.doc, &name)? {
            Some(array) => self.explicit(&array, named),
            None => Ok(Target::Broken { named }),
        }
    }

    fn explicit(&mut self, dest: &PdfObject, named: Option<String>) -> Result<Target> {
        let page = match dest.get_array(0)? {
            Some(p) if p.is_indirect()? && p.is_dict()? => self.page_index(p.as_indirect()?)?,
            // Some producers write a page number instead of a page reference.
            Some(p) if p.is_int()? => usize::try_from(p.as_int()?)
                .ok()
                .filter(|&n| n < self.page_count),
            _ => None,
        };
        let Some(page) = page else {
            return Ok(Target::Broken { named });
        };
        let kind = match dest.get_array(1)? {
            Some(k) if k.is_name()? => k.as_name()?,
            _ => Vec::new(),
        };
        let arg = |i: i32| -> Result<Option<f64>> {
            Ok(match dest.get_array(i)? {
                Some(v) => objects::number(&v)?,
                None => None,
            })
        };
        // The user-space left and top edges the destination asks for.
        let (left, top) = match kind.as_slice() {
            b"XYZ" => (arg(2)?, arg(3)?),
            b"FitH" | b"FitBH" => (None, arg(2)?),
            b"FitV" | b"FitBV" => (arg(2)?, None),
            b"FitR" => (arg(2)?, arg(5)?),
            _ => (None, None),
        };
        let (x, y) = self.view_point(page, left, top)?;
        Ok(Target::Page { page, x, y, named })
    }

    /// Converts a user-space left/top pair to the view-space point to show at the top-left
    /// of the view. With a rotated page, the left edge becomes a vertical position and the
    /// top edge a horizontal one.
    fn view_point(
        &mut self,
        page: usize,
        left: Option<f64>,
        top: Option<f64>,
    ) -> Result<(Option<f32>, Option<f32>)> {
        if left.is_none() && top.is_none() {
            return Ok((None, None));
        }
        let g = self.geometry(page)?;
        let b = g.visible_box;
        let p = g.user_to_view(Point::new(left.unwrap_or(b.x0), top.unwrap_or(b.y1)));
        let clamp = |v: f64, max: f64| v.clamp(0.0, max) as f32;
        let (x, y) = (clamp(p.x, g.width), clamp(p.y, g.height));
        Ok(if g.rotation % 180 == 0 {
            (left.map(|_| x), top.map(|_| y))
        } else {
            (top.map(|_| x), left.map(|_| y))
        })
    }

    fn page_index(&mut self, num: i32) -> Result<Option<usize>> {
        if self.pages.is_none() {
            let mut map = HashMap::with_capacity(self.page_count);
            for i in 0..self.page_count {
                let obj = self.doc.find_page(i32::try_from(i).unwrap_or(i32::MAX))?;
                if obj.is_indirect()? {
                    map.insert(obj.as_indirect()?, i);
                }
            }
            self.pages = Some(map);
        }
        Ok(self.pages.as_ref().and_then(|m| m.get(&num).copied()))
    }

    fn geometry(&mut self, page: usize) -> Result<PageGeometry> {
        if let Some(g) = self.geometry.get(&page) {
            return Ok(*g);
        }
        let obj = self
            .doc
            .find_page(i32::try_from(page).unwrap_or(i32::MAX))?;
        let g = PageGeometry::new(&read_page_boxes(&obj)?);
        self.geometry.insert(page, g);
        Ok(g)
    }
}

/// The file a GoToR, GoToE or Launch action refers to: `/F` as a string or a file
/// specification dictionary (`/UF`, then `/F`).
fn file_name(action: &PdfObject) -> Result<String> {
    let Some(f) = action.get_dict("F")? else {
        return Ok(String::new());
    };
    if f.is_string()? {
        return Ok(f.as_string_lossy()?);
    }
    if f.is_dict()? {
        for key in ["UF", "F"] {
            if let Some(s) = f.get_dict(key)?
                && s.is_string()?
            {
                return Ok(s.as_string_lossy()?);
            }
        }
    }
    Ok(String::new())
}
