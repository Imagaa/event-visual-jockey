//! HAP frame decoding → BCn texture bytes.
//! Spec: https://github.com/Vidvox/hap/blob/master/documentation/HapVideoDRAFT.md
use crate::mov::{MovTrack, read_sample, read_video_track};
use anyhow::{Context, Result, bail, ensure};
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapFormat {
    Bc1,
    Bc3,
    /// Scaled YCoCg in BC3 (HAP Q) — converted to RGB in the shader.
    YCoCgBc3,
}

impl HapFormat {
    pub fn block_bytes(self) -> usize {
        match self {
            Self::Bc1 => 8,
            Self::Bc3 | Self::YCoCgBc3 => 16,
        }
    }
}

/// Bytes of BC data for a `w`×`h` image (dimensions rounded up to 4).
pub fn texture_len(format: HapFormat, w: u32, h: u32) -> usize {
    (w.div_ceil(4) * h.div_ceil(4)) as usize * format.block_bytes()
}

pub fn is_hap(codec: &[u8; 4]) -> bool {
    matches!(codec, b"Hap1" | b"Hap5" | b"HapY")
}

const MAX_FRAME: usize = 256 << 20;

/// One section: (type, body, rest).
fn section(buf: &[u8]) -> Result<(u8, &[u8], &[u8])> {
    ensure!(buf.len() >= 4, "truncated HAP section header");
    let short = u32::from_le_bytes([buf[0], buf[1], buf[2], 0]) as usize;
    let (len, header) = if short == 0 {
        ensure!(buf.len() >= 8, "truncated HAP section header");
        (u32::from_le_bytes(buf[4..8].try_into().unwrap()) as usize, 8)
    } else {
        (short, 4)
    };
    let end = header + len;
    ensure!(buf.len() >= end, "truncated HAP section body");
    Ok((buf[3], &buf[header..end], &buf[end..]))
}

pub fn decode_frame(frame: &[u8], out: &mut Vec<u8>) -> Result<HapFormat> {
    let (kind, body, _) = section(frame)?;
    let format = match kind & 0x0F {
        0xB => HapFormat::Bc1,
        0xE => HapFormat::Bc3,
        0xF => HapFormat::YCoCgBc3,
        other => bail!("unsupported HAP texture format 0x{other:X} (HAP Q Alpha, BC7 and BC6 are not supported yet)"),
    };
    out.clear();
    match kind >> 4 {
        0xA => out.extend_from_slice(body),
        0xB => snappy_into(body, out)?,
        0xC => decode_chunks(body, out)?,
        other => bail!("unknown HAP compressor 0x{other:X}"),
    }
    Ok(format)
}

fn snappy_into(src: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let n = snap::raw::decompress_len(src)?;
    ensure!(out.len() + n <= MAX_FRAME, "HAP frame too large");
    let start = out.len();
    out.resize(start + n, 0);
    snap::raw::Decoder::new().decompress(src, &mut out[start..])?;
    Ok(())
}

fn decode_chunks(body: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let (kind, mut instructions, data) = section(body)?;
    ensure!(kind == 0x01, "expected HAP decode instructions, got 0x{kind:X}");
    let (mut compressors, mut sizes, mut offsets) = (None, None, None);
    while !instructions.is_empty() {
        let (k, b, rest) = section(instructions)?;
        match k {
            0x02 => compressors = Some(b),
            0x03 => sizes = Some(b),
            0x04 => offsets = Some(b),
            _ => {}
        }
        instructions = rest;
    }
    let compressors = compressors.context("HAP chunk compressor table missing")?;
    let sizes = sizes.context("HAP chunk size table missing")?;
    ensure!(sizes.len() == compressors.len() * 4, "HAP chunk tables disagree");
    let le32 = |t: &[u8], i: usize| -> Result<usize> {
        Ok(u32::from_le_bytes(t.get(i * 4..i * 4 + 4).context("HAP chunk table too short")?.try_into().unwrap()) as usize)
    };
    let mut cursor = 0;
    for (i, &comp) in compressors.iter().enumerate() {
        let size = le32(sizes, i)?;
        let offset = match offsets {
            Some(t) => le32(t, i)?,
            None => cursor,
        };
        let chunk = data.get(offset..offset + size).context("HAP chunk out of bounds")?;
        match comp {
            0x0A => out.extend_from_slice(chunk),
            0x0B => snappy_into(chunk, out)?,
            c => bail!("unknown HAP chunk compressor 0x{c:X}"),
        }
        cursor = offset + size;
    }
    Ok(())
}

pub struct HapReader {
    pub track: MovTrack,
    file: File,
    packet: Vec<u8>,
}

impl HapReader {
    pub fn open(path: &Path) -> Result<Self> {
        let track = read_video_track(path)?;
        ensure!(is_hap(&track.codec), "not a HAP file (codec '{}')", String::from_utf8_lossy(&track.codec));
        Ok(Self { file: File::open(path)?, track, packet: Vec::new() })
    }

    pub fn read_frame(&mut self, index: usize, out: &mut Vec<u8>) -> Result<HapFormat> {
        let s = *self.track.samples.get(index).context("frame index out of range")?;
        read_sample(&mut self.file, s, &mut self.packet)?;
        let format = decode_frame(&self.packet, out).with_context(|| format!("HAP frame {index}"))?;
        let want = texture_len(format, self.track.width, self.track.height);
        ensure!(out.len() == want, "HAP frame {index}: {} bytes, expected {want}", out.len());
        Ok(format)
    }
}
