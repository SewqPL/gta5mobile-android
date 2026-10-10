// CPU fallback for desktop BC texture formats unavailable on most phone GPUs.
use anyhow::{ensure, Result};
use wgpu::TextureFormat as T;

pub fn fallback_format(format: T) -> T {
    match format {
        T::Bc1RgbaUnorm | T::Bc2RgbaUnorm | T::Bc3RgbaUnorm | T::Bc7RgbaUnorm => T::Rgba8Unorm,
        T::Bc1RgbaUnormSrgb | T::Bc2RgbaUnormSrgb | T::Bc3RgbaUnormSrgb | T::Bc7RgbaUnormSrgb => T::Rgba8UnormSrgb,
        T::Bc4RUnorm => T::R8Unorm,
        T::Bc4RSnorm => T::R8Snorm,
        T::Bc5RgUnorm => T::Rg8Unorm,
        T::Bc5RgSnorm => T::Rg8Snorm,
        T::Bc6hRgbUfloat | T::Bc6hRgbFloat => T::Rgba16Float,
        other => other,
    }
}

pub fn decode(format: T, data: &[u8], w: u32, h: u32, depth: u32, pitch: u32, rows: u32) -> Result<(Vec<u8>, u32)> {
    ensure!(format.is_compressed() && w > 0 && h > 0 && depth > 0, "invalid BC dimensions/format");
    let (w, h, depth) = (w as usize, h as usize, depth as usize);
    let block_bytes = format.block_copy_size(None).unwrap_or(16) as usize;
    let pixel_bytes = fallback_format(format).block_copy_size(None).unwrap_or(4) as usize;
    let bx = w.div_ceil(4);
    let by = h.div_ceil(4);
    let pitch = if pitch == 0 { bx * block_bytes } else { pitch as usize };
    let rows = (rows as usize).max(by);
    ensure!(pitch >= bx * block_bytes, "BC row pitch is too small");
    let layer_stride = pitch.checked_mul(rows).ok_or_else(|| anyhow::anyhow!("BC pitch overflow"))?;
    let required = layer_stride.checked_mul(depth - 1)
        .and_then(|n| n.checked_add(pitch * (by - 1)))
        .and_then(|n| n.checked_add(bx * block_bytes)).ok_or_else(|| anyhow::anyhow!("BC size overflow"))?;
    ensure!(data.len() >= required, "truncated BC upload: {} bytes, need {required}", data.len());
    let out_pitch = w.checked_mul(pixel_bytes).ok_or_else(|| anyhow::anyhow!("decoded pitch overflow"))?;
    let len = out_pitch.checked_mul(h).and_then(|n| n.checked_mul(depth)).ok_or_else(|| anyhow::anyhow!("decoded size overflow"))?;
    let mut output = Vec::new();
    output.try_reserve_exact(len)?;
    output.resize(len, 0);
    for z in 0..depth {
        for y in 0..by {
            for x in 0..bx {
                let at = z * layer_stride + y * pitch + x * block_bytes;
                let block = &data[at..at + block_bytes];
                let mut pixels = [0u8; 128];
                match format {
                    T::Bc1RgbaUnorm | T::Bc1RgbaUnormSrgb => bcdec_rs::bc1(block, &mut pixels, 16),
                    T::Bc2RgbaUnorm | T::Bc2RgbaUnormSrgb => bcdec_rs::bc2(block, &mut pixels, 16),
                    T::Bc3RgbaUnorm | T::Bc3RgbaUnormSrgb => bcdec_rs::bc3(block, &mut pixels, 16),
                    T::Bc4RUnorm | T::Bc4RSnorm => bcdec_rs::bc4(block, &mut pixels, 4, format == T::Bc4RSnorm),
                    T::Bc5RgUnorm | T::Bc5RgSnorm => bcdec_rs::bc5(block, &mut pixels, 8, format == T::Bc5RgSnorm),
                    T::Bc6hRgbUfloat | T::Bc6hRgbFloat => {
                        let mut rgb = [0u16; 48];
                        bcdec_rs::bc6h_half(block, &mut rgb, 12, format == T::Bc6hRgbFloat);
                        for i in 0..16 {
                            for c in 0..3 { pixels[i * 8 + c * 2..i * 8 + c * 2 + 2].copy_from_slice(&rgb[i * 3 + c].to_le_bytes()); }
                            pixels[i * 8 + 6..i * 8 + 8].copy_from_slice(&0x3c00u16.to_le_bytes());
                        }
                    }
                    T::Bc7RgbaUnorm | T::Bc7RgbaUnormSrgb => bcdec_rs::bc7(block, &mut pixels, 16),
                    _ => anyhow::bail!("unsupported BC format {format:?}"),
                }
                for row in 0..4.min(h - y * 4) {
                    let n = 4.min(w - x * 4) * pixel_bytes;
                    let dst = (z * h + y * 4 + row) * out_pitch + x * 4 * pixel_bytes;
                    let src = row * 4 * pixel_bytes;
                    output[dst..dst + n].copy_from_slice(&pixels[src..src + n]);
                }
            }
        }
    }
    Ok((output, out_pitch.try_into()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bc1_red_and_partial_block() {
        let block = [0, 0xf8, 0, 0, 0, 0, 0, 0];
        let (rgba, pitch) = decode(T::Bc1RgbaUnorm, &block, 3, 2, 1, 8, 1).unwrap();
        assert_eq!(pitch, 12);
        assert_eq!(rgba, [255, 0, 0, 255].repeat(6));
    }

    #[test]
    fn padded_rows_and_layers() {
        let mut bytes = [0u8; 48];
        for at in [0, 16, 32] { bytes[at + 1] = 0xf8; }
        let (rgba, _) = decode(T::Bc1RgbaUnorm, &bytes, 4, 5, 1, 16, 2).unwrap();
        assert_eq!(rgba, [255, 0, 0, 255].repeat(20));
        let (rgba, _) = decode(T::Bc1RgbaUnorm, &bytes, 4, 4, 2, 16, 2).unwrap();
        assert_eq!(rgba, [255, 0, 0, 255].repeat(32));
    }

    #[test]
    fn rejects_truncated_uploads() {
        assert!(decode(T::Bc3RgbaUnorm, &[0; 8], 4, 4, 1, 16, 1).is_err());
        assert!(decode(T::Bc1RgbaUnorm, &[0; 32], 8, 4, 1, 8, 1).is_err());
    }

    #[test]
    fn hdr_keeps_half_float_alpha() {
        let (rgba, pitch) = decode(T::Bc6hRgbUfloat, &[0x13; 16], 4, 4, 1, 16, 1).unwrap();
        assert_eq!(pitch, 32);
        for p in rgba.chunks_exact(8) { assert_eq!(&p[6..], &0x3c00u16.to_le_bytes()); }
    }
}
