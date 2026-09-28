# PRD — EVJ (Event Visual Jockey), Show-Control Visual Native Windows

> Nama kerja: **EVJ**. Versi PRD: **v2** (2026-09-28). UI berbahasa **English**.
> v2 menambahkan: presentasi (PPT/PDF), audio, installer, checklist testing, dan revisi stack decoder + hardware referensi.

## 1. Context

User adalah VJ/operator visual di event (konser, acara formal, acara budaya — mis. sambutan dengan slide PPT dan pertunjukan tari dengan musik MP3). Saat ini memakai Resolume Avenue + aplikasi lain (PowerPoint, audio player). Masalah: lisensi mahal, berat di laptop spek menengah, tidak bisa dikustomisasi, dan harus berpindah-pindah aplikasi saat show. Tujuan: **satu aplikasi native Windows** untuk visual (clip/efek/transisi), **materi presentasi**, dan **audio**, dengan prioritas **stabilitas** dan **optimasi hardware**. Seluruh kode dikerjakan oleh Claude; user adalah pengguna & product owner.

## 2. Tujuan & kriteria sukses

| # | Kriteria sukses MVP |
|---|---|
| G1 | Show 4 jam tanpa crash, memori datar (tidak bertambah). |
| G2 | 4 layer HAP 1080p (2 efek/layer) + 2 output stabil 60 fps; p99 frame time < 16,6 ms — di laptop **batas bawah** dan **target**. |
| G3 | 0 frame drop di output saat trigger clip / ganti transisi / ganti slide / UI sibuk. |
| G4 | Workflow show (grid, layer, efek, BPM, transisi, multi-layar, slide PPT/PDF, audio) bisa dilakukan tanpa Resolume, PowerPoint, atau audio player terbuka. |
| G5 | Terpasang di laptop lain lewat **installer** tunggal; tersedia **checklist testing** manual. |

**Hardware:**
- **Batas bawah (wajib lancar):** Ryzen 7 5700U + Radeon Vega 8 iGPU, RAM 8 GB (laptop development).
- **Target:** Core i5 / Ryzen 5 + RTX 3050/4050, RAM 16 GB.
- Windows 10/11 64-bit.

## 3. Pengguna

1. **Operator** (user): mouse + keyboard, menyiapkan & menjalankan show.
2. **Pembicara**: mengganti slide dengan presenter clicker & pointer, melihat presenter view.

## 4. Scope MVP — Functional Requirements

Prioritas: **M** = Must, **S** = Should.

### 4.1 Clip grid & layer
| ID | Requirement | P |
|---|---|---|
| F-GRID-1 | Grid slot layer × kolom; drag-drop file dari Explorer ke slot; thumbnail otomatis (async). | M |
| F-GRID-2 | Trigger clip via klik & shortcut keyboard; trigger satu kolom sekaligus; clear layer. | M |
| F-GRID-3 | Beberapa **Deck** (halaman grid) dalam satu project. | M |
| F-GRID-4 | Properti clip: mode play (loop / ping-pong / once), speed (termasuk reverse), in/out point, BPM-sync (panjang clip = N beat). | M |
| F-GRID-5 | Pre-warm clip (frame pertama siap di GPU) untuk trigger instan. | S |
| F-LAYER-1 | Per layer: opacity, bypass, solo, blend mode, rantai efek, preset transisi default. | M |
| F-LAYER-2 | Blend mode: Alpha, Add, Screen, Multiply, Overlay, Difference, Lighten, Darken, Subtract, Luma key. | M |
| F-KEY-1 | Mapping keyboard → slot / kolom / layer clear / tap tempo; disimpan di project. | M |

### 4.2 Media
| ID | Requirement | P |
|---|---|---|
| F-MEDIA-1 | **HAP / HAP Alpha / HAP Q** (.mov) jalur utama: parser MOV + decoder HAP sendiri, upload langsung sebagai tekstur BC1/BC3 (HAP Q: BC3 YCoCg → RGB di shader), random access tiap frame. | M |
| F-MEDIA-2 | H.264 / H.265 / VP9 dll. via **Media Foundation** dengan hardware decode ke tekstur D3D11 (copy GPU→GPU). Tanpa hardware: MF software decode otomatis. | M |
| F-MEDIA-3 | **Fallback FFmpeg** (ProRes, codec lain yang tidak didukung MF): decode CPU → upload; badge "heavy". | M |
| F-MEDIA-4 | Gambar PNG / JPG / BMP sebagai clip statis. | M |
| F-MEDIA-5 | **Convert to HAP** bawaan (batch, background, progress; ukuran dipad ke kelipatan 4). | M |
| F-MEDIA-6 | File rusak / tidak didukung → slot ditandai error, aplikasi tetap jalan. | M |

