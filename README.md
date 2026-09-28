<p align="center">
  <img src="assets/evj-logo.png" alt="EVJ — Event Visual Jockey" width="320">
</p>

<h1 align="center">EVJ — Event Visual Jockey</h1>

<p align="center">
  <b>One native Windows app for the whole show: video, visuals, presentations and sound.</b><br>
  A Resolume-style clip grid, live effects and transitions, multi-screen outputs, PowerPoint / PDF slides and audio —
  built for event operators who need a show that simply does not fall over.
</p>

<p align="center">
  <a href="https://github.com/Imagaa/event-visual-jockey/releases/latest"><b>⬇ Download the installer</b></a> ·
  <a href="docs/USER-GUIDE.md">User guide (Bahasa Indonesia)</a> ·
  <a href="docs/TESTING-CHECKLIST.md">Test checklist</a>
</p>

---

## Why EVJ

At a real event the operator juggles a VJ tool for the LED screen, PowerPoint for the speakers, a media player for the walk-in music and a lot of Alt+Tab. EVJ puts all of it in **one window, on one render engine**, so nothing else has to open during the show:

- **Live-safe by design** — PROGRAM / PREVIEW like a broadcast switcher, a 🔒 lock for the live part, BLACKOUT and PANIC always one key away, autosave and crash recovery.
- **Fast on ordinary laptops** — native Rust + Direct3D 11, HAP GPU decoding, hardware H.264, tested on a Ryzen 7 5700U with integrated Vega 8 graphics.
- **Presentations without PowerPoint on stage** — PPTX / PDF are imported once (slides, click animations, embedded video, speaker notes) and then play from the grid with clicker and laser-pointer support.
- **Offline, no account, no subscription.**

## Features

### Clip grid and scenes
- Resolume-style grid: rows are **layers**, columns are **scenes**; several decks per show.
- Drag files or whole folders from Explorer; thumbnails and durations on every slot.
- **1 click = PREVIEW, double-click = PROGRAM**, TAKE (Enter) to put the preview live.
- **Scenes**: click a column header to preview the whole column blended, double-click to go live, rename it ("Opening", "Keynote" …).
- **Sequences**: Shift+click slots → *Make sequence* → they play one after another **without a gap**, stills timed, loop the whole sequence, ⏭ Next.
- Keyboard mapping for slots, scenes and layers; ✔ marks for clips that have been on air.

### Per-clip timeline
- Seek bar with **start / end markers** (I / O keys), A–B loops, waveform of the clip's sound.
- Seek the Preview or — deliberately — the Program.
- **Attach audio** to a video or photo (Replace or Mix, own volume): a photo with music lasts as long as the music.

### Playback
- **HAP / HAP Q / HAP Alpha** (GPU-decoded, instant seek), H.264 / HEVC via Media Foundation hardware decoding, ProRes / DNxHD and more through the FFmpeg fallback.
- Images (EXIF rotation), audio-only clips (MP3, WAV, AAC …).
- Loop, ping-pong, once, reverse, speed, BPM sync. **Convert to HAP** built in.

### Effects, BPM and transitions
- 16 built-in GPU effects on clips, layers and the composition; every parameter can follow an **LFO** synced to the BPM (tap tempo, nudge, resync).
- 10 blend modes, opacity, bypass / solo per layer.
- 12 transitions (crossfade, wipes, dip to black …) with a Transition Manager: presets, durations in seconds or beats, easing, favourites.
- Your own effects: drop an `.hlsl` file into the `effects` folder — hot-reloaded.

### Outputs
- Any number of outputs: full screen on any monitor or as a window.
- An output can show the composition or **a single layer** (e.g. slides on the side screens).
- **Slices** for LED walls and multi-screen setups, numbered test patterns (Identify).
- Monitors unplugged during the show are reopened automatically when they come back.

### Presentations
- PPTX (through a hidden PowerPoint, or LibreOffice) and PDF import into a grid slot.
- Per-click animations and embedded videos keep working; speaker notes are imported.
- Presenter clicker support (works even when another app is in front), laser dot / spotlight pointer, **Presenter view** window with current / next slide, notes, timer and clock.

### Audio
- Every layer has a volume fader in dB (type `-6` or `75%`), mute and a peak meter; master with CLIP lamp.
- **Separate Preview audio**: the cued clip in the operator's headphones — another device or other channels (e.g. 3-4) of the same interface — never on the PA by accident.
- Audio device unplugged → automatic fallback; PANIC fades all sound out in one second.

