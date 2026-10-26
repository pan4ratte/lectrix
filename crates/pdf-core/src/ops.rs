//! Document operations (AGENTS.md section 7). Every change to a document is one
//! [`Operation`], applied as one MuPDF journal step so that undo and redo work on whole
//! user actions and the Edit menu can name them.

use std::fmt;
use std::path::PathBuf;

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::annot::repair::RepairChange;
use crate::annot::{self, AnnotationEdit, Kind, NewAnnotation};
use crate::error::{Error, Result};
use crate::geometry::normalize_rotation;
use crate::labels::{self, LabelRule};
use crate::merge::{self, InsertOptions, MergeReport, MergeSource};
use crate::outline::{ViewDest, edit};
use crate::pages::{self, DeleteReport};
use crate::properties::{self, Description};

/// One annotation's change in [`Operation::UpdateAnnotations`].
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationUpdate {
    pub page: usize,
    pub id: u32,
    pub edit: AnnotationEdit,
}

/// The file pages are inserted from.
#[derive(Clone, PartialEq)]
pub struct InsertSource {
    pub path: PathBuf,
    /// The password that opened it, if it is encrypted.
    pub password: Option<String>,
    /// Pages to insert (0-based, in this order); empty inserts every page.
    pub pages: Vec<usize>,
}

impl fmt::Debug for InsertSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print the password (operations end up in logs).
        f.debug_struct("InsertSource")
            .field("path", &self.path)
            .field("password", &self.password.as_ref().map(|_| "(set)"))
            .field("pages", &self.pages)
            .finish()
    }
}

/// What applying an operation produced.
#[derive(Debug, Default)]
pub(crate) struct Applied {
    /// The id of the object it created (the new bookmark or annotation).
    pub created: Option<u32>,
    /// What inserting pages did.
    pub report: Option<MergeReport>,
    /// What repairing annotations changed.
    pub repairs: Option<Vec<RepairChange>>,
    /// What deleting pages took out with them.
    pub deleted: Option<DeleteReport>,
    /// What it found out about destinations written with a page number.
    pub numbered_dests: NumberedDests,
}

/// What an operation can have done to the bookmarks as the panel shows them. The session
/// reads the outline again only as far as this says: reading all of it takes a tenth of a
/// second in a file with thousands of bookmarks, and most edits cannot have touched it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutlineEffect {
    /// Nothing.
    None,
    /// The pages they lead to have other numbers: pages changed places.
    PageOrder,
    /// The places they name on the pages that changed (a destination's point is given in
    /// view space, which turns with the page).
    PageGeometry,
    /// Anything: the outline is read again.
    Any,
}

impl OutlineEffect {
    /// The effect of two steps taken as one. Different kinds are not worked out together.
    pub(crate) fn and(self, other: OutlineEffect) -> OutlineEffect {
        match (self, other) {
            (OutlineEffect::None, e) | (e, OutlineEffect::None) => e,
            (a, b) if a == b => a,
            _ => OutlineEffect::Any,
        }
    }
}

