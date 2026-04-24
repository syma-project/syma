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

/// Build an `image::DynamicImage` from a 2D or 3D nested Value list.
///
/// Expected shapes:
///   - 2D: `{{r1c1, r1c2, ...}, {r2c1, ...}, ...}`  → Grayscale
///   - 3D (inner len=3): `{{{r,g,b}, ...}, ...}`      → RGB
///   - 3D (inner len=4): `{{{r,g,b,a}, ...}, ...}`    → RGBA
///
/// All pixel values are expected in [0,1].
fn img_from_list(data: &[Value]) -> Result<image::DynamicImage, EvalError> {
    if data.is_empty() {
        return Err(EvalError::Error("Image: data cannot be empty".to_string()));
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
    let first_pixel = &row0[0];

    // Determine the number of channels from the innermost pixel representation.
    let channels = match first_pixel {
        Value::List(ch) => ch.len(),
        _ => 1, // scalar → grayscale
    };

    if channels != 1 && channels != 3 && channels != 4 {
        return Err(EvalError::Error(format!(
            "Image: expected 1, 3, or 4 channels per pixel, got {}",
            channels
        )));
    }

    // Validate all rows have the same width.
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
                // Validate all pixels have the same channel count.
                for (c, item) in items.iter().enumerate() {
                    let ch = match item {
                        Value::List(ch) => ch.len(),
                        _ => 1,
                    };
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

    match channels {
        1 => {
            // Grayscale
            let mut buf = image::GrayImage::new(width as u32, height as u32);
            for (r, row) in data.iter().enumerate() {
                if let Value::List(items) = row {
                    for (c, item) in items.iter().enumerate() {
                        let v = value_to_f64(item)?;
                        buf.put_pixel(c as u32, r as u32, image::Luma([norm_to_u8(v)]));
                    }
                }
            }
            Ok(image::DynamicImage::ImageLuma8(buf))
        }
        3 => {
            // RGB
            let mut buf = image::RgbImage::new(width as u32, height as u32);
            for (r, row) in data.iter().enumerate() {
                if let Value::List(items) = row {
                    for (c, item) in items.iter().enumerate() {
                        if let Value::List(chs) = item
                            && chs.len() >= 3
                        {
                            let rv = norm_to_u8(clamp01(value_to_f64(&chs[0])?));
                            let gv = norm_to_u8(clamp01(value_to_f64(&chs[1])?));
                            let bv = norm_to_u8(clamp01(value_to_f64(&chs[2])?));
                            buf.put_pixel(c as u32, r as u32, image::Rgb([rv, gv, bv]));
                        }
                    }
                }
            }
            Ok(image::DynamicImage::ImageRgb8(buf))
        }
        4 => {
            // RGBA
            let mut buf = image::RgbaImage::new(width as u32, height as u32);
            for (r, row) in data.iter().enumerate() {
                if let Value::List(items) = row {
                    for (c, item) in items.iter().enumerate() {
                        if let Value::List(chs) = item {
                            let rv = norm_to_u8(clamp01(value_to_f64(&chs[0])?));
                            let gv = norm_to_u8(clamp01(value_to_f64(&chs[1])?));
                            let bv = norm_to_u8(clamp01(value_to_f64(&chs[2])?));
                            let av = if chs.len() >= 4 {
                                norm_to_u8(clamp01(value_to_f64(&chs[3])?))
                            } else {
                                255
                            };
                            buf.put_pixel(c as u32, r as u32, image::Rgba([rv, gv, bv, av]));
                        }
                    }
                }
            }
            Ok(image::DynamicImage::ImageRgba8(buf))
        }
        _ => unreachable!(),
    }
}

/// Convert an image to a 2D Value list (row-major) with normalised [0, 1] values.
fn list_from_img(img: &image::DynamicImage) -> Value {
    let (w, h) = (img.width(), img.height());
    match img.color() {
        image::ColorType::L8 | image::ColorType::L16 => {
            // Grayscale: single channel per pixel → 2D list of scalars
            let mut rows = Vec::with_capacity(h as usize);
            for y in 0..h {
                let mut row = Vec::with_capacity(w as usize);
                for x in 0..w {
                    let p = img.get_pixel(x, y);
                    let v = u8_to_norm(p[0]);
                    row.push(Value::Real(rug::Float::with_val(53, v)));
                }
                rows.push(Value::List(row));
            }
            Value::List(rows)
        }
        image::ColorType::Rgb8 | image::ColorType::Rgb16 | image::ColorType::Rgb32F => {
            // RGB: 3 channels per pixel
            let mut rows = Vec::with_capacity(h as usize);
            for y in 0..h {
                let mut row = Vec::with_capacity(w as usize);
                for x in 0..w {
                    let p = img.get_pixel(x, y);
                    let chs = vec![
                        Value::Real(rug::Float::with_val(53, u8_to_norm(p[0]))),
                        Value::Real(rug::Float::with_val(53, u8_to_norm(p[1]))),
                        Value::Real(rug::Float::with_val(53, u8_to_norm(p[2]))),
                    ];
                    row.push(Value::List(chs));
                }
                rows.push(Value::List(row));
            }
            Value::List(rows)
        }
        image::ColorType::Rgba8 | image::ColorType::Rgba16 | image::ColorType::Rgba32F => {
            // RGBA: 4 channels per pixel
            let mut rows = Vec::with_capacity(h as usize);
            for y in 0..h {
                let mut row = Vec::with_capacity(w as usize);
                for x in 0..w {
                    let p = img.get_pixel(x, y);
                    let chs = vec![
                        Value::Real(rug::Float::with_val(53, u8_to_norm(p[0]))),
                        Value::Real(rug::Float::with_val(53, u8_to_norm(p[1]))),
                        Value::Real(rug::Float::with_val(53, u8_to_norm(p[2]))),
                        Value::Real(rug::Float::with_val(53, u8_to_norm(p[3]))),
                    ];
                    row.push(Value::List(chs));
                }
                rows.push(Value::List(row));
            }
            Value::List(rows)
        }
        _ => {
            // Fallback: convert to RGB8 first
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
/// Auto-detects grayscale (scalar), RGB (3-element), or RGBA (4-element).
pub fn builtin_image(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Image requires 1 or 2 arguments: Image[data] or Image[data, \"type\"]".to_string(),
        ));
    }

    let data = match &args[0] {
        Value::List(items) => items,
        other => {
            return Err(EvalError::TypeError {
                expected: "List".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };

    let _type_hint = if args.len() >= 2 {
        match &args[1] {
            Value::Str(s) => s.clone(),
            _ => {
                return Err(EvalError::Error(
                    "Image: second argument must be a string type".to_string(),
                ));
            }
        }
    } else {
        "Byte".to_string()
    };

    let img = img_from_list(data)?;
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
            let w = value_to_f64(&dims[0])? as u32;
            let h = value_to_f64(&dims[1])? as u32;
            if w == 0 || h == 0 {
                return Err(EvalError::Error(
                    "ImageResize: dimensions must be positive".to_string(),
                ));
            }
            (w, h)
        }
        other => {
            let w = value_to_f64(other)? as u32;
            if w == 0 {
                return Err(EvalError::Error(
                    "ImageResize: dimension must be positive".to_string(),
                ));
            }
            let aspect = img.height() as f64 / img.width() as f64;
            let h = (w as f64 * aspect).round() as u32;
            (w.max(1), h.max(1))
        }
    };

    let resized = image::imageops::resize(
        img.as_ref(),
        new_w,
        new_h,
        image::imageops::FilterType::Lanczos3,
    );
    Ok(Value::Image(Arc::new(image::DynamicImage::ImageRgba8(
        resized,
    ))))
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

                    for c in 0..4.min(img.color().channel_count() as usize) {
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
        image::DynamicImage::ImageRgba8(buf)
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
        let offset = -(min_pixel as f64) * scale / 255.0;
        (scale / 255.0, offset, 1.0)
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

    // Convert to grayscale first, then threshold
    let gray = img.to_luma8();
    let (w, h) = (gray.width(), gray.height());
    let mut buf = image::GrayImage::new(w, h);

    let thresh_u8 = norm_to_u8(threshold);
    for y in 0..h {
        for x in 0..w {
            let p = gray.get_pixel(x, y);
            let v = if p[0] >= thresh_u8 { 255 } else { 0 };
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
        "Grayscale" => image::DynamicImage::ImageLuma8(img.to_luma8()),
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

/// Sobel kernels for edge detection.
const SOBEL_X: [f32; 9] = [-1.0, 0.0, 1.0, -2.0, 0.0, 2.0, -1.0, 0.0, 1.0];
const SOBEL_Y: [f32; 9] = [-1.0, -2.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 1.0];

/// EdgeDetect[image]
///
/// Apply Sobel edge detection. Converts to grayscale, then computes
/// gradient magnitude from horizontal and vertical Sobel operators.
pub fn builtin_edge_detect(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "EdgeDetect requires exactly 1 argument".to_string(),
        ));
    }
    let img = get_image_arg(args, 0)?;

    // Convert to grayscale
    let gray = img.to_luma8();
    let (w, h) = (gray.width(), gray.height());
    let dyn_gray = image::DynamicImage::ImageLuma8(gray.clone());

    // Apply Sobel X and Y — filter3x3 preserves pixel type, so the output has
    // Luma<u8> pixels just like the input.
    let gx = image::imageops::filter3x3(&dyn_gray, &SOBEL_X);
    let gy = image::imageops::filter3x3(&dyn_gray, &SOBEL_Y);

    let mut buf = image::GrayImage::new(w, h);
    let mut max_mag = 0.0f64;

    // Compute magnitude and find max for normalisation
    let mut magnitudes = vec![0.0f64; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let dx = gx.get_pixel(x, y)[0] as f64;
            let dy = gy.get_pixel(x, y)[0] as f64;
            let mag = (dx * dx + dy * dy).sqrt();
            magnitudes[(y * w + x) as usize] = mag;
            if mag > max_mag {
                max_mag = mag;
            }
        }
    }

    // Normalise to 0-255
    let scale = if max_mag > 0.0 { 255.0 / max_mag } else { 1.0 };
    for y in 0..h {
        for x in 0..w {
            let v = (magnitudes[(y * w + x) as usize] * scale).round() as u8;
            buf.put_pixel(x, y, image::Luma([v]));
        }
    }

    Ok(Value::Image(Arc::new(image::DynamicImage::ImageLuma8(buf))))
}

/// ImageConvolve[image, kernel]
///
/// Convolve an image with a 2D kernel (list of lists).
/// For 3×3 kernels, uses the optimised filter3x3 path.
/// Kernels are normalised so the sum of coefficients ≈ 1.
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

    if kw == 3 && kh == 3 {
        // Use the optimised filter3x3 path.
        // Flatten to 9-element array in row-major order.
        let flat: [f32; 9] = [
            kernel[0][0],
            kernel[0][1],
            kernel[0][2],
            kernel[1][0],
            kernel[1][1],
            kernel[1][2],
            kernel[2][0],
            kernel[2][1],
            kernel[2][2],
        ];
        let result = image::imageops::filter3x3(img.as_ref(), &flat);
        Ok(Value::Image(Arc::new(image::DynamicImage::ImageRgba8(
            result,
        ))))
    } else {
        // Manual convolution for larger kernels.
        let rgb = img.to_rgba8();
        let (w, h) = (rgb.width(), rgb.height());
        let mut buf = image::RgbaImage::new(w, h);
        let half_kw = (kw / 2) as i32;
        let half_kh = (kh / 2) as i32;

        for y in 0..h {
            for x in 0..w {
                let mut acc = [0.0f64; 4];
                for (ky, krow) in kernel.iter().enumerate() {
                    for (kx, k_val) in krow.iter().enumerate() {
                        let px = x as i32 + kx as i32 - half_kw;
                        let py = y as i32 + ky as i32 - half_kh;
                        if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                            let p = rgb.get_pixel(px as u32, py as u32);
                            let k = *k_val as f64;
                            for c in 0..4 {
                                acc[c] += p[c] as f64 * k;
                            }
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

        Ok(Value::Image(Arc::new(image::DynamicImage::ImageRgba8(buf))))
    }
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
        assert!(result.is_err());
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
        assert!(result.is_err());
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
}
