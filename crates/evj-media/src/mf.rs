//! Media Foundation video decoding. With a D3D11 device: hardware decode + GPU colour conversion,
//! frames stay on the GPU. Without: MF software decode into system memory.
use anyhow::{Context, Result, bail};
use std::path::Path;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::{GUID, HSTRING, Interface};

pub fn mf_init_thread() -> Result<()> {
    unsafe {
        // S_FALSE (already initialised) is fine; a different apartment on this thread is too — MF objects are free-threaded.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
    }
    Ok(())
}

pub enum VideoFrame {
    /// Decoder-owned texture (array slice `subresource`); keep `sample` alive until copied.
    Gpu { texture: ID3D11Texture2D, subresource: u32, sample: IMFSample },
    /// Tight, top-down BGRA.
    Cpu { data: Vec<u8> },
}

pub struct MfVideo {
    reader: IMFSourceReader,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// YUV matrix of NV12 GPU frames (BT.709 vs BT.601).
    pub bt709: bool,
    /// Seconds (0 when the container does not say).
    pub duration: f64,
    /// Frame decoded by `seek`, handed out by the next `next_frame`.
    pending: Option<(VideoFrame, f64)>,
}

const STREAM: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

impl MfVideo {
    pub fn open(path: &Path, device: Option<&ID3D11Device>) -> Result<MfVideo> {
        unsafe {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 3)?;
            let attrs = attrs.context("MFCreateAttributes")?;
            if let Some(device) = device {
                let (mut token, mut mgr) = (0u32, None);
                MFCreateDXGIDeviceManager(&mut token, &mut mgr)?;
                let mgr = mgr.context("MFCreateDXGIDeviceManager")?;
                mgr.ResetDevice(device, token)?;
                attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &mgr)?;
                attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
            }
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
            let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attrs)
                .with_context(|| format!("Media Foundation cannot open {}", path.display()))?;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            reader.SetStreamSelection(STREAM, true)?;
            // GPU path: NV12 straight from the decoder, converted in our shader — MF's own RGB32 conversion
            // (video processor) costs a 4K frame ~25% of a Vega 8's budget. RGB32 when NV12 is refused (10-bit).
            let subtypes: &[GUID] = if device.is_some() { &[MFVideoFormat_NV12, MFVideoFormat_RGB32] } else { &[MFVideoFormat_RGB32] };
            let mut set = Err(anyhow::anyhow!("no output format"));
            for subtype in subtypes {
                let mt = MFCreateMediaType()?;
                mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                mt.SetGUID(&MF_MT_SUBTYPE, subtype)?;
                set = reader.SetCurrentMediaType(STREAM, None, &mt).map_err(Into::into);
                if set.is_ok() {
                    break;
                }
            }
            set.context("no decoder for this video (codec not supported by Media Foundation)")?;
            let duration = reader
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
                .ok()
                .and_then(|v| u64::try_from(&v).ok())
                .map_or(0.0, |d| d as f64 / 10_000_000.0);
            let mut v = MfVideo { reader, width: 0, height: 0, fps: 30.0, bt709: false, duration, pending: None };
            v.read_format()?;
            Ok(v)
        }
    }

    fn read_format(&mut self) -> Result<()> {
        unsafe {
            let cur = self.reader.GetCurrentMediaType(STREAM)?;
            let size = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
            (self.width, self.height) = ((size >> 32) as u32, size as u32);
            // Untagged streams: HD is BT.709, SD is BT.601 (the usual player convention).
            self.bt709 = match cur.GetUINT32(&MF_MT_YUV_MATRIX) {
                Ok(m) if m == MFVideoTransferMatrix_BT709.0 as u32 => true,
                Ok(m) if m == MFVideoTransferMatrix_BT601.0 as u32 => false,
                _ => self.height >= 720,
            };
            if let Ok(rate) = cur.GetUINT64(&MF_MT_FRAME_RATE) {
                let (num, den) = ((rate >> 32) as f64, (rate as u32) as f64);
                if num > 0.0 && den > 0.0 {
                    self.fps = num / den;
                }
            }
        }
        Ok(())
    }

    pub fn next_frame(&mut self) -> Result<Option<(VideoFrame, f64)>> {
        if let Some(f) = self.pending.take() {
            return Ok(Some(f));
        }
        loop {
            let (mut flags, mut ts, mut sample) = (0u32, 0i64, None);
            unsafe { self.reader.ReadSample(STREAM, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample))? };
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                bail!("Media Foundation stream error");
            }
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                self.read_format()?;
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
            let Some(sample) = sample else { continue }; // stream tick / gap
            let pts = ts as f64 / 10_000_000.0;
            return Ok(Some((self.frame_from(sample)?, pts)));
        }
    }

    fn frame_from(&self, sample: IMFSample) -> Result<VideoFrame> {
        unsafe {
            let buffer = sample.GetBufferByIndex(0)?;
            if let Ok(dxgi) = buffer.cast::<IMFDXGIBuffer>() {
                let mut raw = std::ptr::null_mut();
                dxgi.GetResource(&ID3D11Texture2D::IID, &mut raw)?;
                let texture = ID3D11Texture2D::from_raw(raw);
                let subresource = dxgi.GetSubresourceIndex()?;
                return Ok(VideoFrame::Gpu { texture, subresource, sample });
            }
            let b2 = buffer.cast::<IMF2DBuffer>().context("decoder output is neither a GPU nor a 2D buffer")?;
            let (mut scan0, mut pitch) = (std::ptr::null_mut(), 0i32);
            b2.Lock2D(&mut scan0, &mut pitch)?;
            // scan0 is the top row; pitch is negative for bottom-up RGB32.
            let row = self.width as usize * 4;
            let mut data = Vec::with_capacity(row * self.height as usize);
            for y in 0..self.height as isize {
                let p = scan0.offset(y * pitch as isize);
                data.extend_from_slice(std::slice::from_raw_parts(p, row));
            }
            b2.Unlock2D()?;
            Ok(VideoFrame::Cpu { data })
        }
    }

    /// Jumps to `secs`: MF lands on the previous key frame, so frames before the target are skipped.
    pub fn seek(&mut self, secs: f64) -> Result<()> {
        self.pending = None;
        let t = (secs.max(0.0) * 10_000_000.0) as i64;
        unsafe { self.reader.SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from(t))? };
        let min = secs - 1.0 / self.fps;
        while let Some((f, pts)) = self.next_frame()? {
            if pts >= min - 1e-6 {
                self.pending = Some((f, pts));
                break;
            }
        }
        Ok(())
    }

    pub fn rewind(&mut self) -> Result<()> {
        self.pending = None;
        unsafe { self.reader.SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from(0i64))? };
        Ok(())
    }
}
