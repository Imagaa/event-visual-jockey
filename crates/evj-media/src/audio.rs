//! Audio decoding with Media Foundation: any audio track (MP3, AAC, WAV, M4A, audio in MP4/MOV)
//! → interleaved stereo f32 at the output device's sample rate (MF does the resampling).
use anyhow::{Context, Result, bail};
use std::path::Path;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::core::{GUID, HSTRING};

const STREAM: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
pub const CHANNELS: usize = 2;

pub struct AudioDecoder {
    reader: IMFSourceReader,
    pub rate: u32,
    /// Seconds (0 when unknown).
    pub duration: f64,
}

fn reader(path: &Path) -> Result<IMFSourceReader> {
    unsafe {
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), None)
            .with_context(|| format!("Media Foundation cannot open {}", path.display()))?;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        reader.SetStreamSelection(STREAM, true).context("no audio track")?;
        Ok(reader)
    }
}

/// True when `path` has an audio track Media Foundation can read.
pub fn has_audio(path: &Path) -> bool {
    reader(path).is_ok_and(|r| unsafe { r.GetNativeMediaType(STREAM, 0).is_ok() })
}

impl AudioDecoder {
    /// Opens the first audio track, decoded to stereo f32 at `rate` Hz.
    pub fn open(path: &Path, rate: u32) -> Result<AudioDecoder> {
        let reader = reader(path)?;
        unsafe {
            let mt = MFCreateMediaType()?;
            mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            mt.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_Float)?;
            mt.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, CHANNELS as u32)?;
            mt.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
            mt.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 32)?;
            mt.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, (CHANNELS * 4) as u32)?;
            mt.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * (CHANNELS * 4) as u32)?;
            mt.SetUINT32(&MF_MT_ALL_SAMPLES_INDEPENDENT, 1)?;
            reader.SetCurrentMediaType(STREAM, None, &mt).context("cannot decode this audio track")?;
            let cur = reader.GetCurrentMediaType(STREAM)?;
            let got_rate = cur.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
            let got_ch = cur.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)?;
            if got_rate != rate || got_ch as usize != CHANNELS {
                bail!("audio conversion to {rate} Hz stereo not available ({got_rate} Hz, {got_ch} ch)");
            }
            let duration = reader
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
                .ok()
                .and_then(|v| u64::try_from(&v).ok())
                .map_or(0.0, |d| d as f64 / 10_000_000.0);
            Ok(AudioDecoder { reader, rate, duration })
        }
    }

    /// Next block of interleaved stereo samples and its start time (seconds); None at the end.
    pub fn read(&mut self) -> Result<Option<(Vec<f32>, f64)>> {
        loop {
            let (mut flags, mut ts, mut sample) = (0u32, 0i64, None);
            unsafe { self.reader.ReadSample(STREAM, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample))? };
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 {
                bail!("audio stream error");
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
            let Some(sample) = sample else { continue };
            unsafe {
                let buf = sample.ConvertToContiguousBuffer()?;
                let (mut ptr, mut len) = (std::ptr::null_mut(), 0u32);
                buf.Lock(&mut ptr, None, Some(&mut len))?;
                let n = len as usize / 4;
                let data = std::slice::from_raw_parts(ptr as *const f32, n).to_vec();
                buf.Unlock()?;
                return Ok(Some((data, ts as f64 / 10_000_000.0)));
            }
        }
    }

    pub fn seek(&mut self, secs: f64) -> Result<()> {
        let t = (secs.max(0.0) * 10_000_000.0) as i64;
        unsafe { self.reader.SetCurrentPosition(&GUID::zeroed(), &PROPVARIANT::from(t))? };
        Ok(())
    }
}
