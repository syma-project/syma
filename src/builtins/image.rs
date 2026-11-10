//! Image Processing builtins.
//!
//! Provides raster image manipulation using the `image` crate:
//!   Image, ImageData, ImageDimensions, ImageType,
//!   ImageResize, ImageRotate, ImageAdjust,
//!   Binarize, ColorConvert, GaussianFilter,
//!   EdgeDetect, ImageConvolve
//!
//! All builtins are pure (no HoldAll needed). Image values use the
//! `image::DynamicImage` representation internally (default: Byte/RGB8).
//! The user-facing interface normalises pixel values to [0, 1].

use std::sync::Arc;

use crate::value::{EvalError, Value};
use image::GenericImageView;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Extract an Image value from argument list by index.
fn get_image_arg(args: &[Value], index: usize) -> Result<Arc<image::DynamicImage>, EvalError> {
    match &args[index] {
        Value::Image(img) => Ok(Arc::clone(img)),
        other => Err(EvalError::TypeError {
            expected: "Image".to_string(),
            got: other.type_name().to_string(),
        }),
    }
}

/// Extract a single f64 value from a Value argument.
fn value_to_f64(v: &Value) -> Result<f64, EvalError> {
    match v {
        Value::Integer(n) => Ok(n.to_f64()),
        Value::Real(r) => Ok(r.to_f64()),
        Value::Rational(r) => Ok(r.to_f64()),
        _ => Err(EvalError::Error(format!(
            "Expected a numeric value, got {}",
            v.type_name()
        ))),
    }
}

/// Convert a normalised [0, 1] pixel value to u8 (clamping).
fn norm_to_u8(v: f64) -> u8 {
    if v <= 0.0 {
        0
    } else if v >= 1.0 {
        255
    } else {
        (v * 255.0).round() as u8
    }
}

/// Convert a u8 pixel value to normalised [0, 1].
fn u8_to_norm(v: u8) -> f64 {
    v as f64 / 255.0
}

/// Clamp an f64 value to [0, 1].
fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// Convert a normalised [0, 1] pixel value to u16 (clamping).
fn norm_to_u16(v: f64) -> u16 {
    if v <= 0.0 {
        0
    } else if v >= 1.0 {
        65535
    } else {
        (v * 65535.0).round() as u16
    }
}

/// Rec.601 luminance of an 8-bit RGB triple, returned normalised to `[0, 1]`.
///
/// Matches `ColorConvert[image, "Grayscale"]`; the `image` crate's own
/// `to_luma8` uses Rec.709 weights instead, which differs on saturated colours.
fn luma601(r: u8, g: u8, b: u8) -> f64 {
    (0.299 * r as f64 + 0.587 * g as f64 + 0.114 * b as f64) / 255.0
}

/// Convert any image to an 8-bit grayscale image using Rec.601 luma weights.
fn to_luma601(img: &image::DynamicImage) -> image::GrayImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let mut out = image::GrayImage::new(w, h);
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        *o = image::Luma([norm_to_u8(luma601(p[0], p[1], p[2]))]);
    }
    out
}

/// Interpret a pixel-channel `Value` for `type_hint`, normalised to `[0, 1]`.
///
/// `Automatic`/`Real` values are already in `[0, 1]`, `Byte` values are in
/// `0..=255`, and `Bit16` values are in `0..=65535`.
fn channel_to_norm(v: &Value, type_hint: &str) -> Result<f64, EvalError> {
    let x = value_to_f64(v)?;
    let scale = match type_hint {
        "Byte" | "Bit8" | "UnsignedInteger8" => 255.0,
        "Bit16" | "UnsignedInteger16" => 65535.0,
        _ => 1.0,
    };
    Ok(clamp01(x / scale))
}

/// True when `type_hint` names a 16-bit integer image type.
fn is_bit16(type_hint: &str) -> bool {
    matches!(type_hint, "Bit16" | "UnsignedInteger16")
}

/// Channel count of a pixel `Value` (a scalar is single-channel).
fn value_channels(v: &Value) -> usize {
    match v {
        Value::List(ch) => ch.len(),
        _ => 1,
    }
}

/// Convert an image to grayscale using Rec.601 weights, at the requested depth.
fn to_gray601(img: image::DynamicImage, bit16: bool) -> image::DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if bit16 {
        let mut out = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::new(w, h);
        for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
            *o = image::Luma([norm_to_u16(luma601(p[0], p[1], p[2]))]);
        }
        image::DynamicImage::ImageLuma16(out)
    } else {
        let mut out = image::GrayImage::new(w, h);
        for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
            *o = image::Luma([norm_to_u8(luma601(p[0], p[1], p[2]))]);
        }
        image::DynamicImage::ImageLuma8(out)
    }
}

/// Apply a `ColorSpace` hint, converting only when it disagrees with the
/// channel count implied by the data.
fn apply_color_space(
    img: image::DynamicImage,
    hint: Option<&str>,
    bit16: bool,
) -> image::DynamicImage {
    let hint = hint.map(|h| match h {
        "Gray" | "GrayLevel" => "Grayscale",
        "RGBColor" => "RGB",
        other => other,
    });
    let channels = match img.color() {
        image::ColorType::L8 | image::ColorType::L16 => 1,
        image::ColorType::Rgb8 | image::ColorType::Rgb16 => 3,
        _ => 4,
    };
    match hint {
        Some("Grayscale") if channels != 1 => to_gray601(img, bit16),
        Some("RGB") if channels == 1 => {
            if bit16 {
                image::DynamicImage::ImageRgb16(img.to_rgb16())
            } else {
                image::DynamicImage::ImageRgb8(img.to_rgb8())
            }
        }
        _ => img,
    }
}

/// Build an image of `channels` channels from a `get(channel, x, y)` accessor
/// returning normalised `[0, 1]` values.
fn build_channels<F>(
    channels: usize,
    width: u32,
    height: u32,
    bit16: bool,
    get: F,
) -> Result<image::DynamicImage, EvalError>
where
    F: Fn(usize, u32, u32) -> Result<f64, EvalError>,
{
    match (channels, bit16) {
        (1, false) => {
            let mut buf = image::GrayImage::new(width, height);
            for y in 0..height {
                for x in 0..width {
                    buf.put_pixel(x, y, image::Luma([norm_to_u8(get(0, x, y)?)]));
                }
            }
            Ok(image::DynamicImage::ImageLuma8(buf))
        }
        (1, true) => {
            let mut buf = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::new(width, height);
            for y in 0..height {
                for x in 0..width {
                    buf.put_pixel(x, y, image::Luma([norm_to_u16(get(0, x, y)?)]));
                }
            }
            Ok(image::DynamicImage::ImageLuma16(buf))
        }
        (3, false) => {
            let mut buf = image::RgbImage::new(width, height);
            for y in 0..height {
                for x in 0..width {
                    buf.put_pixel(
                        x,
                        y,
                        image::Rgb([
                            norm_to_u8(get(0, x, y)?),
                            norm_to_u8(get(1, x, y)?),
                            norm_to_u8(get(2, x, y)?),
                        ]),
                    );
                }
            }
            Ok(image::DynamicImage::ImageRgb8(buf))
        }
        (3, true) => {
            let mut buf = image::ImageBuffer::<image::Rgb<u16>, Vec<u16>>::new(width, height);
            for y in 0..height {
                for x in 0..width {
                    buf.put_pixel(
                        x,
                        y,
                        image::Rgb([
                            norm_to_u16(get(0, x, y)?),
                            norm_to_u16(get(1, x, y)?),
                            norm_to_u16(get(2, x, y)?),
                        ]),
                    );
                }
            }
            Ok(image::DynamicImage::ImageRgb16(buf))
        }
        (4, false) => {
            let mut buf = image::RgbaImage::new(width, height);
            for y in 0..height {
                for x in 0..width {
                    buf.put_pixel(
                        x,
                        y,
                        image::Rgba([
                            norm_to_u8(get(0, x, y)?),
                            norm_to_u8(get(1, x, y)?),
                            norm_to_u8(get(2, x, y)?),
                            norm_to_u8(get(3, x, y)?),
                        ]),
                    );
                }
            }
            Ok(image::DynamicImage::ImageRgba8(buf))
        }
        (4, true) => {
            let mut buf = image::ImageBuffer::<image::Rgba<u16>, Vec<u16>>::new(width, height);
            for y in 0..height {
                for x in 0..width {
                    buf.put_pixel(
                        x,
                        y,
                        image::Rgba([
                            norm_to_u16(get(0, x, y)?),
                            norm_to_u16(get(1, x, y)?),
                            norm_to_u16(get(2, x, y)?),
                            norm_to_u16(get(3, x, y)?),
                        ]),
                    );
                }
            }
            Ok(image::DynamicImage::ImageRgba16(buf))
        }
        _ => unreachable!("channel count validated by caller"),
    }
}

