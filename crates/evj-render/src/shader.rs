use anyhow::{Result, anyhow};
use windows::Win32::Graphics::Direct3D::Fxc::{D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile};
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::core::{PCSTR, s};

fn blob_bytes(b: &ID3DBlob) -> &[u8] {
    unsafe { std::slice::from_raw_parts(b.GetBufferPointer() as *const u8, b.GetBufferSize()) }
}

/// Compiles HLSL with d3dcompiler_47 (ships with Windows 10+). Error text comes back in the Err.
pub fn compile(src: &str, entry: &str, target: &str) -> Result<Vec<u8>> {
    let entry = std::ffi::CString::new(entry)?;
    let target = std::ffi::CString::new(target)?;
    let (mut code, mut errors) = (None, None);
    let hr = unsafe {
        D3DCompile(
            src.as_ptr().cast(),
            src.len(),
            s!("evj"),
            None,
            None,
            PCSTR(entry.as_ptr().cast()),
            PCSTR(target.as_ptr().cast()),
            D3DCOMPILE_OPTIMIZATION_LEVEL3,
            0,
            &mut code,
            Some(&mut errors),
        )
    };
    let msg = errors.as_ref().map(|e| String::from_utf8_lossy(blob_bytes(e)).into_owned()).unwrap_or_default();
    let msg = msg.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    match (hr, code) {
        (Ok(()), Some(code)) => Ok(blob_bytes(&code).to_vec()),
        (r, _) if msg.is_empty() => Err(anyhow!("shader compile failed: {r:?}")),
        _ => Err(anyhow!("shader compile failed: {msg}")),
    }
}
