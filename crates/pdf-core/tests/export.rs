//! Pages exported as images (section 6.14): whole jobs from an open document, the files
//! they leave, and pages whose geometry is not plain.

mod common;

use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use pdf_core::error::Error;
use pdf_core::export::names::{self, Naming};
use pdf_core::export::{Colour, Format, Job, Options, Output, TiffCompression, run};
use pdf_core::geometry::Rect;
use pdf_core::session::Session;
use pdf_core::testgen::SampleSpec;

fn options(format: Format) -> Options {
    Options {
        format,
        dpi: 150,
        colour: Colour::Colour,
        quality: 85,
        tiff: TiffCompression::Lzw,
        annotations: true,
    }
}

/// A folder of its own for the test, empty, and a document of `spec` open in it.
fn setup(name: &str, spec: SampleSpec) -> (std::path::PathBuf, Session) {
    let dir = common::out_dir(&format!("export/{name}"));
    for entry in std::fs::read_dir(&dir).unwrap() {
        let _ = std::fs::remove_file(entry.unwrap().path());
    }
    let path = common::sample_file(&dir, "in.pdf", spec);
    let (session, _) = Session::open(&path, None).unwrap();
    (dir, session)
}

fn pages(n: usize) -> SampleSpec {
    SampleSpec {
        pages: n,
        ..SampleSpec::default()
    }
}

/// The files in `dir` but the document.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != "in.pdf")
        .collect();
    names.sort();
    names
}

fn png_size(path: &Path) -> (u32, u32) {
    let reader = png::Decoder::new(std::io::BufReader::new(File::open(path).unwrap()))
        .read_info()
        .unwrap();
    (reader.info().width, reader.info().height)
}

fn go() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn every_page_gets_its_file() {
    let (dir, session) = setup("files", pages(5));
    let chosen = vec![0, 2, 4];
    let paths = names::file_names(&dir.join("out.png"), &chosen, 5, Naming::Numbers, None);
    let job = Job {
        pages: chosen,
        options: options(Format::Png),
        output: Output::Files(paths),
    };
    let mut seen = Vec::new();
    run(&session, &job, &go(), &mut |d, t| seen.push((d, t))).unwrap();
    assert_eq!(listing(&dir), ["out-1.png", "out-3.png", "out-5.png"]);
    assert_eq!(seen.last(), Some(&(3, 3)));
    // Letter at 150 dpi.
    assert_eq!(png_size(&dir.join("out-3.png")), (1275, 1650));
}

#[test]
fn every_format_writes_a_file_of_its_kind() {
    let (dir, session) = setup("formats", pages(2));
    let cases: [(Format, &[u8]); 3] = [
        (Format::Png, b"\x89PNG"),
        (Format::Jpeg, b"\xFF\xD8\xFF"),
        (Format::Tiff, b"II*\0"),
    ];
    for (format, magic) in cases {
        let path = dir.join(format!("page.{}", format.extension()));
        let job = Job {
            pages: vec![1],
            options: Options {
                dpi: 72,
                ..options(format)
            },
            output: Output::Files(vec![path.clone()]),
        };
        run(&session, &job, &go(), &mut |_, _| {}).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(magic), "{format:?}");
    }
}

#[test]
fn a_tiff_file_of_every_page_in_order() {
    use tiff::decoder::Decoder;
    let (dir, session) = setup("one-tiff", pages(4));
    let path = dir.join("all.tif");
    let job = Job {
        pages: vec![3, 0, 1, 2],
        options: Options {
            colour: Colour::BlackAndWhite,
            ..options(Format::Tiff)
        },
        output: Output::OneTiff(path.clone()),
    };
    run(&session, &job, &go(), &mut |_, _| {}).unwrap();
    assert_eq!(listing(&dir), ["all.tif"]);
    let mut decoder = Decoder::new(File::open(&path).unwrap()).unwrap();
    let mut count = 1;
    assert_eq!(decoder.dimensions().unwrap(), (1275, 1650));
    while decoder.more_images() {
        decoder.next_image().unwrap();
        decoder.read_image().unwrap();
        count += 1;
    }
    assert_eq!(count, 4);
}