/// What an operation found out about destinations written with a page number, which
/// moving pages has to look for unless the document is known to have none (`pages.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum NumberedDests {
    /// Nothing new.
    #[default]
    AsBefore,
    /// The document had none before the operation, and has none now.
    NoneBefore,
    /// It has none now.
    NoneNow,
    /// It may have some now.
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    /// Turns pages by `degrees` (a multiple of 90; positive is clockwise) by changing
    /// their `/Rotate`. This modifies the document, unlike rotating the view.
    RotatePages {
        pages: Vec<usize>,
        degrees: i32,
    },
    /// Moves pages (they keep their order in the document) into the gap `to` of the
    /// current order: 0 is before the first page, the page count after the last. Page
    /// labels stay with the positions. Pages that end up where they were change nothing.
    MovePages {
        pages: Vec<usize>,
        to: usize,
    },
    /// Deletes pages, with the links, bookmarks, named destinations and form fields that
    /// lead to them or were on them. At least one page stays.
    DeletePages {
        pages: Vec<usize>,
    },
    /// Adds a bookmark as child number `index` of `parent` (`None`: top level).
    AddBookmark {
        parent: Option<u32>,
        index: usize,
        title: String,
        dest: ViewDest,
    },
    RenameBookmark {
        id: u32,
        title: String,
    },
    /// Moves a bookmark to child number `index` of `parent`, counted after it has left
    /// its old place.
    MoveBookmark {
        id: u32,
        parent: Option<u32>,
        index: usize,
    },
    /// Removes a bookmark and its children.
    DeleteBookmark {
        id: u32,
    },
    /// Points a bookmark at a new place ("Set destination to current view").
    SetBookmarkDestination {
        id: u32,
        dest: ViewDest,
    },
    /// Gives a bookmark's title a colour (`/C`), or takes it away (`None`).
    SetBookmarkColor {
        id: u32,
        color: Option<[f32; 3]>,
    },
    /// Replaces the page labels with `rules` (a rule at the first page is added if
    /// missing); an empty list removes them. Rules already stored that way change nothing.
    SetPageLabels {
        rules: Vec<LabelRule>,
    },
    /// Inserts pages of another file before page `at` (the page count appends), with
    /// their annotations, links, named destinations and form fields (section 6.4).
    InsertPages {
        source: InsertSource,
        at: usize,
        options: InsertOptions,
    },
    /// Creates an annotation to the write profile (section 5.1).
    AddAnnotation {
        annotation: NewAnnotation,
    },
    /// Creates several in one undo step (text selected across pages).
    AddAnnotations {
        annotations: Vec<NewAnnotation>,
    },
    /// Changes annotation `id` (object number) on `page`.
    UpdateAnnotation {
        page: usize,
        id: u32,
        edit: AnnotationEdit,
    },
    /// Deletes annotation `id` on `page` with its popup and replies.
    DeleteAnnotation {
        page: usize,
        id: u32,
    },
    /// Changes several annotations in one undo step (selected together, section 6.5).
    UpdateAnnotations {
        updates: Vec<AnnotationUpdate>,
    },
    /// Deletes several annotations (page, object number) in one undo step, each with its
    /// popup and replies. One already gone with another (a reply of one deleted before
    /// it) is passed over.
    DeleteAnnotations {
        annotations: Vec<(usize, u32)>,
    },
    /// Adds a reply by `author` to annotation `parent` on `page` (ADR 0012).
    AddReply {
        page: usize,
        parent: u32,
        text: String,
        author: String,
    },
    /// Fixes every problem the repair scan finds that can be fixed (section 5.3).
    RepairAnnotations,
    /// Changes the document's title, author, subject and keywords (its properties,
    /// section 6.10), in the information dictionary and the XMP metadata alike.
    SetDescription {
        description: Description,
    },
}

