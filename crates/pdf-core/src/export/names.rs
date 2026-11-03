//! What the files of an export are called (section 6.14): the name the Save dialog gave
//! for one page; for several, that name with each page's number ("report-007.png") or
//! label ("report-iv.png") added.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// What each page's file is named by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Naming {
    /// Its number, with as many digits as the document's last page has, so that the files
    /// sort in order.
    Numbers,
    /// Its page label; a page without one, by its number.
    Labels,
}

/// The files `pages` (0-based, in order, without repeats) are written to, given the name
/// the Save dialog gave (`picked`) and a document of `page_count` pages with `labels` (one
/// a page, or none). One page is written to `picked` itself.
pub fn file_names(
    picked: &Path,
    pages: &[usize],
    page_count: usize,
    naming: Naming,
    labels: Option<&[String]>,
) -> Vec<PathBuf> {
    if pages.len() == 1 {
        return vec![picked.to_path_buf()];
    }
    let folder = picked.parent().unwrap_or(Path::new(""));
    let stem = picked
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = picked
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    // Windows and macOS don't tell "A" from "a" in a file name.
    let mut taken: HashSet<String> = HashSet::new();
    pages
        .iter()
        .map(|&page| {
            let mut suffix = page_suffix(page, page_count, naming, labels);
            if !taken.insert(suffix.to_lowercase()) {
                // A second page labelled alike: its number too.
                suffix = format!("{suffix} (p. {})", page + 1);
                taken.insert(suffix.to_lowercase());
            }
            let name = if stem.is_empty() {
                format!("{suffix}{extension}")
            } else {
                format!("{stem}-{suffix}{extension}")
            };
            folder.join(name)
        })
        .collect()
}

/// What names page `page`'s file: its number, padded to `page_count`'s digits, or its
/// label made safe (its number if it has none).
pub fn page_suffix(
    page: usize,
    page_count: usize,
    naming: Naming,
    labels: Option<&[String]>,
) -> String {
    let label = match naming {
        Naming::Numbers => None,
        Naming::Labels => labels
            .and_then(|l| l.get(page))
            .map(|l| safe(l))
            .filter(|l| !l.is_empty()),
    };
    label.unwrap_or_else(|| {
        let digits = page_count.max(1).to_string().len();
        format!("{:0digits$}", page + 1)
    })
}

/// The name the Save dialog suggests for the export of `pages` from a document named
/// `stem`: "report.png" for several, "report-007.png" for one.
pub fn suggested(
    stem: &str,
    pages: &[usize],
    page_count: usize,
    naming: Naming,
    labels: Option<&[String]>,
    extension: &str,
) -> String {
    match pages {
        [page] => format!(
            "{stem}-{}.{extension}",
            page_suffix(*page, page_count, naming, labels)
        ),
        _ => format!("{stem}.{extension}"),
    }
}

/// Extensions an image's file may be given, which a name given for images loses.
const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "jp2", "j2k", "jpx", "tif", "tiff",
];

/// The name given for images without an image file's extension: "report" of "report.png",
/// "report.v2" of "report.v2".
fn image_stem(picked: &Path) -> String {
    let name = picked
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let known = picked
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.as_str()));
    match picked.file_stem() {
        Some(stem) if known => stem.to_string_lossy().into_owned(),
        _ => name,
    }
}

/// An image to name: the page it is first drawn on (0-based), which of that page's images
/// it is (from 1), and its file's extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageName<'a> {
    pub page: usize,
    pub index: usize,
    pub extension: &'a str,
}

