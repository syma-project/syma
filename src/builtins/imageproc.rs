//! image processing: ImageCrop, ImageTake, ImagePad, ImageTrim, ImageEffect,
//! ImageFilter, ColorNegate, Colorize, ColorSeparate/ColorCombine, Dilation,
//! Erosion, Thinning, Thicken, MeanFilter, MedianFilter, Blur, Sharpen,
//! DominantColors, ColorQuantize, ComponentMeasurements, DistanceTransform,
//! CornerFilter, LaplacianFilter, EdgeFilter, FillingTransform,
//! MorphologicalBinarize.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! Shared raster helpers and the `Value::Image`/`image::DynamicImage`
//! representation live in `super::image`; this module keeps the same
//! representation and only adds the operations listed above (ImageResize,
//! ColorConvert, Binarize, EdgeDetect and GaussianFilter already exist there).
//!
//! Coordinate convention for the region-based operations (ImageCrop,
//! ImageTake, ImageTrim, ComponentMeasurements) is the same as `ImageData`:
//! row 1 is the top row and column 1 is the left column. This matches
//! Mathematica's `ImageTake` and keeps the implementation aligned with the
//! crate's internal top-left origin.

use std::collections::HashMap;
use std::sync::Arc;

use crate::env::Env;
use crate::value::{EvalError, Value};
use image::DynamicImage;

// ── Small helpers ─────────────────────────────────────────────────────────────

/// Fetch the image at `args[index]`, if it is an `Image`.
fn get_image(args: &[Value], index: usize) -> Option<Arc<DynamicImage>> {
    match args.get(index) {
        Some(Value::Image(img)) => Some(Arc::clone(img)),
        _ => None,
    }
}

/// An unevaluated `head[args...]` expression, returned for inapplicable input.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

/// Round a numeric value to the nearest `i64`.
fn to_i64(v: &Value) -> Option<i64> {
    super::to_f64(v).map(|f| f.round() as i64)
}

/// Convert a normalised [0, 1] value to a clamped u8.
fn norm_to_u8(v: f64) -> u8 {
    if v <= 0.0 {
        0
    } else if v >= 1.0 {
        255
    } else {
        (v * 255.0).round() as u8
    }
}

/// Convert a u8 channel value to a normalised [0, 1] `Value`.
fn chan_value(v: u8) -> Value {
    super::real(v as f64 / 255.0)
}

/// Build an `Image` value from a `DynamicImage`.
fn make_image(img: DynamicImage) -> Value {
    Value::Image(Arc::new(img))
}

/// Parse an integer radius from an argument, defaulting to 1.
fn radius_arg(args: &[Value], index: usize) -> u32 {
    args.get(index)
        .and_then(super::to_f64)
        .map(|f| f.round().max(0.0) as u32)
        .unwrap_or(1)
}

/// Apply a per-pixel RGB transform, preserving the source colour type.
///
/// Alpha (when present) is copied through unchanged.
fn map_color_type<F>(img: &DynamicImage, f: F) -> DynamicImage
where
    F: Fn(u8, u8, u8) -> (u8, u8, u8),
{
    match img {
        DynamicImage::ImageLuma8(buf) => {
            let (w, h) = buf.dimensions();
            let mut out = image::GrayImage::new(w, h);
            for (o, p) in out.pixels_mut().zip(buf.pixels()) {
                let (r, _, _) = f(p[0], p[0], p[0]);
                *o = image::Luma([r]);
            }
            DynamicImage::ImageLuma8(out)
        }
        DynamicImage::ImageRgb8(buf) => {
            let (w, h) = buf.dimensions();
            let mut out = image::RgbImage::new(w, h);
            for (o, p) in out.pixels_mut().zip(buf.pixels()) {
                let (r, g, b) = f(p[0], p[1], p[2]);
                *o = image::Rgb([r, g, b]);
            }
            DynamicImage::ImageRgb8(out)
        }
        DynamicImage::ImageRgba8(buf) => {
            let (w, h) = buf.dimensions();
            let mut out = image::RgbaImage::new(w, h);
            for (o, p) in out.pixels_mut().zip(buf.pixels()) {
                let (r, g, b) = f(p[0], p[1], p[2]);
                *o = image::Rgba([r, g, b, p[3]]);
            }
            DynamicImage::ImageRgba8(out)
        }
        _ => {
            let src = img.to_rgba8();
            let (w, h) = src.dimensions();
            let mut out = image::RgbaImage::new(w, h);
            for (o, p) in out.pixels_mut().zip(src.pixels()) {
                let (r, g, b) = f(p[0], p[1], p[2]);
                *o = image::Rgba([r, g, b, p[3]]);
            }
            DynamicImage::ImageRgba8(out)
        }
    }
}

/// Rebuild a `DynamicImage` matching `color` from flat channel data.
fn build_like(
    color: image::ColorType,
    w: u32,
    h: u32,
    channels: usize,
    data: &[u8],
) -> DynamicImage {
    match color {
        image::ColorType::L8 if channels == 1 => {
            let mut buf = image::GrayImage::new(w, h);
            for (i, p) in buf.pixels_mut().enumerate() {
                *p = image::Luma([data[i]]);
            }
            DynamicImage::ImageLuma8(buf)
        }
        image::ColorType::Rgb8 if channels == 3 => {
            let mut buf = image::RgbImage::new(w, h);
            for (i, p) in buf.pixels_mut().enumerate() {
                let o = i * 3;
                *p = image::Rgb([data[o], data[o + 1], data[o + 2]]);
            }
            DynamicImage::ImageRgb8(buf)
        }
        _ => {
            let mut buf = image::RgbaImage::new(w, h);
            for (i, p) in buf.pixels_mut().enumerate() {
                let o = i * 4;
                *p = image::Rgba([data[o], data[o + 1], data[o + 2], data[o + 3]]);
            }
            DynamicImage::ImageRgba8(buf)
        }
    }
}

/// Apply `f` over a rectangular neighbourhood for every channel/pixel.
///
/// `f` receives the mutable window of collected channel values and returns the
/// replacement value. Wrap-around-free edge clamping is used.
fn window_filter<F>(img: &DynamicImage, rw: u32, rh: u32, f: F) -> DynamicImage
where
    F: Fn(&mut [u8]) -> u8,
{
    let src = img.to_rgba8();
    let (w, h) = src.dimensions();
    let channels = match img.color() {
        image::ColorType::L8 => 1usize,
        image::ColorType::Rgb8 => 3,
        _ => 4,
    };
    let rwi = rw as i32;
    let rhi = rh as i32;
    let mut out = vec![0u8; w as usize * h as usize * channels];
    let mut win: Vec<u8> = Vec::new();

    for y in 0..h {
        for x in 0..w {
            for c in 0..channels {
                win.clear();
                for dy in -rhi..=rhi {
                    for dx in -rwi..=rwi {
                        let xx = (x as i32 + dx).clamp(0, w as i32 - 1) as u32;
                        let yy = (y as i32 + dy).clamp(0, h as i32 - 1) as u32;
                        win.push(src.get_pixel(xx, yy)[c]);
                    }
                }
                out[(y as usize * w as usize + x as usize) * channels + c] = f(&mut win);
            }
        }
    }
    build_like(img.color(), w, h, channels, &out)
}

/// Box (mean) blur over a rectangular radius.
fn box_blur(img: &DynamicImage, rw: u32, rh: u32) -> DynamicImage {
    window_filter(img, rw, rh, |win| {
        let sum: u32 = win.iter().map(|&v| v as u32).sum();
        (sum / win.len() as u32) as u8
    })
}

/// Median filter over a rectangular radius.
fn median_filter(img: &DynamicImage, rw: u32, rh: u32) -> DynamicImage {
    window_filter(img, rw, rh, |win| {
        win.sort_unstable();
        win[win.len() / 2]
    })
}

/// Grayscale morphological filter: min (erosion) or max (dilation).
fn morph(img: &DynamicImage, rw: u32, rh: u32, is_max: bool) -> DynamicImage {
    window_filter(img, rw, rh, |win| {
        if is_max {
            win.iter().copied().max().unwrap_or(0)
        } else {
            win.iter().copied().min().unwrap_or(0)
        }
    })
}

/// Unsharp-mask sharpening: `out = src + amount * (src - blur(src))`.
fn sharpen_impl(img: &DynamicImage, sigma: f32, amount: f64) -> DynamicImage {
    let blurred = img.blur(sigma.max(0.1));
    let src = img.to_rgba8();
    let blur = blurred.to_rgba8();
    let (w, h) = src.dimensions();
    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let a = src.get_pixel(x, y);
            let b = blur.get_pixel(x, y);
            let mut o = [0u8; 4];
            for c in 0..3 {
                let v = a[c] as f64 + amount * (a[c] as f64 - b[c] as f64);
                o[c] = v.round().clamp(0.0, 255.0) as u8;
            }
            o[3] = a[3];
            out.put_pixel(x, y, image::Rgba(o));
        }
    }
    DynamicImage::ImageRgba8(out)
}

/// Parse a padding/colour argument into opaque-or-transparent RGBA.
fn parse_color(v: &Value) -> Option<[u8; 4]> {
    match v {
        Value::Integer(_) | Value::Real(_) | Value::Rational(_) => {
            let g = norm_to_u8(super::to_f64(v)?);
            Some([g, g, g, 255])
        }
        Value::Call { head, args } if head == "RGBColor" && args.len() >= 3 => Some([
            norm_to_u8(super::to_f64(&args[0])?),
            norm_to_u8(super::to_f64(&args[1])?),
            norm_to_u8(super::to_f64(&args[2])?),
            255,
        ]),
        Value::Call { head, args } if head == "GrayLevel" && !args.is_empty() => {
            let g = norm_to_u8(super::to_f64(&args[0])?);
            Some([g, g, g, 255])
        }
        Value::List(items) if items.len() == 3 => Some([
            norm_to_u8(super::to_f64(&items[0])?),
            norm_to_u8(super::to_f64(&items[1])?),
            norm_to_u8(super::to_f64(&items[2])?),
            255,
        ]),
        Value::List(items) if items.len() == 1 => {
            let g = norm_to_u8(super::to_f64(&items[0])?);
            Some([g, g, g, 255])
        }
        _ => None,
    }
}

