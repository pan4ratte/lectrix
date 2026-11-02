//! A page's pixels written in each format (section 6.14, ADR 0034), and what the dialog's
//! preview shows of a file.
//!
//! - **PNG** through the `png` crate, in colour, grey or one bit a pixel, with its
//!   resolution (`pHYs`) and sRGB.
//! - **JPEG** through libjpeg-turbo (as Compress writes it, ADR 0027), baseline, its colour
//!   halved both ways below quality 90, with its resolution in the JFIF header.
//! - **TIFF** through the `tiff` crate: colour and grey with LZW or Deflate and the
//!   horizontal predictor, black and white as CCITT Group 4 (MuPDF's encoder); a page or a
//!   file of many ([`TiffPages`]), each with its resolution.

use std::io::{Cursor, Seek, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use tiff::encoder::colortype::{Gray8, RGB8};
use tiff::encoder::{Compression, DeflateLevel, Rational, TiffEncoder};
use tiff::tags::{Predictor, ResolutionUnit, Tag};

use super::{Format, Options, Raster, TiffCompression};
use crate::error::{Error, Result};
use crate::ffi::compress as raw;

/// Below this JPEG quality, colour is halved both ways (as Compress does).
const JPEG_FULL_COLOUR: u8 = 90;

/// `raster` written as `options` say; [`Error::Cancelled`] if `stop` was set meanwhile.
pub fn encode(raster: &Raster, options: &Options, stop: &AtomicBool) -> Result<Vec<u8>> {
    let bytes = match options.format {
        Format::Png => png(raster)?,
        Format::Jpeg => jpeg(raster, options.quality)?,
        Format::Tiff => {
            let mut out = Cursor::new(Vec::new());
            TiffPages::new(&mut out, options.tiff)?.add(raster)?;
            out.into_inner()
        }
    };
    if stop.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    Ok(bytes)
}

fn failed(format: Format, detail: impl std::fmt::Display) -> Error {
    Error::Encode {
        format: format.short_name(),
        detail: detail.to_string(),
    }
}

/// Dots per inch as pixels per metre, as PNG keeps it.
fn per_metre(dpi: u16) -> u32 {
    (f64::from(dpi) / 0.0254).round() as u32
}

pub fn png(r: &Raster) -> Result<Vec<u8>> {
    let e = |e: png::EncodingError| failed(Format::Png, e);
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, r.width, r.height);
        let bits;
        let data: &[u8] = if r.mono {
            encoder.set_color(png::ColorType::Grayscale);
            encoder.set_depth(png::BitDepth::One);
            bits = r.bits();
            &bits
        } else {
            encoder.set_color(if r.channels == 1 {
                png::ColorType::Grayscale
            } else {
                png::ColorType::Rgb
            });
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            &r.data
        };
        encoder.set_compression(png::Compression::High);
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: per_metre(r.dpi),
            yppu: per_metre(r.dpi),
            unit: png::Unit::Meter,
        }));
        let mut writer = encoder.write_header().map_err(e)?;
        writer.write_image_data(data).map_err(e)?;
        writer.finish().map_err(e)?;
    }
    Ok(out)
}

pub fn jpeg(r: &Raster, quality: u8) -> Result<Vec<u8>> {
    raw::encode_jpeg_at(
        &r.data,
        r.width as usize,
        r.height as usize,
        usize::from(r.channels),
        quality,
        r.channels == 3 && quality < JPEG_FULL_COLOUR,
        r.dpi,
    )
    .map_err(|e| failed(Format::Jpeg, e))
}

/// Pages written one after another into a TIFF file, each its own image (directory).
pub struct TiffPages<W: Write + Seek> {
    encoder: TiffEncoder<W>,
}

impl<W: Write + Seek> TiffPages<W> {
    /// A TIFF file begun in `out`, its colour and grey pages compressed with `compression`.
    pub fn new(out: W, compression: TiffCompression) -> Result<Self> {
        let compression = match compression {
            TiffCompression::Lzw => Compression::Lzw,
            TiffCompression::Deflate => Compression::Deflate(DeflateLevel::Balanced),
        };
        let encoder = TiffEncoder::new(out)
            .map_err(|e| failed(Format::Tiff, e))?
            .with_compression(compression)
            .with_predictor(Predictor::Horizontal);
        Ok(TiffPages { encoder })
    }