/// Build an `image::DynamicImage` from a 2D or 3D nested Value list.
///
/// Expected shapes (with default `Interleaving -> True`):
///   - 2D: `{{r1c1, r1c2, ...}, {r2c1, ...}, ...}`  → Grayscale
///   - 3D (inner len=3): `{{{r,g,b}, ...}, ...}`      → RGB
///   - 3D (inner len=4): `{{{r,g,b,a}, ...}, ...}`    → RGBA
///
/// With `Interleaving -> False` the data is a list of channel planes, each
/// plane being a full 2D (height × width) array of scalars.
///
/// `color_space_hint` (`"RGB"` / `"Grayscale"`) forces a colour space only when
/// it disagrees with the detected channel count. `type_hint` selects the value
/// range (`Automatic`/`Real` = `[0, 1]`, `Byte` = `0..=255`, `Bit16` =
/// `0..=65535`).
fn img_from_list(
    data: &[Value],
    color_space_hint: Option<&str>,
    type_hint: &str,
    interleaving: bool,
) -> Result<image::DynamicImage, EvalError> {
    if data.is_empty() {
        return Err(EvalError::Error("Image: data cannot be empty".to_string()));
    }

    let bit16 = is_bit16(type_hint);

    if !interleaving {
        return img_from_planes(data, color_space_hint, type_hint, bit16);
    }

    let height = data.len();
    let row0 = match &data[0] {
        Value::List(row) => row,
        other => {
            return Err(EvalError::Error(format!(
                "Image: expected each row to be a List, got {}",
                other.type_name()
            )));
        }
    };

    if row0.is_empty() {
        return Err(EvalError::Error("Image: rows cannot be empty".to_string()));
    }

    let width = row0.len();
    let channels = value_channels(&row0[0]);

    if channels != 1 && channels != 3 && channels != 4 {
        return Err(EvalError::Error(format!(
            "Image: expected 1, 3, or 4 channels per pixel, got {}",
            channels
        )));
    }

    // Validate all rows have the same width and channel count.
    for (r, row) in data.iter().enumerate() {
        match row {
            Value::List(items) => {
                if items.len() != width {
                    return Err(EvalError::Error(format!(
                        "Image: row {} has {} columns, expected {}",
                        r,
                        items.len(),
                        width
                    )));
                }
                for (c, item) in items.iter().enumerate() {
                    let ch = value_channels(item);
                    if ch != channels {
                        return Err(EvalError::Error(format!(
                            "Image: pixel [{}, {}] has {} channels, expected {}",
                            r, c, ch, channels
                        )));
                    }
                }
            }
            other => {
                return Err(EvalError::Error(format!(
                    "Image: expected row {} to be a List, got {}",
                    r,
                    other.type_name()
                )));
            }
        }
    }

    let get = |c: usize, x: u32, y: u32| -> Result<f64, EvalError> {
        if let Value::List(items) = &data[y as usize] {
            if channels == 1 {
                return channel_to_norm(&items[x as usize], type_hint);
            }
            if let Value::List(chs) = &items[x as usize] {
                return channel_to_norm(&chs[c], type_hint);
            }
        }
        unreachable!("shape validated above")
    };

    let img = build_channels(channels, width as u32, height as u32, bit16, get)?;
    Ok(apply_color_space(img, color_space_hint, bit16))
}

/// Build an image from `Interleaving -> False` channel-plane data
/// (`{{plane_r}, {plane_g}, {plane_b}, ...}`, each plane height × width).
fn img_from_planes(
    data: &[Value],
    color_space_hint: Option<&str>,
    type_hint: &str,
    bit16: bool,
) -> Result<image::DynamicImage, EvalError> {
    let channels = data.len();
    if channels != 1 && channels != 3 && channels != 4 {
        return Err(EvalError::Error(format!(
            "Image: Interleaving -> False expects 1, 3, or 4 planes, got {}",
            channels
        )));
    }

    let first_plane = match &data[0] {
        Value::List(plane) => plane,
        other => {
            return Err(EvalError::Error(format!(
                "Image: expected each channel plane to be a List, got {}",
                other.type_name()
            )));
        }
    };
    if first_plane.is_empty() {
        return Err(EvalError::Error(
            "Image: planes cannot be empty".to_string(),
        ));
    }
    let height = first_plane.len();
    let width = match &first_plane[0] {
        Value::List(row) => row.len(),
        other => {
            return Err(EvalError::Error(format!(
                "Image: expected a plane row to be a List, got {}",
                other.type_name()
            )));
        }
    };
    if width == 0 {
        return Err(EvalError::Error(
            "Image: planes cannot be empty".to_string(),
        ));
    }

    for (ci, plane_v) in data.iter().enumerate() {
        let plane = match plane_v {
            Value::List(plane) => plane,
            other => {
                return Err(EvalError::Error(format!(
                    "Image: channel plane {} must be a List, got {}",
                    ci,
                    other.type_name()
                )));
            }
        };
        if plane.len() != height {
            return Err(EvalError::Error(format!(
                "Image: channel plane {} has {} rows, expected {}",
                ci,
                plane.len(),
                height
            )));
        }
        for (ri, row_v) in plane.iter().enumerate() {
            let row = match row_v {
                Value::List(row) => row,
                other => {
                    return Err(EvalError::Error(format!(
                        "Image: plane {} row {} must be a List, got {}",
                        ci,
                        ri,
                        other.type_name()
                    )));
                }
            };
            if row.len() != width {
                return Err(EvalError::Error(format!(
                    "Image: plane {} row {} has {} columns, expected {}",
                    ci,
                    ri,
                    row.len(),
                    width
                )));
            }
            for item in row {
                if matches!(item, Value::List(_)) {
                    return Err(EvalError::Error(
                        "Image: Interleaving -> False planes must contain scalars".to_string(),
                    ));
                }
            }
        }
    }

    let get = |c: usize, x: u32, y: u32| -> Result<f64, EvalError> {
        if let Value::List(plane) = &data[c]
            && let Value::List(row) = &plane[y as usize]
        {
            return channel_to_norm(&row[x as usize], type_hint);
        }
        unreachable!("shape validated above")
    };

    let img = build_channels(channels, width as u32, height as u32, bit16, get)?;
    Ok(apply_color_space(img, color_space_hint, bit16))
}

/// Build a row-major nested `Value` list from a per-pixel channel accessor.
fn build_list(
    width: u32,
    height: u32,
    channels: usize,
    get: impl Fn(u32, u32, usize) -> f64,
) -> Value {
    let mut rows = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut row = Vec::with_capacity(width as usize);
        for x in 0..width {
            if channels == 1 {
                row.push(Value::Real(rug::Float::with_val(53, get(x, y, 0))));
            } else {
                let mut chs = Vec::with_capacity(channels);
                for c in 0..channels {
                    chs.push(Value::Real(rug::Float::with_val(53, get(x, y, c))));
                }
                row.push(Value::List(chs));
            }
        }
        rows.push(Value::List(row));
    }
    Value::List(rows)
}