/// The files `images` (in order) are written to, given the name the Save dialog gave
/// (`picked`), in a document of `page_count` pages with `labels`: one image to `picked`
/// itself with its own extension ("cover.jpg"); several beside it, named after it with
/// the page's number or label and the image's place on the page ("report-007-2.png"),
/// that place padded to the most images a page has.
pub fn image_file_names(
    picked: &Path,
    images: &[ImageName],
    page_count: usize,
    naming: Naming,
    labels: Option<&[String]>,
) -> Vec<PathBuf> {
    let folder = picked.parent().unwrap_or(Path::new(""));
    let stem = image_stem(picked);
    if let [one] = images {
        return vec![folder.join(format!("{stem}.{}", one.extension))];
    }
    // Each page's part of the names, a page labelled as another one before it with its
    // number too (Windows and macOS don't tell "A" from "a").
    let mut suffixes: std::collections::HashMap<usize, String> = std::collections::HashMap::new();
    let mut taken: HashSet<String> = HashSet::new();
    for image in images {
        if suffixes.contains_key(&image.page) {
            continue;
        }
        let mut suffix = page_suffix(image.page, page_count, naming, labels);
        if !taken.insert(suffix.to_lowercase()) {
            suffix = format!("{suffix} (p. {})", image.page + 1);
            taken.insert(suffix.to_lowercase());
        }
        suffixes.insert(image.page, suffix);
    }
    let most = images.iter().map(|i| i.index).max().unwrap_or(1);
    let digits = most.to_string().len();
    images
        .iter()
        .map(|image| {
            let page = &suffixes[&image.page];
            let name = format!("{page}-{:0digits$}.{}", image.index, image.extension);
            folder.join(if stem.is_empty() {
                name
            } else {
                format!("{stem}-{name}")
            })
        })
        .collect()
}

/// The name the Save dialog suggests for exporting `images` from a document named `stem`:
/// "report.jpg" for several (the first one's extension), "report-007-1.jpg" for one.
pub fn suggested_for_images(
    stem: &str,
    images: &[ImageName],
    page_count: usize,
    naming: Naming,
    labels: Option<&[String]>,
) -> String {
    let extension = images.first().map_or("png", |i| i.extension);
    match images {
        [one] => format!(
            "{stem}-{}-{}.{extension}",
            page_suffix(one.page, page_count, naming, labels),
            one.index
        ),
        _ => format!("{stem}.{extension}"),
    }
}

/// `label` as it can be part of a file name on every system: characters Windows doesn't
/// allow, and control characters, as `_`; no dots or spaces at its end.
pub fn safe(label: &str) -> String {
    let replaced: String = label
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    replaced.trim().trim_end_matches(['.', ' ']).to_owned()
}

