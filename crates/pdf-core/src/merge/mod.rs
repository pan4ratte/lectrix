//! Stitching (AGENTS.md section 6.4): combining PDFs into a new one ([`merge`]), and
//! inserting pages from a file into an open document ([`insert_pages`]). Both run the same
//! engine:
//!
//! 1. Every picked source page gets an empty page object in the destination, and the
//!    source objects that must not be copied are marked (see `copy.rs`).
//! 2. Pages are copied in output order: contents, resources, boxes, rotation and the other
//!    page keys, and their annotations (links, widgets and markup alike; `/P`, `/Popup`
//!    and `/Parent` follow through the copier's map). Links to pages that were not picked
//!    are left out.
//! 3. Named destinations come along; a name already taken is renamed with the source's
//!    prefix, and copied links, bookmarks and actions follow the new name.
//! 4. Form fields of copied widgets join the form (renamed the same way when their name is
//!    taken), and optional content groups join the document's layer list.
//! 5. Bookmarks and page labels are combined as the options say.
//!
//! Nothing here touches the sources; they are only read.

mod bookmarks;
mod copy;
mod forms;
mod labels;
mod names;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use mupdf::document::MetadataName;
use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::geometry::normalize_rotation;
use crate::labels::{self as page_labels, LabelRule};
use crate::objects::{array_items, child_array, child_dict, dict_entries, rect_array, text_string};
use crate::outline::edit::{self, NewBookmark};
use crate::outline::{self, Target};

use copy::Copier;
use labels::PageLabel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BookmarkMode {
    /// Each source's outline under a new top-level bookmark named after the source.
    #[default]
    NestUnderSource,
    /// All sources' outlines at the top level.
    Flat,
    /// No bookmarks.
    Drop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelMode {
    /// Every page keeps the label it had in its source (pages of a source without labels
    /// are numbered 1, 2, 3… from the source's first page).
    #[default]
    KeepSources,
    /// One decimal sequence over the whole result.
    Continuous,
    /// No `/PageLabels`.
    None,
}

/// How inserted pages are labeled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InsertLabels {
    /// The inserted pages keep the labels they had in their file, if it has labels; the
    /// document's own pages keep theirs.
    #[default]
    KeepSource,
    /// The inserted pages continue the numbering of the pages around them.
    FollowDocument,
}

pub struct MergeSource {
    pub doc: PdfDocument,
    /// Shown as the top-level bookmark title (document title or file name).
    pub name: String,
}

/// One page of the result: page `page` of source `source`, turned by `rotate` degrees
/// (a multiple of 90; positive is clockwise) on top of its own rotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagePick {
    pub source: usize,
    pub page: usize,
    pub rotate: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MergeOptions {
    pub bookmarks: BookmarkMode,
    pub labels: LabelMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InsertOptions {
    pub bookmarks: BookmarkMode,
    pub labels: InsertLabels,
}

/// What combining did, for telling the user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub pages: usize,
    /// Named destinations renamed because another source already used the name.
    pub renamed_destinations: usize,
    /// Form fields renamed for the same reason.
    pub renamed_fields: usize,
    /// Links left out because the page they lead to was not included.
    pub dropped_links: usize,
    /// Bookmarks left out for the same reason (bookmarks with children stay as headings).
    pub dropped_bookmarks: usize,
    /// The document's outline is damaged, so the inserted file's bookmarks were not added.
    pub bookmarks_skipped: bool,
}

impl MergeSource {
    /// Opens a file to copy pages from, through the same share-delete stream as open
    /// documents (so it can still be replaced while it is read), unlocked with `password`.
    /// A file whose permissions do not allow copying its content is refused, as Acrobat
    /// refuses page extraction from it.
    pub fn open(path: &Path, password: Option<&str>) -> Result<MergeSource> {
        let mut doc = crate::ffi::open_pdf_shared(path)?;
        if doc.needs_password()? {
            let Some(password) = password else {
                return Err(Error::PasswordRequired);
            };
            if !doc.authenticate(password)? {
                return Err(Error::WrongPassword);
            }
        }
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !crate::docinfo::read_flags(&doc)?.can_copy {
            return Err(Error::CopyNotPermitted(file_name));
        }
        let name = source_name(&doc, path);
        Ok(MergeSource { doc, name })
    }
}