/// Convert an image to a 2D/3D Value list (row-major) with normalised [0, 1]
/// values. Grayscale → 2D scalars; RGB/RGBA/LumaA → 3D channel triples/quads.
///
/// Values are normalised by the depth of the underlying pixel type so that a
/// `Bit16` image maps to the same `[0, 1]` range as a `Byte` image.
fn list_from_img(img: &image::DynamicImage) -> Value {
    let (w, h) = (img.width(), img.height());
    match img {
        image::DynamicImage::ImageLuma8(b) => {
            build_list(w, h, 1, |x, y, _| u8_to_norm(b.get_pixel(x, y)[0]))
        }
        image::DynamicImage::ImageLumaA8(b) => {
            build_list(w, h, 2, |x, y, c| u8_to_norm(b.get_pixel(x, y)[c]))
        }
        image::DynamicImage::ImageRgb8(b) => {
            build_list(w, h, 3, |x, y, c| u8_to_norm(b.get_pixel(x, y)[c]))
        }
        image::DynamicImage::ImageRgba8(b) => {
            build_list(w, h, 4, |x, y, c| u8_to_norm(b.get_pixel(x, y)[c]))
        }
        image::DynamicImage::ImageLuma16(b) => {
            build_list(w, h, 1, |x, y, _| b.get_pixel(x, y)[0] as f64 / 65535.0)
        }
        image::DynamicImage::ImageLumaA16(b) => {
            build_list(w, h, 2, |x, y, c| b.get_pixel(x, y)[c] as f64 / 65535.0)
        }
        image::DynamicImage::ImageRgb16(b) => {
            build_list(w, h, 3, |x, y, c| b.get_pixel(x, y)[c] as f64 / 65535.0)
        }
        image::DynamicImage::ImageRgba16(b) => {
            build_list(w, h, 4, |x, y, c| b.get_pixel(x, y)[c] as f64 / 65535.0)
        }
        image::DynamicImage::ImageRgb32F(b) => {
            build_list(w, h, 3, |x, y, c| b.get_pixel(x, y)[c] as f64)
        }
        image::DynamicImage::ImageRgba32F(b) => {
            build_list(w, h, 4, |x, y, c| b.get_pixel(x, y)[c] as f64)
        }
        _ => {
            // Fallback: convert to RGB8 first.
            let rgb = img.to_rgb8();
            list_from_img(&image::DynamicImage::ImageRgb8(rgb))
        }
    }
}

/// Determine the WL ImageType string for a DynamicImage.
fn image_type_string(img: &image::DynamicImage) -> &'static str {
    match img.color() {
        image::ColorType::L8
        | image::ColorType::La8
        | image::ColorType::Rgb8
        | image::ColorType::Rgba8 => "Byte",
        image::ColorType::L16
        | image::ColorType::La16
        | image::ColorType::Rgb16
        | image::ColorType::Rgba16 => "Bit16",
        image::ColorType::Rgb32F | image::ColorType::Rgba32F => "Real32",
        _ => "Byte",
    }
}

/// Apply gamma correction: `v_out = 255 * (v_in / 255)^(1/gamma)`.
fn gamma_correct(v: u8, gamma: f64) -> u8 {
    if gamma <= 0.0 {
        return if v > 127 { 255 } else { 0 };
    }
    let norm = v as f64 / 255.0;
    norm_to_u8(norm.powf(1.0 / gamma))
}

// ── Builtins ─────────────────────────────────────────────────────────────────

/// Image[data]
///
/// Create an image from a 2D or 3D list of pixel values.
/// Values are expected in [0, 1] range.
///
/// Accepts Mathematica-compatible options:
/// - `ColorSpace -> "RGB"` / `"Grayscale"` / `Automatic` (default)
/// - `Interleaving -> True` (default) / `False`
pub fn builtin_image(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Image requires at least 1 argument".to_string(),
        ));
    }

    let data = match &args[0] {
        Value::List(items) => items,
        other => {
            crate::messages::emit("Image::imgarray", &[format!("{}", other)]);
            return Ok(Value::Call {
                head: "Image".to_string(),
                args: args.to_vec(),
            });
        }
    };

    // Parse optional arguments: type strings and Rule options.
    let mut type_hint = "Automatic".to_string();
    let mut color_space: Option<String> = None;
    let mut interleaving = true;

    for arg in &args[1..] {
        match arg {
            Value::Str(s) => {
                // A bare string may name either a data type ("Byte", "Real32",
                // …) or a colour space ("RGB", "Grayscale", …).
                if matches!(
                    s.as_str(),
                    "RGB" | "Grayscale" | "Gray" | "RGBA" | "RGBColor" | "GrayLevel"
                ) {
                    color_space = Some(s.clone());
                } else {
                    type_hint = s.clone();
                }
            }
            Value::Rule { lhs, rhs, .. } => {
                let key = match lhs.as_ref() {
                    Value::Symbol(s) => s.as_str(),
                    _ => continue,
                };
                match key {
                    "ColorSpace" => match rhs.as_ref() {
                        Value::Str(s) => color_space = Some(s.clone()),
                        Value::Symbol(s) if s == "Automatic" => {
                            // Automatic = auto-detect (default)
                        }
                        _ => {}
                    },
                    "Interleaving" => {
                        if let Value::Bool(b) = rhs.as_ref() {
                            interleaving = *b;
                        }
                    }
                    _ => {}
                }
            }
            _ => {
                return Err(EvalError::Error(
                    "Image: unexpected argument type — expected string type or Rule option"
                        .to_string(),
                ));
            }
        }
    }

    let img = match img_from_list(data, color_space.as_deref(), &type_hint, interleaving) {
        Ok(img) => img,
        Err(_) => {
            crate::messages::emit("Image::imgarray", &[format!("{}", args[0])]);
            return Ok(Value::Call {
                head: "Image".to_string(),
                args: args.to_vec(),
            });
        }
    };
    Ok(Value::Image(Arc::new(img)))
}

/// ImageData[image]
///
/// Extract pixel data as a list of lists, values in [0, 1].
pub fn builtin_image_data(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ImageData requires exactly 1 argument".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;
    Ok(list_from_img(&img))
}

/// ImageDimensions[image]
///
/// Return {width, height} as integers.
pub fn builtin_image_dimensions(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ImageDimensions requires exactly 1 argument".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;
    Ok(Value::List(vec![
        Value::Integer(rug::Integer::from(img.width())),
        Value::Integer(rug::Integer::from(img.height())),
    ]))
}

/// ImageType[image]
///
/// Return the image type as a string: "Byte", "Bit16", "Real32".
pub fn builtin_image_type(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ImageType requires exactly 1 argument".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;
    Ok(Value::Str(image_type_string(&img).to_string()))
}

/// ImageResize[image, {w, h}]  or  ImageResize[image, n]
///
/// Resize an image to the given dimensions using Lanczos3 filter.
/// If n is given, scales width to n pixels preserving aspect ratio.
/// The colour space (channel count) of the source is preserved.
pub fn builtin_image_resize(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ImageResize requires 2 arguments: ImageResize[image, {w, h}] or ImageResize[image, n]"
                .to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;

    let (new_w, new_h) = match &args[1] {
        Value::List(dims) if dims.len() == 2 => {
            let w = value_to_f64(&dims[0])?.round();
            let h = value_to_f64(&dims[1])?.round();
            if w < 1.0 || h < 1.0 {
                return Err(EvalError::Error(
                    "ImageResize: dimensions must be positive".to_string(),
                ));
            }
            (w as u32, h as u32)
        }
        other => {
            let w = value_to_f64(other)?.round();
            if w < 1.0 {
                return Err(EvalError::Error(
                    "ImageResize: dimension must be positive".to_string(),
                ));
            }
            let aspect = img.height() as f64 / img.width() as f64;
            let h = (w * aspect).round().max(1.0);
            (w as u32, h as u32)
        }
    };

    // `DynamicImage::resize_exact` preserves the source colour type, so a
    // grayscale image stays grayscale (`ImageData` keeps its 2D shape).
    let resized = img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3);
    Ok(Value::Image(Arc::new(resized)))
}