### Operator comfort and safety
- **Modular panels**: every part of the window is a dockable tab (drag, split, float, close) with View menu, *Live* / *Setup* layouts, remembered per laptop.
- **LOCK LIVE** keeps show control but blocks anything destructive (adding / removing media, effects editing, layout changes).
- Countdown of every layer on air (red and blinking in the last 10 s), dB meters, frame-time monitor.
- App-wide, customisable shortcuts; welcome screen with recent shows; autosave every 60 s + recovery after a crash; unsaved-changes prompts.

## Getting started

1. **Download** `EVJ-Setup-0.1.0.exe` from the [latest release](https://github.com/Imagaa/event-visual-jockey/releases/latest) and run it (Windows 10 / 11, 64-bit). No admin rights needed.
2. Start EVJ → **New show**.
3. Drag videos, images, music or a PowerPoint / PDF onto the grid.
4. Click a clip to see it on **PREVIEW**, press **Enter** (TAKE) or double-click to put it on **PROGRAM**.
5. **Outputs…** → pick the projector / LED screen → **Go live**.
6. During the show: 🔒 **Lock**, **B** = BLACKOUT, **F12** = PANIC.

The full guide (Bahasa Indonesia) is in [docs/USER-GUIDE.md](docs/USER-GUIDE.md); [docs/TESTING-CHECKLIST.md](docs/TESTING-CHECKLIST.md) lists every check to run before an event.

### Default shortcuts

| Key | Action |
|---|---|
| Enter / Shift+Enter | TAKE (Preview → Program) / clear Program |
| Space / Shift+Space | Preview play-pause / back to start |
| B / F12 | BLACKOUT / PANIC |
| I / O / N | Timeline start / end, next clip in a sequence |
| Ctrl+S, Ctrl+O, Ctrl+N | Save, open, new show |
| Ctrl+Shift+L | Lock live |
| T | Tap tempo |
| F1 / F2 / F3 | Shortcuts / Performance / Outputs panels |
| PageDown / PageUp | Next / previous slide (clickers, also in the background) |

All of them can be changed in **Shortcuts** (F1).

## System requirements

- Windows 10 or 11, 64-bit, a Direct3D 11 GPU (integrated graphics is fine; HAP clips are recommended on iGPUs).
- Optional: Microsoft PowerPoint (best PPTX import with animations) or LibreOffice (static slides). PDF needs nothing extra.

## Built with

| | |
|---|---|
| Language | **Rust** (2024 edition) — plus **HLSL** for effects and transitions |
| Graphics | Direct3D 11 (`windows` crate), flip-model swap chains, shared keyed-mutex textures |
| UI | egui + egui_dock, winit |
| Video | HAP (own decoder), Media Foundation (hardware H.264 / HEVC), FFmpeg (LGPL, fallback + Convert to HAP) |
| Audio | WASAPI through cpal, lock-free mixer (rtrb) |
| Presentations | Windows.Data.Pdf, PowerPoint automation / LibreOffice in a separate importer process |
| Installer | Inno Setup |
| App UI language | English (the user guide is in Bahasa Indonesia) |

## Building from source

```powershell
cargo build --release          # target\release\evj.exe + evj-import.exe
cargo test                     # GPU tests run on WARP (software Direct3D 11)
.\tools\build-installer.ps1    # dist\EVJ-Setup-<version>.exe (needs Inno Setup 6)
```

The FFmpeg fallback decoder (`--features evj-app/ffmpeg`) needs LLVM and the FFmpeg LGPL shared build: `tools\setup-ffmpeg-dev.ps1`.

| Crate | Role |
|---|---|
| `evj-core` | Show model, file I/O, keymap, tempo / LFO, effects metadata, transitions, outputs, slide decks, sequences |
| `evj-media` | MOV parser + HAP decoder, Media Foundation video / audio, images, FFmpeg fallback, Convert to HAP |
| `evj-render` | D3D11 device, textures, compositor (10 blend modes), effect runner, swap chains |
| `evj-audio` | Real-time mixer, WASAPI outputs (device + channel routes) |
| `evj-present` | PDF and PPTX import (`evj-import.exe`) |
| `evj-engine` | Render thread: layers, sequences, seek, transitions, effects, outputs, presentations, Program / Preview audio |
| `evj-app` | `evj.exe`: dockable operator UI, output windows, presenter view, clicker, `--bench`, `--soak` |

Command line: `evj [show.vjproj | clip …]`, `evj --bench SECONDS show.vjproj`, `evj --soak HOURS show.vjproj`, `evj --output MONITOR …`.

## Status

**v0.1.0 — first public pre-release.** Built and automatically tested (260+ tests), used on a real event; the manual checklist in `docs/TESTING-CHECKLIST.md` is the sign-off before your own show. Feedback and issues are welcome.

FFmpeg is used under the LGPL; its license is installed next to EVJ.