impl Operation {
    /// Human-readable name of the journal step, shown as "Undo <name>".
    pub fn name(&self) -> String {
        match self {
            Operation::RotatePages { pages, .. } if pages.len() == 1 => "Rotate page".into(),
            Operation::RotatePages { .. } => "Rotate pages".into(),
            Operation::MovePages { pages, .. } if pages.len() == 1 => "Move page".into(),
            Operation::MovePages { .. } => "Move pages".into(),
            Operation::DeletePages { pages } if pages.len() == 1 => "Delete page".into(),
            Operation::DeletePages { .. } => "Delete pages".into(),
            Operation::AddBookmark { .. } => "Add bookmark".into(),
            Operation::RenameBookmark { .. } => "Rename bookmark".into(),
            Operation::MoveBookmark { .. } => "Move bookmark".into(),
            Operation::DeleteBookmark { .. } => "Delete bookmark".into(),
            Operation::SetBookmarkDestination { .. } => "Change bookmark destination".into(),
            Operation::SetBookmarkColor { .. } => "Change bookmark colour".into(),
            Operation::SetPageLabels { rules } if rules.is_empty() => "Remove page labels".into(),
            Operation::SetPageLabels { .. } => "Change page labels".into(),
            Operation::InsertPages { source, .. } if source.pages.len() == 1 => {
                "Insert page".into()
            }
            Operation::InsertPages { .. } => "Insert pages".into(),
            Operation::AddAnnotation { annotation } => {
                format!("Add {}", annotation.body.kind().label())
            }
            Operation::AddAnnotations { annotations } => match annotations.first() {
                Some(a) => format!("Add {}", a.body.kind().label()),
                None => "Add annotations".into(),
            },
            Operation::UpdateAnnotation { edit, .. } => format!("{} annotation", edit.verb()),
            Operation::DeleteAnnotation { .. } => "Delete annotation".into(),
            Operation::UpdateAnnotations { updates } => match updates.first() {
                Some(u) if updates.len() > 1 => {
                    format!("{} {} annotations", u.edit.verb(), updates.len())
                }
                Some(u) => format!("{} annotation", u.edit.verb()),
                None => "Change annotations".into(),
            },
            Operation::DeleteAnnotations { annotations } if annotations.len() == 1 => {
                "Delete annotation".into()
            }
            Operation::DeleteAnnotations { annotations } => {
                format!("Delete {} annotations", annotations.len())
            }
            Operation::AddReply { .. } => "Add reply".into(),
            Operation::RepairAnnotations => "Repair annotations".into(),
            Operation::SetDescription { .. } => "Change properties".into(),
        }
    }

    /// The step name with what the document knows: "Move note" rather than "Move
    /// annotation".
    pub(crate) fn step_name(&self, doc: &PdfDocument) -> String {
        let label = |page: usize, id: u32| {
            annot::subtype_of(doc, page, id).map(|s| match Kind::from_subtype(&s) {
                Some(k) => k.label().to_owned(),
                None => s.to_lowercase(),
            })
        };
        match self {
            Operation::UpdateAnnotation { page, id, .. }
                if annot::reply::is_reply_at(doc, *page, *id) =>
            {
                "Edit reply".into()
            }
            Operation::DeleteAnnotation { page, id }
                if annot::reply::is_reply_at(doc, *page, *id) =>
            {
                "Delete reply".into()
            }
            Operation::UpdateAnnotation { page, id, edit } => {
                match (label(*page, *id), edit.kind) {
                    (Some(l), Some(k)) if k.kind().label() != l => {
                        format!("Change {l} to {}", k.kind().label())
                    }
                    (Some(l), _) => format!("{} {l}", edit.verb()),
                    (None, _) => self.name(),
                }
            }
            Operation::DeleteAnnotation { page, id } => match label(*page, *id) {
                Some(l) => format!("Delete {l}"),
                None => self.name(),
            },
            _ => self.name(),
        }
    }

    /// True if the operation needs the "annotate" permission (bit 6).
    pub(crate) fn needs_annotate(&self) -> bool {
        matches!(
            self,
            Operation::AddAnnotation { .. }
                | Operation::AddAnnotations { .. }
                | Operation::UpdateAnnotation { .. }
                | Operation::DeleteAnnotation { .. }
                | Operation::UpdateAnnotations { .. }
                | Operation::DeleteAnnotations { .. }
                | Operation::AddReply { .. }
                | Operation::RepairAnnotations
        )
    }

    /// True if the operation may leave a file that claims PDF/A no longer complying, so
    /// that saving it takes the claim out (section 5.4): whatever writes or changes an
    /// annotation (appearances use transparency and a font that is not embedded, and
    /// Lectrix checks none of the standard's other rules for them), pages that come from
    /// another file, and pages deleted (a tagged file's structure tree is left describing
    /// them). Bookmarks, page labels, page rotation and the order of the pages are no part
    /// of what PDF/A constrains.
    pub(crate) fn may_break_pdfa(&self) -> bool {
        self.needs_annotate()
            || matches!(
                self,
                Operation::InsertPages { .. } | Operation::DeletePages { .. }
            )
    }