/// Parse a 1-based inclusive span `{first, last}` (or a single count).
fn parse_span(v: &Value, len: i64) -> Option<(i64, i64)> {
    let resolve = |n: i64| if n < 0 { len + n + 1 } else { n };
    match v {
        Value::Integer(n) => {
            let n = n.to_f64() as i64;
            if n > 0 {
                Some((1, n.min(len)))
            } else if n < 0 {
                Some(((len + n + 1).max(1), len))
            } else {
                None
            }
        }
        Value::List(items) if items.len() == 2 => {
            let a = resolve(to_i64(&items[0])?);
            let b = resolve(to_i64(&items[1])?);
            let a = a.clamp(1, len.max(1));
            let b = b.clamp(1, len.max(1));
            Some((a.min(b), a.max(b)))
        }
        _ => None,
    }
}

/// Parse a `w x h` pair of positive integer dimensions.
fn parse_dims(v: &Value) -> Option<(i64, i64)> {
    match v {
        Value::List(items) if items.len() == 2 => Some((to_i64(&items[0])?, to_i64(&items[1])?)),
        _ => None,
    }
}

// ── Geometric builtins ────────────────────────────────────────────────────────

/// ImageCrop[image, {w, h}]
/// ImageCrop[image, {w, h}, {x, y}]
///
/// Crop to a `w x h` region. With no centre the region is centred in the image;
/// otherwise the region is centred on `{x, y}` (top-left coordinates).
pub fn builtin_image_crop(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ImageCrop", args));
    };
    if args.len() != 2 && args.len() != 3 {
        return Ok(unevaluated("ImageCrop", args));
    }
    let Some((rw, rh)) = parse_dims(&args[1]) else {
        return Ok(unevaluated("ImageCrop", args));
    };
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let cw = rw.clamp(1, iw.max(1));
    let ch = rh.clamp(1, ih.max(1));

    let (cx, cy) = if args.len() == 3 {
        let Some((x, y)) = parse_dims(&args[2]) else {
            return Ok(unevaluated("ImageCrop", args));
        };
        (x, y)
    } else {
        (iw / 2, ih / 2)
    };

    let x0 = (cx - cw / 2).clamp(0, (iw - cw).max(0));
    let y0 = (cy - ch / 2).clamp(0, (ih - ch).max(0));
    Ok(make_image(
        img.crop_imm(x0 as u32, y0 as u32, cw as u32, ch as u32),
    ))
}

/// ImageTake[image, n]
/// ImageTake[image, {r1, r2}]
/// ImageTake[image, rows, cols]
///
/// Take rows/columns from the image. Positive indices count from the top/left,
/// negative indices from the bottom/right.
pub fn builtin_image_take(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ImageTake", args));
    };
    if args.len() != 2 && args.len() != 3 {
        return Ok(unevaluated("ImageTake", args));
    }
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let Some((r0, r1)) = parse_span(&args[1], ih) else {
        return Ok(unevaluated("ImageTake", args));
    };
    let (c0, c1) = if args.len() == 3 {
        let Some(span) = parse_span(&args[2], iw) else {
            return Ok(unevaluated("ImageTake", args));
        };
        span
    } else {
        (1, iw)
    };

    let x0 = (c0 - 1).clamp(0, (iw - 1).max(0));
    let y0 = (r0 - 1).clamp(0, (ih - 1).max(0));
    let w = (c1 - c0 + 1).clamp(1, iw - x0);
    let h = (r1 - r0 + 1).clamp(1, ih - y0);
    Ok(make_image(
        img.crop_imm(x0 as u32, y0 as u32, w as u32, h as u32),
    ))
}

/// ImagePad[image, n]
/// ImagePad[image, {left, right, bottom, top}]
/// ImagePad[image, spec, color]
///
/// Pad an image with `n` (or per-side) pixels of the given colour (default
/// black).
pub fn builtin_image_pad(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ImagePad", args));
    };
    if args.len() < 2 || args.len() > 3 {
        return Ok(unevaluated("ImagePad", args));
    }
    let (l, r, b, t) = match &args[1] {
        Value::List(items) if items.len() == 4 => {
            let v: Vec<i64> = items.iter().filter_map(to_i64).collect();
            if v.len() != 4 {
                return Ok(unevaluated("ImagePad", args));
            }
            (v[0].max(0), v[1].max(0), v[2].max(0), v[3].max(0))
        }
        other => match to_i64(other) {
            Some(n) => {
                let n = n.max(0);
                (n, n, n, n)
            }
            None => return Ok(unevaluated("ImagePad", args)),
        },
    };

    let fill = args.get(2).and_then(parse_color).unwrap_or([0, 0, 0, 255]);

    let (w, h) = (img.width(), img.height());
    let new_w = w + (l + r) as u32;
    let new_h = h + (b + t) as u32;
    let mut canvas = image::RgbaImage::from_pixel(new_w.max(1), new_h.max(1), image::Rgba(fill));
    let src = img.to_rgba8();
    image::imageops::overlay(&mut canvas, &src, l, t);
    Ok(make_image(DynamicImage::ImageRgba8(canvas)))
}

/// ImageTrim[image]
/// ImageTrim[image, {x, y, w, h}]
/// ImageTrim[image, {{x1, y1}, {x2, y2}}]
/// ImageTrim[image, {x1, y1}, {x2, y2}]
///
/// Trim the image to a bounding region (top-left coordinates). With a single
/// argument, trim the uniform border whose colour matches the top-left pixel.
pub fn builtin_image_trim(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ImageTrim", args));
    };
    if args.is_empty() || args.len() > 3 {
        return Ok(unevaluated("ImageTrim", args));
    }
    let (iw, ih) = (img.width() as i64, img.height() as i64);

    let region = if args.len() == 1 {
        trim_uniform_border(&img)
    } else if args.len() == 3 {
        match (parse_dims(&args[1]), parse_dims(&args[2])) {
            (Some(p1), Some(p2)) => Some((
                p1.0.min(p2.0),
                p1.1.min(p2.1),
                (p1.0 - p2.0).abs() + 1,
                (p1.1 - p2.1).abs() + 1,
            )),
            _ => return Ok(unevaluated("ImageTrim", args)),
        }
    } else {
        match &args[1] {
            Value::List(items) if items.len() == 4 => {
                let v: Vec<i64> = items.iter().filter_map(to_i64).collect();
                if v.len() != 4 {
                    return Ok(unevaluated("ImageTrim", args));
                }
                Some((v[0], v[1], v[2].max(1), v[3].max(1)))
            }
            Value::List(pts) if pts.len() == 2 => {
                let (Some(p1), Some(p2)) = (parse_dims(&pts[0]), parse_dims(&pts[1])) else {
                    return Ok(unevaluated("ImageTrim", args));
                };
                Some((
                    p1.0.min(p2.0),
                    p1.1.min(p2.1),
                    (p1.0 - p2.0).abs() + 1,
                    (p1.1 - p2.1).abs() + 1,
                ))
            }
            _ => return Ok(unevaluated("ImageTrim", args)),
        }
    };
    let Some((x0, y0, w, h)) = region else {
        return Ok(unevaluated("ImageTrim", args));
    };

    let x0 = x0.clamp(0, (iw - 1).max(0));
    let y0 = y0.clamp(0, (ih - 1).max(0));
    let w = w.clamp(1, iw - x0);
    let h = h.clamp(1, ih - y0);
    Ok(make_image(
        img.crop_imm(x0 as u32, y0 as u32, w as u32, h as u32),
    ))
}

/// Bounding rectangle `{x, y, w, h}` of the non-border-coloured content, or
/// `None` if the image is empty.
fn trim_uniform_border(img: &DynamicImage) -> Option<(i64, i64, i64, i64)> {
    let (w, h) = (img.width() as i64, img.height() as i64);
    if w == 0 || h == 0 {
        return None;
    }
    let src = img.to_rgba8();
    let bg = src.get_pixel(0, 0);
    let is_bg = |x: i64, y: i64| -> bool {
        let p = src.get_pixel(x as u32, y as u32);
        (0..3).all(|c| (p[c] as i32 - bg[c] as i32).abs() <= 16)
    };
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, -1i64, -1i64);
    for y in 0..h {
        for x in 0..w {
            if !is_bg(x, y) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < x0 || y1 < y0 {
        // Fully uniform image: keep a single pixel.
        return Some((0, 0, 1, 1));
    }
    Some((x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

// ── Effect / colour builtins ──────────────────────────────────────────────────

/// Round a channel value to the nearest of `levels` evenly spaced levels.
fn posterize_channel(c: u8, levels: u32) -> u8 {
    let levels = levels.max(2) as f64;
    let step = 255.0 / (levels - 1.0);
    ((c as f64 / step).round() * step).round().clamp(0.0, 255.0) as u8
}

/// Emboss the image with a directional `3x3` kernel and a mid-grey bias.
fn emboss_image(img: &DynamicImage) -> DynamicImage {
    let kernel = [[-2.0, -1.0, 0.0], [-1.0, 1.0, 1.0], [0.0, 1.0, 2.0]];
    let src = img.to_rgba8();
    let (w, h) = src.dimensions();
    if w == 0 || h == 0 {
        return DynamicImage::ImageRgba8(src);
    }
    let at = |x: i64, y: i64, c: usize| -> f64 {
        let xx = x.clamp(0, w as i64 - 1) as u32;
        let yy = y.clamp(0, h as i64 - 1) as u32;
        src.get_pixel(xx, yy)[c] as f64
    };
    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let mut px = [0u8; 4];
            for (c, slot) in px.iter_mut().enumerate().take(3) {
                let mut sum = 0.0;
                for ky in 0..3i64 {
                    for kx in 0..3i64 {
                        sum += kernel[ky as usize][kx as usize] * at(x + kx - 1, y + ky - 1, c);
                    }
                }
                *slot = (sum + 128.0).round().clamp(0.0, 255.0) as u8;
            }
            px[3] = src.get_pixel(x as u32, y as u32)[3];
            out.put_pixel(x as u32, y as u32, image::Rgba(px));
        }
    }
    DynamicImage::ImageRgba8(out)
}

/// ImageEffect[image, "effect"]
///
/// Apply a named colour effect. Supported: "GrayLevel"/"Grayscale", "Negate",
/// "Sepia", "Red", "Green", "Blue", "Posterize", "Solarize", "Emboss".
pub fn builtin_image_effect(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ImageEffect", args));
    };
    if args.len() != 2 {
        return Ok(unevaluated("ImageEffect", args));
    }
    let effect = match &args[1] {
        Value::Str(s) => s.to_ascii_lowercase(),
        Value::Symbol(s) => s.to_ascii_lowercase(),
        _ => return Ok(unevaluated("ImageEffect", args)),
    };

    let out = match effect.as_str() {
        "graylevel" | "grayscale" | "gray" | "monochrome" => {
            DynamicImage::ImageLuma8(img.to_luma8())
        }
        "negate" | "invert" => map_color_type(&img, |r, g, b| (255 - r, 255 - g, 255 - b)),
        "sepia" => map_color_type(&img, |r, g, b| {
            let (r, g, b) = (r as f64, g as f64, b as f64);
            let nr = (0.393 * r + 0.769 * g + 0.189 * b).min(255.0);
            let ng = (0.349 * r + 0.686 * g + 0.168 * b).min(255.0);
            let nb = (0.272 * r + 0.534 * g + 0.131 * b).min(255.0);
            (nr as u8, ng as u8, nb as u8)
        }),
        "red" => map_color_type(&img, |r, g, b| {
            (r, (g as f64 * 0.3) as u8, (b as f64 * 0.3) as u8)
        }),
        "green" => map_color_type(&img, |r, g, b| {
            ((r as f64 * 0.3) as u8, g, (b as f64 * 0.3) as u8)
        }),
        "blue" => map_color_type(&img, |r, g, b| {
            ((r as f64 * 0.3) as u8, (g as f64 * 0.3) as u8, b)
        }),
        "posterize" | "posterise" | "posterization" | "posterisation" => {
            map_color_type(&img, |r, g, b| {
                (
                    posterize_channel(r, 4),
                    posterize_channel(g, 4),
                    posterize_channel(b, 4),
                )
            })
        }
        "solarize" | "solarise" => map_color_type(&img, |r, g, b| {
            (
                if r >= 128 { 255 - r } else { r },
                if g >= 128 { 255 - g } else { g },
                if b >= 128 { 255 - b } else { b },
            )
        }),
        "emboss" => emboss_image(&img),
        _ => return Ok(unevaluated("ImageEffect", args)),
    };
    Ok(make_image(out))
}

/// ColorNegate[image]
///
/// Invert every colour channel, preserving alpha.
pub fn builtin_color_negate(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ColorNegate", args));
    };
    if args.len() != 1 {
        return Ok(unevaluated("ColorNegate", args));
    }
    Ok(make_image(map_color_type(&img, |r, g, b| {
        (255 - r, 255 - g, 255 - b)
    })))
}

