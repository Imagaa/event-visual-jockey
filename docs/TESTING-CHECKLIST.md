# EVJ — Checklist Testing Manual

Untuk laptop RTX (dan ulangi bagian yang ditandai ★ di laptop Ryzen 5700U). Centang `[x]` saat lulus; bila gagal, catat apa yang terlihat dan lampirkan `%APPDATA%\EVJ\evj.log` (dan isi folder `crash` bila ada).

Siapkan folder media acara (mis. `Materi Event Juwangi`), minimal 1 PPTX, 1 PDF, 1 MP3, dan beberapa video 1080p/4K.

## A. Instalasi

- [ ] A1 Jalankan `EVJ-Setup-0.1.0.exe` → install selesai tanpa error (tidak perlu VC++ redist).
- [ ] A2 Shortcut Start Menu (dan desktop bila dipilih) membuka EVJ; ikon logo EVJ tampil di taskbar.
- [ ] A3 Double-click file `.vjproj` membuka EVJ dengan show itu (bila opsi asosiasi dipilih).
- [ ] A4 Bar atas kanan menunjukkan GPU **NVIDIA RTX …** (bukan iGPU). Bila tertulis iGPU: Windows Settings → Display → Graphics → EVJ → High performance, lalu catat.
- [ ] A5 Uninstall dari Settings → Apps menghapus program; show dan media tidak tersentuh.

## B. Media & grid ★