/// The name a source's top-level bookmark gets: its title, or else its file name without
/// the extension (AGENTS.md section 6.4).
pub fn source_name(doc: &PdfDocument, path: &Path) -> String {
    if let Ok(title) = doc.metadata(MetadataName::Title)
        && !title.trim().is_empty()
    {
        return title.trim().to_owned();
    }
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Document".into())
}

/// Prefix for names a source must rename when combining: "src2_" for the second source.
fn source_prefix(source: usize) -> String {
    format!("src{}_", source + 1)
}

/// Prefix for names inserted pages must rename.
const INSERT_PREFIX: &str = "inserted_";

/// Combines the picked pages into a new document. `progress` is called after each page
/// with (pages done, pages in total); returning false stops with [`Error::Cancelled`].
pub fn merge(
    sources: &[MergeSource],
    picks: &[PagePick],
    options: MergeOptions,
    progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<(PdfDocument, MergeReport)> {
    if picks.is_empty() {
        return Err(Error::InvalidArgument("nothing to combine".into()));
    }
    let mut out = PdfDocument::new();
    move_info_to_trailer(&mut out)?;
    let prefixes = (0..sources.len()).map(source_prefix).collect();
    let mut engine = Engine::new(&out, sources, prefixes)?;
    let copied = engine.run(&mut out, 0, picks, options.bookmarks, progress)?;

    if options.bookmarks != BookmarkMode::Drop {
        // Sources in the order their first page appears in the result.
        let mut parts: Vec<(usize, usize, Vec<NewBookmark>)> = copied
            .into_iter()
            .enumerate()
            .filter_map(|(s, part)| Some((engine.states[s].first_output_page?, s, part)))
            .collect();
        parts.sort_by_key(|(first, _, _)| *first);
        let mut items = Vec::new();
        for (first, s, part) in parts {
            match options.bookmarks {
                BookmarkMode::NestUnderSource => {
                    let mut dict = out.new_dict()?;
                    dict.dict_put("Title", text_string(&out, &sources[s].name)?)?;
                    dict.dict_put("Dest", xyz_dest(&out, first)?)?;
                    items.push(NewBookmark {
                        dict,
                        open: false,
                        children: part,
                    });
                }
                _ => items.extend(part),
            }
        }
        edit::insert_items(&mut out, None, 0, items)?;
    }

    let rules = match options.labels {
        LabelMode::KeepSources => {
            let source_rules = sources
                .iter()
                .map(|s| page_labels::read_rules(&s.doc))
                .collect::<Result<Vec<_>>>()?;
            let per_page: Vec<PageLabel> = picks
                .iter()
                .map(|p| labels::page_label(&source_rules[p.source], p.page))
                .collect();
            let rules = labels::rules_for(&per_page, 0);
            if labels::is_plain(&rules) {
                Vec::new()
            } else {
                rules
            }
        }
        LabelMode::Continuous => vec![LabelRule::decimal_from_one(0)],
        LabelMode::None => Vec::new(),
    };
    if !rules.is_empty() {
        page_labels::write_rules(&mut out, rules)?;
    }
    Ok((out, engine.report))
}

/// Combines every page of every source, in order.
pub fn merge_all(
    sources: &[MergeSource],
    options: MergeOptions,
) -> Result<(PdfDocument, MergeReport)> {
    let mut picks = Vec::new();
    for (s, source) in sources.iter().enumerate() {
        for page in 0..usize::try_from(source.doc.page_count()?).unwrap_or(0) {
            picks.push(PagePick {
                source: s,
                page,
                rotate: 0,
            });
        }
    }
    merge(sources, &picks, options, &mut |_, _| true)
}

/// Inserts `pages` of `source` (all of them if empty) into `doc` before page `at` (the
/// page count appends). The caller wraps this in one journal step.
pub fn insert_pages(
    doc: &mut PdfDocument,
    at: usize,
    source: &MergeSource,
    pages: &[usize],
    options: InsertOptions,
) -> Result<MergeReport> {
    let old_count = usize::try_from(doc.page_count()?).unwrap_or(0);
    if at > old_count {
        return Err(Error::PageOutOfRange(at));
    }
    let picks: Vec<PagePick> = if pages.is_empty() {
        (0..usize::try_from(source.doc.page_count()?).unwrap_or(0))
            .map(|page| PagePick {
                source: 0,
                page,
                rotate: 0,
            })
            .collect()
    } else {
        pages
            .iter()
            .map(|&page| PagePick {
                source: 0,
                page,
                rotate: 0,
            })
            .collect()
    };
    if picks.is_empty() {
        return Err(Error::InvalidArgument("the file has no pages".into()));
    }
    let n = picks.len();
    let old_rules = page_labels::read_rules(doc)?;
    let old_outline = outline::read_bookmarks(doc)?;

    let sources = std::slice::from_ref(source);
    let mut engine = Engine::new(doc, sources, vec![INSERT_PREFIX.to_owned()])?;
    let mut copied = engine.run(doc, at, &picks, options.bookmarks, &mut |_, _| true)?;

    if options.bookmarks != BookmarkMode::Drop
        && let Some(part) = copied.pop()
    {
        if old_outline.damaged {
            engine.report.bookmarks_skipped = true;
        } else {
            let items = match options.bookmarks {
                BookmarkMode::NestUnderSource => {
                    let mut dict = doc.new_dict()?;
                    dict.dict_put("Title", text_string(doc, &source.name)?)?;
                    dict.dict_put("Dest", xyz_dest(doc, at)?)?;
                    vec![NewBookmark {
                        dict,
                        open: false,
                        children: part,
                    }]
                }
                _ => part,
            };
            // Among the top-level bookmarks, before the first one that leads to a page
            // after the insertion point.
            let index = old_outline
                .items
                .iter()
                .position(|b| matches!(b.target, Target::Page { page, .. } if page >= at))
                .unwrap_or(old_outline.items.len());
            edit::insert_items(doc, None, index, items)?;
        }
    }

    let source_rules = page_labels::read_rules(&source.doc)?;
    let mut rules: Vec<LabelRule> = old_rules
        .iter()
        .map(|r| LabelRule {
            start_page: if r.start_page >= at {
                r.start_page + n
            } else {
                r.start_page
            },
            ..r.clone()
        })
        .collect();
    if options.labels == InsertLabels::KeepSource && !source_rules.is_empty() {
        let inserted: Vec<PageLabel> = picks
            .iter()
            .map(|p| labels::page_label(&source_rules, p.page))
            .collect();
        rules.extend(labels::rules_for(&inserted, at));
        // The page that followed the insertion point continues its own numbering.
        if at < old_count && !old_rules.iter().any(|r| r.start_page == at) {
            let next = labels::page_label(&old_rules, at);
            rules.extend(labels::rules_for(&[next], at + n));
        }
        if old_rules.is_empty() && at > 0 {
            rules.push(LabelRule::decimal_from_one(0));
        }
    }
    rules.sort_by_key(|r| r.start_page);
    if labels::is_plain(&rules) && old_rules.is_empty() {
        rules.clear();
    }
    // MuPDF shifts stored labels while inserting pages; this writes the intended ones.
    page_labels::set_rules(doc, rules)?;
    Ok(engine.report)
}

/// MuPDF 1.27 creates new documents with their `/Info` dictionary inside the catalog
/// (`pdf_create_document`); readers look for it in the trailer.
fn move_info_to_trailer(doc: &mut PdfDocument) -> Result<()> {
    let mut catalog = doc.catalog()?;
    if let Some(info) = catalog.get_dict("Info")? {
        catalog.dict_delete("Info")?;
        let info = doc.add_object(&info)?;
        doc.trailer()?.dict_put("Info", info)?;
    }
    Ok(())
}

/// `[page /XYZ null null null]`: the top of the page at the reader's zoom.
fn xyz_dest(doc: &PdfDocument, page: usize) -> Result<PdfObject> {
    let page_no = i32::try_from(page).map_err(|_| Error::PageOutOfRange(page))?;
    let mut dest = doc.new_array_with_capacity(5)?;
    dest.array_push(doc.find_page(page_no)?)?;
    dest.array_push(PdfObject::new_name("XYZ")?)?;
    for _ in 0..3 {
        dest.array_push(PdfObject::new_null())?;
    }
    Ok(dest)
}

/// Page keys never copied: the page tree link, keys read with inheritance and written
/// explicitly, annotations (copied one by one), and keys that only make sense with
/// structures that are not copied (article beads, the structure tree).
const PAGE_KEYS_SKIPPED: [&[u8]; 9] = [
    b"Type",
    b"Parent",
    b"Annots",
    b"Resources",
    b"MediaBox",
    b"CropBox",
    b"Rotate",
    b"B",
    b"StructParents",
];

/// Per-source state while copying.
struct SourceState {
    copier: Copier,
    /// Source page index to destination page object number.
    pages: HashMap<usize, i32>,
    /// Name renames (old to new).
    renames: HashMap<Vec<u8>, Vec<u8>>,
    /// Named destinations to copy, by their name in the result.
    names: Vec<(Vec<u8>, PdfObject)>,
    /// Copied annotations, for the destination-reference pass.
    annots: Vec<PdfObject>,
    first_output_page: Option<usize>,
}

struct Engine<'a> {
    sources: &'a [MergeSource],
    /// Each source's prefix for renamed names.
    prefixes: Vec<String>,
    states: Vec<SourceState>,
    taken_names: HashSet<Vec<u8>>,
    taken_fields: HashSet<String>,
    report: MergeReport,
}