/// A bright, well-separated palette colour for index `i`.
fn palette_color(i: usize) -> [u8; 3] {
    let h = (i as f64 * 0.618_033_988_749_894_8_f64).fract();
    let s = 0.65;
    let v = 0.9;
    let hi = (h * 6.0).floor() as i32 % 6;
    let f = h * 6.0 - (h * 6.0).floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match hi {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [norm_to_u8(r), norm_to_u8(g), norm_to_u8(b)]
}

/// Colorize[image]
/// Colorize[list]
///
/// Replace intensities/scalars with a colour palette. A list argument produces
/// a 1-row image whose pixels are coloured by distinct value.
pub fn builtin_colorize(args: &[Value]) -> Result<Value, EvalError> {
    match args.first() {
        Some(Value::Image(img)) if args.len() == 1 => {
            let gray = img.to_luma8();
            let (w, h) = gray.dimensions();
            let mut out = image::RgbImage::new(w, h);
            for (o, p) in out.pixels_mut().zip(gray.pixels()) {
                let c = palette_color(p[0] as usize);
                *o = image::Rgb(c);
            }
            Ok(make_image(DynamicImage::ImageRgb8(out)))
        }
        Some(Value::List(items)) if args.len() == 1 => {
            let mut out = image::RgbImage::new(items.len().max(1) as u32, 1);
            for (i, item) in items.iter().enumerate() {
                let c = match item {
                    Value::List(chs) if chs.len() >= 3 => [
                        norm_to_u8(super::to_f64(&chs[0]).unwrap_or(0.0)),
                        norm_to_u8(super::to_f64(&chs[1]).unwrap_or(0.0)),
                        norm_to_u8(super::to_f64(&chs[2]).unwrap_or(0.0)),
                    ],
                    other => match super::to_f64(other) {
                        Some(x) => palette_color((x.abs() * 32.0) as usize),
                        None => [0, 0, 0],
                    },
                };
                out.put_pixel(i as u32, 0, image::Rgb(c));
            }
            Ok(make_image(DynamicImage::ImageRgb8(out)))
        }
        _ => Ok(unevaluated("Colorize", args)),
    }
}

/// Number of colour channels for a colour type (used by ColorSeparate).
fn channel_count(color: image::ColorType) -> usize {
    match color {
        image::ColorType::L8 | image::ColorType::L16 => 1,
        image::ColorType::La8 | image::ColorType::La16 => 2,
        image::ColorType::Rgb8 | image::ColorType::Rgb16 | image::ColorType::Rgb32F => 3,
        _ => 4,
    }
}

/// ColorSeparate[image]
///
/// Separate an image into a list of single-channel grayscale images.
pub fn builtin_color_separate(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ColorSeparate", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("ColorSeparate", args));
    }
    let channels = channel_count(img.color());
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut result = Vec::with_capacity(channels);
    for c in 0..channels {
        let mut buf = image::GrayImage::new(w, h);
        for (o, p) in buf.pixels_mut().zip(rgba.pixels()) {
            *o = image::Luma([p[c]]);
        }
        result.push(make_image(DynamicImage::ImageLuma8(buf)));
    }
    Ok(Value::List(result))
}

/// ColorCombine[list]
/// ColorCombine[list, "RGB"]
///
/// Combine single-channel images into one image. The output colour type is
/// inferred from the number of channels (1→gray, 3→RGB, 4→RGBA).
pub fn builtin_color_combine(args: &[Value]) -> Result<Value, EvalError> {
    let Some(Value::List(imgs)) = args.first() else {
        return Ok(unevaluated("ColorCombine", args));
    };
    if args.len() > 2 || imgs.is_empty() {
        return Ok(unevaluated("ColorCombine", args));
    }
    let mut grays = Vec::with_capacity(imgs.len());
    for v in imgs {
        match v {
            Value::Image(img) => grays.push(img.to_luma8()),
            _ => return Ok(unevaluated("ColorCombine", args)),
        }
    }
    let (w, h) = grays[0].dimensions();
    if grays.iter().any(|g| g.dimensions() != (w, h)) {
        return Ok(unevaluated("ColorCombine", args));
    }

    let out = match grays.len() {
        1 => DynamicImage::ImageLuma8(grays[0].clone()),
        3 => {
            let mut buf = image::RgbImage::new(w, h);
            for (i, p) in buf.pixels_mut().enumerate() {
                let x = (i as u32) % w;
                let y = (i as u32) / w;
                *p = image::Rgb([
                    grays[0].get_pixel(x, y)[0],
                    grays[1].get_pixel(x, y)[0],
                    grays[2].get_pixel(x, y)[0],
                ]);
            }
            DynamicImage::ImageRgb8(buf)
        }
        4 => {
            let mut buf = image::RgbaImage::new(w, h);
            for (i, p) in buf.pixels_mut().enumerate() {
                let x = (i as u32) % w;
                let y = (i as u32) / w;
                *p = image::Rgba([
                    grays[0].get_pixel(x, y)[0],
                    grays[1].get_pixel(x, y)[0],
                    grays[2].get_pixel(x, y)[0],
                    grays[3].get_pixel(x, y)[0],
                ]);
            }
            DynamicImage::ImageRgba8(buf)
        }
        _ => return Ok(unevaluated("ColorCombine", args)),
    };
    Ok(make_image(out))
}

// ── Filter builtins ───────────────────────────────────────────────────────────

/// ImageFilter[image, "name"]
/// ImageFilter[image, "name", r]
///
/// Apply a named neighbourhood filter. Supported names: "BoxBlur"/"Mean",
/// "Median", "Gaussian"/"Blur", "Min", "Max", "Sharpen".
pub fn builtin_image_filter(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ImageFilter", args));
    };
    if args.len() < 2 || args.len() > 3 {
        return Ok(unevaluated("ImageFilter", args));
    }
    let name = match &args[1] {
        Value::Str(s) => s.to_ascii_lowercase().replace(' ', ""),
        Value::Symbol(s) => s.to_ascii_lowercase(),
        _ => return Ok(unevaluated("ImageFilter", args)),
    };
    let r = radius_arg(args, 2);

    let out = match name.as_str() {
        "boxblur" | "box" | "mean" | "average" => box_blur(&img, r, r),
        "median" => median_filter(&img, r, r),
        "gaussian" | "gaussianblur" | "blur" => img.blur(r.max(1) as f32),
        "min" | "erosion" => morph(&img, r, r, false),
        "max" | "dilation" => morph(&img, r, r, true),
        "sharpen" => sharpen_impl(&img, r.max(1) as f32, 1.5),
        _ => return Ok(unevaluated("ImageFilter", args)),
    };
    Ok(make_image(out))
}

/// Dilation[image]  or  Dilation[image, r]
///
/// Grayscale dilation (max filter) with radius `r` (default 1).
pub fn builtin_dilation(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("Dilation", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("Dilation", args));
    }
    let r = radius_arg(args, 1);
    Ok(make_image(morph(&img, r, r, true)))
}

/// Erosion[image]  or  Erosion[image, r]
///
/// Grayscale erosion (min filter) with radius `r` (default 1).
pub fn builtin_erosion(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("Erosion", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("Erosion", args));
    }
    let r = radius_arg(args, 1);
    Ok(make_image(morph(&img, r, r, false)))
}

/// MeanFilter[image, r]
///
/// Box/mean filter with radius `r` (default 1).
pub fn builtin_mean_filter(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("MeanFilter", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("MeanFilter", args));
    }
    let r = radius_arg(args, 1);
    Ok(make_image(box_blur(&img, r, r)))
}

/// MedianFilter[image, r]
///
/// Median filter with radius `r` (default 1).
pub fn builtin_median_filter(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("MedianFilter", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("MedianFilter", args));
    }
    let r = radius_arg(args, 1);
    Ok(make_image(median_filter(&img, r, r)))
}