#[test]
fn stopped_it_leaves_nothing_behind() {
    let (dir, session) = setup("stop", pages(6));
    let all: Vec<usize> = (0..6).collect();
    let paths = names::file_names(&dir.join("out.png"), &all, 6, Naming::Numbers, None);
    let job = Job {
        pages: all,
        options: options(Format::Png),
        output: Output::Files(paths),
    };
    // Stopped before it starts.
    let stop = AtomicBool::new(true);
    assert!(matches!(
        run(&session, &job, &stop, &mut |_, _| {}),
        Err(Error::Cancelled)
    ));
    assert!(listing(&dir).is_empty());
    // Stopped once a page is written.
    let stop = AtomicBool::new(false);
    let result = run(&session, &job, &stop, &mut |done, _| {
        if done >= 1 {
            stop.store(true, Ordering::Relaxed);
        }
    });
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(listing(&dir).is_empty(), "{:?}", listing(&dir));
    // The same for one TIFF file.
    let job = Job {
        output: Output::OneTiff(dir.join("all.tif")),
        options: options(Format::Tiff),
        ..job
    };
    let stop = AtomicBool::new(false);
    let result = run(&session, &job, &stop, &mut |_, _| {
        stop.store(true, Ordering::Relaxed);
    });
    assert!(matches!(result, Err(Error::Cancelled)), "{result:?}");
    assert!(listing(&dir).is_empty(), "{:?}", listing(&dir));
}

#[test]
fn an_existing_file_is_replaced() {
    let (dir, session) = setup("replace", pages(2));
    let path = dir.join("page.jpg");
    std::fs::write(&path, b"old").unwrap();
    let job = Job {
        pages: vec![1],
        options: options(Format::Jpeg),
        output: Output::Files(vec![path.clone()]),
    };
    run(&session, &job, &go(), &mut |_, _| {}).unwrap();
    assert_eq!(&std::fs::read(&path).unwrap()[..2], &[0xFF, 0xD8]);
    assert_eq!(listing(&dir), ["page.jpg"]);
}

#[test]
fn a_page_too_large_stops_the_job_before_it_starts() {
    let (dir, session) = setup("too-large", pages(2));
    let job = Job {
        pages: vec![0, 1],
        options: Options {
            dpi: 2400,
            ..options(Format::Png)
        },
        output: Output::Files(vec![dir.join("a.png"), dir.join("b.png")]),
    };
    let result = run(&session, &job, &go(), &mut |_, _| {});
    assert!(
        matches!(result, Err(Error::PageTooLarge { page: 0, .. })),
        "{result:?}"
    );
    assert!(listing(&dir).is_empty());
}

/// A page is exported as it shows: turned by `/Rotate`, cut to its CropBox wherever that
/// lies, and as large as `/UserUnit` makes it (AGENTS.md section 5.1, rule 8).
#[test]
fn pages_are_exported_as_they_show() {
    let one = |name: &str, spec: SampleSpec| -> (u32, u32) {
        let (dir, session) = setup(name, spec);
        let path = dir.join("page.png");
        let job = Job {
            pages: vec![0],
            options: Options {
                dpi: 72,
                ..options(Format::Png)
            },
            output: Output::Files(vec![path.clone()]),
        };
        run(&session, &job, &go(), &mut |_, _| {}).unwrap();
        png_size(&path)
    };
    let letter = SampleSpec {
        pages: 1,
        ..SampleSpec::default()
    };
    assert_eq!(one("plain", letter.clone()), (612, 792));
    assert_eq!(
        one(
            "rotated",
            SampleSpec {
                rotate: 90,
                ..letter.clone()
            }
        ),
        (792, 612)
    );
    assert_eq!(
        one(
            "cropped",
            SampleSpec {
                crop_box: Some(Rect::new(100.0, 200.0, 400.0, 600.0)),
                ..letter.clone()
            }
        ),
        (300, 400)
    );
    assert_eq!(
        one(
            "user-unit",
            SampleSpec {
                user_unit: Some(2.0),
                ..letter
            }
        ),
        (1224, 1584)
    );
}
