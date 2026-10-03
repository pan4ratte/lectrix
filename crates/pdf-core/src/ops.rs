//! Document operations (AGENTS.md section 7). Every change to a document is one
//! [`Operation`], applied as one MuPDF journal step so that undo and redo work on whole
//! user actions and the Edit menu can name them.

use mupdf::pdf::{PdfDocument, PdfObject};

use crate::error::{Error, Result};
use crate::geometry::normalize_rotation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Turns pages by `degrees` (a multiple of 90; positive is clockwise) by changing
    /// their `/Rotate`. This modifies the document, unlike rotating the view.
    RotatePages { pages: Vec<usize>, degrees: i32 },
}

impl Operation {
    /// Human-readable name of the journal step, shown as "Undo <name>".
    pub fn name(&self) -> String {
        match self {
            Operation::RotatePages { pages, .. } if pages.len() == 1 => "Rotate page".into(),
            Operation::RotatePages { .. } => "Rotate pages".into(),
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
        }
    }

    /// Applies the operation. The caller wraps this in a journal step.
    pub(crate) fn apply(&self, doc: &mut PdfDocument) -> Result<()> {
        match self {
            Operation::RotatePages { pages, degrees } => {
                let mut pages = pages.clone();
                pages.sort_unstable();
                pages.dedup();
                for page in pages {
                    rotate_page(doc, page, *degrees)?;
                }
                Ok(())
            }
        }
    }
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
        op.apply(&mut doc).unwrap();
        assert_eq!(rotation(&doc, 0), 180);
        assert_eq!(rotation(&doc, 1), 90);
        assert_eq!(rotation(&doc, 2), 180);

        Operation::RotatePages {
            pages: vec![0],
            degrees: -270,
        }
        .apply(&mut doc)
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