### 4.3 Efek & BPM
| ID | Requirement | P |
|---|---|---|
| F-FX-1 | ±15 efek HLSL bawaan: Transform, Brightness/Contrast, Hue/Sat, Colorize, Invert, Blur, Kaleidoscope, Mirror, RGB Shift, Pixelate, Posterize, Edge, Strobe, Wave, Feedback/Trails. | M |
| F-FX-2 | Efek di clip, layer, composition; urutan bisa diubah; bypass per efek. | M |
| F-FX-3 | Parameter numerik bisa dianimasikan **LFO sync BPM** (sine / saw / square / random; 1/4–16 beat) atau manual. | M |
| F-FX-4 | **Efek custom** `.hlsl` + metadata di folder `effects/`, auto-load & **hot-reload**; error compile ditampilkan, tidak crash. | M |
| F-BPM-1 | BPM manual, **tap tempo**, nudge ±, resync ke beat 1, indikator beat. | M |

### 4.4 Transisi & Transition Manager
| ID | Requirement | P |
|---|---|---|
| F-TR-1 | Transisi = shader (A, B, progress, params). ±12 bawaan: Cut, Crossfade, Additive, Wipe, Slide, Zoom, Luma fade, Dissolve, Blur fade, Flash, Pixelate, Radial. | M |
| F-TR-2 | **Transition Manager**: library dengan preview animasi, favorit, parameter. | M |
| F-TR-3 | **Preset transisi** = transisi + durasi (detik atau beat) + easing + params. | M |
| F-TR-4 | Preset default per layer, override per clip, **"next transition" bar**. Transisi juga dipakai antar slide. | M |
| F-TR-5 | Import transisi `.hlsl` custom. | M |

### 4.5 Multi-output
| ID | Requirement | P |
|---|---|---|
| F-OUT-1 | Output Manager: deteksi monitor; output = fullscreen di monitor tertentu atau window. | M |
| F-OUT-2 | Sumber output: composition, layer tertentu, atau **presentasi** (speaker visual output). | M |
| F-OUT-3 | Beberapa **slice** per output (crop input → area output, tanpa warp). | M |
| F-OUT-4 | Identify display, test pattern, preview output di UI. | M |
| F-OUT-5 | Monitor dicabut → output pause; dicolok lagi → otomatis lanjut; tanpa crash. | M |

### 4.6 Presentasi (Materi) — BARU v2
| ID | Requirement | P |
|---|---|---|
| F-PRES-1 | **PDF** di-render native (`Windows.Data.Pdf`) ke gambar resolusi output saat import. | M |
| F-PRES-2 | **PPTX** di-import via **PowerPoint tersembunyi** (COM automation, tidak terlihat user): gambar per slide + **video animasi per slide** dengan titik potong **per klik** (dibaca dari timeline animasi). Media tersimpan di project; saat show **tidak butuh Office**. | M |
| F-PRES-3 | Fallback tanpa PowerPoint: import statis via **LibreOffice** (headless) bila terpasang; bila tidak ada keduanya → pesan jelas. | M |
| F-PRES-4 | **Slide clip**: deck materi sebagai satu clip (di grid atau layer khusus); next/prev step; lompat ke slide; transisi antar slide memakai F-TR. | M |
| F-PRES-5 | **Materi checklist**: daftar deck materi untuk acara (urutan sambutan), ditandai aktif → masuk **mode materi** yang dirutekan ke output speaker visual. | M |
| F-PRES-6 | **Clicker global**: PageUp/PageDown/panah/B (black) dari presenter clicker diterima saat mode materi aktif walau aplikasi tidak fokus (Raw Input, bukan hook). | M |
| F-PRES-7 | **Digital pointer/spotlight**: titik laser / lingkaran sorot dirender di output, digerakkan pointer gyro atau mouse; toggle via tombol/hotkey. | M |
| F-PRES-8 | **Presenter view**: window di monitor pilihan: slide sekarang, berikutnya, catatan speaker (dari PPTX), timer & jam. | M |