/// ImageRotate[image, angle]
///
/// Rotate by 90, 180, or 270 degrees (exact multiples).
/// Arbitrary angles use rotation with white background.
pub fn builtin_image_rotate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ImageRotate requires 2 arguments: ImageRotate[image, angle]".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;
    let angle = value_to_f64(&args[1])?;

    let rotated = if (angle - 90.0).abs() < 1.0 {
        img.rotate90()
    } else if (angle - 180.0).abs() < 1.0 {
        img.rotate180()
    } else if (angle - 270.0).abs() < 1.0 || (angle + 90.0).abs() < 1.0 {
        img.rotate270()
    } else if angle.abs() < 1.0 || (angle - 360.0).abs() < 1.0 {
        // 0° or 360° — no rotation needed
        (*img).clone()
    } else {
        // Arbitrary angle: use the image crate's rotate with white background
        let radians = angle.to_radians();
        let (w, h) = (img.width() as f64, img.height() as f64);
        let cos_a = radians.cos().abs();
        let sin_a = radians.sin().abs();
        let new_w = (w * cos_a + h * sin_a).ceil() as u32;
        let new_h = (w * sin_a + h * cos_a).ceil() as u32;

        // Manual rotation with white background
        let mut buf = image::RgbaImage::new(new_w.max(1), new_h.max(1));
        let cx = new_w as f64 / 2.0;
        let cy = new_h as f64 / 2.0;
        let ocx = w / 2.0;
        let ocy = h / 2.0;
        let (sin_r, cos_r) = radians.sin_cos();

        for ny in 0..new_h {
            for nx in 0..new_w {
                let ox = (nx as f64 - cx) * cos_r + (ny as f64 - cy) * sin_r + ocx;
                let oy = -(nx as f64 - cx) * sin_r + (ny as f64 - cy) * cos_r + ocy;

                if ox >= 0.0 && ox < w - 1.0 && oy >= 0.0 && oy < h - 1.0 {
                    let ox0 = ox.floor() as u32;
                    let oy0 = oy.floor() as u32;
                    let ox1 = (ox0 + 1).min(img.width() - 1);
                    let oy1 = (oy0 + 1).min(img.height() - 1);
                    let fx = ox - ox0 as f64;
                    let fy = oy - oy0 as f64;

                    let p00 = img.get_pixel(ox0, oy0);
                    let p10 = img.get_pixel(ox1, oy0);
                    let p01 = img.get_pixel(ox0, oy1);
                    let p11 = img.get_pixel(ox1, oy1);

                    // `DynamicImage::get_pixel` always returns RGBA, so copy all
                    // four channels (otherwise RGB/gray images lose G/B/alpha).
                    for c in 0..4 {
                        let v00 = p00[c] as f64;
                        let v10 = p10[c] as f64;
                        let v01 = p01[c] as f64;
                        let v11 = p11[c] as f64;
                        let v = v00 * (1.0 - fx) * (1.0 - fy)
                            + v10 * fx * (1.0 - fy)
                            + v01 * (1.0 - fx) * fy
                            + v11 * fx * fy;
                        buf.put_pixel(nx, ny, {
                            let mut p = *buf.get_pixel(nx, ny);
                            p[c] = v.round() as u8;
                            p
                        });
                    }
                } else {
                    buf.put_pixel(nx, ny, image::Rgba([255, 255, 255, 255]));
                }
            }
        }
        // Preserve the source colour type so a grayscale/RGB image does not
        // silently become RGBA after rotation.
        let dynbuf = image::DynamicImage::ImageRgba8(buf);
        match img.color() {
            image::ColorType::L8 => image::DynamicImage::ImageLuma8(dynbuf.to_luma8()),
            image::ColorType::Rgb8 => image::DynamicImage::ImageRgb8(dynbuf.to_rgb8()),
            _ => dynbuf,
        }
    };

    Ok(Value::Image(Arc::new(rotated)))
}

/// ImageAdjust[image]  or  ImageAdjust[image, {c, b, g}]
///
/// Adjust contrast (c), brightness (b), and gamma (g) of an image.
/// With no arguments, auto-stretches contrast to full range.
pub fn builtin_image_adjust(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "ImageAdjust takes 1 or 2 arguments".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;

    let (contrast, brightness, gamma) = if args.len() >= 2 {
        match &args[1] {
            Value::List(params) => {
                let c = if !params.is_empty() {
                    value_to_f64(&params[0])?
                } else {
                    1.0
                };
                let b = if params.len() >= 2 {
                    value_to_f64(&params[1])?
                } else {
                    0.0
                };
                let g = if params.len() >= 3 {
                    value_to_f64(&params[2])?
                } else {
                    1.0
                };
                (c, b, g)
            }
            _ => {
                return Err(EvalError::Error(
                    "ImageAdjust: second argument must be a list {c, b, g} or omitted".to_string(),
                ));
            }
        }
    } else {
        // Auto contrast stretch
        let rgba = img.to_rgba8();
        let mut min_pixel = 255u8;
        let mut max_pixel = 0u8;
        for p in rgba.pixels() {
            for c in 0..3 {
                min_pixel = min_pixel.min(p[c]);
                max_pixel = max_pixel.max(p[c]);
            }
        }
        let range = (max_pixel as f64 - min_pixel as f64).max(1.0);
        let scale = 255.0 / range;
        // Choose contrast/brightness so `min_pixel` maps to 0 and `max_pixel`
        // maps to 1 under `(norm - 0.5) * contrast + 0.5 + brightness`.
        let contrast = scale;
        let brightness = -(min_pixel as f64 / 255.0) * scale - 0.5 * (1.0 - scale);
        (contrast, brightness, 1.0)
    };

    // Apply brightness (additive shift), contrast (multiplicative), and gamma
    let rgb = img.to_rgba8();
    let (w, h) = (rgb.width(), rgb.height());
    let mut buf = image::RgbaImage::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let p = rgb.get_pixel(x, y);
            let mut out = [0u8; 4];
            for c in 0..3 {
                // Contrast: center around 0.5 and scale, Brightness: add offset
                let norm = p[c] as f64 / 255.0;
                let adjusted = (norm - 0.5) * contrast + 0.5 + brightness;
                let clamped = clamp01(adjusted);
                // Gamma correction
                let gamma_c = gamma_correct(norm_to_u8(clamped), gamma);
                out[c] = gamma_c;
            }
            out[3] = p[3]; // preserve alpha
            buf.put_pixel(x, y, image::Rgba(out));
        }
    }

    Ok(Value::Image(Arc::new(image::DynamicImage::ImageRgba8(buf))))
}

/// Binarize[image]  or  Binarize[image, threshold]
///
/// Convert to binary (black & white) using the given threshold (default 0.5).
/// Values ≥ threshold become 1 (white), < threshold become 0 (black).
pub fn builtin_binarize(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Binarize takes 1 or 2 arguments: Binarize[image] or Binarize[image, t]".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;

    let threshold = if args.len() >= 2 {
        clamp01(value_to_f64(&args[1])?)
    } else {
        0.5
    };

    // Convert to grayscale (Rec.601 luma, matching WL), then threshold.
    let gray = to_luma601(&img);
    let (w, h) = (gray.width(), gray.height());
    let mut buf = image::GrayImage::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let p = gray.get_pixel(x, y);
            let v = if p[0] as f64 / 255.0 >= threshold {
                255
            } else {
                0
            };
            buf.put_pixel(x, y, image::Luma([v]));
        }
    }

    Ok(Value::Image(Arc::new(image::DynamicImage::ImageLuma8(buf))))
}

