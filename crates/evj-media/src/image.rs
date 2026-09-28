//! Still images via WIC (PNG, JPEG, BMP, TIFF, GIF, ...) → top-down BGRA.
use anyhow::{Context, Result, ensure};
use std::path::Path;
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx};
use windows::core::{HSTRING, Interface, w};

pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "bmp", "gif", "tif", "tiff", "webp", "jfif"];

pub fn is_image(path: &Path) -> bool {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
}

/// Decodes an image, applies EXIF rotation and scales it down so the long side is at most `max_side`.
pub fn load_image(path: &Path, max_side: u32) -> Result<(u32, u32, Vec<u8>)> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let decoder = factory
            .CreateDecoderFromFilename(&HSTRING::from(path.as_os_str()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand)
            .with_context(|| format!("cannot decode image {}", path.display()))?;
        let frame = decoder.GetFrame(0)?;

        // ponytail: rotations only; mirrored EXIF orientations (2, 4, 5, 7) are rare from cameras.
        let orientation = frame
            .GetMetadataQueryReader()
            .ok()
            .and_then(|q| {
                let mut v = PROPVARIANT::default();
                q.GetMetadataByName(w!("/app1/ifd/{ushort=274}"), &mut v).ok()?;
                u16::try_from(&v).ok()
            })
            .unwrap_or(1);
        let mut src: IWICBitmapSource = frame.cast()?;
        let transform = match orientation {
            3 => WICBitmapTransformRotate180,
            6 => WICBitmapTransformRotate90,
            8 => WICBitmapTransformRotate270,
            _ => WICBitmapTransformRotate0,
        };
        if transform != WICBitmapTransformRotate0 {
            let r = factory.CreateBitmapFlipRotator()?;
            r.Initialize(&src, transform)?;
            src = r.cast()?;
        }

        let (mut w, mut h) = (0u32, 0u32);
        src.GetSize(&mut w, &mut h)?;
        ensure!(w > 0 && h > 0, "empty image");
        if w.max(h) > max_side {
            let k = max_side as f64 / w.max(h) as f64;
            (w, h) = (((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1));
            let s = factory.CreateBitmapScaler()?;
            s.Initialize(&src, w, h, WICBitmapInterpolationModeHighQualityCubic)?;
            src = s.cast()?;
        }

        let conv = factory.CreateFormatConverter()?;
        conv.Initialize(&src, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom)?;
        let mut px = vec![0u8; (w * h * 4) as usize];
        conv.CopyPixels(std::ptr::null(), w * 4, &mut px)?;
        Ok((w, h, px))
    }
}