    /// True if the operation needs the "assemble" permission (bit 4 or 11), which covers
    /// rotating, moving, deleting and inserting pages, creating outline items and other
    /// document-level changes such as page labels.
    pub(crate) fn needs_assemble(&self) -> bool {
        match self {
            Operation::RotatePages { .. }
            | Operation::MovePages { .. }
            | Operation::DeletePages { .. }
            | Operation::AddBookmark { .. }
            | Operation::RenameBookmark { .. }
            | Operation::MoveBookmark { .. }
            | Operation::DeleteBookmark { .. }
            | Operation::SetBookmarkDestination { .. }
            | Operation::SetBookmarkColor { .. }
            | Operation::SetPageLabels { .. }
            | Operation::InsertPages { .. }
            | Operation::SetDescription { .. } => true,
            Operation::AddAnnotation { .. }
            | Operation::AddAnnotations { .. }
            | Operation::UpdateAnnotation { .. }
            | Operation::DeleteAnnotation { .. }
            | Operation::UpdateAnnotations { .. }
            | Operation::DeleteAnnotations { .. }
            | Operation::AddReply { .. }
            | Operation::RepairAnnotations => false,
        }
    }

    /// What the operation can do to the bookmarks as the panel shows them.
    pub(crate) fn outline_effect(&self) -> OutlineEffect {
        match self {
            Operation::RotatePages { .. } => OutlineEffect::PageGeometry,
            Operation::MovePages { .. } => OutlineEffect::PageOrder,
            // Deleted pages take bookmarks with them, and inserted ones bring theirs.
            Operation::DeletePages { .. }
            | Operation::InsertPages { .. }
            | Operation::AddBookmark { .. }
            | Operation::RenameBookmark { .. }
            | Operation::MoveBookmark { .. }
            | Operation::DeleteBookmark { .. }
            | Operation::SetBookmarkDestination { .. }
            | Operation::SetBookmarkColor { .. } => OutlineEffect::Any,
            Operation::SetPageLabels { .. }
            | Operation::AddAnnotation { .. }
            | Operation::AddAnnotations { .. }
            | Operation::UpdateAnnotation { .. }
            | Operation::DeleteAnnotation { .. }
            | Operation::UpdateAnnotations { .. }
            | Operation::DeleteAnnotations { .. }
            | Operation::AddReply { .. }
            | Operation::RepairAnnotations
            | Operation::SetDescription { .. } => OutlineEffect::None,
        }
    }