/// ColorConvert[image, "Grayscale"]
///
/// Convert between color spaces. Currently supports: "Grayscale".
pub fn builtin_color_convert(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ColorConvert requires 2 arguments: ColorConvert[image, \"target\"]".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;
    let target = match &args[1] {
        Value::Str(s) => s,
        other => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };

    let converted = match target.as_str() {
        "Grayscale" => to_gray601((*img).clone(), false),
        "RGB" => image::DynamicImage::ImageRgb8(img.to_rgb8()),
        _ => {
            return Err(EvalError::Error(format!(
                "ColorConvert: unknown target '{}'. Supported: Grayscale, RGB",
                target
            )));
        }
    };

    Ok(Value::Image(Arc::new(converted)))
}

/// GaussianFilter[image, r]
///
/// Apply Gaussian blur with sigma = r.
pub fn builtin_gaussian_filter(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "GaussianFilter requires 2 arguments: GaussianFilter[image, r]".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;
    let sigma = value_to_f64(&args[1])?;

    if sigma < 0.0 {
        return Err(EvalError::Error(
            "GaussianFilter: radius must be non-negative".to_string(),
        ));
    }

    if sigma == 0.0 {
        return Ok(Value::Image(Arc::clone(&img)));
    }

    // The image crate's blur function takes sigma as a parameter.
    let blurred = img.blur(sigma.max(0.5) as f32);
    Ok(Value::Image(Arc::new(blurred)))
}

/// EdgeDetect[image]
///
/// Apply Sobel edge detection. Converts to grayscale (Rec.601 luma), then
/// computes the gradient magnitude from signed horizontal and vertical Sobel
/// operators in `f64`. The magnitude is normalised to `[0, 255]`.
///
/// Note: `image::imageops::filter3x3` cannot be used directly here because it
/// clamps each response into the *unsigned* pixel range, discarding the
/// negative lobe of the Sobel operator and saturating strong edges.
pub fn builtin_edge_detect(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "EdgeDetect requires exactly 1 argument".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;

    // Convert to grayscale (Rec.601, matching WL).
    let gray = to_luma601(&img);
    let (w, h) = (gray.width(), gray.height());
    if w == 0 || h == 0 {
        return Ok(Value::Image(Arc::new(image::DynamicImage::ImageLuma8(
            gray,
        ))));
    }

    // Sample with edge clamping (replicate border pixels).
    let at = |x: i64, y: i64| -> f64 {
        let xx = x.clamp(0, w as i64 - 1) as u32;
        let yy = y.clamp(0, h as i64 - 1) as u32;
        gray.get_pixel(xx, yy)[0] as f64
    };

    let mut magnitudes = vec![0.0f64; (w * h) as usize];
    let mut max_mag = 0.0f64;
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let (p00, p10, p20) = (at(x - 1, y - 1), at(x, y - 1), at(x + 1, y - 1));
            let (p01, p21) = (at(x - 1, y), at(x + 1, y));
            let (p02, p12, p22) = (at(x - 1, y + 1), at(x, y + 1), at(x + 1, y + 1));

            // Signed Sobel responses (positive on one side of an edge,
            // negative on the other).
            let gx = (p20 + 2.0 * p21 + p22) - (p00 + 2.0 * p01 + p02);
            let gy = (p02 + 2.0 * p12 + p22) - (p00 + 2.0 * p10 + p20);
            let mag = (gx * gx + gy * gy).sqrt();

            magnitudes[(y as u32 * w + x as u32) as usize] = mag;
            if mag > max_mag {
                max_mag = mag;
            }
        }
    }

    let scale = if max_mag > 0.0 { 255.0 / max_mag } else { 0.0 };
    let mut buf = image::GrayImage::new(w, h);
    for (i, p) in buf.pixels_mut().enumerate() {
        *p = image::Luma([(magnitudes[i] * scale).round().clamp(0.0, 255.0) as u8]);
    }

    Ok(Value::Image(Arc::new(image::DynamicImage::ImageLuma8(buf))))
}

