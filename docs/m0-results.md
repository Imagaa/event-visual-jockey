# M0 Spike — Results (2026-09-28)

**Machine (floor spec):** AMD Ryzen 7 5700U, Radeon Vega 8 iGPU (driver 31.0.21923.11000), ~8 GB RAM, 60 Hz panel, Windows 11 Pro.
**Build:** `cargo build -p evj-app --release`, `evj --bench <s> <clips>`. Raw reports: `docs/bench/`.

## Risks answered

| Risk (PRD §8) | Verdict |
|---|---|
| egui ↔ D3D11 | `egui-directx11` 0.13 works on the shared device; UI cost 0.6–2 ms/frame. Keep. |
| Media Foundation hardware decode | Works on Vega 8: frames stay on the GPU (test `decodes_h264_with_hardware_device`: 30/30 GPU frames). |
| FFmpeg linking | Code written (`ff.rs`, feature `ffmpeg`); build blocked until LLVM install is approved (UAC). See Task 0.6. |

## Bug found and fixed during bench

**Symptom:** any H.264 clip dragged the render loop to 26–45 fps although decoders were never late.
**Evidence:** CPU phases < 2 ms, swapchain wait up to 23 ms → GPU-bound. 4K H.264 alone: 44.9 fps / 219 dropped.
**Root cause:** Media Foundation's advanced video processing (NV12 → RGB32 via the D3D11 video processor) is expensive on the iGPU.
**Fix:** decode to NV12 on the GPU path and convert YUV → RGB in our blit shader (BT.709 / BT.601, limited range; golden tests `nv12_bt709_limited_range`, `nv12_bt601_limited_range`). A second bug surfaced by that test — the constant buffer was never bound to the pixel shader — is fixed too.

## Numbers after the fix (30 s / 15 s runs, 60 Hz)

| Scenario | fps | p99 ms | dropped | notes |
|---|---|---|---|---|
| 4× HAP 1080p60 (Hap1, HapQ, Hap5, Hap1) | 59.7–59.9 | 17.2 | 2–5 | pump 3.7 ms (CPU upload) |
| 2× H.264 1080p60 | 60.0 | 17.2 | 0 | |
| 4K H.264 72 Mbps (real: *Indonesia Raya*) | 60.0 | 17.2 | 0 | was 44.9 fps before fix |
| Real set: 4K + 2× 1080p H.264 | 58.9 | 33.3 | 22 | was 33.8 fps before fix |
| HAP + real 1080p H.264 | 60.0 | 17.2 | 0 | |
| 2× HAP + 1× H.264 1080p30 | 60.0 | 17.2 | 0 | |
| 3× HAP + 1× H.264 1080p30 | 57.8 | — | 26 | |
| 4× HAP + 2× H.264 (30 or 60 fps) | 30 | 33.8 | 440 | GPU saturated — see below |

## Open issues carried into M1

1. **HAP upload cost on the render thread** (`UpdateSubresource`, ~1 ms per 1080p60 layer on Vega 8) — M1 task 1.3: decoder-side staging ring + GPU `CopyResource`, measure against this table.
2. **4 HAP + 2 H.264 saturates the iGPU** (swapchain wait 21 ms, UI 5 ms). Candidates, to be measured in M1: (a) upload fix above, (b) decode on a second D3D11 device with shared textures so MF work does not serialize with the render context. Not required by G2 (HAP-only) or NFR-PERF-1 (2× H.264, passes).
3. Clip fit mode (portrait / odd-size material) — M1 model (ruling in ledger).

## M1 engine bench (same laptop, engine thread + separate UI device + 1 output window)

| Scenario | fps | p99 ms | dropped (20 s) |
|---|---|---|---|
| 4× HAP 1080p60 composited (Alpha blend) | 60.0 | 17.0 | 7 |
| Real: 4K H.264 + 1080p H.264 + JPEG logo | 60.0 | 17.2 | 0 |

## M5: HAP upload path A/B (4× HAP 1080p60, full app, 15 s × 3 alternating runs)

| Upload | dropped | p99 ms |
|---|---|---|
| DYNAMIC + Map(WRITE_DISCARD) — kept | 4 / 5 / 3 | 17.3 / 18.9 / 17.0 |
| UpdateSubresource | 1 / 11 / 8 | 17.0 / 24.0 / 20.6 |

Measured while the laptop was also in normal use; DYNAMIC has the steadier p99.