impl<'a> Engine<'a> {
    fn new(
        dst: &PdfDocument,
        sources: &'a [MergeSource],
        prefixes: Vec<String>,
    ) -> Result<Engine<'a>> {
        let taken_names = names::collect(dst)?.into_iter().map(|(n, _)| n).collect();
        let taken_fields = forms::top_level_names(dst)?;
        Ok(Engine {
            sources,
            prefixes,
            states: Vec::new(),
            taken_names,
            taken_fields,
            report: MergeReport::default(),
        })
    }

    /// Copies the picked pages into `dst` before page `at`, with their annotations, named
    /// destinations, form fields and layers. Returns each source's bookmarks, copied
    /// (empty when `bookmarks` is `Drop`).
    fn run(
        &mut self,
        dst: &mut PdfDocument,
        at: usize,
        picks: &[PagePick],
        bookmarks: BookmarkMode,
        progress: &mut dyn FnMut(usize, usize) -> bool,
    ) -> Result<Vec<Vec<NewBookmark>>> {
        self.validate(picks)?;
        self.states = self
            .sources
            .iter()
            .map(|_| SourceState {
                copier: Copier::new(),
                pages: HashMap::new(),
                renames: HashMap::new(),
                names: Vec::new(),
                annots: Vec::new(),
                first_output_page: None,
            })
            .collect();

        // 1. Reserve the new pages, and mark what must not be copied.
        let mut targets = Vec::with_capacity(picks.len());
        for (k, pick) in picks.iter().enumerate() {
            let target = dst.create_object()?;
            let src = &self.sources[pick.source].doc;
            let src_num = src.find_page(page_no(pick.page)?)?.as_indirect()?;
            let state = &mut self.states[pick.source];
            state.copier.map_to(src_num, target.as_indirect()?);
            state.pages.insert(pick.page, target.as_indirect()?);
            state.first_output_page.get_or_insert(at + k);
            targets.push(target);
        }
        for (s, source) in self.sources.iter().enumerate() {
            if self.states[s].pages.is_empty() {
                continue;
            }
            mark_uncopied(&source.doc, &mut self.states[s])?;
            self.plan_names(s)?;
        }

        // 2. Pages, in output order.
        for (k, (pick, target)) in picks.iter().zip(targets).enumerate() {
            self.copy_page(dst, pick, target, at + k)?;
            if !progress(k + 1, picks.len()) {
                return Err(Error::Cancelled);
            }
        }

        // 3. Named destinations, form fields, layers and bookmarks of each source.
        let mut new_names = Vec::new();
        let mut copied_bookmarks = Vec::new();
        for (s, source) in self.sources.iter().enumerate() {
            let state = &mut self.states[s];
            if state.pages.is_empty() {
                copied_bookmarks.push(Vec::new());
                continue;
            }
            let mut source_names = Vec::new();
            for (name, value) in std::mem::take(&mut state.names) {
                if let Some(copied) = state.copier.copy(dst, &value)? {
                    source_names.push((name, copied));
                }
            }
            state.copier.finish(dst)?;
            self.report.renamed_fields += forms::merge_fields(
                dst,
                &source.doc,
                &mut state.copier,
                &self.prefixes[s],
                &mut self.taken_fields,
            )?;
            merge_layers(dst, &source.doc, &mut state.copier)?;
            let mut items = Vec::new();
            if bookmarks != BookmarkMode::Drop {
                let copied = bookmarks::copy(dst, &source.doc, &mut state.copier, &state.pages)?;
                self.report.dropped_bookmarks += copied.dropped;
                items = copied.items;
            }
            state.copier.finish(dst)?;

            // 4. References to renamed names, name objects and page indices.
            let mut fixer = names::DestFixer {
                renames: &state.renames,
                pages: &state.pages,
                seen: HashSet::new(),
            };
            for annot in &state.annots {
                if let Some(mut dict) = annot.resolve()? {
                    fixer.fix_item(dst, &mut dict)?;
                }
            }
            fix_bookmarks(dst, &mut fixer, &mut items)?;
            for (_, value) in &mut source_names {
                fixer.fix_value(dst, value)?;
            }
            new_names.extend(source_names);
            copied_bookmarks.push(items);
        }
        names::add_to_tree(dst, new_names)?;
        self.report.pages += picks.len();
        Ok(copied_bookmarks)
    }

    fn validate(&self, picks: &[PagePick]) -> Result<()> {
        let mut seen = HashSet::new();
        for pick in picks {
            let source = self
                .sources
                .get(pick.source)
                .ok_or_else(|| Error::InvalidArgument(format!("no source {}", pick.source)))?;
            let count = usize::try_from(source.doc.page_count()?).unwrap_or(0);
            if pick.page >= count {
                return Err(Error::PageOutOfRange(pick.page));
            }
            if pick.rotate % 90 != 0 {
                return Err(Error::InvalidArgument(format!(
                    "pages can only be rotated in steps of 90 degrees, not {}",
                    pick.rotate
                )));
            }
            if !seen.insert((pick.source, pick.page)) {
                return Err(Error::InvalidArgument(format!(
                    "page {} of source {} is picked twice",
                    pick.page + 1,
                    pick.source + 1
                )));
            }
        }
        Ok(())
    }

    /// Decides which of a source's named destinations come along, and under what name.
    fn plan_names(&mut self, s: usize) -> Result<()> {
        let src = &self.sources[s].doc;
        let state = &mut self.states[s];
        for (name, value) in names::collect(src)? {
            let Some(dest) = names::resolve_dest(src, &value)? else {
                continue;
            };
            let kept = match names::dest_page(&dest)? {
                names::PageRef::Object(num) => state.copier.mapped(num).is_some(),
                names::PageRef::Index(i) => state.pages.contains_key(&i),
                names::PageRef::Unknown => false,
            };
            if !kept {
                continue;
            }
            let final_name = if self.taken_names.contains(&name) {
                let new = names::unique_name(&name, &self.prefixes[s], &self.taken_names);
                state.renames.insert(name, new.clone());
                self.report.renamed_destinations += 1;
                new
            } else {
                name
            };
            self.taken_names.insert(final_name.clone());
            // An index-based destination is fixed up after copying, like links.
            state.names.push((final_name, value));
        }
        Ok(())
    }

    fn copy_page(
        &mut self,
        dst: &mut PdfDocument,
        pick: &PagePick,
        mut target: PdfObject,
        position: usize,
    ) -> Result<()> {
        let src = &self.sources[pick.source].doc;
        let state = &mut self.states[pick.source];
        let src_page = src.find_page(page_no(pick.page)?)?;
        let mut page = dst.new_dict()?;
        page.dict_put("Type", PdfObject::new_name("Page")?)?;
        for key in ["Resources", "MediaBox", "CropBox"] {
            if let Some(value) = src_page.get_dict_inheritable(key)?
                && let Some(copied) = state.copier.copy(dst, &value)?
            {
                page.dict_put(key, copied)?;
            }
        }
        if page.get_dict("MediaBox")?.is_none() {
            // What readers assume for a page without one (US Letter).
            page.dict_put(
                "MediaBox",
                rect_array(dst, crate::geometry::Rect::new(0.0, 0.0, 612.0, 792.0))?,
            )?;
        }
        let rotate = match src_page.get_dict_inheritable("Rotate")? {
            Some(r) if r.is_number()? => r.as_int()?,
            _ => 0,
        };
        let rotate = normalize_rotation(normalize_rotation(rotate) + pick.rotate);
        if rotate != 0 {
            page.dict_put("Rotate", PdfObject::new_int(rotate)?)?;
        }
        for (key, value) in dict_entries(&src_page)? {
            if PAGE_KEYS_SKIPPED.contains(&key.as_name()?.as_slice()) {
                continue;
            }
            if let Some(copied) = state.copier.copy(dst, &value)? {
                page.dict_put(key, copied)?;
            }
        }

        if let Some(annots) = src_page.get_dict("Annots")?
            && annots.is_array()?
        {
            let mut out = dst.new_array()?;
            for annot in array_items(&annots)? {
                if annot.is_indirect()? && state.copier.is_dropped(annot.as_indirect()?) {
                    continue;
                }
                let Some(dict) = annot.resolve()?.filter(|d| d.is_dict().unwrap_or(false)) else {
                    continue;
                };
                let is_link = dict
                    .get_dict("Subtype")?
                    .is_some_and(|s| s.as_name().ok().as_deref() == Some(b"Link"));
                if is_link && bookmarks::target_removed(src, &dict, &state.copier, &state.pages)? {
                    self.report.dropped_links += 1;
                    continue;
                }
                let Some(copied) = state.copier.copy(dst, &annot)? else {
                    continue;
                };
                // Annotations are indirect objects (some files inline them).
                let copied = if copied.is_indirect()? {
                    copied
                } else {
                    dst.add_object(&copied)?
                };
                state.annots.push(copied.try_clone()?);
                out.array_push(copied)?;
            }
            if out.len()? > 0 {
                page.dict_put("Annots", out)?;
            }
        }

        target.write_object(&page)?;
        dst.insert_page(page_no(position)?, &target)?;
        state.copier.finish(dst)
    }
}