/// ImageConvolve[image, kernel]
///
/// Convolve an image with a 2D kernel (list of lists).
/// The kernel is applied as-is and is *not* normalised (matching WL's
/// `ImageConvolve`; pass a normalised kernel such as `BoxMatrix[1]/9` for an
/// average). Odd-sized kernels of any size are supported; border pixels use
/// clamped (replicated) edge samples. The source colour space is preserved.
pub fn builtin_image_convolve(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ImageConvolve requires 2 arguments: ImageConvolve[image, kernel]".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;

    // Parse kernel from list-of-lists
    let kernel_data = match &args[1] {
        Value::List(kernel_rows) => kernel_rows,
        other => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };

    if kernel_data.is_empty() {
        return Err(EvalError::Error(
            "ImageConvolve: kernel cannot be empty".to_string(),
        ));
    }

    let kh = kernel_data.len();
    let kw = match &kernel_data[0] {
        Value::List(row) => row.len(),
        _ => {
            return Err(EvalError::Error(
                "ImageConvolve: kernel must be a 2D list".to_string(),
            ));
        }
    };

    if kw == 0 || kw % 2 != 1 || kh % 2 != 1 {
        return Err(EvalError::Error(
            "ImageConvolve: kernel dimensions must be odd (e.g., 3×3, 5×5)".to_string(),
        ));
    }

    // Parse kernel values as f32
    let mut kernel = Vec::with_capacity(kh);
    for (r, row_v) in kernel_data.iter().enumerate() {
        match row_v {
            Value::List(row) => {
                if row.len() != kw {
                    return Err(EvalError::Error(format!(
                        "ImageConvolve: kernel row {} has {} columns, expected {}",
                        r,
                        row.len(),
                        kw
                    )));
                }
                let mut krow = Vec::with_capacity(kw);
                for v in row {
                    krow.push(value_to_f64(v)? as f32);
                }
                kernel.push(krow);
            }
            other => {
                return Err(EvalError::TypeError {
                    expected: "List".to_string(),
                    got: other.type_name().to_string(),
                });
            }
        }
    }

    // Manual convolution over the RGBA view, then restore the source colour
    // type. Edge samples are clamped (replicated) at the borders.
    let src = img.to_rgba8();
    let (w, h) = (src.width(), src.height());
    let half_kw = (kw / 2) as i64;
    let half_kh = (kh / 2) as i64;
    let mut buf = image::RgbaImage::new(w, h);

    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f64; 4];
            for (ky, krow) in kernel.iter().enumerate() {
                for (kx, k_val) in krow.iter().enumerate() {
                    let px = (x as i64 + kx as i64 - half_kw).clamp(0, w as i64 - 1) as u32;
                    let py = (y as i64 + ky as i64 - half_kh).clamp(0, h as i64 - 1) as u32;
                    let p = src.get_pixel(px, py);
                    let k = *k_val as f64;
                    for c in 0..4 {
                        acc[c] += p[c] as f64 * k;
                    }
                }
            }
            let mut out = [0u8; 4];
            for c in 0..4 {
                out[c] = acc[c].round().clamp(0.0, 255.0) as u8;
            }
            buf.put_pixel(x, y, image::Rgba(out));
        }
    }

    let dynbuf = image::DynamicImage::ImageRgba8(buf);
    let result = match img.color() {
        image::ColorType::L8 => image::DynamicImage::ImageLuma8(dynbuf.to_luma8()),
        image::ColorType::Rgb8 => image::DynamicImage::ImageRgb8(dynbuf.to_rgb8()),
        _ => dynbuf,
    };
    Ok(Value::Image(Arc::new(result)))
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    /// Create a simple 2×2 grayscale test image as nested Value lists.
    fn gray2x2() -> Value {
        Value::List(vec![
            Value::List(vec![
                Value::Real(rug::Float::with_val(53, 0.0)),
                Value::Real(rug::Float::with_val(53, 0.5)),
            ]),
            Value::List(vec![
                Value::Real(rug::Float::with_val(53, 1.0)),
                Value::Real(rug::Float::with_val(53, 0.0)),
            ]),
        ])
    }

    /// Create a simple 2×2 RGB test image.
    fn rgb2x2() -> Value {
        Value::List(vec![
            Value::List(vec![
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 1.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                ]),
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 1.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                ]),
            ]),
            Value::List(vec![
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 1.0)),
                ]),
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.5)),
                    Value::Real(rug::Float::with_val(53, 0.5)),
                    Value::Real(rug::Float::with_val(53, 0.5)),
                ]),
            ]),
        ])
    }

    fn extract_image(v: &Value) -> Arc<image::DynamicImage> {
        match v {
            Value::Image(img) => Arc::clone(img),
            _ => panic!("expected Value::Image"),
        }
    }

    // ── Image ──

    #[test]
    fn test_image_create_grayscale() {
        let result = builtin_image(&[gray2x2()]).unwrap();
        let img = extract_image(&result);
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        assert_eq!(img.color(), image::ColorType::L8);
    }

    #[test]
    fn test_image_create_rgb() {
        let result = builtin_image(&[rgb2x2()]).unwrap();
        let img = extract_image(&result);
        assert_eq!(img.width(), 2);
        assert_eq!(img.height(), 2);
        assert_eq!(img.color(), image::ColorType::Rgb8);
    }

    #[test]
    fn test_image_empty_data() {
        let result = builtin_image(&[Value::List(vec![])]);
        assert!(
            matches!(result, Ok(Value::Call { ref head, .. }) if head == "Image"),
            "empty list should return unevaluated, got {:?}",
            result
        );
    }

    #[test]
    fn test_image_ragged_array() {
        let ragged = Value::List(vec![
            Value::List(vec![Value::Real(rug::Float::with_val(53, 0.5))]),
            Value::List(vec![
                Value::Real(rug::Float::with_val(53, 0.5)),
                Value::Real(rug::Float::with_val(53, 0.5)),
            ]),
        ]);
        let result = builtin_image(&[ragged]);
        assert!(
            matches!(result, Ok(Value::Call { ref head, .. }) if head == "Image"),
            "ragged array should return unevaluated, got {:?}",
            result
        );
    }

    #[test]
    fn test_image_invalid_arg_returns_unevaluated() {
        // Image[42] should not error; should return unevaluated Image[42]
        let result = builtin_image(&[Value::Integer(rug::Integer::from(42))]);
        assert!(
            matches!(result, Ok(Value::Call { ref head, ref args }) if head == "Image" && args.len() == 1),
            "expected Image[42] unevaluated, got {:?}",
            result
        );
    }

    #[test]
    fn test_image_wrong_arity() {
        assert!(builtin_image(&[]).is_err());
        assert!(builtin_image(&[gray2x2(), Value::Str("Byte".to_string()), Value::Null]).is_err());
    }

    // ── ImageData ──

    #[test]
    fn test_image_data_roundtrip() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let data = builtin_image_data(&[img]).unwrap();
        // Should be a 2×2 list of scalars
        if let Value::List(rows) = &data {
            assert_eq!(rows.len(), 2);
            if let Value::List(row0) = &rows[0] {
                assert_eq!(row0.len(), 2);
            } else {
                panic!("expected List row");
            }
        } else {
            panic!("expected List");
        }
    }

    #[test]
    fn test_image_data_not_image() {
        let result = builtin_image_data(&[Value::Integer(rug::Integer::from(42))]);
        assert!(result.is_err());
    }

    // ── ImageDimensions ──

    #[test]
    fn test_image_dimensions() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let dims = builtin_image_dimensions(&[img]).unwrap();
        assert_eq!(
            dims,
            Value::List(vec![
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(2)),
            ])
        );
    }

    // ── ImageType ──

    #[test]
    fn test_image_type_byte() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let t = builtin_image_type(&[img]).unwrap();
        assert_eq!(t, Value::Str("Byte".to_string()));
    }

    // ── ImageResize ──

    #[test]
    fn test_image_resize_exact() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let resized = builtin_image_resize(&[
            img,
            Value::List(vec![
                Value::Integer(rug::Integer::from(4)),
                Value::Integer(rug::Integer::from(4)),
            ]),
        ])
        .unwrap();
        let r = extract_image(&resized);
        assert_eq!(r.width(), 4);
        assert_eq!(r.height(), 4);
    }

    #[test]
    fn test_image_resize_scalar() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let resized = builtin_image_resize(&[img, Value::Integer(rug::Integer::from(4))]).unwrap();
        let r = extract_image(&resized);
        assert_eq!(r.width(), 4);
        assert_eq!(r.height(), 4); // square image so aspect ratio is 1:1
    }

    // ── ImageRotate ──

    #[test]
    fn test_image_rotate_90() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let rotated =
            builtin_image_rotate(&[img, Value::Real(rug::Float::with_val(53, 90.0))]).unwrap();
        let r = extract_image(&rotated);
        // 2×2 rotated 90° → 2×2
        assert_eq!(r.width(), 2);
        assert_eq!(r.height(), 2);
    }

    // ── ImageAdjust ──

    #[test]
    fn test_image_adjust_basic() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let result = builtin_image_adjust(&[img]).unwrap();
        assert!(matches!(&result, Value::Image(_)));
    }

    #[test]
    fn test_image_adjust_with_params() {
        let img = builtin_image(&[rgb2x2()]).unwrap();
        let result = builtin_image_adjust(&[
            img,
            Value::List(vec![
                Value::Real(rug::Float::with_val(53, 1.0)),
                Value::Real(rug::Float::with_val(53, 0.1)),
                Value::Real(rug::Float::with_val(53, 1.0)),
            ]),
        ])
        .unwrap();
        assert!(matches!(&result, Value::Image(_)));
    }

    // ── Binarize ──

    #[test]
    fn test_binarize_default() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let bin = builtin_binarize(&[img]).unwrap();
        let b = extract_image(&bin);
        assert_eq!(b.color(), image::ColorType::L8);
        // Pixel (0,0) is 0.0 < 0.5 → black (0)
        assert_eq!(b.get_pixel(0, 0)[0], 0);
        // Pixel (0,1) is 0.5 ≥ 0.5 → white (255)
        assert_eq!(b.get_pixel(0, 1)[0], 255);
    }

    #[test]
    fn test_binarize_with_threshold() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let bin = builtin_binarize(&[img, Value::Real(rug::Float::with_val(53, 0.25))]).unwrap();
        let b = extract_image(&bin);
        // Pixel (0,0) is 0.0 < 0.25 → black
        assert_eq!(b.get_pixel(0, 0)[0], 0);
        // Pixel (0,1) is 0.5 ≥ 0.25 → white
        assert_eq!(b.get_pixel(0, 1)[0], 255);
    }

    // ── ColorConvert ──

    #[test]
    fn test_color_convert_grayscale() {
        let img = builtin_image(&[rgb2x2()]).unwrap();
        let conv = builtin_color_convert(&[img, Value::Str("Grayscale".to_string())]).unwrap();
        let c = extract_image(&conv);
        assert_eq!(c.color(), image::ColorType::L8);
    }

    #[test]
    fn test_color_convert_unknown() {
        let img = builtin_image(&[rgb2x2()]).unwrap();
        let result = builtin_color_convert(&[img, Value::Str("Lab".to_string())]);
        assert!(result.is_err());
    }

    // ── GaussianFilter ──

    #[test]
    fn test_gaussian_filter() {
        let img = builtin_image(&[rgb2x2()]).unwrap();
        let result =
            builtin_gaussian_filter(&[img, Value::Real(rug::Float::with_val(53, 1.0))]).unwrap();
        assert!(matches!(&result, Value::Image(_)));
    }

    #[test]
    fn test_gaussian_filter_zero_sigma() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        // sigma=0 should return the image unchanged
        let result =
            builtin_gaussian_filter(&[img.clone(), Value::Real(rug::Float::with_val(53, 0.0))])
                .unwrap();
        // Same source image → same bytes
        assert_eq!(
            extract_image(&img).as_bytes(),
            extract_image(&result).as_bytes()
        );
    }

    // ── EdgeDetect ──

    #[test]
    fn test_edge_detect() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let result = builtin_edge_detect(&[img]).unwrap();
        let e = extract_image(&result);
        assert_eq!(e.color(), image::ColorType::L8);
        assert_eq!(e.width(), 2);
        assert_eq!(e.height(), 2);
    }

    // ── ImageConvolve ──

    #[test]
    fn test_image_convolve_identity() {
        // Use a 3×3 image so the center pixel (1,1) has all neighbors for accurate identity kernel test.
        let data = Value::List(vec![
            Value::List(vec![
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 1.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                ]),
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 1.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                ]),
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 1.0)),
                ]),
            ]),
            Value::List(vec![
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.5)),
                    Value::Real(rug::Float::with_val(53, 0.5)),
                    Value::Real(rug::Float::with_val(53, 0.5)),
                ]),
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 1.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                ]),
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.25)),
                    Value::Real(rug::Float::with_val(53, 0.5)),
                    Value::Real(rug::Float::with_val(53, 0.75)),
                ]),
            ]),
            Value::List(vec![
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                ]),
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 1.0)),
                    Value::Real(rug::Float::with_val(53, 0.0)),
                    Value::Real(rug::Float::with_val(53, 1.0)),
                ]),
                Value::List(vec![
                    Value::Real(rug::Float::with_val(53, 0.5)),
                    Value::Real(rug::Float::with_val(53, 0.5)),
                    Value::Real(rug::Float::with_val(53, 0.5)),
                ]),
            ]),
        ]);
        let img = builtin_image(&[data]).unwrap();
        let identity_kernel = Value::List(vec![
            Value::List(vec![
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(0)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(0)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(0)),
            ]),
        ]);
        let result = builtin_image_convolve(&[img, identity_kernel]).unwrap();
        let r = extract_image(&result);
        // With identity kernel, center pixel (1,1) should remain unchanged.
        // Edge pixels may differ due to boundary handling, so only check center.
        assert_eq!(r.get_pixel(1, 1), image::Rgba([0, 255, 0, 255]));
    }

    #[test]
    fn test_image_convolve_sharpen() {
        let img = builtin_image(&[rgb2x2()]).unwrap();
        // Simple sharpen kernel
        let sharpen = Value::List(vec![
            Value::List(vec![
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(-1)),
                Value::Integer(rug::Integer::from(0)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(-1)),
                Value::Integer(rug::Integer::from(5)),
                Value::Integer(rug::Integer::from(-1)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(-1)),
                Value::Integer(rug::Integer::from(0)),
            ]),
        ]);
        let result = builtin_image_convolve(&[img, sharpen]).unwrap();
        assert!(matches!(&result, Value::Image(_)));
    }

    // ── Error cases ──

    #[test]
    fn test_image_convolve_non_list_kernel() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let result = builtin_image_convolve(&[img, Value::Str("not a kernel".to_string())]);
        assert!(result.is_err());
    }

    #[test]
    fn test_binarize_wrong_arity() {
        assert!(builtin_binarize(&[]).is_err());
        assert!(
            builtin_binarize(&[
                Value::Integer(rug::Integer::from(1)),
                Value::Null,
                Value::Null
            ])
            .is_err()
        );
    }

    #[test]
    fn test_image_adjust_auto_stretches_contrast() {
        let img = builtin_image(&[gray2x2()]).unwrap();
        let adjusted = builtin_image_adjust(&[img]).unwrap();
        let rgba = extract_image(&adjusted).to_rgba8();
        // The darkest source pixel (0.0) → 0 and the brightest (1.0) → 255.
        assert_eq!(rgba.get_pixel(0, 0)[0], 0);
        assert_eq!(rgba.get_pixel(0, 1)[0], 255);
    }

    #[test]
    fn test_image_rotate_arbitrary_angle_preserves_channels() {
        let mut src = image::RgbImage::new(4, 4);
        for p in src.pixels_mut() {
            *p = image::Rgb([10, 20, 30]);
        }
        let data = Value::Image(Arc::new(image::DynamicImage::ImageRgb8(src)));
        let rotated =
            builtin_image_rotate(&[data, Value::Real(rug::Float::with_val(53, 30.0))]).unwrap();
        let out = extract_image(&rotated).to_rgba8();
        for p in out.pixels() {
            // Alpha must be preserved (previously left at 0).
            assert_eq!(p[3], 255, "alpha should stay opaque");
            if p[0] == 10 {
                assert_eq!((p[1], p[2]), (20, 30), "channels must stay aligned");
            }
        }
    }

    // ── Type hints / Interleaving / ColorSpace (WL conventions) ──

    fn real(v: f64) -> Value {
        Value::Real(rug::Float::with_val(53, v))
    }

    fn int(n: i64) -> Value {
        Value::Integer(rug::Integer::from(n))
    }

    /// Build a channel plane (list of row lists) from a slice of row slices.
    fn plane(rows: &[&[f64]]) -> Value {
        Value::List(
            rows.iter()
                .map(|r| Value::List(r.iter().map(|&v| real(v)).collect()))
                .collect(),
        )
    }

    #[test]
    fn test_image_default_values_are_normalised() {
        // The default (no type argument) interprets values on the [0, 1] range.
        let img = extract_image(&builtin_image(&[gray2x2()]).unwrap());
        assert_eq!(img.color(), image::ColorType::L8);
        assert_eq!(img.get_pixel(1, 0)[0], 128); // 0.5 → 128
    }

    #[test]
    fn test_image_byte_type_hint_uses_0_255_range() {
        let data = Value::List(vec![Value::List(vec![int(0), int(128), int(255)])]);
        let img = extract_image(&builtin_image(&[data, Value::Str("Byte".to_string())]).unwrap());
        assert_eq!(img.color(), image::ColorType::L8);
        assert_eq!(img.get_pixel(0, 0)[0], 0);
        assert_eq!(img.get_pixel(1, 0)[0], 128);
        assert_eq!(img.get_pixel(2, 0)[0], 255);
    }

    #[test]
    fn test_image_bit16_type_hint_and_image_data() {
        // 16-bit values are interpreted on a 0..=65535 range and stored as L16.
        let data = Value::List(vec![Value::List(vec![int(0), int(65535)])]);
        let img = builtin_image(&[data, Value::Str("Bit16".to_string())]).unwrap();
        assert_eq!(
            builtin_image_type(&[img.clone()]).unwrap(),
            Value::Str("Bit16".to_string())
        );
        let stored = extract_image(&img);
        assert_eq!(stored.color(), image::ColorType::L16);
        assert_eq!(stored.as_luma16().unwrap().get_pixel(1, 0)[0], 65535);

        // ImageData normalises by 65535 so a Bit16 image keeps a [0, 1] range.
        let out = builtin_image_data(&[img]).unwrap();
        if let Value::List(rows) = out
            && let Value::List(row) = &rows[0]
            && let (Value::Real(a), Value::Real(b)) = (&row[0], &row[1])
        {
            assert_eq!(a.to_f64(), 0.0);
            assert_eq!(b.to_f64(), 1.0);
        } else {
            panic!("expected a 2D list of scalars, got unexpected shape");
        }
    }

    #[test]
    fn test_image_interleaving_false_planes() {
        // 2×2 RGB given as three separate channel planes.
        let planes = Value::List(vec![
            plane(&[&[1.0, 0.0], &[0.0, 0.0]]),
            plane(&[&[0.0, 0.0], &[0.0, 1.0]]),
            plane(&[&[0.0, 1.0], &[0.0, 0.0]]),
        ]);
        let opts = Value::Rule {
            lhs: Box::new(Value::Symbol("Interleaving".to_string())),
            rhs: Box::new(Value::Bool(false)),
            delayed: false,
        };
        let img = extract_image(&builtin_image(&[planes, opts]).unwrap());
        assert_eq!(img.color(), image::ColorType::Rgb8);
        assert_eq!(img.get_pixel(0, 0), image::Rgba([255, 0, 0, 255]));
        assert_eq!(img.get_pixel(1, 0), image::Rgba([0, 0, 255, 255]));
    }

    #[test]
    fn test_image_colorspace_grayscale_option_converts() {
        let data = Value::List(vec![Value::List(vec![Value::List(vec![
            real(0.0),
            real(1.0),
            real(0.0),
        ])])]);
        let rule = Value::Rule {
            lhs: Box::new(Value::Symbol("ColorSpace".to_string())),
            rhs: Box::new(Value::Str("Grayscale".to_string())),
            delayed: false,
        };
        let img = extract_image(&builtin_image(&[data, rule]).unwrap());
        assert_eq!(img.color(), image::ColorType::L8);
        // Rec.601 luma of pure green ≈ 0.587 × 255 ≈ 150.
        let v = img.get_pixel(0, 0)[0];
        assert!((v as i32 - 150).abs() <= 1, "expected ≈150, got {v}");
    }

    #[test]
    fn test_image_colorspace_string_arg_forces_gray() {
        // `Image[data, "Grayscale"]` uses the second argument as a colour space.
        let data = Value::List(vec![Value::List(vec![Value::List(vec![
            real(0.0),
            real(1.0),
            real(0.0),
        ])])]);
        let img =
            extract_image(&builtin_image(&[data, Value::Str("Grayscale".to_string())]).unwrap());
        assert_eq!(img.color(), image::ColorType::L8);
        let v = img.get_pixel(0, 0)[0];
        assert!((v as i32 - 150).abs() <= 1, "expected ≈150, got {v}");
    }

    // ── Colour-space / channel conventions of core ops ──

    #[test]
    fn test_image_data_luma_alpha_two_channels() {
        let buf = image::GrayAlphaImage::from_pixel(1, 1, image::LumaA([255, 128]));
        let img = Value::Image(Arc::new(image::DynamicImage::ImageLumaA8(buf)));
        let out = builtin_image_data(&[img]).unwrap();
        if let Value::List(rows) = out
            && let Value::List(row) = &rows[0]
            && let Value::List(chs) = &row[0]
        {
            assert_eq!(chs.len(), 2, "LumaA must expose two channels");
            return;
        }
        panic!("expected a 2-channel pixel");
    }

    #[test]
    fn test_image_resize_preserves_colour_space() {
        let gray = builtin_image(&[gray2x2()]).unwrap();
        let r = extract_image(
            &builtin_image_resize(&[gray, Value::List(vec![int(4), int(4)])]).unwrap(),
        );
        assert_eq!(r.color(), image::ColorType::L8);

        let rgb = builtin_image(&[rgb2x2()]).unwrap();
        let r2 = extract_image(&builtin_image_resize(&[rgb, int(3)]).unwrap());
        assert_eq!(r2.color(), image::ColorType::Rgb8);
    }

    #[test]
    fn test_image_convolve_not_normalised_and_preserves_gray() {
        // Uniform gray 3×3 at 0.1 (≈26/255); box kernel of ones.
        let data = Value::List(vec![
            Value::List(vec![real(0.1), real(0.1), real(0.1)]),
            Value::List(vec![real(0.1), real(0.1), real(0.1)]),
            Value::List(vec![real(0.1), real(0.1), real(0.1)]),
        ]);
        let img = builtin_image(&[data]).unwrap();
        let kernel = Value::List(vec![
            Value::List(vec![int(1), int(1), int(1)]),
            Value::List(vec![int(1), int(1), int(1)]),
            Value::List(vec![int(1), int(1), int(1)]),
        ]);
        let r = extract_image(&builtin_image_convolve(&[img, kernel]).unwrap());
        assert_eq!(r.color(), image::ColorType::L8);
        // ImageConvolve does not normalise: 9 × 26 = 234.
        assert_eq!(r.get_pixel(1, 1)[0], 234);
    }

    #[test]
    fn test_edge_detect_keeps_signed_response() {
        // Hard vertical step: bright columns then dark columns.
        let data = Value::List(vec![
            Value::List(vec![real(1.0), real(1.0), real(0.0)]),
            Value::List(vec![real(1.0), real(1.0), real(0.0)]),
            Value::List(vec![real(1.0), real(1.0), real(0.0)]),
        ]);
        let img = builtin_image(&[data]).unwrap();
        let e = extract_image(&builtin_edge_detect(&[img]).unwrap());
        // Both sides of the edge are bright because the Sobel response has
        // opposite signs there; the old filter3x3 path clamped the negative
        // lobe to zero.
        assert!(e.get_pixel(1, 1)[0] > 200, "positive lobe missing");
        assert!(e.get_pixel(2, 1)[0] > 200, "negative lobe missing");
        assert_eq!(e.get_pixel(0, 1)[0], 0);
    }

    #[test]
    fn test_binarize_uses_rec601_luma() {
        let red = Value::List(vec![Value::List(vec![Value::List(vec![
            real(1.0),
            real(0.0),
            real(0.0),
        ])])]);
        let b = extract_image(&builtin_binarize(&[builtin_image(&[red]).unwrap()]).unwrap());
        // luma601(red) ≈ 0.299 < 0.5 → black.
        assert_eq!(b.get_pixel(0, 0)[0], 0);

        let green = Value::List(vec![Value::List(vec![Value::List(vec![
            real(0.0),
            real(1.0),
            real(0.0),
        ])])]);
        let b2 = extract_image(&builtin_binarize(&[builtin_image(&[green]).unwrap()]).unwrap());
        // luma601(green) ≈ 0.587 ≥ 0.5 → white.
        assert_eq!(b2.get_pixel(0, 0)[0], 255);
    }

    #[test]
    fn test_color_convert_grayscale_uses_rec601() {
        let green = Value::List(vec![Value::List(vec![Value::List(vec![
            real(0.0),
            real(1.0),
            real(0.0),
        ])])]);
        let img = builtin_image(&[green]).unwrap();
        let c = extract_image(
            &builtin_color_convert(&[img, Value::Str("Grayscale".to_string())]).unwrap(),
        );
        assert_eq!(c.color(), image::ColorType::L8);
        let v = c.get_pixel(0, 0)[0];
        assert!((v as i32 - 150).abs() <= 1, "expected ≈150, got {v}");
    }

    #[test]
    fn test_image_rotate_arbitrary_preserves_rgb_type() {
        let mut src = image::RgbImage::new(3, 3);
        for p in src.pixels_mut() {
            *p = image::Rgb([10, 20, 30]);
        }
        let img = Value::Image(Arc::new(image::DynamicImage::ImageRgb8(src)));
        let rotated = builtin_image_rotate(&[img, real(30.0)]).unwrap();
        assert_eq!(extract_image(&rotated).color(), image::ColorType::Rgb8);
    }
}
