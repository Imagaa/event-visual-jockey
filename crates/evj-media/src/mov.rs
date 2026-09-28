//! Minimal QuickTime/MP4 sample-table reader: enough to pull raw video samples (HAP) out of a .mov.
use anyhow::{Context, Result, bail, ensure};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub offset: u64,
    pub size: u32,
}

#[derive(Debug, Clone)]
pub struct MovTrack {
    pub codec: [u8; 4],
    pub width: u32,
    pub height: u32,
    pub timescale: u32,
    // ponytail: constant frame rate from the first stts entry; keep per-sample times if VFR sources show up
    pub sample_delta: u32,
    pub samples: Vec<Sample>,
}

impl MovTrack {
    pub fn fps(&self) -> f64 {
        self.timescale as f64 / self.sample_delta as f64
    }
}

const MAX_MOOV: u64 = 64 << 20;
const MAX_SAMPLE: u32 = 256 << 20;

pub fn read_video_track(path: &Path) -> Result<MovTrack> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let len = f.metadata()?.len();
    let moov = read_top_level(&mut f, len, b"moov")?;
    parse_moov(&moov).with_context(|| format!("parse {}", path.display()))
}

pub fn read_sample(file: &mut File, s: Sample, buf: &mut Vec<u8>) -> Result<()> {
    ensure!(s.size <= MAX_SAMPLE, "sample too large ({} bytes)", s.size);
    buf.resize(s.size as usize, 0);
    file.seek(SeekFrom::Start(s.offset))?;
    file.read_exact(buf).context("sample past end of file")?;
    Ok(())
}

fn read_top_level(f: &mut File, len: u64, want: &[u8; 4]) -> Result<Vec<u8>> {
    let mut pos = 0u64;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 16];
        f.read_exact(&mut h[..8])?;
        let (mut size, mut header) = (u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64, 8u64);
        if size == 1 {
            f.read_exact(&mut h[8..16])?;
            size = u64::from_be_bytes(h[8..16].try_into().unwrap());
            header = 16;
        } else if size == 0 {
            size = len - pos;
        }
        ensure!(size >= header && pos + size <= len, "corrupt atom at offset {pos}");
        if &h[4..8] == want {
            ensure!(size - header <= MAX_MOOV, "moov atom too large");
            let mut buf = vec![0; (size - header) as usize];
            f.read_exact(&mut buf)?;
            return Ok(buf);
        }
        pos += size;
    }
    bail!("no '{}' atom (not a QuickTime/MP4 file?)", String::from_utf8_lossy(want))
}

fn be16(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes(s.try_into().unwrap())).context("truncated atom")
}
fn be32(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes(s.try_into().unwrap())).context("truncated atom")
}
fn be64(b: &[u8], at: usize) -> Result<u64> {
    b.get(at..at + 8).map(|s| u64::from_be_bytes(s.try_into().unwrap())).context("truncated atom")
}

/// Child atoms of `buf` as (fourcc, body).
fn children(mut buf: &[u8]) -> Result<Vec<([u8; 4], &[u8])>> {
    let mut out = Vec::new();
    while buf.len() >= 8 {
        let kind: [u8; 4] = buf[4..8].try_into().unwrap();
        let (header, size) = match be32(buf, 0)? as u64 {
            1 => (16, be64(buf, 8)?),
            0 => (8, buf.len() as u64),
            s => (8, s),
        };
        ensure!(size >= header && size <= buf.len() as u64, "corrupt '{}' atom", String::from_utf8_lossy(&kind));
        out.push((kind, &buf[header as usize..size as usize]));
        buf = &buf[size as usize..];
    }
    Ok(out)
}

fn child<'a>(buf: &'a [u8], kind: &[u8; 4]) -> Result<&'a [u8]> {
    children(buf)?
        .into_iter()
        .find(|(k, _)| k == kind)
        .map(|(_, b)| b)
        .with_context(|| format!("missing '{}' atom", String::from_utf8_lossy(kind)))
}

fn parse_moov(moov: &[u8]) -> Result<MovTrack> {
    for (kind, trak) in children(moov)? {
        if &kind != b"trak" {
            continue;
        }
        let mdia = child(trak, b"mdia")?;
        // hdlr: version/flags(4) pre_defined(4) handler_type(4)
        if child(mdia, b"hdlr")?.get(8..12) != Some(b"vide".as_slice()) {
            continue;
        }
        return parse_video_trak(mdia);
    }
    bail!("no video track")
}

fn parse_video_trak(mdia: &[u8]) -> Result<MovTrack> {
    let mdhd = child(mdia, b"mdhd")?;
    // v0: vf(4) ctime(4) mtime(4) timescale(4) — v1: vf(4) ctime(8) mtime(8) timescale(4)
    let timescale = if mdhd.first() == Some(&1) { be32(mdhd, 20)? } else { be32(mdhd, 12)? };
    let stbl = child(child(mdia, b"minf")?, b"stbl")?;

    // stsd: vf(4) count(4) entry[size(4) format(4) reserved(6) dref(2) ver(2) rev(2) vendor(4) tq(4) sq(4) width(2) height(2)]
    let stsd = child(stbl, b"stsd")?;
    let codec: [u8; 4] = stsd.get(12..16).context("truncated stsd")?.try_into().unwrap();
    let (width, height) = (be16(stsd, 40)? as u32, be16(stsd, 42)? as u32);

    let stts = child(stbl, b"stts")?;
    let sample_delta = be32(stts, 12)?;
    ensure!(timescale > 0 && sample_delta > 0 && width > 0 && height > 0, "invalid video track header");

    let stsz = child(stbl, b"stsz")?;
    let fixed = be32(stsz, 4)?;
    let n_samples = be32(stsz, 8)? as usize;
    ensure!(n_samples > 0 && n_samples <= 10_000_000, "bad sample count {n_samples}");

    let stsc = child(stbl, b"stsc")?;
    let stsc_entries = (0..be32(stsc, 4)? as usize)
        .map(|i| Ok((be32(stsc, 8 + 12 * i)?, be32(stsc, 12 + 12 * i)?)))
        .collect::<Result<Vec<(u32, u32)>>>()?;

    let chunk_offsets: Vec<u64> = if let Ok(stco) = child(stbl, b"stco") {
        (0..be32(stco, 4)? as usize).map(|i| be32(stco, 8 + 4 * i).map(u64::from)).collect::<Result<_>>()?
    } else {
        let co64 = child(stbl, b"co64")?;
        (0..be32(co64, 4)? as usize).map(|i| be64(co64, 8 + 8 * i)).collect::<Result<_>>()?
    };

    let mut samples = Vec::with_capacity(n_samples);
    for (ci, &chunk_offset) in chunk_offsets.iter().enumerate() {
        let chunk_no = ci as u32 + 1;
        let per_chunk = stsc_entries
            .iter()
            .rev()
            .find(|(first, _)| *first <= chunk_no)
            .map(|e| e.1)
            .context("bad stsc table")?;
        let mut offset = chunk_offset;
        for _ in 0..per_chunk {
            if samples.len() == n_samples {
                break;
            }
            let size = if fixed != 0 { fixed } else { be32(stsz, 12 + 4 * samples.len())? };
            samples.push(Sample { offset, size });
            offset += size as u64;
        }
    }
    ensure!(samples.len() == n_samples, "sample table mismatch ({} of {n_samples})", samples.len());
    Ok(MovTrack { codec, width, height, timescale, sample_delta, samples })
}