/// Blur[image, r]
///
/// Gaussian blur with standard deviation `r` (default 1).
pub fn builtin_blur(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("Blur", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("Blur", args));
    }
    let sigma = args.get(1).and_then(super::to_f64).unwrap_or(1.0);
    if sigma <= 0.0 {
        return Ok(make_image((*img).clone()));
    }
    Ok(make_image(img.blur(sigma as f32)))
}

/// Sharpen[image, r]
///
/// Unsharp-mask sharpening with radius `r` (default 1).
pub fn builtin_sharpen(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("Sharpen", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("Sharpen", args));
    }
    let sigma = args.get(1).and_then(super::to_f64).unwrap_or(1.0).max(0.1);
    Ok(make_image(sharpen_impl(&img, sigma as f32, 1.5)))
}

// ── Morphological / thinning ──────────────────────────────────────────────────

/// One Zhang–Suen thinning sub-iteration.
fn thinning_pass(bin: &[bool], w: usize, h: usize, step: usize) -> (Vec<usize>, bool) {
    let at = |x: i64, y: i64| bin[y as usize * w + x as usize];
    let mut delete = Vec::new();
    for y in 1..h as i64 - 1 {
        for x in 1..w as i64 - 1 {
            if !at(x, y) {
                continue;
            }
            let p = [
                at(x, y - 1),
                at(x + 1, y - 1),
                at(x + 1, y),
                at(x + 1, y + 1),
                at(x, y + 1),
                at(x - 1, y + 1),
                at(x - 1, y),
                at(x - 1, y - 1),
            ];
            let b: u32 = p.iter().filter(|&&v| v).count() as u32;
            if !(2..=6).contains(&b) {
                continue;
            }
            let seq: Vec<bool> = p.iter().copied().chain(std::iter::once(p[0])).collect();
            let a = seq.windows(2).filter(|w| !w[0] && w[1]).count();
            if a != 1 {
                continue;
            }
            let (c1, c2) = if step == 0 {
                (!p[0] || !p[2] || !p[4], !p[2] || !p[4] || !p[6])
            } else {
                (!p[0] || !p[2] || !p[6], !p[0] || !p[4] || !p[6])
            };
            if c1 && c2 {
                delete.push(y as usize * w + x as usize);
            }
        }
    }
    let changed = !delete.is_empty();
    (delete, changed)
}

/// Thinning[image]
///
/// Morphological thinning (Zhang–Suen skeletonisation) of a binary image.
pub fn builtin_thinning(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("Thinning", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("Thinning", args));
    }
    let max_iter = args
        .get(1)
        .and_then(super::to_f64)
        .map(|x| x.max(0.0) as usize)
        .unwrap_or(usize::MAX);

    let gray = img.to_luma8();
    let (w, h) = (gray.width() as usize, gray.height() as usize);
    // Work on a zero-padded grid so that image-border pixels can be thinned.
    let pw = w + 2;
    let ph = h + 2;
    let mut bin = vec![false; pw * ph];
    for (i, p) in gray.pixels().enumerate() {
        bin[(i / w + 1) * pw + (i % w + 1)] = p[0] >= 128;
    }

    let mut iter = 0usize;
    'outer: while iter < max_iter {
        let mut any = false;
        for step in 0..2 {
            let (del, changed) = thinning_pass(&bin, pw, ph, step);
            for idx in del {
                bin[idx] = false;
            }
            any |= changed;
        }
        if !any {
            break 'outer;
        }
        iter += 1;
    }

    let mut out = image::GrayImage::new(w as u32, h as u32);
    for y in 0..h {
        for x in 0..w {
            let v = if bin[(y + 1) * pw + (x + 1)] { 255 } else { 0 };
            out.put_pixel(x as u32, y as u32, image::Luma([v]));
        }
    }
    Ok(make_image(DynamicImage::ImageLuma8(out)))
}

// ── Colour analysis ───────────────────────────────────────────────────────────

/// DominantColors[image]
/// DominantColors[image, n]
/// DominantColors[image, n, "Coverage"]
/// DominantColors[image, n, "Count"]
///
/// Return up to `n` (default 5) dominant colours as `RGBColor[...]` values,
/// ordered by pixel frequency. Colours are quantised to 4 bits per channel.
/// With a third argument, each colour is paired with its pixel `"Coverage"`
/// (fraction of the image, in `[0, 1]`) or `"Count"` instead.
pub fn builtin_dominant_colors(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("DominantColors", args));
    };
    if args.len() > 3 {
        return Ok(unevaluated("DominantColors", args));
    }
    let mode = match args.get(2) {
        None => None,
        Some(Value::Str(s)) => Some(s.to_ascii_lowercase()),
        Some(Value::Symbol(s)) => Some(s.to_ascii_lowercase()),
        Some(_) => return Ok(unevaluated("DominantColors", args)),
    };
    if let Some(m) = &mode
        && !matches!(m.as_str(), "coverage" | "count")
    {
        return Ok(unevaluated("DominantColors", args));
    }
    let n = args
        .get(1)
        .and_then(to_i64)
        .map(|v| v.max(1) as usize)
        .unwrap_or(5);

    let rgba = img.to_rgba8();
    let mut counts: HashMap<u32, u32> = HashMap::new();
    for p in rgba.pixels() {
        let key = ((p[0] as u32 >> 4) << 8) | ((p[1] as u32 >> 4) << 4) | (p[2] as u32 >> 4);
        *counts.entry(key).or_insert(0) += 1;
    }
    let total: u64 = counts.values().map(|&c| c as u64).sum();
    let mut entries: Vec<(u32, u32)> = counts.into_iter().collect();
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let colors = entries
        .into_iter()
        .take(n)
        .map(|(key, count)| {
            let r = ((key >> 8) & 0xF) * 16 + 8;
            let g = ((key >> 4) & 0xF) * 16 + 8;
            let b = (key & 0xF) * 16 + 8;
            let color = Value::Call {
                head: "RGBColor".to_string(),
                args: vec![
                    chan_value(r as u8),
                    chan_value(g as u8),
                    chan_value(b as u8),
                ],
            };
            match mode.as_deref() {
                None => color,
                Some("count") => {
                    Value::List(vec![color, Value::Integer(rug::Integer::from(count))])
                }
                Some(_) => {
                    Value::List(vec![color, super::real(count as f64 / total.max(1) as f64)])
                }
            }
        })
        .collect();
    Ok(Value::List(colors))
}

/// ComponentMeasurements[image, property]
///
/// Label-connected-component measurement on a binarised image (threshold 0.5).
/// Returns an association `1 -> value, ...`. Supported properties: "Count",
/// "Area", "Centroid", "BoundingBox", "Mean", "MeanIntensity", "Perimeter".
pub fn builtin_component_measurements(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ComponentMeasurements", args));
    };
    if args.len() != 2 {
        return Ok(unevaluated("ComponentMeasurements", args));
    }
    let prop = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Ok(unevaluated("ComponentMeasurements", args)),
    };
    if !matches!(
        prop.as_str(),
        "Count" | "Area" | "Centroid" | "BoundingBox" | "Mean" | "MeanIntensity" | "Perimeter"
    ) {
        return Ok(unevaluated("ComponentMeasurements", args));
    }

    let gray = img.to_luma8();
    let (w, h) = (gray.width(), gray.height());
    let binary: Vec<bool> = gray.pixels().map(|p| p[0] >= 128).collect();
    let mut labels = vec![0u32; binary.len()];
    let mut components: Vec<Vec<(u32, u32)>> = Vec::new();
    let mut next = 0u32;

    for y in 0..h {
        for x in 0..w {
            let start = (y * w + x) as usize;
            if !binary[start] || labels[start] != 0 {
                continue;
            }
            next += 1;
            let mut stack = vec![(x, y)];
            let mut pixels = Vec::new();
            labels[start] = next;
            while let Some((cx, cy)) = stack.pop() {
                pixels.push((cx, cy));
                let neighbors = [
                    (cx as i64 - 1, cy as i64),
                    (cx as i64 + 1, cy as i64),
                    (cx as i64, cy as i64 - 1),
                    (cx as i64, cy as i64 + 1),
                ];
                for (nx, ny) in neighbors {
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let idx = ny as usize * w as usize + nx as usize;
                    if binary[idx] && labels[idx] == 0 {
                        labels[idx] = next;
                        stack.push((nx as u32, ny as u32));
                    }
                }
            }
            components.push(pixels);
        }
    }

    let mut assoc = HashMap::new();
    for (i, pixels) in components.iter().enumerate() {
        let value = measure_component(&prop, pixels, &gray);
        assoc.insert((i + 1).to_string(), value);
    }
    Ok(Value::Assoc(assoc))
}

/// Compute a single component measurement.
fn measure_component(prop: &str, pixels: &[(u32, u32)], gray: &image::GrayImage) -> Value {
    match prop {
        "Count" | "Area" => Value::Integer(rug::Integer::from(pixels.len())),
        "Centroid" => {
            let (sx, sy) = pixels.iter().fold((0.0, 0.0), |acc, &(x, y)| {
                (acc.0 + x as f64, acc.1 + y as f64)
            });
            let n = pixels.len() as f64;
            Value::List(vec![super::real(sx / n), super::real(sy / n)])
        }
        "BoundingBox" => {
            let (x0, y0) = pixels.iter().fold((u32::MAX, u32::MAX), |acc, &(x, y)| {
                (acc.0.min(x), acc.1.min(y))
            });
            let (x1, y1) = pixels
                .iter()
                .fold((0u32, 0u32), |acc, &(x, y)| (acc.0.max(x), acc.1.max(y)));
            Value::List(vec![
                Value::Integer(rug::Integer::from(x0)),
                Value::Integer(rug::Integer::from(y0)),
                Value::Integer(rug::Integer::from(x1 - x0 + 1)),
                Value::Integer(rug::Integer::from(y1 - y0 + 1)),
            ])
        }
        "Mean" | "MeanIntensity" => {
            let sum: u64 = pixels
                .iter()
                .map(|&(x, y)| gray.get_pixel(x, y)[0] as u64)
                .sum();
            super::real(sum as f64 / (pixels.len() as f64 * 255.0))
        }
        "Perimeter" => {
            let mut count = 0usize;
            for &(x, y) in pixels {
                let boundary = [
                    (x as i64 - 1, y as i64),
                    (x as i64 + 1, y as i64),
                    (x as i64, y as i64 - 1),
                    (x as i64, y as i64 + 1),
                ]
                .iter()
                .any(|&(nx, ny)| {
                    if nx < 0 || ny < 0 || nx >= gray.width() as i64 || ny >= gray.height() as i64 {
                        return true;
                    }
                    gray.get_pixel(nx as u32, ny as u32)[0] < 128
                });
                if boundary {
                    count += 1;
                }
            }
            Value::Integer(rug::Integer::from(count))
        }
        _ => Value::Null,
    }
}