### 4.7 Audio — BARU v2
| ID | Requirement | P |
|---|---|---|
| F-AUD-1 | **Audio ikut clip video**: track audio video diputar sinkron dengan clip; mengikuti trigger, stop, dan opacity/fade transisi (opsional per clip). | M |
| F-AUD-2 | **Audio layer dedicated**: clip audio (MP3 / WAV / AAC / M4A / FLAC) di layer audio grid, independen dari visual; mode play once/loop; fade in/out. | M |
| F-AUD-3 | **Mixer**: volume & mute per layer (visual & audio), master volume, fade out cepat (panic fade), meter level. | M |
| F-AUD-4 | Pilih **output device** (soundcard / HDMI / USB) via WASAPI shared; device dicabut → fallback ke default tanpa crash. | M |
| F-AUD-5 | Tidak ada glitch/klik audio saat trigger, ganti clip, atau UI sibuk (buffer ≥ 20 ms, mixing di thread audio sendiri). | M |

### 4.8 Project & sistem
| ID | Requirement | P |
|---|---|---|
| F-PRJ-1 | Project `.vjproj` (JSON, path media relatif) save / open / save as. Hasil import PPT/PDF disimpan di folder project. | M |
| F-PRJ-2 | Autosave tiap 60 dtk, tulis atomik; recovery saat start setelah crash. | M |
| F-PRJ-3 | Media hilang saat open → daftar + relink. | M |
| F-SYS-1 | **Performance HUD**: fps, grafik frame time, status decode per layer, VRAM, audio underrun. | M |
| F-SYS-2 | Konfirmasi keluar saat output live. | M |

### 4.9 Distribusi & QA — BARU v2
| ID | Requirement | P |
|---|---|---|
| F-DIST-1 | **Installer** `EVJ-Setup-x.y.z.exe` (Inno Setup): app + DLL FFmpeg + efek/transisi bawaan + shortcut + uninstaller; CRT statis (tanpa VC++ redist). | M |
| F-DIST-2 | Mode **`--bench`**: skenario G2 otomatis → laporan fps / p99 / CPU / VRAM ke file. | M |
| F-DIST-3 | **Checklist testing manual** (`docs/TESTING-CHECKLIST.md`) untuk dicoba user di laptop lain. | M |

## 5. Non-goals MVP → Roadmap

1. MIDI mapping & OSC
2. Audio-reactive (FFT → parameter)
3. Trigger quantize & crossfader A/B
4. Spout / NDI
5. Ableton Link
6. Input live (webcam / capture card), generator (solid, gradient, teks)
7. Warp / projection mapping
8. Import efek ISF
9. Window capture sebagai source

Bukan tujuan: cross-platform, multi-user, lisensi/penjualan, renderer PPTX buatan sendiri.

## 6. Non-Functional Requirements

| ID | Requirement |
|---|---|
| NFR-PERF-1 | G2 & G3 di kedua kelas hardware. 2 layer H.264 1080p simultan via hardware decode. |
| NFR-PERF-2 | CPU < 40%, VRAM < 2 GB (iGPU: shared < 1,5 GB) pada skenario G2; startup < 3 dtk. |
| NFR-PERF-3 | Latensi trigger: ≤ 1 frame (warm), ≤ 3 frame (HAP cold). Latensi clicker → slide ≤ 2 frame. |
| NFR-STAB-1 | Error/panic decoder terisolasi ke clip; tidak ada `unwrap` pada input eksternal (file, monitor, driver, device audio, PowerPoint). |
| NFR-STAB-2 | **GPU device lost / TDR**: device & resource dibuat ulang, show lanjut < 2 dtk. |
| NFR-STAB-3 | Crash log + minidump ke `%APPDATA%\EVJ\crash`. |
| NFR-HW-1 | Paksa GPU dedicated di laptop hybrid (`NvOptimusEnablement` / `AmdPowerXpressRequestHighPerformance` + `DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE`); peringatan bila output tersambung ke adapter lain. |
| NFR-HW-2 | Flip-model swapchain + waitable object; vsync di output utama. |
| NFR-HW-3 | Tanpa alokasi GPU per frame (pool tekstur); ring buffer decoder berbatas. |
| NFR-HW-4 | Engine thread prioritas MMCSS, EcoQoS off, timer resolusi tinggi, cegah sleep saat output live. |

## 7. Arsitektur