/// `picked` with `extension` added unless it ends with one of `allowed` (any case):
/// "report" and "report.v2" become "report.png", "report.v2.png"; "scan.JPEG" stays.
pub fn with_extension(picked: &Path, extension: &str, allowed: &[&str]) -> PathBuf {
    let has = picked
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .is_some_and(|e| allowed.contains(&e.as_str()));
    if has {
        picked.to_path_buf()
    } else {
        let mut name = picked.as_os_str().to_owned();
        name.push(".");
        name.push(extension);
        PathBuf::from(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn one_page_keeps_the_name_given() {
        let picked = Path::new("C:/out/cover.png");
        assert_eq!(
            file_names(picked, &[4], 120, Naming::Numbers, None),
            vec![picked.to_path_buf()]
        );
    }

    #[test]
    fn numbers_are_padded_to_the_last_page() {
        let picked = Path::new("C:/out/report.png");
        let files = file_names(picked, &[0, 6, 119], 120, Naming::Numbers, None);
        assert_eq!(
            names(&files),
            ["report-001.png", "report-007.png", "report-120.png"]
        );
        assert!(
            files
                .iter()
                .all(|f| f.parent() == Some(Path::new("C:/out")))
        );
        assert_eq!(
            names(&file_names(picked, &[0, 1], 9, Naming::Numbers, None)),
            ["report-1.png", "report-2.png"]
        );
    }

    #[test]
    fn labels_are_made_safe_and_repeats_get_their_number() {
        let labels: Vec<String> = ["i", "ii", "1", "A:1", "1", "", "a?/b."]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let files = file_names(
            Path::new("out/book.jpg"),
            &[0, 1, 2, 3, 4, 5, 6],
            7,
            Naming::Labels,
            Some(&labels),
        );
        assert_eq!(
            names(&files),
            [
                "book-i.jpg",
                "book-ii.jpg",
                "book-1.jpg",
                "book-A_1.jpg",
                "book-1 (p. 5).jpg",
                "book-6.jpg",
                "book-a__b.jpg"
            ]
        );
        // Labels that differ only in case are repeats too.
        let labels: Vec<String> = vec!["A".into(), "a".into()];
        assert_eq!(
            names(&file_names(
                Path::new("x.png"),
                &[0, 1],
                2,
                Naming::Labels,
                Some(&labels)
            )),
            ["x-A.png", "x-a (p. 2).png"]
        );
        // Without labels, numbers.
        assert_eq!(
            names(&file_names(
                Path::new("x.png"),
                &[0, 1],
                2,
                Naming::Labels,
                None
            )),
            ["x-1.png", "x-2.png"]
        );
    }

    #[test]
    fn the_suggested_name() {
        let labels: Vec<String> = vec!["i".into(), "ii".into(), "1".into()];
        assert_eq!(
            suggested("report", &[0, 1], 3, Naming::Numbers, None, "png"),
            "report.png"
        );
        assert_eq!(
            suggested("report", &[2], 120, Naming::Numbers, None, "jpg"),
            "report-003.jpg"
        );
        assert_eq!(
            suggested("report", &[1], 3, Naming::Labels, Some(&labels), "tif"),
            "report-ii.tif"
        );
    }

    #[test]
    fn images_are_named_after_their_page_and_place_on_it() {
        let image = |page, index, extension| ImageName {
            page,
            index,
            extension,
        };
        let picked = Path::new("C:/out/report.png");
        let images = [image(0, 1, "jpg"), image(0, 2, "png"), image(6, 1, "jp2")];
        let files = image_file_names(picked, &images, 120, Naming::Numbers, None);
        assert_eq!(
            names(&files),
            ["report-001-1.jpg", "report-001-2.png", "report-007-1.jp2"]
        );
        assert!(
            files
                .iter()
                .all(|f| f.parent() == Some(Path::new("C:/out")))
        );
        // A page of ten images or more: the places padded.
        let many: Vec<ImageName> = (1..=10).map(|i| image(2, i, "png")).collect();
        assert_eq!(
            names(&image_file_names(picked, &many, 9, Naming::Numbers, None))[..2],
            ["report-3-01.png", "report-3-02.png"]
        );
        // One image: the name given, with its own extension.
        assert_eq!(
            image_file_names(picked, &[image(4, 2, "jpg")], 120, Naming::Numbers, None),
            vec![PathBuf::from("C:/out/report.jpg")]
        );
        assert_eq!(
            names(&image_file_names(
                Path::new("out/scan.v2"),
                &[image(0, 1, "png")],
                3,
                Naming::Numbers,
                None
            )),
            ["scan.v2.png"]
        );
        // By labels, a page labelled again with its number.
        let labels: Vec<String> = vec!["i".into(), "1".into(), "1".into()];
        let images = [
            image(0, 1, "png"),
            image(1, 1, "png"),
            image(2, 1, "png"),
            image(2, 2, "jpg"),
        ];
        assert_eq!(
            names(&image_file_names(
                Path::new("book.png"),
                &images,
                3,
                Naming::Labels,
                Some(&labels)
            )),
            [
                "book-i-1.png",
                "book-1-1.png",
                "book-1 (p. 3)-1.png",
                "book-1 (p. 3)-2.jpg"
            ]
        );
        assert_eq!(
            suggested_for_images("report", &images, 3, Naming::Numbers, None),
            "report.png"
        );
        assert_eq!(
            suggested_for_images(
                "report",
                &[image(1, 2, "jpg")],
                3,
                Naming::Labels,
                Some(&labels)
            ),
            "report-1-2.jpg"
        );
    }

    #[test]
    fn the_extension_is_added_unless_it_is_there() {
        let png = ["png"];
        assert_eq!(
            with_extension(Path::new("a/report"), "png", &png),
            Path::new("a/report.png")
        );
        assert_eq!(
            with_extension(Path::new("a/report.v2"), "png", &png),
            Path::new("a/report.v2.png")
        );
        assert_eq!(
            with_extension(Path::new("scan.JPEG"), "jpg", &["jpg", "jpeg"]),
            Path::new("scan.JPEG")
        );
    }
}