    /// Checks the operation against the document before anything is changed.
    pub(crate) fn validate(&self, page_count: usize) -> Result<()> {
        match self {
            Operation::RotatePages { pages, degrees } => {
                if pages.is_empty() {
                    return Err(Error::InvalidArgument("no pages to rotate".into()));
                }
                if degrees % 90 != 0 {
                    return Err(Error::InvalidArgument(format!(
                        "pages can only be rotated in steps of 90 degrees, not {degrees}"
                    )));
                }
                if let Some(&bad) = pages.iter().find(|&&p| p >= page_count) {
                    return Err(Error::PageOutOfRange(bad));
                }
                Ok(())
            }
            Operation::MovePages { pages, to } => {
                check_pages(pages, page_count)?;
                if *to > page_count {
                    return Err(Error::PageOutOfRange(*to));
                }
                Ok(())
            }
            Operation::DeletePages { pages } => {
                check_pages(pages, page_count)?;
                let distinct: std::collections::HashSet<usize> = pages.iter().copied().collect();
                if distinct.len() >= page_count {
                    return Err(Error::InvalidArgument(
                        "a document needs at least one page".into(),
                    ));
                }
                Ok(())
            }
            Operation::AddBookmark { title, dest, .. } => {
                edit::clean_title(title)?;
                check_dest(dest, page_count)
            }
            Operation::RenameBookmark { title, .. } => edit::clean_title(title).map(drop),
            Operation::SetBookmarkDestination { dest, .. } => check_dest(dest, page_count),
            Operation::SetBookmarkColor { color, .. } => match color {
                Some(c) if c.iter().any(|v| !(0.0..=1.0).contains(v)) => Err(
                    Error::InvalidArgument(format!("bookmark colour out of range: {c:?}")),
                ),
                _ => Ok(()),
            },
            Operation::MoveBookmark { .. } | Operation::DeleteBookmark { .. } => Ok(()),
            Operation::SetPageLabels { rules } if rules.is_empty() => Ok(()),
            Operation::SetPageLabels { rules } => {
                labels::normalize_rules(rules.clone(), page_count).map(drop)
            }
            Operation::InsertPages { at, .. } => {
                if *at > page_count {
                    return Err(Error::PageOutOfRange(*at));
                }
                Ok(())
            }
            Operation::AddAnnotation { annotation } => {
                if annotation.page >= page_count {
                    return Err(Error::PageOutOfRange(annotation.page));
                }
                annotation.validate()
            }
            Operation::AddAnnotations { annotations } => {
                if annotations.is_empty() {
                    return Err(Error::InvalidArgument("no annotations to add".into()));
                }
                for a in annotations {
                    if a.page >= page_count {
                        return Err(Error::PageOutOfRange(a.page));
                    }
                    a.validate()?;
                }
                Ok(())
            }
            Operation::UpdateAnnotation { page, edit, .. } => {
                if *page >= page_count {
                    return Err(Error::PageOutOfRange(*page));
                }
                edit.validate()
            }
            Operation::DeleteAnnotation { page, .. } => {
                if *page >= page_count {
                    return Err(Error::PageOutOfRange(*page));
                }
                Ok(())
            }
            Operation::UpdateAnnotations { updates } => {
                if updates.is_empty() {
                    return Err(Error::InvalidArgument("no annotations to change".into()));
                }
                for u in updates {
                    if u.page >= page_count {
                        return Err(Error::PageOutOfRange(u.page));
                    }
                    u.edit.validate()?;
                }
                Ok(())
            }
            Operation::DeleteAnnotations { annotations } => {
                if annotations.is_empty() {
                    return Err(Error::InvalidArgument("no annotations to delete".into()));
                }
                match annotations.iter().find(|(page, _)| *page >= page_count) {
                    Some(&(bad, _)) => Err(Error::PageOutOfRange(bad)),
                    None => Ok(()),
                }
            }
            Operation::AddReply { page, text, .. } => {
                if *page >= page_count {
                    return Err(Error::PageOutOfRange(*page));
                }
                if text.trim().is_empty() {
                    return Err(Error::InvalidArgument("a reply needs some text".into()));
                }
                Ok(())
            }
            Operation::RepairAnnotations | Operation::SetDescription { .. } => Ok(()),
        }
    }

    /// Applies the operation. With `by_reference`, the caller knows that no destination in
    /// the document is written with a page number (`pages.rs`). The caller wraps this in a
    /// journal step.
    pub(crate) fn apply(&self, doc: &mut PdfDocument, by_reference: bool) -> Result<Applied> {
        if let Operation::InsertPages {
            source,
            at,
            options,
        } = self
        {
            let from = MergeSource::open(&source.path, source.password.as_deref())?;
            let report = merge::insert_pages(doc, *at, &from, &source.pages, *options)?;
            return Ok(Applied {
                report: Some(report),
                // What comes from another file was not gone through.
                numbered_dests: NumberedDests::Unknown,
                ..Applied::default()
            });
        }
        if let Operation::DeletePages { pages } = self {
            return Ok(Applied {
                deleted: Some(pages::delete_pages(doc, pages)?),
                // Not `NoneBefore`: the links on the deleted pages were not looked at.
                numbered_dests: NumberedDests::NoneNow,
                ..Applied::default()
            });
        }
        if let Operation::MovePages { pages, to } = self {
            let report = pages::move_pages(doc, pages, *to, by_reference)?;
            return Ok(Applied {
                numbered_dests: match report.renumbered {
                    Some(0) => NumberedDests::NoneBefore,
                    Some(_) => NumberedDests::NoneNow,
                    None => NumberedDests::AsBefore,
                },
                ..Applied::default()
            });
        }
        if let Operation::RepairAnnotations = self {
            return Ok(Applied {
                repairs: Some(annot::repair::repair(doc)?),
                ..Applied::default()
            });
        }
        let created = self.apply_edit(doc)?;
        Ok(Applied {
            created,
            ..Applied::default()
        })
    }