    /// Adds `r` as the file's next page.
    pub fn add(&mut self, r: &Raster) -> Result<()> {
        let e = |e: tiff::TiffError| failed(Format::Tiff, e);
        let resolution = Rational {
            n: u32::from(r.dpi),
            d: 1,
        };
        if r.mono {
            return self.add_group4(r, resolution).map_err(e);
        }
        if r.channels == 3 {
            let mut image = self
                .encoder
                .new_image::<RGB8>(r.width, r.height)
                .map_err(e)?;
            image.resolution(ResolutionUnit::Inch, resolution);
            image.write_data(&r.data).map_err(e)
        } else {
            let mut image = self
                .encoder
                .new_image::<Gray8>(r.width, r.height)
                .map_err(e)?;
            image.resolution(ResolutionUnit::Inch, resolution);
            image.write_data(&r.data).map_err(e)
        }
    }

    /// A black-and-white page: one strip of CCITT Group 4, white is zero, as fax readers
    /// and every TIFF reader take it. The `tiff` crate has no bilevel images, so the
    /// directory is written here; the crate keeps its tags in order.
    fn add_group4(&mut self, r: &Raster, resolution: Rational) -> tiff::TiffResult<()> {
        let data = raw::encode_g4(&r.bits(), r.width as usize, r.height as usize)
            .map_err(|err| tiff::TiffError::IoError(std::io::Error::other(err.to_string())))?;
        let length = u32::try_from(data.len()).map_err(|_| {
            tiff::TiffError::IoError(std::io::Error::other("the page is too large for TIFF"))
        })?;
        let mut dir = self.encoder.image_directory()?;
        let offset = dir.write_data(&data[..])?;
        let offset = u32::try_from(offset).map_err(|_| {
            tiff::TiffError::IoError(std::io::Error::other("the file is too large for TIFF"))
        })?;
        dir.write_tag(Tag::ImageWidth, r.width)?;
        dir.write_tag(Tag::ImageLength, r.height)?;
        dir.write_tag(Tag::BitsPerSample, 1u16)?;
        // 4: CCITT T.6 (Group 4).
        dir.write_tag(Tag::Compression, 4u16)?;
        // 0: WhiteIsZero, the fax convention.
        dir.write_tag(Tag::PhotometricInterpretation, 0u16)?;
        dir.write_tag(Tag::StripOffsets, offset)?;
        dir.write_tag(Tag::SamplesPerPixel, 1u16)?;
        dir.write_tag(Tag::RowsPerStrip, r.height)?;
        dir.write_tag(Tag::StripByteCounts, length)?;
        dir.write_tag(Tag::XResolution, resolution.clone())?;
        dir.write_tag(Tag::YResolution, resolution)?;
        dir.write_tag(Tag::ResolutionUnit, ResolutionUnit::Inch.to_u16())?;
        dir.finish()
    }
}

/// What a page written as `options` say looks like, for the dialog's preview: the file
/// itself where the webview shows it (PNG, JPEG), else a PNG of the page's own pixels,
/// which TIFF keeps. The image and its media type.
pub fn viewable(
    raster: &Raster,
    options: &Options,
    file: Vec<u8>,
) -> Result<(Vec<u8>, &'static str)> {
    Ok(match options.format {
        Format::Png => (file, "image/png"),
        Format::Jpeg => (file, "image/jpeg"),
        Format::Tiff => (quick_png(raster)?, "image/png"),
    })
}

