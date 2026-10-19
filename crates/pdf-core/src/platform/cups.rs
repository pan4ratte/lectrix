//! Linux and macOS: printers through CUPS's command-line tools (ADR 0028), which both
//! systems have. `lpstat` lists the printers, `lpoptions -l` says what each takes, and a
//! job is a PDF of its sheets ([`crate::print::pdf`]) handed to `lp`. CUPS does not say
//! where a printer's margins are, so pages are fitted inside margins of [`MARGIN`].
//!
//! The parsers are plain functions, tested on every platform.

// Built on other platforms for its tests alone.
#![cfg_attr(not(unix), allow(dead_code))]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{Paper, PrinterCapabilities, PrinterEntry, Printers};
use crate::error::{Error, Result};
use crate::print::pdf::PdfTarget;
use crate::print::{Area, Duplex, PrintSetup, Sheet, Target};

/// The margins pages are fitted inside, in points (a sixth of an inch, about 4 mm: what
/// most printers leave unprinted).
pub const MARGIN: f32 = 12.0;

pub struct Cups;

/// Runs a CUPS tool in the C locale, so its output can be read.
fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| Error::Printer {
            printer: String::new(),
            detail: format!("{program} could not run: {e}"),
        })?;
    if !output.status.success() {
        return Err(Error::Printer {
            printer: args.last().map(|s| s.to_string()).unwrap_or_default(),
            detail: format!(
                "{program} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The printers `lpstat -e` lists (one name a line).
pub(crate) fn parse_destinations(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| l.split_whitespace().next().unwrap_or(l).to_owned())
        .collect()
}

/// The printers `lpstat -p` lists ("printer NAME is idle.  enabled since ...").
pub(crate) fn parse_printer_lines(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("printer "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
        .collect()
}

/// The default printer `lpstat -d` names ("system default destination: NAME").
pub(crate) fn parse_default(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.split_once("system default destination:"))
        .map(|(_, name)| name.trim().to_owned())
        .filter(|n| !n.is_empty())
}

/// An option `lpoptions -l` lists: "PageSize/Media Size: Letter *A4 Legal".
#[derive(Debug, PartialEq)]
pub(crate) struct CupsOption {
    pub key: String,
    pub values: Vec<String>,
    /// The value marked with `*`.
    pub current: Option<String>,
}

pub(crate) fn parse_options(text: &str) -> Vec<CupsOption> {
    text.lines()
        .filter_map(|line| {
            let (head, values) = line.split_once(':')?;
            let key = head.split('/').next()?.trim().to_owned();
            let mut current = None;
            let values = values
                .split_whitespace()
                .map(|v| match v.strip_prefix('*') {
                    Some(v) => {
                        current = Some(v.to_owned());
                        v.to_owned()
                    }
                    None => v.to_owned(),
                })
                .collect();
            Some(CupsOption {
                key,
                values,
                current,
            })
        })
        .collect()
}

/// The size of a CUPS page size name, in points, portrait: the PPD names of the common
/// sizes, or the size a PWG name carries ("iso_a4_210x297mm", "na_letter_8.5x11in").
pub(crate) fn paper_size(name: &str) -> Option<(f32, f32)> {
    let known = [
        ("Letter", (612.0, 792.0)),
        ("Legal", (612.0, 1008.0)),
        ("Executive", (522.0, 756.0)),
        ("Tabloid", (792.0, 1224.0)),
        ("Ledger", (792.0, 1224.0)),
        ("Statement", (396.0, 612.0)),
        ("A3", (842.0, 1191.0)),
        ("A4", (595.0, 842.0)),
        ("A5", (420.0, 595.0)),
        ("A6", (298.0, 420.0)),
        ("B4", (729.0, 1032.0)),
        ("B5", (516.0, 729.0)),
        ("Env10", (297.0, 684.0)),
        ("EnvDL", (312.0, 624.0)),
        ("EnvC5", (459.0, 649.0)),
    ];
    if let Some((_, size)) = known.iter().find(|(n, _)| *n == name) {
        return Some(*size);
    }
    // "<class>_<name>_<w>x<h><unit>"
    let last = name.rsplit('_').next()?;
    let (dims, factor) = match last.strip_suffix("mm") {
        Some(dims) => (dims, 72.0 / 25.4),
        None => (last.strip_suffix("in")?, 72.0),
    };
    let (w, h) = dims.split_once('x')?;
    let (w, h): (f32, f32) = (w.parse().ok()?, h.parse().ok()?);
    (w > 0.0 && h > 0.0).then(|| ((w * factor).min(h * factor), (w * factor).max(h * factor)))
}

/// What `lpoptions -l` says a printer takes.
pub(crate) fn capabilities_from(options: &[CupsOption]) -> PrinterCapabilities {
    let option = |key: &str| options.iter().find(|o| o.key == key);
    let mut papers = Vec::new();
    let mut paper = None;
    if let Some(sizes) = option("PageSize").or_else(|| option("media")) {
        for (i, name) in sizes.values.iter().enumerate() {
            if let Some((width, height)) = paper_size(name) {
                let id = i32::try_from(i).unwrap_or(i32::MAX);
                if sizes.current.as_deref() == Some(name) {
                    paper = Some(id);
                }
                papers.push(Paper {
                    id,
                    name: name.clone(),
                    width,
                    height,
                });
            }
        }
    }
    let duplex = option("Duplex")
        .or_else(|| option("sides"))
        .is_some_and(|o| o.values.len() > 1);
    let duplex_on = option("Duplex")
        .or_else(|| option("sides"))
        .and_then(|o| o.current.as_deref())
        .is_some_and(|v| v.starts_with("Duplex") || v.starts_with("two-sided"));
    let colour = match option("ColorModel").or_else(|| option("print-color-mode")) {
        Some(o) => o.values.iter().any(|v| {
            let v = v.to_ascii_lowercase();
            !(v.contains("gray") || v.contains("grey") || v.contains("black") || v == "monochrome")
        }),
        None => true,
    };
    PrinterCapabilities {
        papers,
        paper,
        duplex,
        colour,
        duplex_on,
    }
}

/// The `lp` arguments for a job.
pub(crate) fn lp_arguments(
    printer: &str,
    paper: Option<&str>,
    setup: &PrintSetup,
    file: &Path,
) -> Vec<String> {
    let mut args = vec![
        "-d".to_owned(),
        printer.to_owned(),
        "-t".to_owned(),
        setup.title.clone(),
        "-n".to_owned(),
        setup.copies.max(1).to_string(),
        "-o".to_owned(),
        format!("collate={}", setup.collate),
        "-o".to_owned(),
        format!(
            "sides={}",
            match setup.duplex {
                Duplex::OneSided => "one-sided",
                Duplex::LongEdge if setup.landscape => "two-sided-short-edge",
                Duplex::LongEdge => "two-sided-long-edge",
                Duplex::ShortEdge if setup.landscape => "two-sided-long-edge",
                Duplex::ShortEdge => "two-sided-short-edge",
            }
        ),
    ];
    if let Some(paper) = paper {
        args.extend(["-o".to_owned(), format!("media={paper}")]);
    }
    if setup.grey {
        args.extend([
            "-o".to_owned(),
            "print-color-mode=monochrome".to_owned(),
            "-o".to_owned(),
            "ColorModel=Gray".to_owned(),
        ]);
    }
    args.push("--".to_owned());
    args.push(file.to_string_lossy().into_owned());
    args
}

impl Cups {
    fn options(printer: &str) -> Result<Vec<CupsOption>> {
        Ok(parse_options(&run("lpoptions", &["-p", printer, "-l"])?))
    }

    fn paper(printer: &str, paper: Option<i32>) -> Result<Option<Paper>> {
        let caps = capabilities_from(&Cups::options(printer)?);
        let id = paper.or(caps.paper);
        Ok(caps.papers.into_iter().find(|p| Some(p.id) == id))
    }
}

impl Printers for Cups {
    fn list(&self) -> Result<Vec<PrinterEntry>> {
        let names = match run("lpstat", &["-e"]) {
            Ok(text) => parse_destinations(&text),
            // Older CUPS has no -e.
            Err(_) => parse_printer_lines(&run("lpstat", &["-p"]).unwrap_or_default()),
        };
        let default = run("lpstat", &["-d"]).ok().and_then(|t| parse_default(&t));
        Ok(names
            .into_iter()
            .map(|name| PrinterEntry {
                default: Some(&name) == default.as_ref(),
                name,
            })
            .collect())
    }

    fn capabilities(&self, printer: &str) -> Result<PrinterCapabilities> {
        Ok(capabilities_from(&Cups::options(printer)?))
    }

    fn sheet(&self, printer: &str, paper: Option<i32>, landscape: bool) -> Result<Sheet> {
        // A printer whose sizes cannot be read prints on A4 or Letter as the system's
        // locale has it; CUPS fits the page to the paper it has.
        let (w, h) = Cups::paper(printer, paper)?
            .map(|p| (p.width, p.height))
            .unwrap_or((595.0, 842.0));
        Ok(Sheet::with_margin(w, h, MARGIN).oriented(landscape))
    }

    fn start(&self, printer: &str, setup: &PrintSetup) -> Result<Box<dyn Target>> {
        let paper = Cups::paper(printer, setup.paper)?;
        let (w, h) = paper
            .as_ref()
            .map(|p| (p.width, p.height))
            .unwrap_or((595.0, 842.0));
        let sheet = Sheet::with_margin(w, h, MARGIN).oriented(setup.landscape);
        static JOBS: AtomicU32 = AtomicU32::new(0);
        let file = std::env::temp_dir().join(format!(
            "lectrix-print-{}-{}.pdf",
            std::process::id(),
            JOBS.fetch_add(1, Ordering::Relaxed)
        ));
        Ok(Box::new(CupsJob {
            pdf: Some(Box::new(PdfTarget::new(&file, sheet)?)),
            file,
            printer: printer.to_owned(),
            paper: paper.map(|p| p.name),
            setup: setup.clone(),
        }))
    }
}

/// A job: its sheets written to a PDF, handed to `lp` at the end.
struct CupsJob {
    pdf: Option<Box<PdfTarget>>,
    file: PathBuf,
    printer: String,
    paper: Option<String>,
    setup: PrintSetup,
}

impl CupsJob {
    fn pdf(&mut self) -> Result<&mut PdfTarget> {
        self.pdf
            .as_deref_mut()
            .ok_or_else(|| Error::InvalidArgument("the print job has ended".into()))
    }
}

impl Target for CupsJob {
    fn sheet(&self) -> Sheet {
        self.pdf.as_ref().map_or(
            Sheet {
                width: 0.0,
                height: 0.0,
                printable: Area {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                },
            },
            |p| p.sheet(),
        )
    }

    fn makes_copies(&self) -> bool {
        true
    }

    fn begin_sheet(&mut self) -> Result<()> {
        self.pdf()?.begin_sheet()
    }

    fn draw(
        &mut self,
        list: &mupdf::DisplayList,
        ctm: &mupdf::Matrix,
        clip: Area,
        grey: bool,
    ) -> Result<()> {
        self.pdf()?.draw(list, ctm, clip, grey)
    }

    fn end_sheet(&mut self) -> Result<()> {
        self.pdf()?.end_sheet()
    }

    fn finish(mut self: Box<Self>) -> Result<()> {
        if let Some(pdf) = self.pdf.take() {
            pdf.finish()?;
        }
        let result = match &self.setup.output {
            // A test: the file CUPS would get.
            Some(output) => std::fs::copy(&self.file, output)
                .map(|_| ())
                .map_err(Error::from),
            None => {
                let args = lp_arguments(
                    &self.printer,
                    self.paper.as_deref(),
                    &self.setup,
                    &self.file,
                );
                let args: Vec<&str> = args.iter().map(String::as_str).collect();
                run("lp", &args).map(|_| ()).map_err(|e| match e {
                    Error::Printer { detail, .. } => Error::Printer {
                        printer: self.printer.clone(),
                        detail,
                    },
                    e => e,
                })
            }
        };
        // lp has copied the file into the queue by the time it returns.
        let _ = std::fs::remove_file(&self.file);
        result
    }

    fn abort(mut self: Box<Self>) {
        if let Some(pdf) = self.pdf.take() {
            pdf.abort();
        }
        let _ = std::fs::remove_file(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printers_and_the_default() {
        assert_eq!(
            parse_destinations("Office_Laser\nHP_Color\n\n"),
            ["Office_Laser", "HP_Color"]
        );
        assert_eq!(
            parse_printer_lines(
                "printer Office_Laser is idle.  enabled since Mon 01 Jan\n\tReady\nprinter PDF disabled since x"
            ),
            ["Office_Laser", "PDF"]
        );
        assert_eq!(
            parse_default("system default destination: Office_Laser\n").as_deref(),
            Some("Office_Laser")
        );
        assert_eq!(parse_default("no system default destination\n"), None);
    }

    #[test]
    fn options_sizes_duplex_and_colour() {
        let text = "PageSize/Media Size: Letter *A4 Legal Custom.WIDTHxHEIGHT\n\
                    Duplex/2-Sided Printing: *None DuplexNoTumble DuplexTumble\n\
                    ColorModel/Color Mode: Gray *RGB\n";
        let options = parse_options(text);
        assert_eq!(options[0].key, "PageSize");
        assert_eq!(options[0].current.as_deref(), Some("A4"));
        let caps = capabilities_from(&options);
        assert_eq!(
            caps.papers
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["Letter", "A4", "Legal"]
        );
        assert_eq!(caps.paper, Some(1));
        assert!(caps.duplex && !caps.duplex_on && caps.colour);
        let grey = capabilities_from(&parse_options("ColorModel/Color Mode: *Gray\n"));
        assert!(!grey.colour);
    }

    #[test]
    fn pwg_names_carry_their_size() {
        let a4 = paper_size("iso_a4_210x297mm").unwrap();
        assert!((a4.0 - 595.3).abs() < 0.5 && (a4.1 - 841.9).abs() < 0.5);
        assert_eq!(paper_size("na_letter_8.5x11in"), Some((612.0, 792.0)));
        assert_eq!(paper_size("Custom.WIDTHxHEIGHT"), None);
    }

    #[test]
    fn lp_is_told_the_settings() {
        let setup = PrintSetup {
            paper: Some(1),
            landscape: false,
            duplex: Duplex::LongEdge,
            grey: true,
            copies: 3,
            collate: false,
            title: "report.pdf".into(),
            output: None,
            max_dpi: 600.0,
        };
        let args = lp_arguments("Office", Some("A4"), &setup, Path::new("/tmp/job.pdf"));
        let joined = args.join(" ");
        assert!(joined.starts_with("-d Office -t report.pdf -n 3 -o collate=false"));
        assert!(joined.contains("-o sides=two-sided-long-edge"));
        assert!(joined.contains("-o media=A4"));
        assert!(joined.contains("print-color-mode=monochrome"));
        assert!(joined.ends_with("-- /tmp/job.pdf"));
    }
}