**Stack:** Rust (edition 2024) · Direct3D 11 / DXGI via `windows` 0.62 · **Media Foundation** (decode video/audio, hardware) · **parser MOV + decoder HAP sendiri** (`snap`) · **FFmpeg** (`ffmpeg-next`, fallback decode; `ffmpeg.exe` untuk Convert to HAP) · `egui` 0.35 + `egui-directx11` + `egui-winit` + `winit` 0.30 · HLSL via `D3DCompile` · WASAPI (`cpal`) untuk audio · WinRT `Windows.Data.Pdf` · PowerPoint COM / LibreOffice headless (import) · `serde_json` · `tracing` · `crash-handler`/`minidumper` · Inno Setup (installer).

**Thread:**
```
UI thread (egui)  ──Command──▶  Engine/Render thread  ◀──frames──  Decoder workers (1 per clip aktif)
      ▲                              │   │                          │
      └────── Snapshot state ◀───────┘   └──audio cmds──▶ Audio thread (WASAPI mix)
                                                          I/O thread (thumbnail, autosave, convert, import materi)
```
- Engine thread memiliki device D3D11, detak mengikuti vsync output utama.
- UI hanya kirim `Command` dan baca `Snapshot`. UI lag ≠ output lag.
- Audio thread memiliki mixer; clock clip ber-audio mengikuti posisi audio.
- Decoder telat → engine mengulang frame terakhir.

**Pemilihan decoder per file:** HAP (fourcc Hap1/Hap5/HapY) → decoder HAP; lainnya → Media Foundation; gagal → FFmpeg; gagal → error slot.

**Crate:** `evj-core` (model, clock/BPM, LFO, serialisasi — murni), `evj-media` (MOV/HAP, MF, FFmpeg, audio decode), `evj-render` (device, shader, tekstur, output), `evj-audio` (mixer, device), `evj-present` (import PDF/PPTX, slide deck), `evj-app` (UI, wiring, main).

## 8. Risiko utama & mitigasi

| Risiko | Mitigasi |
|---|---|
| egui ↔ D3D11 | `egui-directx11` 0.13 (dicek di M0); fallback renderer sendiri. |
| MF hardware decode di iGPU/driver tertentu | MF software fallback otomatis → FFmpeg. |
| FFmpeg linking (bindgen/libclang) | Di balik cargo feature; build utama tidak bergantung. |
| PowerPoint COM rapuh (dialog, versi Office) | Proses import terisolasi dengan timeout; fallback LibreOffice; import di laptop persiapan, bukan saat show. |
| Laptop hybrid: output di iGPU | Deteksi adapter + peringatan. |
| Driver TDR | NFR-STAB-2. |

## 9. Milestone

| M | Isi | Selesai bila |
|---|---|---|
| M0 Spike | Render D3D11 + HAP + MF + FFmpeg fallback + egui + `--bench` | Klip HAP & H.264 1080p mulus; angka bench laptop ini tercatat |
| M1 Engine core | Model data, thread Command/Snapshot, composition, layer, blend, playback mode, 1 output | 4 layer HAP 60 fps di bench |
| M2 UI & project | Grid, deck, drag-drop, thumbnail, properti, keymap, save/open/autosave/relink | Bisa menyusun & menyimpan set |
| M3 Efek & BPM | Efek bawaan, rantai, LFO, tap tempo, efek custom hot-reload | F-FX-*, F-BPM-1 |
| M4 Transisi | Transisi bawaan, manager, preset, next bar | F-TR-* |
| M5 Output & hardening | Multi-output, slice, hotplug, device-lost, HUD, Convert to HAP, crash dump | F-OUT-*, NFR-STAB-* |
| M6 Audio | Decode audio, mixer, audio layer, audio clip video, device select | F-AUD-* |
| M7 Presentasi | PDF, PPTX (PowerPoint/LibreOffice), slide clip, materi checklist, clicker, pointer, presenter view | F-PRES-* |
| M8 Rilis | Soak 4 jam, bench, installer, checklist testing | G1–G5 |

## 10. Verifikasi

- Unit test (`cargo test`) untuk logika murni: MOV/HAP parser, clock BPM, LFO, easing transisi, mixer, segmentasi klik PPT, serialisasi.
- Golden image test render di device **WARP** (headless).
- Soak test 4 jam otomatis; `--bench`; checklist manual (F-DIST-3).