fn page_no(page: usize) -> Result<i32> {
    i32::try_from(page).map_err(|_| Error::PageOutOfRange(page))
}

/// Marks the source objects a copy must never reach: the catalog, the page tree's inner
/// nodes, the structure tree and outline roots, and the pages that were not picked with
/// their annotations.
fn mark_uncopied(src: &PdfDocument, state: &mut SourceState) -> Result<()> {
    let catalog = src.catalog()?;
    if let Some(root) = src.trailer()?.get_dict("Root")?
        && root.is_indirect()?
    {
        state.copier.drop_object(root.as_indirect()?);
    }
    for key in ["StructTreeRoot", "Outlines", "AcroForm", "Names"] {
        if let Some(obj) = catalog.get_dict(key)?
            && obj.is_indirect()?
        {
            state.copier.drop_object(obj.as_indirect()?);
        }
    }
    if let Some(pages) = catalog.get_dict("Pages")? {
        let mut visited = HashSet::new();
        mark_page_tree(&pages, 0, &mut visited, &mut state.copier)?;
    }
    for page in 0..usize::try_from(src.page_count()?).unwrap_or(0) {
        if state.pages.contains_key(&page) {
            continue;
        }
        let obj = src.find_page(page_no(page)?)?;
        if obj.is_indirect()? {
            state.copier.drop_object(obj.as_indirect()?);
        }
        if let Some(annots) = obj.get_dict("Annots")?
            && annots.is_array()?
        {
            for annot in array_items(&annots)? {
                if annot.is_indirect()? {
                    state.copier.drop_object(annot.as_indirect()?);
                }
            }
        }
    }
    Ok(())
}