// ── Extended morphology / feature detection ───────────────────────────────────

/// Convert an image to a row-major `f64` luma buffer with its dimensions.
fn luma_f64(img: &DynamicImage) -> (u32, u32, Vec<f64>) {
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    (w, h, gray.pixels().map(|p| p[0] as f64).collect())
}

/// Build a grayscale image by normalising a non-negative response buffer so that
/// its largest value maps to 255.
fn response_image(w: u32, h: u32, data: &[f64]) -> image::GrayImage {
    let max = data.iter().copied().fold(0.0f64, f64::max);
    let mut buf = image::GrayImage::new(w, h);
    for (p, &v) in buf.pixels_mut().zip(data.iter()) {
        *p = image::Luma([if max > 0.0 {
            norm_to_u8(v.max(0.0) / max)
        } else {
            0
        }]);
    }
    buf
}

/// A large finite cost used in place of infinity by the distance transform.
const EDT_BIG: f64 = 1.0e7;

/// Exact 1-D squared Euclidean distance transform of `f` into `d`.
///
/// Felzenszwalb & Huttenlocher's lower-envelope algorithm. `f` holds the squared
/// sample cost (`0` at background samples, a large finite value elsewhere).
fn edt_1d(f: &[f64], d: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0.0f64; n + 1];
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let qf = q as f64;
        let s = loop {
            let r = v[k];
            let rf = r as f64;
            let s = ((f[q] + qf * qf) - (f[r] + rf * rf)) / (2.0 * qf - 2.0 * rf);
            if s <= z[k] {
                if k == 0 {
                    break s;
                }
                k -= 1;
            } else {
                break s;
            }
        };
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, dv) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let r = v[k];
        *dv = (q as f64 - r as f64) * (q as f64 - r as f64) + f[r];
    }
}

/// Euclidean distance transform of the binarised image (distance to the nearest
/// background pixel), normalised so the largest distance maps to 255.
fn distance_transform(img: &DynamicImage) -> image::GrayImage {
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    let (wi, hi) = (w as usize, h as usize);
    if wi == 0 || hi == 0 {
        return gray;
    }
    let mut f = vec![EDT_BIG; wi * hi];
    let mut has_bg = false;
    for (i, p) in gray.pixels().enumerate() {
        if p[0] < 128 {
            f[i] = 0.0;
            has_bg = true;
        }
    }
    if !has_bg {
        // Treat the frame as background so an all-foreground image measures the
        // distance from the border instead of producing a flat result.
        for x in 0..wi {
            f[x] = 0.0;
            f[(hi - 1) * wi + x] = 0.0;
        }
        for y in 0..hi {
            f[y * wi] = 0.0;
            f[y * wi + wi - 1] = 0.0;
        }
    }
    let mut d = vec![0.0f64; wi * hi];
    for y in 0..hi {
        let row = y * wi;
        edt_1d(&f[row..row + wi], &mut d[row..row + wi]);
    }
    let mut column = vec![0.0f64; hi];
    let mut scratch = vec![0.0f64; hi];
    for x in 0..wi {
        for y in 0..hi {
            column[y] = d[y * wi + x];
        }
        edt_1d(&column, &mut scratch);
        for y in 0..hi {
            d[y * wi + x] = scratch[y];
        }
    }
    let max = d.iter().copied().fold(0.0f64, f64::max).sqrt();
    let mut buf = image::GrayImage::new(w, h);
    for (p, &v) in buf.pixels_mut().zip(d.iter()) {
        *p = image::Luma([if max > 0.0 {
            norm_to_u8(v.sqrt() / max)
        } else {
            0
        }]);
    }
    buf
}

/// Harris corner response of the image (structure-tensor determinant minus
/// `k * trace²`, clamped to non-negative values).
fn corner_response(img: &DynamicImage) -> image::GrayImage {
    let (w, h, g) = luma_f64(img);
    let (wi, hi) = (w as usize, h as usize);
    if wi == 0 || hi == 0 {
        return image::GrayImage::new(w, h);
    }
    let at = |x: i64, y: i64| -> f64 {
        let xx = x.clamp(0, wi as i64 - 1) as usize;
        let yy = y.clamp(0, hi as i64 - 1) as usize;
        g[yy * wi + xx]
    };
    let sobel = |x: i64, y: i64| -> (f64, f64) {
        let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
            - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
        let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
            - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
        (gx, gy)
    };
    let k = 0.04f64;
    let mut resp = vec![0.0f64; wi * hi];
    for y in 0..hi as i64 {
        for x in 0..wi as i64 {
            let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (gx, gy) = sobel(x + dx, y + dy);
                    sxx += gx * gx;
                    syy += gy * gy;
                    sxy += gx * gy;
                }
            }
            let det = sxx * syy - sxy * sxy;
            let trace = sxx + syy;
            resp[y as usize * wi + x as usize] = (det - k * trace * trace).max(0.0);
        }
    }
    response_image(w, h, &resp)
}

/// Absolute Laplacian response, optionally after Gaussian smoothing with `sigma`.
fn laplacian_response(img: &DynamicImage, sigma: f32) -> image::GrayImage {
    let smoothed = if sigma > 0.0 {
        img.blur(sigma)
    } else {
        img.clone()
    };
    let (w, h, g) = luma_f64(&smoothed);
    let (wi, hi) = (w as usize, h as usize);
    if wi == 0 || hi == 0 {
        return image::GrayImage::new(w, h);
    }
    let at = |x: i64, y: i64| -> f64 {
        let xx = x.clamp(0, wi as i64 - 1) as usize;
        let yy = y.clamp(0, hi as i64 - 1) as usize;
        g[yy * wi + xx]
    };
    let mut resp = vec![0.0f64; wi * hi];
    for y in 0..hi as i64 {
        for x in 0..wi as i64 {
            let v = at(x, y - 1) + at(x, y + 1) + at(x - 1, y) + at(x + 1, y) - 4.0 * at(x, y);
            resp[y as usize * wi + x as usize] = v.abs();
        }
    }
    response_image(w, h, &resp)
}

/// Scharr gradient magnitude, optionally after Gaussian smoothing with `sigma`.
fn edge_response(img: &DynamicImage, sigma: f32) -> image::GrayImage {
    let smoothed = if sigma > 0.0 {
        img.blur(sigma)
    } else {
        img.clone()
    };
    let (w, h, g) = luma_f64(&smoothed);
    let (wi, hi) = (w as usize, h as usize);
    if wi == 0 || hi == 0 {
        return image::GrayImage::new(w, h);
    }
    let at = |x: i64, y: i64| -> f64 {
        let xx = x.clamp(0, wi as i64 - 1) as usize;
        let yy = y.clamp(0, hi as i64 - 1) as usize;
        g[yy * wi + xx]
    };
    let mut resp = vec![0.0f64; wi * hi];
    for y in 0..hi as i64 {
        for x in 0..wi as i64 {
            let gx = 3.0 * (at(x + 1, y - 1) - at(x - 1, y - 1))
                + 10.0 * (at(x + 1, y) - at(x - 1, y))
                + 3.0 * (at(x + 1, y + 1) - at(x - 1, y + 1));
            let gy = 3.0 * (at(x - 1, y + 1) - at(x - 1, y - 1))
                + 10.0 * (at(x, y + 1) - at(x, y - 1))
                + 3.0 * (at(x + 1, y + 1) - at(x + 1, y - 1));
            resp[y as usize * wi + x as usize] = (gx * gx + gy * gy).sqrt();
        }
    }
    response_image(w, h, &resp)
}

/// Fill background regions that are not 4-connected to the image border.
fn fill_holes(binary: &[bool], w: usize, h: usize) -> Vec<bool> {
    let mut reachable = vec![false; w * h];
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for x in 0..w {
        for y in [0usize, h - 1] {
            let i = y * w + x;
            if !binary[i] && !reachable[i] {
                reachable[i] = true;
                stack.push((x, y));
            }
        }
    }
    for y in 0..h {
        for x in [0usize, w - 1] {
            let i = y * w + x;
            if !binary[i] && !reachable[i] {
                reachable[i] = true;
                stack.push((x, y));
            }
        }
    }
    while let Some((x, y)) = stack.pop() {
        let neighbors = [
            (x as i64 - 1, y as i64),
            (x as i64 + 1, y as i64),
            (x as i64, y as i64 - 1),
            (x as i64, y as i64 + 1),
        ];
        for (nx, ny) in neighbors {
            if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                continue;
            }
            let (nx, ny) = (nx as usize, ny as usize);
            let i = ny * w + nx;
            if !binary[i] && !reachable[i] {
                reachable[i] = true;
                stack.push((nx, ny));
            }
        }
    }
    (0..w * h).map(|i| binary[i] || !reachable[i]).collect()
}

/// DistanceTransform[image]
///
/// For each foreground (pixel ≥ 0.5) pixel, the Euclidean distance to the
/// nearest background pixel, normalised so the largest distance is white. An
/// all-foreground image measures distance from the frame.
pub fn builtin_distance_transform(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("DistanceTransform", args));
    };
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("DistanceTransform", args));
    }
    if let Some(v) = args.get(1)
        && super::to_f64(v).is_none()
        && !matches!(v, Value::Str(_) | Value::Symbol(_))
    {
        return Ok(unevaluated("DistanceTransform", args));
    }
    Ok(make_image(DynamicImage::ImageLuma8(distance_transform(
        &img,
    ))))
}

/// CornerFilter[image]
///
/// Harris corner response, normalised to `[0, 1]`.
pub fn builtin_corner_filter(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("CornerFilter", args));
    };
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("CornerFilter", args));
    }
    Ok(make_image(DynamicImage::ImageLuma8(corner_response(&img))))
}

