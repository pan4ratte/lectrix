//! Document operations (AGENTS.md section 7). Every change to a document is one
//! [`Operation`], applied as one MuPDF journal step so that undo and redo work on whole
//! user actions and the Edit menu can name them.

use std::fmt;
use std::path::PathBuf;

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::geometry::normalize_rotation;
use crate::labels::{self, LabelRule};
use crate::merge::{self, InsertOptions, MergeReport, MergeSource};
use crate::outline::{ViewDest, edit};

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
    /// The id of the object it created (the new bookmark).
    pub created: Option<u32>,
    /// What inserting pages did.
    pub report: Option<MergeReport>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    /// Turns pages by `degrees` (a multiple of 90; positive is clockwise) by changing
    /// their `/Rotate`. This modifies the document, unlike rotating the view.
    RotatePages {
        pages: Vec<usize>,
        degrees: i32,
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
}

impl Operation {
    /// Human-readable name of the journal step, shown as "Undo <name>".
    pub fn name(&self) -> String {
        match self {
            Operation::RotatePages { pages, .. } if pages.len() == 1 => "Rotate page".into(),
            Operation::RotatePages { .. } => "Rotate pages".into(),
            Operation::AddBookmark { .. } => "Add bookmark".into(),
            Operation::RenameBookmark { .. } => "Rename bookmark".into(),
            Operation::MoveBookmark { .. } => "Move bookmark".into(),
            Operation::DeleteBookmark { .. } => "Delete bookmark".into(),
            Operation::SetBookmarkDestination { .. } => "Change bookmark destination".into(),
            Operation::SetPageLabels { rules } if rules.is_empty() => "Remove page labels".into(),
            Operation::SetPageLabels { .. } => "Change page labels".into(),
            Operation::InsertPages { source, .. } if source.pages.len() == 1 => {
                "Insert page".into()
            }
            Operation::InsertPages { .. } => "Insert pages".into(),
        }
    }

    /// True if the operation needs the "assemble" permission (bit 4 or 11), which covers
    /// rotating pages, creating outline items and other document-level changes such as
    /// page labels.
    pub(crate) fn needs_assemble(&self) -> bool {
        match self {
            Operation::RotatePages { .. }
            | Operation::AddBookmark { .. }
            | Operation::RenameBookmark { .. }
            | Operation::MoveBookmark { .. }
            | Operation::DeleteBookmark { .. }
            | Operation::SetBookmarkDestination { .. }
            | Operation::SetPageLabels { .. }
            | Operation::InsertPages { .. } => true,
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
            Operation::AddBookmark { title, dest, .. } => {
                edit::clean_title(title)?;
                check_dest(dest, page_count)
            }
            Operation::RenameBookmark { title, .. } => edit::clean_title(title).map(drop),
            Operation::SetBookmarkDestination { dest, .. } => check_dest(dest, page_count),
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
        }
    }

    /// Applies the operation. The caller wraps this in a journal step.
    pub(crate) fn apply(&self, doc: &mut PdfDocument) -> Result<Applied> {
        if let Operation::InsertPages {
            source,
            at,
            options,
        } = self
        {
            let from = MergeSource::open(&source.path, source.password.as_deref())?;
            let report = merge::insert_pages(doc, *at, &from, &source.pages, *options)?;
            return Ok(Applied {
                created: None,
                report: Some(report),
            });
        }
        let created = self.apply_edit(doc)?;
        Ok(Applied {
            created,
            report: None,
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
            Operation::SetPageLabels { rules } => {
                labels::set_rules(doc, rules.clone()).map(|_| None)
            }
            // Handled by `apply`.
            Operation::InsertPages { .. } => Ok(None),
        }
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
    }
}