fn mark_page_tree(
    node: &PdfObject,
    depth: u32,
    visited: &mut HashSet<i32>,
    copier: &mut Copier,
) -> Result<()> {
    let Some(kids) = node
        .get_dict("Kids")?
        .filter(|k| k.is_array().unwrap_or(false))
    else {
        return Ok(());
    };
    if depth > 64 {
        return Ok(());
    }
    if node.is_indirect()? {
        if !visited.insert(node.as_indirect()?) {
            return Ok(());
        }
        copier.drop_object(node.as_indirect()?);
    }
    for kid in array_items(&kids)? {
        mark_page_tree(&kid, depth + 1, visited, copier)?;
    }
    Ok(())
}

/// Adds the source's optional content groups (layers) to the document's list, with their
/// default visibility and panel order.
fn merge_layers(dst: &mut PdfDocument, src: &PdfDocument, copier: &mut Copier) -> Result<()> {
    let Some(src_props) = src.catalog()?.get_dict("OCProperties")? else {
        return Ok(());
    };
    let Some(src_groups) = src_props
        .get_dict("OCGs")?
        .filter(|g| g.is_array().unwrap_or(false))
    else {
        return Ok(());
    };
    let mut catalog = dst.catalog()?;
    let mut props = match catalog.get_dict("OCProperties")? {
        Some(p) if p.is_dict()? => p,
        _ => {
            let p = dst.new_dict()?;
            let p = dst.add_object(&p)?;
            catalog.dict_put("OCProperties", p.try_clone()?)?;
            p
        }
    };
    let mut groups = child_array(dst, &mut props, "OCGs")?;
    let mut present: HashSet<i32> = HashSet::new();
    for g in array_items(&groups)? {
        if g.is_indirect()? {
            present.insert(g.as_indirect()?);
        }
    }
    for group in array_items(&src_groups)? {
        if let Some(copied) = copier.copy(dst, &group)?
            && (!copied.is_indirect()? || present.insert(copied.as_indirect()?))
        {
            groups.array_push(copied)?;
        }
    }
    let Some(src_config) = src_props.get_dict("D")? else {
        return Ok(());
    };
    let mut config = child_dict(dst, &mut props, "D")?;
    for key in ["ON", "OFF", "Order", "RBGroups", "Locked"] {
        if let Some(values) = src_config.get_dict(key)?
            && values.is_array()?
        {
            let mut target = child_array(dst, &mut config, key)?;
            for value in array_items(&values)? {
                if let Some(copied) = copier.copy(dst, &value)? {
                    target.array_push(copied)?;
                }
            }
        }
    }
    // A source whose layers start hidden unless listed under /ON.
    if src_config
        .get_dict("BaseState")?
        .is_some_and(|b| b.as_name().ok().as_deref() == Some(b"OFF"))
    {
        let on: HashSet<i32> = match src_config.get_dict("ON")? {
            Some(list) => array_items(&list)?
                .iter()
                .filter_map(|g| g.as_indirect().ok())
                .collect(),
            _ => HashSet::new(),
        };
        let mut off = child_array(dst, &mut config, "OFF")?;
        for group in array_items(&src_groups)? {
            if group.is_indirect()?
                && !on.contains(&group.as_indirect()?)
                && let Some(copied) = copier.copy(dst, &group)?
            {
                off.array_push(copied)?;
            }
        }
    }
    copier.finish(dst)
}

fn fix_bookmarks(
    dst: &PdfDocument,
    fixer: &mut names::DestFixer<'_>,
    items: &mut [NewBookmark],
) -> Result<()> {
    for item in items {
        fixer.fix_item(dst, &mut item.dict)?;
        fix_bookmarks(dst, fixer, &mut item.children)?;
    }
    Ok(())
}