/// `r` as PNG at the fastest compression, to be shown at once (black and white, whose
/// samples are 0 or 255, as eight-bit grey).
fn quick_png(r: &Raster) -> Result<Vec<u8>> {
    let e = |e: png::EncodingError| failed(Format::Png, e);
    let mut out = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut out, r.width, r.height);
        encoder.set_color(if r.channels == 1 {
            png::ColorType::Grayscale
        } else {
            png::ColorType::Rgb
        });
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fastest);
        let mut writer = encoder.write_header().map_err(e)?;
        writer.write_image_data(&r.data).map_err(e)?;
        writer.finish().map_err(e)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{Colour, render, tests::options};
    use crate::testgen::{SampleSpec, sample_document};

    fn page(options: &Options) -> Raster {
        let doc = sample_document(&SampleSpec::default()).unwrap();
        let list = crate::render::display_list(&doc, 0).unwrap();
        render(&list, 0, options).unwrap()
    }

    fn go() -> AtomicBool {
        AtomicBool::new(false)
    }

    /// Mean absolute difference of two rasters' samples.
    fn difference(a: &Raster, b: &[u8]) -> f64 {
        assert_eq!(a.data.len(), b.len());
        a.data
            .iter()
            .zip(b)
            .map(|(&x, &y)| f64::from(x.abs_diff(y)))
            .sum::<f64>()
            / b.len() as f64
    }

    #[test]
    fn png_keeps_every_pixel_and_its_resolution() {
        for colour in [Colour::Colour, Colour::Grey, Colour::BlackAndWhite] {
            let o = Options {
                colour,
                ..options(Format::Png)
            };
            let r = page(&o);
            let file = encode(&r, &o, &go()).unwrap();
            let mut reader = png::Decoder::new(Cursor::new(&file)).read_info().unwrap();
            let info = reader.info();
            assert_eq!((info.width, info.height), (r.width, r.height));
            let dims = info.pixel_dims.unwrap();
            assert_eq!(dims.xppu, 5906, "150 dpi");
            assert_eq!(
                info.bit_depth,
                if colour == Colour::BlackAndWhite {
                    png::BitDepth::One
                } else {
                    png::BitDepth::Eight
                }
            );
            let mut buf = vec![0; reader.output_buffer_size().unwrap()];
            let frame = reader.next_frame(&mut buf).unwrap();
            if colour == Colour::BlackAndWhite {
                let stride = frame.line_size;
                assert_eq!(&buf[..stride * r.height as usize], &r.bits()[..]);
            } else {
                assert_eq!(&buf[..frame.buffer_size()], &r.data[..], "{colour:?}");
            }
        }
    }

    #[test]
    fn jpeg_is_close_and_has_its_resolution() {
        let o = options(Format::Jpeg);
        let r = page(&o);
        let file = encode(&r, &o, &go()).unwrap();
        assert_eq!(&file[..2], &[0xFF, 0xD8]);
        // JFIF APP0: "JFIF\0", version, units 1 (dpi), then X and Y density.
        let at = file.windows(5).position(|w| w == b"JFIF\0").unwrap();
        assert_eq!(file[at + 7], 1);
        assert_eq!(u16::from_be_bytes([file[at + 8], file[at + 9]]), 150);
        assert_eq!(u16::from_be_bytes([file[at + 10], file[at + 11]]), 150);
        // Read back by MuPDF's JPEG decoder, as an image in a PDF.
        let mut doc = mupdf::pdf::PdfDocument::new();
        let dict = doc
            .new_object_from_str(&format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode >>",
                r.width, r.height
            ))
            .unwrap();
        let image = doc
            .add_stream(
                &mupdf::Buffer::from_copied_bytes(&file).unwrap(),
                Some(&dict),
                true,
            )
            .unwrap();
        let back = raw::decode_image(&doc, image.as_indirect().unwrap()).unwrap();
        assert_eq!(
            (back.width, back.height),
            (r.width as usize, r.height as usize)
        );
        let samples: Vec<u8> = (0..back.height)
            .flat_map(|y| back.row(y).to_vec())
            .collect();
        assert!(difference(&r, &samples) < 3.0);
    }

    #[test]
    fn tiff_pages_read_back_as_written() {
        use tiff::decoder::{Decoder, DecodingResult};
        let colour = options(Format::Tiff);
        let grey = Options {
            colour: Colour::Grey,
            tiff: TiffCompression::Deflate,
            ..colour
        };
        let mono = Options {
            colour: Colour::BlackAndWhite,
            ..colour
        };
        let pages: Vec<Raster> = [colour, grey, mono].iter().map(page).collect();
        let mut out = Cursor::new(Vec::new());
        {
            let mut tiff = TiffPages::new(&mut out, TiffCompression::Lzw).unwrap();
            for p in &pages {
                tiff.add(p).unwrap();
            }
        }
        let mut decoder = Decoder::new(Cursor::new(out.into_inner())).unwrap();
        for (i, p) in pages.iter().enumerate() {
            if i > 0 {
                assert!(decoder.more_images());
                decoder.next_image().unwrap();
            }
            assert_eq!(decoder.dimensions().unwrap(), (p.width, p.height));
            let x: Vec<u32> = decoder.get_tag_u32_vec(Tag::XResolution).unwrap();
            assert_eq!(x, vec![150, 1]);
            if p.mono {
                // Group 4 (the crate reads it with its "fax" feature, in tests only), white
                // is zero: the crate gives the page's bits with 0 for black, whatever the
                // file says white is, so they are ours. Windows' TIFF codec shows such a
                // file black on white.
                assert_eq!(decoder.get_tag_u32(Tag::Compression).unwrap(), 4);
                assert_eq!(
                    decoder.get_tag_u32(Tag::PhotometricInterpretation).unwrap(),
                    0
                );
                let DecodingResult::U8(bits) = decoder.read_image().unwrap() else {
                    panic!("not eight-bit samples");
                };
                assert_eq!(bits, p.bits(), "black and white comes back as written");
            } else {
                let DecodingResult::U8(data) = decoder.read_image().unwrap() else {
                    panic!("not eight-bit samples");
                };
                assert_eq!(data, p.data, "page {i}");
            }
        }
        assert!(!decoder.more_images());
    }

    #[test]
    fn a_stopped_export_is_cancelled() {
        let o = options(Format::Png);
        let r = page(&o);
        let stop = AtomicBool::new(true);
        assert!(matches!(encode(&r, &o, &stop), Err(Error::Cancelled)));
    }
}