- [ ] B1 Tarik 10+ file campuran (video, jpg, mp3) ke grid → thumbnail muncul, mp3 bergambar gelombang.
- [ ] B0 Buka EVJ tanpa file → Welcome screen tampil (New show / Open show / Recent shows). Setelah Save, show muncul di Recent dan File → Open Recent.
- [ ] B1b 1 klik sebuah clip → tampil di **PREVIEW** (hijau) tetapi tidak di PROGRAM / output; TAKE ▶ atau Enter menayangkannya; double-click langsung ke PROGRAM.
- [ ] B2 Klik video 4K H.264 → main mulus, fps di bar atas ≥ 59.
- [ ] B3 Foto portrait dari HP tampil tegak (rotasi EXIF).
- [ ] B4 File rusak / bukan media (mis. `.txt` diganti nama `.mp4`) → slot merah + pesan error; aplikasi tetap jalan.
- [ ] B5 Klik kanan video → Convert to HAP → progres di status bar → slot memakai `.hap.mov`; Speed negatif, PingPong, In/out bisa dipakai.
- [ ] B6 Double-click header Scene mengganti semua layer; ✖ mengosongkan layer.
- [ ] B7 Map keys: bind tombol ke slot, kolom, clear, TAP → berfungsi setelah Map keys dimatikan; tersimpan setelah Save + Open.
- [ ] B8 Save, tutup, Open lagi → semua slot, efek, transisi, materi kembali. Pindahkan folder show+media ke drive lain → Open tetap menemukan media.
- [ ] B9 Rename satu file media → Open → jendela Missing media → Relink from folder… memperbaikinya.
- [ ] B10 Matikan paksa EVJ (Task Manager) setelah mengubah show → buka lagi → ditawarkan Recover → isi kembali.
- [ ] B11 Video ProRes / DNxHD (.mov dari editor) → thumbnail tampil dengan label oranye **HEAVY** dan clip bisa diputar (decode FFmpeg). Convert to HAP menghilangkan label.
- [ ] B12 Bingkai slot: merah = tayang, hijau = di Preview. Space / Shift+Space mengontrol Preview; Shift+Enter mengosongkan Program.
- [ ] B13 Di bawah PROGRAM tiap layer tampil sisa waktu; < 10 detik merah berkedip; slot menampilkan durasi; clip loop bertanda LOOP.
- [ ] B14 BLACKOUT (B) fade ke hitam 0,5 dtk di semua output, tekan lagi kembali. PANIC (F12) kosongkan layer + putar panic media + suara hilang. Tanpa panic media → hitam, tidak crash.
- [ ] B15 🔒 Lock: drag-drop, Remove, + Layer/Column/Deck, efek, New/Open diblokir; trigger/TAKE/volume/blackout tetap; tahan 1 dtk untuk unlock.
- [ ] B16 F1 Shortcuts: ubah Blackout ke Ctrl+B, tabrakan ditandai oranye, tetap tersimpan setelah EVJ dibuka ulang; mengetik nama layer tidak memicu shortcut.
- [ ] B17 Ketik "-6" di volume dan "50%" di opacity; double-click fader = 0 dB; meter dB bergerak; CLIP menyala saat 0 dBFS dan mati saat diklik.
- [ ] B18 Autosave: ubah show yang sudah di-Save, tunggu 60 dtk → "✓ Saved hh:mm"; tutup / New / Open dengan perubahan → pilihan Save / tanpa simpan / Cancel.
- [ ] B19 Jendela pilih file (Add clips, Open, Save As) tetap di depan EVJ; monitor PROGRAM tetap bergerak selama jendela itu terbuka.
- [ ] B20 Welcome: kartu Recent dengan thumbnail & "saved … ago", "Resume last show", Import media folder, Import presentation.
- [ ] B21 Timeline: klik clip HAP dan clip H.264 → geser di bar → Preview melompat (juga saat di-pause, tetap pause). Clip tayang + **Seek: Program** → layar penonton ikut; default kembali ke Preview.
- [ ] B22 Start/end: geser penanda kuning, lalu I / O di posisi Preview; Once berhenti di end, ⟲ Loop mengulang A–B (HAP mulus, H.264 lompatan kecil); Reset kembali ke seluruh clip; sisa waktu Program menghitung sampai end.
- [ ] B23 Gelombang suara muncul di bawah bar untuk video bersuara, MP3, dan audio tempelan.
- [ ] B24 Attach audio ke foto (Replace) → foto tayang selama lagu, hitung mundur sesuai lagu; ke video bersuara: Replace = hanya lagu, Mix = keduanya; volume tempelan berfungsi; file lagu dihapus → muncul di Missing media, gambar tetap tayang.
- [ ] B25a Layer chain: Shift+klik 3 slot di satu kolom → Chain layers; langkah 2 *after 3 s* + Overlay, langkah 3 *when the one before ends* + Replace → trigger langkah 1: layer 2 muncul 3 dtk kemudian di atas layer 1, layer 3 menggantikan layer 2 saat selesai; ■ Stop berhenti di layer 3; ⟲ Loop mengosongkan dan mulai lagi; ⏭ / N lompat. Save + Open → tetap.
- [ ] B25b Scene chain: Shift+klik 3 header → Chain scenes → trigger scene 1: berganti saat clip terpanjang selesai; ■ Stop di scene 3, ⟲ Loop kembali ke scene 1; scene berisi clip A–B loop menunggu ⏭; scene berisi layer chain menunggu chain itu selesai. CHAIN n/m tampil di bawah PROGRAM.
- [ ] B29 Buka show lama (sebelum versi ini): tampilan Program sama persis, layer paling atas sekarang "Layer 1" di baris teratas; shortcut, panic media dan output "Layer N" tetap menunjuk layer yang sama; sequence lama menjadi scene chain.
- [ ] B30 ⏶ / ⏷ pada layer yang sedang tayang: clip tidak restart, tally merah ikut pindah, urutan tumpukan di Program berubah.
- [ ] B31 Delete layer / Delete scene (klik kanan) → konfirmasi → hilang di semua deck; layer lain tetap tayang; terkunci saat Lock.
- [ ] B33 Chroma Key: video green screen di layer 1 di atas background di layer 2 → Add effect Chroma Key → Pick → klik hijau di Preview → hijau (termasuk bayangan) hilang, background terlihat; Pick di Program saat tayang juga bisa; logo berlatar putih → latar hilang, teks abu-abu tetap; spill mengurangi tepi hijau; Esc membatalkan Pick; terkunci saat Lock.
- [ ] B32 Gambar: pill durasi 0:05 di slot, Image duration diubah → hitung mundur Program/Preview dengan milidetik dan progress bar; Once berhenti di 0 dengan gambar tetap tampil; seek di timeline gambar.
- [ ] B26 Scene: 1 klik header → Preview menampilkan gabungan layer (opacity layer terlihat), Program tidak berubah; double-click / TAKE → tayang; Rename tersimpan setelah Save.
- [ ] B27 Tarik PPTX dan PDF ke slot → "Importing…" → slot jadi presentasi; double-click → slide tayang; PageDown/clicker, pointer, Presenter view dari panel Timeline; slot bertanda ✔; file PPTX rusak → "Import failed" + Retry import.
- [ ] B28 Buka show lama yang punya daftar Materi → presentasi pindah ke grid (layer presentasi, kolom kosong), yang sudah selesai bertanda ✔; tidak ada lagi tombol/menu Materi.

