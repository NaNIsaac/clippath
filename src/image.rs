//! Image work that needs no Windows API: CF_DIB to PNG, comparing PNGs by their
//! pixels, and WSL paths.

/// CF_DIB (BITMAPINFOHEADER and pixels) to an RGB PNG.
///
/// Alpha is dropped: the alpha channel of CF_DIB is unreliable, and the clipboard's
/// own PNG format keeps it when an app provides one.
pub fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>, String> {
    let u32_at = |o: usize| dib.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let (Some(size), Some(width), Some(height), Some(comp)) = (u32_at(0), u32_at(4), u32_at(8), u32_at(16)) else {
        return Err("truncated DIB".into());
    };
    let (width, height) = (width as i32, height as i32);
    let bpp = u16::from_le_bytes([dib[14], dib[15]]);
    if !matches!(bpp, 24 | 32) || !matches!(comp, 0 | 3) {
        return Err(format!("unsupported DIB: {bpp}bpp, compression {comp}"));
    }
    // BI_BITFIELDS masks sit at offset 40 for every header version.
    if comp == 3 && (u32_at(40), u32_at(44), u32_at(48)) != (Some(0xFF0000), Some(0xFF00), Some(0xFF)) {
        return Err("unsupported BI_BITFIELDS masks".into());
    }
    if width <= 0 || height == 0 {
        return Err(format!("unsupported DIB size {width}x{height}"));
    }
    let offset = size as usize + if comp == 3 && size == 40 { 12 } else { 0 };
    let (w, rows, n) = (width as usize, height.unsigned_abs() as usize, bpp as usize / 8);
    let stride = (w * bpp as usize).div_ceil(32) * 4;
    if dib.len() < offset + stride * rows {
        return Err("truncated DIB".into());
    }
    let mut rgb = Vec::with_capacity(w * rows * 3);
    for r in 0..rows {
        // Positive height means the rows are stored bottom-up.
        let y = if height < 0 { r } else { rows - 1 - r };
        for px in dib[offset + y * stride..][..w * n].chunks_exact(n) {
            rgb.extend_from_slice(&[px[2], px[1], px[0]]);
        }
    }
    encode_rgb(w as u32, rows as u32, &rgb)
}

pub fn encode_rgb(width: u32, height: u32, rgb: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgb).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Width, height and RGB rows of a PNG, or None if it cannot be decoded. Alpha is
/// dropped, as `dib_to_png` does, so a tool's RGBA file and the clipboard's RGB copy
/// of the same screenshot compare equal.
pub fn rgb_pixels(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let rgb = match info.color_type {
        png::ColorType::Rgb => data.to_vec(),
        png::ColorType::Rgba => data.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g]).collect(),
        png::ColorType::GrayscaleAlpha => data.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0]]).collect(),
        png::ColorType::Indexed => return None, // expanded by normalize_to_color8
    };
    Some((info.width, info.height, rgb))
}

/// The IHDR width and height bytes, for a cheap check before decoding.
pub fn png_size(png: &[u8]) -> Option<&[u8]> {
    png.get(16..24)
}

/// `C:\Users\me\a.png` -> `/mnt/c/Users/me/a.png`; None without a drive letter.
pub fn to_wsl(path: &str) -> Option<String> {
    let mut chars = path.chars();
    let (drive, colon) = (chars.next()?, chars.next()?);
    if !drive.is_ascii_alphabetic() || colon != ':' {
        return None;
    }
    Some(format!("/mnt/{}{}", drive.to_ascii_lowercase(), chars.as_str().replace('\\', "/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A DIB header (BITMAPINFOHEADER) followed by `body`.
    fn dib(width: i32, height: i32, bpp: u16, comp: u32, body: &[u8]) -> Vec<u8> {
        let mut d = Vec::new();
        for v in [40u32, width as u32, height as u32] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&bpp.to_le_bytes());
        d.extend_from_slice(&comp.to_le_bytes());
        d.extend_from_slice(&[0; 20]);
        d.extend_from_slice(body);
        d
    }

    #[test]
    fn bottom_up_24bpp_dib() {
        // 2x2, stride 8; bottom row first, BGR: red, green / blue, white
        let body = [0, 0, 255, 0, 255, 0, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0];
        let (w, h, rgb) = rgb_pixels(&dib_to_png(&dib(2, 2, 24, 0, &body)).unwrap()).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(rgb, [0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255, 0]);
    }

    #[test]
    fn top_down_32bpp_bitfields_dib() {
        let mut body = Vec::new();
        for m in [0xFF0000u32, 0xFF00, 0xFF] {
            body.extend_from_slice(&m.to_le_bytes());
        }
        body.extend_from_slice(&[1, 2, 3, 0]);
        let (_, _, rgb) = rgb_pixels(&dib_to_png(&dib(1, -1, 32, 3, &body)).unwrap()).unwrap();
        assert_eq!(rgb, [3, 2, 1]);
    }

    #[test]
    fn unsupported_and_truncated_dibs_are_errors() {
        assert!(dib_to_png(&dib(1, 1, 16, 0, &[0; 4])).unwrap_err().contains("16bpp"));
        assert!(dib_to_png(&dib(4, 4, 24, 0, &[0; 8])).unwrap_err().contains("truncated"));
        assert!(dib_to_png(&[0; 10]).is_err());
    }

    /// Same pixels as `encode_rgb`, different bytes: RGBA, maximum compression.
    fn rgba_png(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
        let rgba: Vec<u8> = rgb.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect();
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, width, height);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.set_compression(png::Compression::High);
        e.write_header().unwrap().write_image_data(&rgba).unwrap();
        out
    }

    #[test]
    fn same_picture_from_different_encoders_has_the_same_pixels() {
        let shot = [255, 0, 0, 0, 255, 0, 0, 0, 255, 9, 9, 9, 200, 100, 50, 255, 255, 255];
        let (clip, tool) = (encode_rgb(3, 2, &shot).unwrap(), rgba_png(3, 2, &shot));
        assert_ne!(clip, tool);
        assert_eq!(rgb_pixels(&clip), rgb_pixels(&tool));

        let mut other = shot;
        other[13] = 101;
        assert_ne!(rgb_pixels(&clip), rgb_pixels(&rgba_png(3, 2, &other)));
        assert_eq!(rgb_pixels(b"\x89PNG not really"), None);
    }

    #[test]
    fn wsl_paths() {
        assert_eq!(to_wsl(r"C:\Users\me\shot.png").as_deref(), Some("/mnt/c/Users/me/shot.png"));
        assert_eq!(to_wsl(r"\\server\share\a.png"), None);
    }
}