    /// Applies an edit and returns the id of the object it created, if any.
    fn apply_edit(&self, doc: &mut PdfDocument) -> Result<Option<u32>> {
        match self {
            Operation::RotatePages { pages, degrees } => {
                let mut pages = pages.clone();
                pages.sort_unstable();
                pages.dedup();
                for page in pages {
                    rotate_page(doc, page, *degrees)?;
                }
                Ok(None)
            }
            Operation::AddBookmark {
                parent,
                index,
                title,
                dest,
            } => edit::add(doc, *parent, *index, title, dest).map(Some),
            Operation::RenameBookmark { id, title } => edit::rename(doc, *id, title).map(|()| None),
            Operation::MoveBookmark { id, parent, index } => {
                edit::move_to(doc, *id, *parent, *index).map(|()| None)
            }
            Operation::DeleteBookmark { id } => edit::delete(doc, *id).map(|()| None),
            Operation::SetBookmarkDestination { id, dest } => {
                edit::set_destination(doc, *id, dest).map(|()| None)
            }
            Operation::SetBookmarkColor { id, color } => {
                edit::set_color(doc, *id, *color).map(|()| None)
            }
            Operation::SetPageLabels { rules } => {
                labels::set_rules(doc, rules.clone()).map(|_| None)
            }
            Operation::SetDescription { description } => {
                properties::write_description(doc, description).map(|()| None)
            }
            Operation::AddAnnotation { annotation } => {
                let created = annot::create(doc, annotation)?;
                Ok(u32::try_from(created.xref).ok())
            }
            Operation::AddAnnotations { annotations } => {
                let mut first = None;
                for a in annotations {
                    let created = annot::create(doc, a)?;
                    first = first.or(u32::try_from(created.xref).ok());
                }
                Ok(first)
            }
            Operation::UpdateAnnotation { page, id, edit } => {
                annot::edit(doc, *page, *id, edit).map(|()| None)
            }
            Operation::DeleteAnnotation { page, id } => {
                annot::delete(doc, *page, *id).map(|()| None)
            }
            Operation::UpdateAnnotations { updates } => {
                for u in updates {
                    annot::edit(doc, u.page, u.id, &u.edit)?;
                }
                Ok(None)
            }
            Operation::DeleteAnnotations { annotations } => {
                for &(page, id) in annotations {
                    // A reply goes with the annotation it answers.
                    if annot::subtype_of(doc, page, id).is_some() {
                        annot::delete(doc, page, id)?;
                    }
                }
                Ok(None)
            }
            Operation::AddReply {
                page,
                parent,
                text,
                author,
            } => {
                let created = annot::reply::add_reply(doc, *page, *parent, text, author)?;
                Ok(u32::try_from(created.xref).ok())
            }
            // Handled by `apply`.
            Operation::InsertPages { .. }
            | Operation::MovePages { .. }
            | Operation::DeletePages { .. }
            | Operation::RepairAnnotations => Ok(None),
        }
    }
}

fn check_pages(pages: &[usize], page_count: usize) -> Result<()> {
    if pages.is_empty() {
        return Err(Error::InvalidArgument("no pages were picked".into()));
    }
    match pages.iter().find(|&&p| p >= page_count) {
        Some(&bad) => Err(Error::PageOutOfRange(bad)),
        None => Ok(()),
    }
}

fn check_dest(dest: &ViewDest, page_count: usize) -> Result<()> {
    if dest.page >= page_count {
        return Err(Error::PageOutOfRange(dest.page));
    }
    if !dest.x.is_finite() || !dest.y.is_finite() {
        return Err(Error::InvalidArgument(
            "the position is not a number".into(),
        ));
    }
    Ok(())
}