## C. Efek, BPM, transisi ★

- [ ] C1 Tambah 3 efek ke clip, 2 ke layer, 1 ke composition → gambar berubah, fps tetap ≥ 59.
- [ ] C2 LFO pada parameter → bergerak sesuai BPM; TAP 4 kali mengubah BPM.
- [ ] C3 Next: Crossfade / Wipe / Dip to Black → transisi terlihat saat ganti clip; durasi beat mengikuti BPM.
- [ ] C4 Transition Manager: pratinjau animasi berjalan; buat preset baru, jadikan default layer, override di clip.
- [ ] C5 Salin satu `.hlsl` dari `effects\README.md` ke folder `effects` di samping `evj.exe` → muncul di daftar dalam ±1 detik; sengaja dibuat error → pesan error tampil, aplikasi tetap jalan.

## D. Output & multi-monitor

- [ ] D1 Sambungkan monitor/proyektor HDMI → Outputs… → pilih monitor → Go live → fullscreen di monitor itu.
- [ ] D2 Identify → nomor output dan test pattern tampil di tiap layar.
- [ ] D3 Output kedua dengan sumber **Layer** (layer slide) → hanya slide yang tampil di layar itu.
- [ ] D4 Slice: potong setengah kiri gambar ke layar penuh → benar, tanpa garis tepi.
- [ ] D5 Cabut HDMI saat live → EVJ tidak crash; colok lagi → output kembali sendiri dalam ±2 detik.
- [ ] D6 Tutup EVJ saat live → muncul konfirmasi "Outputs are live".
- [ ] D8 Tarik tab **Timeline** ke samping **Preview**, lalu tarik **Outputs** keluar dock hingga melayang → keduanya berfungsi di posisi baru.
- [ ] D9 Tutup tab **Program** (×), buka lagi lewat **View** → kembali. Tutup dan buka EVJ → susunan sama seperti sebelum ditutup.
- [ ] D10 🔒 Lock → tab tidak bisa di-drag, dilayangkan atau ditutup; **View** tetap bisa membuka panel; PANIC/BLACKOUT tetap di bar atas.
- [ ] D11 **View → Setup layout** lalu **Reset layout** → susunan berganti; F2 / F3 / F1 membuka Performance / Outputs / Shortcuts sebagai tab.
- [ ] D7 Di Output Manager, pilih monitor HDMI: bila layar itu digerakkan GPU lain (mis. iGPU sementara show di RTX), muncul peringatan ⚠ oranye. Catat apakah muncul dan fps saat live di layar itu.

## E. Audio ★

- [ ] E1 Video dengan suara → suara terdengar dan sinkron dengan gambar (lihat bibir/ketukan) setelah 5 menit.
- [ ] E2 MP3 di grid → diputar; volume layer, mute (M), master, dan meter berfungsi.
- [ ] E3 Composition → Audio output: pilih perangkat lain (headset/HDMI) → suara pindah.
- [ ] E4 Cabut headset USB/Bluetooth saat audio main → pindah ke default, tidak crash.
- [ ] E5 PANIC → semua suara fade out 1 detik (Program dan Preview).
- [ ] E6 Composition → Audio outputs: Program = speaker/HDMI, Preview = headphone. Cue video bersuara → hanya headphone; TAKE → speaker; fader 🎧 dan meter Preview bergerak; Pause Preview → headphone diam.
- [ ] E7 (Soundcard ≥ 4 channel) Program 1-2, Preview 3-4 di soundcard yang sama → dua output terpisah.
- [ ] E8 Cabut headphone saat Preview berbunyi → "Preview audio off", Program tetap jalan, tidak ada suara Preview di speaker.
- [ ] E9 Pilih perangkat + channel Program untuk Preview → ditolak dengan tulisan merah. Tutup dan buka EVJ → kedua pilihan tetap tersimpan.

