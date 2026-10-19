//! Layer 1, pixels (APP-DESIGN 7.3): a pixel counts when any of its channel bytes differ. On
//! these opaque screenshots that is exactly what pixelmatch counts with threshold 0 and
//! anti-aliased pixels counted (parity/ds-gallery/diff.mjs): it skips equal pixels, and the YIQ
//! delta of an unequal pair is a nonzero quadratic form of an invertible transform of the
//! difference, so it exceeds a threshold of 0.

use std::io::Cursor;
use std::path::Path;

use super::inputs::Rect;

/// An image as 8-bit RGBA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decodes PNG bytes of 8-bit gray, gray with alpha, RGB, RGBA or a palette into RGBA.
pub fn decode(bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let size = reader
        .output_buffer_size()
        .ok_or("the image is too large to decode")?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(format!(
            "{:?} bits per channel; the gate reads 8",
            info.bit_depth
        ));
    }
    let data = &buf[..info.buffer_size()];
    let pixels = (info.width as usize) * (info.height as usize);
    let rgba = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return Err("a palette the decoder did not expand".to_owned()),
    };
    if rgba.len() != pixels * 4 {
        return Err(format!("{} bytes for {pixels} pixels", rgba.len()));
    }
    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
}

pub fn read(path: &Path) -> Result<Image, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    decode(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn encode(image: &Image) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&image.rgba)
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// How many pixels differ, and how many of those lie outside every mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Count {
    pub mismatched: u64,
    pub outside_masks: u64,
}

/// What a comparison of two screenshots found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compared {
    pub count: Count,
    /// The masks with a differing pixel inside, in the order given.
    pub masks_used: Vec<usize>,
    /// Set when the sizes differ: every pixel of tC4's screenshot then counts.
    pub size_error: Option<String>,
}

fn masked(masks: &[Rect], x: u32, y: u32) -> Option<usize> {
    masks.iter().position(|m| m.contains(x, y))
}

/// Compares two images of one size. Images of different sizes are an error.
pub fn count(tc4: &Image, rust: &Image, masks: &[Rect]) -> Result<Compared, String> {
    if (tc4.width, tc4.height) != (rust.width, rust.height) {
        return Err(format!(
            "rust {}x{}, tC4 {}x{}",
            rust.width, rust.height, tc4.width, tc4.height
        ));
    }
    let mut out = Compared {
        count: Count {
            mismatched: 0,
            outside_masks: 0,
        },
        masks_used: Vec::new(),
        size_error: None,
    };
    for (i, (a, b)) in tc4
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(rust.rgba.as_chunks::<4>().0)
        .enumerate()
    {
        if a == b {
            continue;
        }
        let (x, y) = ((i as u32) % tc4.width, (i as u32) / tc4.width);
        out.count.mismatched += 1;
        match masked(masks, x, y) {
            Some(m) => {
                if !out.masks_used.contains(&m) {
                    out.masks_used.push(m);
                }
            }
            None => out.count.outside_masks += 1,
        }
    }
    out.masks_used.sort_unstable();
    Ok(out)
}

/// Compares two PNG files. Equal bytes are equal pixels, so they are not decoded; a size
/// mismatch counts every pixel of tC4's screenshot and says why.
pub fn compare_files(tc4: &Path, rust: &Path, masks: &[Rect]) -> Result<Compared, String> {
    let a = std::fs::read(tc4).map_err(|e| format!("{}: {e}", tc4.display()))?;
    let b = std::fs::read(rust).map_err(|e| format!("{}: {e}", rust.display()))?;
    if a == b {
        return Ok(Compared {
            count: Count {
                mismatched: 0,
                outside_masks: 0,
            },
            masks_used: Vec::new(),
            size_error: None,
        });
    }
    let a = decode(&a).map_err(|e| format!("{}: {e}", tc4.display()))?;
    let b = decode(&b).map_err(|e| format!("{}: {e}", rust.display()))?;
    match count(&a, &b, masks) {
        Ok(compared) => Ok(compared),
        Err(size) => {
            let all = u64::from(a.width) * u64::from(a.height);
            Ok(Compared {
                count: Count {
                    mismatched: all,
                    outside_masks: all,
                },
                masks_used: Vec::new(),
                size_error: Some(size),
            })
        }
    }
}

/// The image a person reviews: each differing pixel red outside the masks and yellow inside,
/// on tC4's screenshot faded to 10 %, as pixelmatch draws its diff.
pub fn diff_image(tc4: &Image, rust: &Image, masks: &[Rect]) -> Image {
    let mut rgba = Vec::with_capacity(tc4.rgba.len());
    let same_size = (tc4.width, tc4.height) == (rust.width, rust.height);
    for (i, a) in tc4.rgba.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = ((i as u32) % tc4.width, (i as u32) / tc4.width);
        let differs = !same_size || rust.rgba.get(i * 4..i * 4 + 4) != Some(a);
        let pixel = if differs && masked(masks, x, y).is_some() {
            [255, 255, 0, 255]
        } else if differs {
            [255, 0, 0, 255]
        } else {
            let luma = 0.298_895_31 * f64::from(a[0])
                + 0.586_622_47 * f64::from(a[1])
                + 0.114_482_23 * f64::from(a[2]);
            let faded = (255.0 + (luma - 255.0) * 0.1).round().clamp(0.0, 255.0) as u8;
            [faded, faded, faded, 255]
        };
        rgba.extend_from_slice(&pixel);
    }
    Image {
        width: tc4.width,
        height: tc4.height,
        rgba,
    }
}
