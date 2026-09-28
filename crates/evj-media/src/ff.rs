//! FFmpeg fallback for codecs Media Foundation cannot decode (ProRes, DNxHD, ...). CPU decode → BGRA.
use anyhow::{Context, Result};
use ffmpeg_next as ff;
use std::path::Path;

pub struct FfVideo {
    input: ff::format::context::Input,
    decoder: ff::decoder::Video,
    scaler: ff::software::scaling::Context,
    stream: usize,
    time_base: f64,
    eof_sent: bool,
    frame: ff::frame::Video,
    bgra: ff::frame::Video,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
}

// SAFETY: FfVideo is only ever used from the one decoder thread that owns it.
unsafe impl Send for FfVideo {}

impl FfVideo {
    pub fn open(path: &Path) -> Result<FfVideo> {
        ff::init()?;
        let input = ff::format::input(path).with_context(|| format!("FFmpeg cannot open {}", path.display()))?;
        let st = input.streams().best(ff::media::Type::Video).context("no video stream")?;
        let (stream, tb, rate) = (st.index(), st.time_base(), st.avg_frame_rate());
        let decoder = ff::codec::context::Context::from_parameters(st.parameters())?.decoder().video()?;
        let (w, h) = (decoder.width(), decoder.height());
        let scaler =
            ff::software::scaling::Context::get(decoder.format(), w, h, ff::format::Pixel::BGRA, w, h, ff::software::scaling::Flags::BILINEAR)?;
        let fps = if rate.denominator() > 0 && rate.numerator() > 0 { f64::from(rate) } else { 30.0 };
        let duration = (input.duration().max(0) as f64) / f64::from(ff::ffi::AV_TIME_BASE);
        Ok(FfVideo {
            input,
            decoder,
            scaler,
            stream,
            time_base: f64::from(tb),
            eof_sent: false,
            frame: ff::frame::Video::empty(),
            bgra: ff::frame::Video::empty(),
            width: w,
            height: h,
            fps,
            duration,
        })
    }

    pub fn next_frame(&mut self, out: &mut Vec<u8>) -> Result<Option<f64>> {
        loop {
            if self.decoder.receive_frame(&mut self.frame).is_ok() {
                self.scaler.run(&self.frame, &mut self.bgra)?;
                let (row, stride) = (self.width as usize * 4, self.bgra.stride(0));
                let data = self.bgra.data(0);
                out.clear();
                for y in 0..self.height as usize {
                    out.extend_from_slice(&data[y * stride..y * stride + row]);
                }
                let pts = self.frame.timestamp().unwrap_or(0) as f64 * self.time_base;
                return Ok(Some(pts));
            }
            if self.eof_sent {
                return Ok(None);
            }
            let next = self.input.packets().next().map(|(s, p)| (s.index(), p));
            match next {
                Some((idx, packet)) if idx == self.stream => self.decoder.send_packet(&packet)?,
                Some(_) => {}
                None => {
                    self.decoder.send_eof()?;
                    self.eof_sent = true;
                }
            }
        }
    }

    pub fn rewind(&mut self) -> Result<()> {
        self.input.seek(0, ..0)?;
        self.decoder.flush();
        self.eof_sent = false;
        Ok(())
    }
}