/// Adds `degrees` to the page's effective (possibly inherited) `/Rotate` and writes the
/// result on the page itself.
fn rotate_page(doc: &PdfDocument, page: usize, degrees: i32) -> Result<()> {
    let index = i32::try_from(page).map_err(|_| Error::PageOutOfRange(page))?;
    let mut obj = doc.find_page(index)?;
    let current = match obj.get_dict_inheritable("Rotate")? {
        Some(r) if r.is_number()? => r.as_int()?,
        _ => 0,
    };
    let rotated = normalize_rotation(normalize_rotation(current) + degrees);
    obj.dict_put("Rotate", PdfObject::new_int(rotated)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{PageGeometry, read_page_boxes};
    use crate::testgen::{SampleSpec, sample_document};

    fn rotation(doc: &PdfDocument, page: i32) -> i32 {
        PageGeometry::new(&read_page_boxes(&doc.find_page(page).unwrap()).unwrap()).rotation
    }

    #[test]
    fn rotates_selected_pages_relative_to_current() {
        let mut doc = sample_document(&SampleSpec {
            pages: 3,
            rotate: 90,
            ..SampleSpec::default()
        })
        .unwrap();
        let op = Operation::RotatePages {
            pages: vec![0, 2, 2],
            degrees: 90,
        };
        op.validate(3).unwrap();
        op.apply_edit(&mut doc).unwrap();
        assert_eq!(rotation(&doc, 0), 180);
        assert_eq!(rotation(&doc, 1), 90);
        assert_eq!(rotation(&doc, 2), 180);

        Operation::RotatePages {
            pages: vec![0],
            degrees: -270,
        }
        .apply_edit(&mut doc)
        .unwrap();
        assert_eq!(rotation(&doc, 0), 270);
    }

    #[test]
    fn validation_rejects_bad_input() {
        let rotate = |pages: Vec<usize>, degrees| Operation::RotatePages { pages, degrees };
        assert!(rotate(vec![], 90).validate(3).is_err());
        assert!(rotate(vec![0], 45).validate(3).is_err());
        assert!(rotate(vec![3], 90).validate(3).is_err());
        assert!(rotate(vec![2], -90).validate(3).is_ok());
    }

    #[test]
    fn names_steps_for_the_edit_menu() {
        let one = Operation::RotatePages {
            pages: vec![4],
            degrees: 90,
        };
        let many = Operation::RotatePages {
            pages: vec![1, 2],
            degrees: 180,
        };
        assert_eq!(one.name(), "Rotate page");
        assert_eq!(many.name(), "Rotate pages");
        let pages = |n: usize| (0..n).collect::<Vec<_>>();
        let moving = |n| Operation::MovePages {
            pages: pages(n),
            to: 3,
        };
        let deleting = |n| Operation::DeletePages { pages: pages(n) };
        assert_eq!(moving(1).name(), "Move page");
        assert_eq!(moving(2).name(), "Move pages");
        assert_eq!(deleting(1).name(), "Delete page");
        assert_eq!(deleting(2).name(), "Delete pages");
    }

    #[test]
    fn moving_and_deleting_pages_is_checked_first() {
        let moving = |pages: Vec<usize>, to| Operation::MovePages { pages, to };
        assert!(moving(vec![], 0).validate(3).is_err());
        assert!(moving(vec![3], 0).validate(3).is_err());
        assert!(moving(vec![0], 4).validate(3).is_err());
        assert!(moving(vec![0], 3).validate(3).is_ok());
        let deleting = |pages: Vec<usize>| Operation::DeletePages { pages };
        assert!(deleting(vec![]).validate(3).is_err());
        assert!(deleting(vec![3]).validate(3).is_err());
        // The last page of a document stays, however the pages are listed.
        assert!(deleting(vec![0, 1, 2]).validate(3).is_err());
        assert!(deleting(vec![0, 1, 1]).validate(3).is_ok());
    }
}