## F. Presentasi ★

- [ ] F1 Tarik PPTX ke slot (dengan PowerPoint terpasang) → slot "Importing…" lalu jadi presentasi; tidak ada jendela PowerPoint yang terlihat.
- [ ] F2 Double-click slot presentasi → slide 1 tampil; Next/Prev; slide dengan animasi: tiap klik memutar animasi berikutnya; video di dalam slide ikut main.
- [ ] F3 Import PDF → semua halaman tampil tajam di 1920×1080.
- [ ] F4 (Opsional, laptop tanpa Office tapi dengan LibreOffice) Import PPTX → slide statis tampil. Tanpa keduanya → pesan jelas, bukan crash.
- [ ] F5 **Clicker** (Logitech R400/Spotlight atau sejenis): Next/Prev berfungsi saat EVJ aktif **dan** saat jendela lain (mis. browser) di depan.
- [ ] F6 Mengetik panah/huruf di aplikasi lain tidak mengganti slide (hanya PageUp/PageDown yang global).
- [ ] F7 Tombol "black/B" clicker atau Hide → slide tersembunyi, layer bawah tetap tampil; Next memunculkan lagi.
- [ ] F8 Pointer Laser dot dan Spotlight tampil di output; bergerak dengan mouse / gyro clicker; atur speed.
- [ ] F9 Presenter view di monitor kedua: output sekarang, slide berikutnya, catatan, timer, jam; tombol besar Prev/Next/Hide berfungsi.
- [ ] F10 Slot presentasi yang pernah tayang bertanda ✔; Save → tanda tersimpan; klik kanan tab deck → Clear ✔ marks.

## G. Ketahanan & performa

- [ ] G1 ★ **Soak 4 jam**: siapkan show berisi media acara di beberapa layer dan minimal 1 materi (di mesin dev bisa dibuat otomatis: `python tools\make-soak-show.py "<folder media>" "%USERPROFILE%\Desktop\soak.vjproj" "<folder deck>\deck.json"`), simpan sebagai `soak.vjproj` di Desktop, lalu jalankan `"C:\Program Files\EVJ\evj.exe" --soak 4 "%USERPROFILE%\Desktop\soak.vjproj"`. Selama 4 jam EVJ memicu clip, kolom, transisi, slide, dan pointer secara acak. Hasil di `%APPDATA%\EVJ\soak-*.csv`: aplikasi tidak crash, `private_mb` 30 menit terakhir tidak jauh di atas 30 menit pertama (baris `# summary` di akhir), `layer_errors` 0, `device_resets` 0.
- [ ] G2 ★ **Bench**: dari folder yang bisa ditulis (mis. Desktop) jalankan `"C:\Program Files\EVJ\evj.exe" --bench 60 a.hap.mov b.hap.mov c.hap.mov d.hap.mov` (4 clip HAP 1080p) → file `evj-*.bench.txt`: fps ≥ 59, p99 ≤ 20 ms.
- [ ] G3 Perf window: grafik frame time datar, VRAM wajar.
- [ ] G4 Laptop di baterai (tanpa charger) 10 menit → tetap ≥ 55 fps (catat bila turun).
- [ ] G5 Sleep/lock layar tidak terjadi selama EVJ live.

## H. Catatan hasil

| Item | Hasil / catatan |
|---|---|
| GPU & driver | |
| Windows versi | |
| Bench G2 (fps / p99 / dropped) | |
| Soak G1 (summary) | |
| Masalah lain | |