/// LaplacianFilter[image]
/// LaplacianFilter[image, r]
///
/// Absolute Laplacian (Laplacian-of-Gaussian when `r > 0`), normalised to
/// `[0, 1]`.
pub fn builtin_laplacian_filter(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("LaplacianFilter", args));
    };
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("LaplacianFilter", args));
    }
    let sigma = args.get(1).and_then(super::to_f64).unwrap_or(0.0).max(0.0);
    Ok(make_image(DynamicImage::ImageLuma8(laplacian_response(
        &img,
        sigma as f32,
    ))))
}

/// EdgeFilter[image]
/// EdgeFilter[image, r]
///
/// Scharr gradient magnitude (optionally of a Gaussian-smoothed image),
/// normalised to `[0, 1]`. Complements the Sobel-based `EdgeDetect`.
pub fn builtin_edge_filter(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("EdgeFilter", args));
    };
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("EdgeFilter", args));
    }
    let sigma = args.get(1).and_then(super::to_f64).unwrap_or(0.0).max(0.0);
    Ok(make_image(DynamicImage::ImageLuma8(edge_response(
        &img,
        sigma as f32,
    ))))
}

/// FillingTransform[image]
///
/// Fill the holes of a binary image: background regions not connected to the
/// border become foreground.
pub fn builtin_filling_transform(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("FillingTransform", args));
    };
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("FillingTransform", args));
    }
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    let (wi, hi) = (w as usize, h as usize);
    if wi == 0 || hi == 0 {
        return Ok(make_image(DynamicImage::ImageLuma8(gray)));
    }
    let binary: Vec<bool> = gray.pixels().map(|p| p[0] >= 128).collect();
    let filled = fill_holes(&binary, wi, hi);
    let mut buf = image::GrayImage::new(w, h);
    for (p, &b) in buf.pixels_mut().zip(filled.iter()) {
        *p = image::Luma([if b { 255 } else { 0 }]);
    }
    Ok(make_image(DynamicImage::ImageLuma8(buf)))
}

/// MorphologicalBinarize[image]
/// MorphologicalBinarize[image, t]
///
/// Adaptive local-mean binarisation: a pixel is foreground when it exceeds its
/// neighbourhood mean by a relative margin `t` (default 0.04).
pub fn builtin_morphological_binarize(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("MorphologicalBinarize", args));
    };
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("MorphologicalBinarize", args));
    }
    let t = match args.get(1) {
        None => 0.04,
        Some(v) => match super::to_f64(v) {
            Some(x) => x,
            None => return Ok(unevaluated("MorphologicalBinarize", args)),
        },
    };
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    if w == 0 || h == 0 {
        return Ok(make_image(DynamicImage::ImageLuma8(gray)));
    }
    let r = (w.min(h) / 4).clamp(1, 15);
    let local = box_blur(&DynamicImage::ImageLuma8(gray.clone()), r, r).to_luma8();
    let mut buf = image::GrayImage::new(w, h);
    for (p, (v, m)) in buf.pixels_mut().zip(gray.pixels().zip(local.pixels())) {
        let threshold = m[0] as f64 * (1.0 + t);
        *p = image::Luma([if v[0] as f64 > threshold { 255 } else { 0 }]);
    }
    Ok(make_image(DynamicImage::ImageLuma8(buf)))
}

/// Thicken[image]
/// Thicken[image, r]
///
/// Grow the foreground of a binary image by a square structuring element of
/// radius `r` (default 1), the counterpart to `Thinning`.
pub fn builtin_thicken(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("Thicken", args));
    };
    if args.len() > 2 {
        return Ok(unevaluated("Thicken", args));
    }
    let r = radius_arg(args, 1);
    let gray = img.to_luma8();
    let mut bin = image::GrayImage::new(gray.width(), gray.height());
    for (p, s) in bin.pixels_mut().zip(gray.pixels()) {
        *p = image::Luma([if s[0] >= 128 { 255 } else { 0 }]);
    }
    Ok(make_image(morph(
        &DynamicImage::ImageLuma8(bin),
        r,
        r,
        true,
    )))
}

/// Split `pixels` into at most `n` boxes by median cut and return box averages.
fn median_cut(pixels: Vec<[u8; 3]>, n: usize) -> Vec<[u8; 3]> {
    if pixels.is_empty() {
        return vec![[0, 0, 0]];
    }
    let mut boxes: Vec<Vec<[u8; 3]>> = vec![pixels];
    while boxes.len() < n {
        let mut best: Option<usize> = None;
        let mut best_score = 0usize;
        for (i, b) in boxes.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            let mut lo = [255u8; 3];
            let mut hi = [0u8; 3];
            for p in b {
                for c in 0..3 {
                    lo[c] = lo[c].min(p[c]);
                    hi[c] = hi[c].max(p[c]);
                }
            }
            let range = (0..3).map(|c| (hi[c] - lo[c]) as usize).max().unwrap_or(0);
            let score = range * b.len();
            if score >= best_score {
                best_score = score;
                best = Some(i);
            }
        }
        let Some(i) = best else {
            break;
        };
        let mut b = boxes.swap_remove(i);
        let mut lo = [255u8; 3];
        let mut hi = [0u8; 3];
        for p in &b {
            for c in 0..3 {
                lo[c] = lo[c].min(p[c]);
                hi[c] = hi[c].max(p[c]);
            }
        }
        let ch = (0..3).max_by_key(|&c| hi[c] - lo[c]).unwrap_or(0);
        b.sort_by_key(|p| p[ch]);
        let right = b.split_off(b.len() / 2);
        boxes.push(b);
        boxes.push(right);
    }
    boxes
        .iter()
        .filter(|b| !b.is_empty())
        .map(|b| {
            let mut sum = [0u64; 3];
            for p in b {
                for c in 0..3 {
                    sum[c] += p[c] as u64;
                }
            }
            let len = b.len() as u64;
            [
                (sum[0] / len) as u8,
                (sum[1] / len) as u8,
                (sum[2] / len) as u8,
            ]
        })
        .collect()
}

/// ColorQuantize[image]
/// ColorQuantize[image, n]
///
/// Reduce an image to at most `n` (default 8) colours using median-cut
/// quantisation.
pub fn builtin_color_quantize(args: &[Value]) -> Result<Value, EvalError> {
    let Some(img) = get_image(args, 0) else {
        return Ok(unevaluated("ColorQuantize", args));
    };
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("ColorQuantize", args));
    }
    let n = match args.get(1) {
        None => 8usize,
        Some(v) => match to_i64(v) {
            Some(x) if x >= 1 => (x as usize).min(256),
            _ => return Ok(unevaluated("ColorQuantize", args)),
        },
    };
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let pixels: Vec<[u8; 3]> = rgb.pixels().map(|p| [p[0], p[1], p[2]]).collect();
    let palette = median_cut(pixels, n);
    let mut out = image::RgbImage::new(w, h);
    for (o, p) in out.pixels_mut().zip(rgb.pixels()) {
        let mut best = palette[0];
        let mut best_d = u32::MAX;
        for c in &palette {
            let dr = p[0] as i32 - c[0] as i32;
            let dg = p[1] as i32 - c[1] as i32;
            let db = p[2] as i32 - c[2] as i32;
            let d = (dr * dr + dg * dg + db * db) as u32;
            if d < best_d {
                best_d = d;
                best = *c;
            }
        }
        *o = image::Rgb(best);
    }
    Ok(make_image(DynamicImage::ImageRgb8(out)))
}

// ── Registration ──────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "ImageCrop", builtin_image_crop);
    super::register_builtin(env, "ImageTake", builtin_image_take);
    super::register_builtin(env, "ImagePad", builtin_image_pad);
    super::register_builtin(env, "ImageTrim", builtin_image_trim);
    super::register_builtin(env, "ImageEffect", builtin_image_effect);
    super::register_builtin(env, "ImageFilter", builtin_image_filter);
    super::register_builtin(env, "ColorNegate", builtin_color_negate);
    super::register_builtin(env, "Colorize", builtin_colorize);
    super::register_builtin(env, "ColorSeparate", builtin_color_separate);
    super::register_builtin(env, "ColorCombine", builtin_color_combine);
    super::register_builtin(env, "Dilation", builtin_dilation);
    super::register_builtin(env, "Erosion", builtin_erosion);
    super::register_builtin(env, "Thinning", builtin_thinning);
    super::register_builtin(env, "MeanFilter", builtin_mean_filter);
    super::register_builtin(env, "MedianFilter", builtin_median_filter);
    super::register_builtin(env, "Blur", builtin_blur);
    super::register_builtin(env, "Sharpen", builtin_sharpen);
    super::register_builtin(env, "DominantColors", builtin_dominant_colors);
    super::register_builtin(env, "ColorQuantize", builtin_color_quantize);
    super::register_builtin(env, "ComponentMeasurements", builtin_component_measurements);
    super::register_builtin(env, "DistanceTransform", builtin_distance_transform);
    super::register_builtin(env, "CornerFilter", builtin_corner_filter);
    super::register_builtin(env, "LaplacianFilter", builtin_laplacian_filter);
    super::register_builtin(env, "EdgeFilter", builtin_edge_filter);
    super::register_builtin(env, "FillingTransform", builtin_filling_transform);
    super::register_builtin(env, "MorphologicalBinarize", builtin_morphological_binarize);
    super::register_builtin(env, "Thicken", builtin_thicken);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn gray_img(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> Value {
        let mut buf = image::GrayImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                buf.put_pixel(x, y, image::Luma([f(x, y)]));
            }
        }
        make_image(DynamicImage::ImageLuma8(buf))
    }

    fn rgb_img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Value {
        let mut buf = image::RgbImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                buf.put_pixel(x, y, image::Rgb(f(x, y)));
            }
        }
        make_image(DynamicImage::ImageRgb8(buf))
    }

    fn extract(v: &Value) -> Arc<DynamicImage> {
        match v {
            Value::Image(img) => Arc::clone(img),
            other => panic!("expected Image, got {other:?}"),
        }
    }

    fn dims(v: &Value) -> (u32, u32) {
        let img = extract(v);
        (img.width(), img.height())
    }

    fn real(v: f64) -> Value {
        crate::builtins::real(v)
    }

    fn is_unevaluated(v: &Value, head: &str) -> bool {
        matches!(v, Value::Call { head: h, .. } if h == head)
    }

    // ── ImageCrop ──

    #[test]
    fn test_image_crop_centered() {
        let img = gray_img(4, 4, |x, y| (x * 10 + y) as u8);
        let out = builtin_image_crop(&[
            img,
            Value::List(vec![
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(2)),
            ]),
        ])
        .unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (2, 2));
    }

    #[test]
    fn test_image_crop_with_center() {
        let img = gray_img(6, 6, |_, _| 100);
        let out = builtin_image_crop(&[
            img,
            Value::List(vec![
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(2)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(1)),
            ]),
        ])
        .unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (2, 2));
    }

    #[test]
    fn test_image_crop_inapplicable() {
        let out = builtin_image_crop(&[Value::Integer(rug::Integer::from(3))]).unwrap();
        assert!(is_unevaluated(&out, "ImageCrop"));
    }

    // ── ImageTake ──

    #[test]
    fn test_image_take_rows() {
        let img = gray_img(3, 4, |_, _| 0);
        let out = builtin_image_take(&[img, Value::Integer(rug::Integer::from(2))]).unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (3, 2));
    }

    #[test]
    fn test_image_take_negative_rows() {
        let img = gray_img(3, 4, |_, _| 0);
        let out = builtin_image_take(&[img, Value::Integer(rug::Integer::from(-2))]).unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (3, 2));
    }

    #[test]
    fn test_image_take_range_both_axes() {
        let img = gray_img(5, 5, |_, _| 0);
        let out = builtin_image_take(&[
            img,
            Value::List(vec![
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(4)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(3)),
            ]),
        ])
        .unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (3, 3));
    }

    // ── ImagePad ──

    #[test]
    fn test_image_pad_scalar() {
        let img = gray_img(2, 2, |_, _| 255);
        let out = builtin_image_pad(&[img, Value::Integer(rug::Integer::from(1))]).unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (4, 4));
        assert_eq!(o.to_rgba8().get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn test_image_pad_list_with_color() {
        let img = gray_img(2, 2, |_, _| 0);
        let out = builtin_image_pad(&[
            img,
            Value::List(vec![
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(0)),
            ]),
            Value::Call {
                head: "RGBColor".to_string(),
                args: vec![real(1.0), real(0.0), real(0.0)],
            },
        ])
        .unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (4, 3));
    }

    // ── ImageTrim ──

    #[test]
    fn test_image_trim_rect() {
        let img = gray_img(8, 8, |_, _| 10);
        let out = builtin_image_trim(&[
            img,
            Value::List(vec![
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(3)),
                Value::Integer(rug::Integer::from(4)),
            ]),
        ])
        .unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (3, 4));
    }

    #[test]
    fn test_image_trim_points() {
        let img = gray_img(8, 8, |_, _| 10);
        let pt = |a: i64, b: i64| {
            Value::List(vec![
                Value::Integer(rug::Integer::from(a)),
                Value::Integer(rug::Integer::from(b)),
            ])
        };
        let out = builtin_image_trim(&[img, Value::List(vec![pt(1, 1), pt(5, 4)])]).unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (5, 4));
    }

    // ── ImageEffect ──

    #[test]
    fn test_image_effect_negate() {
        let img = gray_img(2, 2, |_, _| 0);
        let out = builtin_image_effect(&[img, Value::Str("Negate".to_string())]).unwrap();
        assert_eq!(extract(&out).to_rgba8().get_pixel(0, 0)[0], 255);
    }

    #[test]
    fn test_image_effect_sepia() {
        let img = rgb_img(2, 2, |_, _| [128, 128, 128]);
        let out = builtin_image_effect(&[img, Value::Str("Sepia".to_string())]).unwrap();
        let rgba = extract(&out).to_rgba8();
        let p = rgba.get_pixel(0, 0);
        // Sepia pushes red above the original.
        assert!(p[0] > 128);
    }

    #[test]
    fn test_image_effect_unknown() {
        let img = gray_img(2, 2, |_, _| 0);
        let out = builtin_image_effect(&[img, Value::Str("Nope".to_string())]).unwrap();
        assert!(is_unevaluated(&out, "ImageEffect"));
    }

    // ── ImageFilter ──

    #[test]
    fn test_image_filter_median() {
        let img = gray_img(4, 4, |x, y| ((x + y) * 10) as u8);
        let out = builtin_image_filter(&[img, Value::Str("Median".to_string())]).unwrap();
        assert_eq!(dims(&out), (4, 4));
    }

    #[test]
    fn test_image_filter_boxblur() {
        let img = gray_img(4, 4, |_, _| 200);
        let out = builtin_image_filter(&[img, Value::Str("BoxBlur".to_string())]).unwrap();
        assert_eq!(dims(&out), (4, 4));
    }

    #[test]
    fn test_image_filter_unknown() {
        let img = gray_img(2, 2, |_, _| 1);
        let out = builtin_image_filter(&[img, Value::Str("Wat".to_string())]).unwrap();
        assert!(is_unevaluated(&out, "ImageFilter"));
    }

    // ── ColorNegate ──

    #[test]
    fn test_color_negate() {
        let img = rgb_img(2, 2, |_, _| [10, 20, 30]);
        let out = builtin_color_negate(&[img]).unwrap();
        let rgba = extract(&out).to_rgba8();
        let p = rgba.get_pixel(0, 0);
        assert_eq!([p[0], p[1], p[2]], [245, 235, 225]);
    }

    #[test]
    fn test_color_negate_alpha_preserved() {
        let mut buf = image::RgbaImage::new(1, 1);
        buf.put_pixel(0, 0, image::Rgba([10, 20, 30, 77]));
        let img = make_image(DynamicImage::ImageRgba8(buf));
        let out = builtin_color_negate(&[img]).unwrap();
        assert_eq!(extract(&out).to_rgba8().get_pixel(0, 0)[3], 77);
    }

    // ── Colorize ──

    #[test]
    fn test_colorize_image() {
        let img = gray_img(3, 1, |x, _| (x * 100) as u8);
        let out = builtin_colorize(&[img]).unwrap();
        assert_eq!(dims(&out), (3, 1));
    }

    #[test]
    fn test_colorize_list() {
        let out = builtin_colorize(&[Value::List(vec![
            Value::Integer(rug::Integer::from(1)),
            Value::Integer(rug::Integer::from(2)),
            Value::Integer(rug::Integer::from(3)),
        ])])
        .unwrap();
        assert_eq!(dims(&out), (3, 1));
    }

    // ── ColorSeparate / ColorCombine ──

    #[test]
    fn test_color_separate_roundtrip() {
        let img = rgb_img(2, 2, |x, y| [(x * 50) as u8, (y * 50) as u8, 90]);
        let parts = builtin_color_separate(&[img]).unwrap();
        let Value::List(list) = &parts else {
            panic!("expected list");
        };
        assert_eq!(list.len(), 3);
        let combined = builtin_color_combine(&[Value::List(list.clone())]).unwrap();
        let c = extract(&combined);
        assert_eq!(c.color(), image::ColorType::Rgb8);
        assert_eq!((c.width(), c.height()), (2, 2));
    }

    // ── Morphology ──

    #[test]
    fn test_dilation_grows_bright_pixel() {
        let img = gray_img(5, 5, |x, y| if x == 2 && y == 2 { 255 } else { 0 });
        let out = builtin_dilation(&[img, Value::Integer(rug::Integer::from(1))]).unwrap();
        // The plus-shaped dilation covers (2,3) etc.
        assert_eq!(extract(&out).to_luma8().get_pixel(2, 3)[0], 255);
    }

    #[test]
    fn test_erosion_shrinks_bright_pixel() {
        let img = gray_img(5, 5, |_, _| 255);
        let out = builtin_erosion(&[img, Value::Integer(rug::Integer::from(1))]).unwrap();
        assert_eq!(extract(&out).to_luma8().get_pixel(2, 2)[0], 255);
    }

    #[test]
    fn test_thinning_produces_lines() {
        // A filled 5x5 block thins to a small set of pixels.
        let img = gray_img(5, 5, |_, _| 255);
        let out = builtin_thinning(&[img]).unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (5, 5));
        let bright = o.to_luma8().pixels().filter(|p| p[0] > 0).count();
        assert!((1..25).contains(&bright), "expected skeleton, got {bright}");
    }

    // ── Linear filters ──

    #[test]
    fn test_mean_filter() {
        let img = gray_img(4, 4, |x, _| if x == 0 { 0 } else { 255 });
        let out = builtin_mean_filter(&[img, Value::Integer(rug::Integer::from(1))]).unwrap();
        assert_eq!(dims(&out), (4, 4));
    }

    #[test]
    fn test_median_filter_removes_speckle() {
        let img = gray_img(3, 3, |x, y| if x == 1 && y == 1 { 255 } else { 0 });
        let out = builtin_median_filter(&[img, Value::Integer(rug::Integer::from(1))]).unwrap();
        // Center neighbourhood median is 0.
        assert_eq!(extract(&out).to_luma8().get_pixel(1, 1)[0], 0);
    }

    #[test]
    fn test_blur() {
        let img = rgb_img(4, 4, |_, _| [200, 100, 50]);
        let out = builtin_blur(&[img, real(1.0)]).unwrap();
        assert_eq!(dims(&out), (4, 4));
    }

    #[test]
    fn test_sharpen() {
        let img = rgb_img(4, 4, |_, _| [200, 100, 50]);
        let out = builtin_sharpen(&[img]).unwrap();
        assert_eq!(dims(&out), (4, 4));
    }

    // ── DominantColors ──

    #[test]
    fn test_dominant_colors_count() {
        let img = rgb_img(4, 4, |x, _| if x < 2 { [255, 0, 0] } else { [0, 0, 255] });
        let out = builtin_dominant_colors(&[img, Value::Integer(rug::Integer::from(2))]).unwrap();
        let Value::List(list) = &out else {
            panic!("expected list");
        };
        assert_eq!(list.len(), 2);
        assert!(matches!(&list[0], Value::Call { head, .. } if head == "RGBColor"));
    }

    // ── ComponentMeasurements ──

    #[test]
    fn test_component_measurements_area() {
        // Two separate bright 2x2 squares.
        let img = gray_img(6, 3, |x, y| {
            let a = x < 2 && y < 2;
            let b = x >= 4 && y < 2;
            if a || b { 255 } else { 0 }
        });
        let out = builtin_component_measurements(&[img, Value::Str("Area".to_string())]).unwrap();
        let Value::Assoc(map) = &out else {
            panic!("expected assoc");
        };
        assert_eq!(map.len(), 2);
        for v in map.values() {
            assert_eq!(*v, Value::Integer(rug::Integer::from(4)));
        }
    }

    #[test]
    fn test_component_measurements_centroid() {
        let img = gray_img(4, 4, |x, y| if x < 2 && y < 2 { 255 } else { 0 });
        let out =
            builtin_component_measurements(&[img, Value::Str("Centroid".to_string())]).unwrap();
        let Value::Assoc(map) = &out else {
            panic!("expected assoc");
        };
        let Some(Value::List(c)) = map.values().next() else {
            panic!("expected centroid list");
        };
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn test_component_measurements_unknown_prop() {
        let img = gray_img(2, 2, |_, _| 255);
        let out = builtin_component_measurements(&[img, Value::Str("Zzz".to_string())]).unwrap();
        assert!(is_unevaluated(&out, "ComponentMeasurements"));
    }

    // ── DistanceTransform ──

    #[test]
    fn test_distance_transform_point() {
        let img = gray_img(5, 5, |x, y| if x == 2 && y == 2 { 0 } else { 255 });
        let out = builtin_distance_transform(&[img]).unwrap();
        let o = extract(&out).to_luma8();
        // The background pixel itself is zero; the immediate neighbour is the
        // smallest non-zero distance.
        assert_eq!(o.get_pixel(2, 2)[0], 0);
        assert!(o.get_pixel(2, 3)[0] > 0);
        assert!(o.get_pixel(2, 3)[0] < 128);
        // The corner is farthest from the single background pixel.
        assert!(o.get_pixel(0, 0)[0] > o.get_pixel(2, 3)[0]);
    }

    #[test]
    fn test_distance_transform_all_foreground_uses_frame() {
        let img = gray_img(5, 5, |_, _| 255);
        let out = builtin_distance_transform(&[img]).unwrap();
        let o = extract(&out).to_luma8();
        // Border pixels are at distance 0 from the frame; centre is largest.
        assert_eq!(o.get_pixel(0, 0)[0], 0);
        assert!(o.get_pixel(2, 2)[0] > o.get_pixel(1, 1)[0]);
    }

    #[test]
    fn test_distance_transform_inapplicable() {
        let out = builtin_distance_transform(&[Value::Integer(rug::Integer::from(1))]).unwrap();
        assert!(is_unevaluated(&out, "DistanceTransform"));
    }

    // ── Corner / edge / laplacian filters ──

    #[test]
    fn test_corner_filter_finds_corner() {
        // A single right-angle corner.
        let img = gray_img(8, 8, |x, y| if x < 4 && y < 4 { 255 } else { 0 });
        let out = builtin_corner_filter(&[img]).unwrap();
        let o = extract(&out).to_luma8();
        assert_eq!((o.width(), o.height()), (8, 8));
        let peak = o.pixels().map(|p| p[0]).max().unwrap();
        assert!(peak > 0, "expected a non-zero corner response");
    }

    #[test]
    fn test_laplacian_filter_edge() {
        // Vertical step edge: the Laplacian is non-zero only at the seam.
        let img = gray_img(6, 6, |x, _| if x < 3 { 0 } else { 255 });
        let out = builtin_laplacian_filter(&[img]).unwrap();
        let o = extract(&out).to_luma8();
        assert_eq!((o.width(), o.height()), (6, 6));
        assert!(o.get_pixel(3, 2)[0] > 0);
        assert_eq!(o.get_pixel(0, 2)[0], 0);
    }

    #[test]
    fn test_edge_filter_step() {
        let img = gray_img(6, 6, |x, _| if x < 3 { 0 } else { 255 });
        let out = builtin_edge_filter(&[img]).unwrap();
        let o = extract(&out).to_luma8();
        assert!(o.get_pixel(2, 3)[0] > 0 || o.get_pixel(3, 3)[0] > 0);
        assert_eq!(o.get_pixel(0, 3)[0], 0);
    }

    // ── FillingTransform ──

    #[test]
    fn test_filling_transform_fills_ring() {
        let img = gray_img(5, 5, |x, y| {
            if x == 0 || y == 0 || x == 4 || y == 4 {
                255
            } else {
                0
            }
        });
        let out = builtin_filling_transform(&[img]).unwrap();
        let o = extract(&out).to_luma8();
        assert_eq!(o.get_pixel(2, 2)[0], 255, "interior hole should be filled");
    }

    #[test]
    fn test_filling_transform_keeps_open_region() {
        // The left two columns are background and touch the border.
        let img = gray_img(5, 5, |x, _| if x < 2 { 0 } else { 255 });
        let out = builtin_filling_transform(&[img]).unwrap();
        let o = extract(&out).to_luma8();
        assert_eq!(o.get_pixel(0, 0)[0], 0);
        assert_eq!(o.get_pixel(4, 4)[0], 255);
    }

    // ── MorphologicalBinarize / Thicken ──

    #[test]
    fn test_morphological_binarize_square() {
        let img = gray_img(9, 9, |x, y| {
            if (3..6).contains(&x) && (3..6).contains(&y) {
                255
            } else {
                0
            }
        });
        let out = builtin_morphological_binarize(&[img]).unwrap();
        let o = extract(&out).to_luma8();
        assert_eq!(o.get_pixel(4, 4)[0], 255);
        assert_eq!(o.get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn test_thicken_grows_foreground() {
        let img = gray_img(5, 5, |x, y| if x == 2 && y == 2 { 255 } else { 0 });
        let out = builtin_thicken(&[img, Value::Integer(rug::Integer::from(1))]).unwrap();
        let o = extract(&out).to_luma8();
        assert_eq!(o.get_pixel(2, 2)[0], 255);
        assert_eq!(o.get_pixel(2, 3)[0], 255);
        assert_eq!(o.get_pixel(0, 0)[0], 0);
    }

    // ── ColorQuantize ──

    #[test]
    fn test_color_quantize_limits_palette() {
        let img = rgb_img(8, 8, |x, _| {
            [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0]][(x / 2) as usize % 4]
        });
        let out = builtin_color_quantize(&[img, Value::Integer(rug::Integer::from(2))]).unwrap();
        let o = extract(&out).to_rgb8();
        assert_eq!((o.width(), o.height()), (8, 8));
        let mut distinct: Vec<[u8; 3]> = o.pixels().map(|p| [p[0], p[1], p[2]]).collect();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(distinct.len() <= 2, "got {} colours", distinct.len());
    }

    // ── DominantColors extras ──

    #[test]
    fn test_dominant_colors_coverage() {
        let img = rgb_img(4, 4, |x, _| if x < 1 { [255, 0, 0] } else { [0, 0, 255] });
        let out = builtin_dominant_colors(&[
            img,
            Value::Integer(rug::Integer::from(2)),
            Value::Str("Coverage".to_string()),
        ])
        .unwrap();
        let Value::List(list) = &out else {
            panic!("expected list");
        };
        assert_eq!(list.len(), 2);
        let Some(Value::List(pair)) = Some(&list[0]) else {
            panic!("expected pair");
        };
        assert!(matches!(&pair[0], Value::Call { head, .. } if head == "RGBColor"));
        let coverage = crate::builtins::to_f64(&pair[1]).unwrap();
        assert!((0.0..=1.0).contains(&coverage) && coverage > 0.0);
    }

    // ── ImageEffect extras ──

    #[test]
    fn test_image_effect_solarize() {
        let img = gray_img(2, 2, |_, _| 200);
        let out = builtin_image_effect(&[img, Value::Str("Solarize".to_string())]).unwrap();
        assert_eq!(extract(&out).to_luma8().get_pixel(0, 0)[0], 55);
    }

    #[test]
    fn test_image_effect_posterize() {
        let img = gray_img(2, 2, |_, _| 200);
        let out = builtin_image_effect(&[img, Value::Str("Posterize".to_string())]).unwrap();
        // 200 rounds to the nearest of {0, 85, 170, 255}.
        assert_eq!(extract(&out).to_luma8().get_pixel(0, 0)[0], 170);
    }

    #[test]
    fn test_image_effect_emboss() {
        let img = gray_img(4, 4, |_, _| 128);
        let out = builtin_image_effect(&[img, Value::Str("Emboss".to_string())]).unwrap();
        assert_eq!(dims(&out), (4, 4));
    }

    // ── ImageTrim extras ──

    #[test]
    fn test_image_trim_uniform_border() {
        let img = gray_img(8, 8, |x, y| {
            if (2..6).contains(&x) && (2..6).contains(&y) {
                255
            } else {
                0
            }
        });
        let out = builtin_image_trim(&[img]).unwrap();
        assert_eq!(dims(&out), (4, 4));
    }

    #[test]
    fn test_image_trim_two_points_three_args() {
        let img = gray_img(8, 8, |_, _| 10);
        let pt = |a: i64, b: i64| {
            Value::List(vec![
                Value::Integer(rug::Integer::from(a)),
                Value::Integer(rug::Integer::from(b)),
            ])
        };
        let out = builtin_image_trim(&[img, pt(2, 2), pt(5, 6)]).unwrap();
        let o = extract(&out);
        assert_eq!((o.width(), o.height()), (4, 5));
    }

    // ── Registration ──

    #[test]
    fn test_register_installs_builtins() {
        let env = Env::new();
        register(&env);
        for name in [
            "ImageCrop",
            "ImageTake",
            "ImagePad",
            "ImageTrim",
            "ImageEffect",
            "ImageFilter",
            "ColorNegate",
            "Colorize",
            "ColorSeparate",
            "ColorCombine",
            "Dilation",
            "Erosion",
            "Thinning",
            "MeanFilter",
            "MedianFilter",
            "Blur",
            "Sharpen",
            "DominantColors",
            "ColorQuantize",
            "ComponentMeasurements",
            "DistanceTransform",
            "CornerFilter",
            "LaplacianFilter",
            "EdgeFilter",
            "FillingTransform",
            "MorphologicalBinarize",
            "Thicken",
        ] {
            assert!(env.get(name).is_some(), "{name} not registered");
        }
    }
}
