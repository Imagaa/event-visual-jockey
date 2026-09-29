# EVJ — Panduan Singkat

EVJ (Event Visual Jockey) memutar video, gambar, audio, dan slide presentasi ke layar acara, mirip Resolume Avenue. Tampilan aplikasi berbahasa Inggris; panduan ini menyebut nama tombol persis seperti di layar.

## 1. Tampilan utama

| Bagian | Isi |
|---|---|
| Bar atas (tetap) | File, **View**, nama show, **Map keys**, **Go live**, **Outputs…**, **Perf**, 🔒 Lock, **BLACKOUT**, **PANIC**, volume master, BPM (TAP, −/+, 1), **Next:** transisi, fps / p99 / dropped / resolusi |
| Panel (dock) | **Grid** (clip & scene), **Program**, **Preview**, **Timeline**, **Properties**, **Outputs**, **Transitions**, **Performance**, **Shortcuts** |
| Bawah (tetap) | Status, simpan otomatis, audio Preview, pekerjaan latar |

**Panel modular.** Setiap panel adalah tab yang bisa **di-drag** ke sisi panel lain (kiri/kanan/atas/bawah) untuk memisah, ke deretan tab untuk menggabung, atau keluar dari dock untuk **melayang** di dalam jendela EVJ. Tombol × menutup tab; **View** menampilkan/menyembunyikan tiap panel. **View → Live layout** (susunan untuk show), **Setup layout** (Properties, Outputs, Transitions terbuka untuk persiapan), **Reset layout**. Susunan disimpan otomatis per laptop. Saat 🔒 Lock, tab tidak bisa dipindah, dilayangkan atau ditutup (View tetap bisa). Grid dan Program selalu kembali saat EVJ dibuka.

## 2. Menyiapkan show

1. Saat EVJ dibuka muncul **Welcome**: *New show*, *Open show…*, atau klik salah satu **Recent shows** (juga ada di **File → Open Recent**). Lalu lalu tarik file dari Explorer ke slot (bisa banyak file atau satu folder sekaligus; file mengisi slot ke kanan). Klik slot kosong untuk dialog **Add clips…**.
2. Format yang didukung: video (mp4, mov, mkv, avi, wmv, webm; H.264/HEVC/HAP, dan codec lain lewat FFmpeg), gambar (png, jpg, bmp, gif, tif, webp; rotasi EXIF diikuti), audio (mp3, wav, m4a, aac, flac, wma).
3. **File → Save** menyimpan `.vjproj`. Path media disimpan relatif terhadap file show, jadi satu folder show + media bisa dipindah ke laptop lain. Autosave berjalan tiap 60 detik; setelah crash, EVJ menawarkan **Recover**.
4. Media yang hilang (misalnya drive berbeda) muncul di jendela **Missing media**. Pakai **Relink from folder…**: EVJ mencari file bernama sama di folder itu.

### Tips performa (laptop iGPU)

Clip berlabel **HEAVY** di-decode CPU lewat FFmpeg (mis. ProRes). Klik kanan clip → **Convert to HAP**. HAP diputar paling ringan dan bisa mundur, ping-pong, in/out, serta sinkron BPM. Pilih **HAP Q** untuk kualitas terbaik, atau **HAP Alpha** untuk video dengan transparansi. Konversi berjalan di latar; slot otomatis diganti ke file `.hap.mov` begitu selesai.

## 3. Memainkan

**Program & Preview** (panel kanan atas):

- **PROGRAM** (merah) = yang dilihat penonton. Di bawahnya setiap layer yang tayang punya baris hitung mundur (`-0:45`). Kurang dari 10 detik: merah berkedip. Di sampingnya meter master L/R dalam dB dan lampu **CLIP** (klik untuk mematikan).
- **PREVIEW** (hijau) = clip atau scene yang dipilih, belum tayang. **1 klik** slot = masuk Preview. **Double-click** = langsung ke Program. **TAKE ▶** atau **Enter** menayangkan isi Preview dengan transisi.
- Tombol di bawah Preview: ▶/⏸ (**Space**) dan ■ kembali ke awal (**Shift+Space**). **Shift+Enter** mengosongkan semua layer Program.
- Bingkai slot: merah = tayang (Program), hijau = di Preview. Pojok kanan bawah slot menampilkan durasi media. Gambar juga punya durasi (default 5 detik, ubah di *Clip settings → Image duration*): hitung mundur, progress dan timeline-nya sama seperti video. **Loop** mengulang hitungannya (gambar tetap tampil), **Once** berhenti di 0 dan gambarnya tetap tampil.


- **Scene** (header kolom, mis. "Scene 1"): **1 klik** = seluruh kolom tampil di Preview (semua layer digabung dengan opacity/blend-nya, tanpa efek). **Double-click** atau **TAKE** = semua layer berganti di Program; layer tanpa clip di kolom itu dikosongkan. Klik kanan → **Rename…** untuk memberi nama (mis. "Opening").
- **Urutan layer**: **Layer 1 ada di baris paling atas dan tampil paling depan** (menutupi layer di bawahnya). **+ Layer** menambah layer di bawah. **⏶ / ⏷** di kontrol layer memindahkan layer beserta semua slotnya; clip yang sedang tayang tetap jalan. Show lama otomatis dibalik saat dibuka sehingga tampilannya tetap sama (nama default "Layer N" ikut nomor baru).
- **Hapus**: klik kanan nama layer → **Delete layer…**, klik kanan header scene → **Delete scene…** (dengan konfirmasi, berlaku di semua deck; yang sedang tayang dikosongkan dulu). Terkunci saat 🔒 Lock.
- **✖** di layer mengosongkan layer. **B** = bypass (sembunyikan), **S** = solo, slider = opacity, **M** + slider kedua = mute dan volume layer.
- **Map keys**: aktifkan, klik slot / scene / ✖ layer / TAP, lalu tekan tombol keyboard. Esc menghapus binding. Matikan Map keys setelah selesai.
- **BLACKOUT** (**B**): semua output fade ke hitam dalam 0,5 detik. Tekan lagi untuk kembali.
- **PANIC** (**F12**, tombol merah kanan atas): semua layer dikosongkan, *panic media* diputar di layer 1, suara fade out. Klik kanan slot → **Set as panic media** untuk memilihnya. Kalau belum diset, dipakai layer 1 kolom 1.
- **🔒 Lock** (**Ctrl+Shift+L**): kunci saat live. Trigger, TAKE, volume, opacity, blackout, panic dan slide tetap bisa. Tambah/hapus media, efek, output, New/Open diblokir. Tahan tombol 1 detik untuk membuka.
- **Opacity** (bar berwarna layer) dan **volume** (fader dB, double-click = 0 dB) bisa diketik: klik angka lalu ketik `75%` atau `-6`. Meter kecil tiap layer menunjukkan level L/R.
- **Shortcuts** (**F1**): semua tombol keyboard aplikasi bisa diubah. Tambahan timeline: **I** / **O** = start / end di posisi Preview, **N** = langkah / scene berikutnya di chain. Tabrakan ditandai oranye. Tombol yang dipakai shortcut aplikasi tidak bisa dipakai di Map keys.
- **Simpan**: setelah show pernah di-Save, perubahan disimpan otomatis tiap 60 detik (status bar: *✓ Saved hh:mm*). Menutup EVJ, New atau Open dengan perubahan belum disimpan akan bertanya dulu.

### Timeline clip (panel bawah)

Klik sebuah clip: panel **Timeline** di bawah grid menampilkan clip itu.

- **Bar**: bagian terang = yang diputar (start–end). Garis **hijau** = posisi Preview, **merah** = posisi Program (bila clip tayang). Klik / geser di bar untuk **seek**.
- **Seek: Preview | Program** memilih yang digeser. Default Preview, jadi layar penonton tidak bergeser tanpa sengaja. Program hanya aktif saat clip itu tayang.
- **Start / end**: geser penanda kuning, atau putar Preview ke titik yang diinginkan lalu **[ Set start** (**I**) / **Set end ]** (**O**). **Reset** = seluruh clip. **⟲ Loop** mengulang antara start dan end (A–B). Berlaku untuk HAP, H.264, audio. Loop A–B di H.264 ada lompatan kecil; HAP mulus.
- **Gelombang suara** di bawah bar: suara clip sendiri dan audio tempelan.
- **♪ Attach audio…**: tempelkan file musik ke video atau foto. **Replace** = suara asli clip dimatikan, **Mix** = keduanya. Volume sendiri. Audio mulai bersama clip dan ikut seek/stop. Foto dengan audio tempelan tayang selama audionya (hitung mundur Program ikut).
- Saat 🔒 Lock: seek dan play tetap bisa; penanda, Loop dan audio tempelan terkunci.

### Chain: layer berurutan dan scene berurutan

Di dalam chain, clip diputar sekali (gambar selama durasinya). Clip dengan loop A–B (⟲ Loop + start/end diatur) terus berulang sampai **⏭** / **N**.

**Layer chain** (beberapa layer dalam satu scene, 1 → 2 → 3):

1. **Shift+klik** slot beberapa layer di **satu scene** (bingkai biru muda), klik kanan salah satunya → **Chain layers**. Slot-slotnya tersambung garis berwarna di tepi kiri dengan nomor langkah.
2. Pilih salah satu slotnya: di panel **Preview → Chain**, tiap langkah setelah yang pertama punya **start** — *when the one before ends* (saat langkah sebelumnya selesai) atau *after N s* (N detik setelah langkah sebelumnya mulai) — dan **mode** — **Replace** (layer sebelumnya dikosongkan) atau **Overlay 🗗** (layer sebelumnya tetap jalan, ditumpuk). Di akhir: **■ Stop** atau **⟲ Loop** (semua layer chain dikosongkan lalu mulai lagi dari langkah 1).
3. Trigger slot langkah pertama (atau scene-nya) → chain berjalan. Tanda di slot: `1 ■` / `1 ⟲` di langkah pertama, `+10.0s` untuk start berdetik, `🗗` untuk overlay.

**Scene chain** (scene 1 → 2 → 3):

1. **Shift+klik** beberapa header scene — atau satu slot di tiap scene — lalu klik kanan salah satunya → **Chain scenes**. Headernya mendapat pita berwarna dan nomor urut.
2. Trigger scene pertama → scene berikutnya mulai saat **clip terpanjang** di scene itu selesai (layer chain di dalamnya dihitung sampai chain itu selesai; layer chain yang Loop menahan scene sampai ⏭). **■ Stop** di scene terakhir atau **⟲ Loop** kembali ke scene pertama (atur di Preview → Chain).

Di bawah PROGRAM tampil **CHAIN 2/3 · Scene** atau **CHAIN 2/3 · Layer** dengan tombol **⏭**. Trigger manual di layer yang ikut chain, ✖, PANIC atau show baru menghentikan chain (overlay di layer lain tidak). Klik kanan → **Break layer chain / Break scene chain** untuk membubarkan. Status bar menunjukkan apa yang akan dibuat dari pilihan Shift+klik; di menu, *Chain…* abu-abu berarti belum ada 2 pilihan. Rangkaian (*sequence*) dari show lama otomatis menjadi scene chain.

### Properti clip (panel kanan)

Play (Loop / PingPong / Once), Speed (negatif = mundur, khusus HAP / gambar), BPM sync (panjang clip = N beat), Transition (override per clip), Sound (audio clip ikut diputar pada speed 1x), Fit (Fit / Fill / Stretch), dan efek clip.

### Layer & Composition

Klik nama layer untuk mengatur blend mode (10 mode), transisi default, dan efek layer. **Composition** berisi resolusi output, perangkat audio, dan efek composition.

## 4. Efek & BPM

- **+ Add effect** di panel properti. Setiap parameter punya slider dan tombol **LFO** (sine/saw/square/triangle/random, 1/4–16 beat).
- **Chroma Key** (menghapus satu warna dari gambar atau video):
  - Pilih clip, lalu di *Clip settings & effects* pilih **+ Add effect → Chroma Key**.
  - Tekan **Pick**, lalu klik warna yang ingin dihapus di monitor **Preview**, atau di **Program** bila clip itu sedang tayang. Esc membatalkan.
  - Warna diambil dari gambar asli clip, jadi tetap benar walau area itu sudah terhapus. Kotak warna di sebelah Pick juga bisa diklik untuk memilih warna secara manual.
  - **tolerance** = seberapa mirip warna yang ikut terhapus, **softness** = kehalusan tepi, **spill** = mengurangi pantulan hijau/biru di tepi dan rambut.
  - Untuk green/blue screen, bayangan di layar ikut terhapus. Untuk latar putih/hitam, teks abu-abu tetap aman.
  - Untuk menghapus dua warna, tambahkan Chroma Key dua kali. Bagian yang terhapus memperlihatkan layer di bawahnya; di monitor Preview tampil hitam.
- BPM: ketik angka, tekan **TAP** mengikuti ketukan, **−/+** untuk nudge, **1** untuk resync (sekarang = beat satu).
- Efek buatan sendiri: file `.hlsl` di folder `effects` (lihat `effects\README.md`).

## 5. Transisi

- **Next:** di bar atas memilih transisi untuk trigger berikutnya. Urutan prioritas: override clip → Next → default layer → Cut.
- **Transitions…** membuka Transition Manager: pratinjau animasi, buat / ubah / hapus preset, durasi dalam detik atau beat, easing, dan favorit.

## 6. Output (layar acara)

1. **Outputs…** → tambah output, pilih monitor (atau *window*), dan sumber: **Composition** atau satu **Layer** (misalnya layar samping yang hanya menampilkan slide pembicara).
2. **Slices**: ambil sebagian gambar (input) lalu taruh ke sebagian layar (output), untuk LED strip atau layar gabungan.
3. **Identify** menampilkan nomor dan test pattern di setiap output.
4. Peringatan ⚠ oranye di Output Manager berarti layar itu tersambung ke GPU lain (umum di laptop gaming): gambar tetap tampil, tetapi disalin antar-GPU sehingga lebih berat. Pakai port yang tersambung ke GPU yang sama bila ada.
5. **Go live** membuka semua output. Monitor yang dicabut akan ditutup, lalu dibuka lagi otomatis saat terpasang kembali. Keluar dari EVJ saat live meminta konfirmasi.

## 7. Presentasi (PPTX / PDF di grid)

1. Tarik file **.pptx / .ppt / .pdf** ke slot (atau **File → Import presentation…**). Slot menampilkan **Importing…** selama PowerPoint berjalan tersembunyi dan mengekspor slide, animasi per klik, video di dalam slide, serta catatan pembicara. Proses ini hanya sekali; saat show tidak ada aplikasi lain yang dibuka. Tanpa PowerPoint, EVJ memakai LibreOffice bila terpasang (slide statis). PDF selalu bisa. Gagal → slot merah, klik kanan → **Retry import**.
2. Presentasi diperlakukan seperti clip lain: 1 klik = Preview (slide pertama), double-click / TAKE = tayang. Layer paling atas yang sedang menayangkan presentasi menerima clicker dan pointer.
3. Kendali ada di panel **Timeline** saat slot presentasi dipilih dan tayang:
   - **Next ▶ / ◀ Prev**, atau clicker / keyboard: PageDown/PageUp (juga saat EVJ tidak aktif), serta panah/Space/Backspace saat jendela EVJ aktif.
   - **Hide (B)**: sembunyikan slide; layer di bawahnya (misalnya video background) tetap tampil.
   - Klik thumbnail untuk lompat ke slide mana pun; catatan pembicara tampil di sampingnya.
   - **Pointer**: *Laser dot* atau *Spotlight* (tombol **L** berganti off → dot → spotlight saat jendela EVJ aktif). Pointer mengikuti mouse / clicker laser (gyro). Kecepatannya diatur di **speed**.
   - **Presenter view**: jendela untuk monitor operator/pembicara berisi output sekarang, slide berikutnya, catatan, timer, dan jam.
4. Slot yang pernah tayang diberi tanda **✔** (pengganti checklist). Klik kanan tab deck → **Clear ✔ marks**.
5. Show lama yang masih punya daftar Materi: saat dibuka, isinya pindah otomatis ke layer presentasi di kolom kosong pertama (yang sudah dicentang mendapat ✔).

## 8. Audio

Ada dua jalur suara, diatur di **Composition → Audio outputs**:

- **Program** = yang didengar penonton (sound system). Pilih perangkat dan channel (1-2, 3-4 …). Volume + meter per layer, master di bar atas. Bila perangkat dicabut, EVJ pindah ke default Windows.
- **Preview 🎧** = headphone operator: hanya suara clip/scene yang sedang di Preview (termasuk audio tempel), ikut Pause/Stop/Seek Preview. Pilih perangkat lain (mis. jack headphone laptop) **atau** channel lain di soundcard yang sama (mis. Program 1-2, Preview 3-4). Default: Off.
- Preview di perangkat **dan** channel yang sama dengan Program ditolak (tulisan merah), supaya suara Preview tidak pernah bocor ke sound system.
- Di bawah monitor PREVIEW: fader 🎧 volume Preview (bisa diketik) dan meter L/R. Status bar menampilkan **🎧 perangkat channel**.
- Headphone dicabut saat Preview berbunyi → Preview mati dengan pesan; tidak pernah pindah ke speaker. Pilih lagi perangkatnya untuk menyalakan.
- **PANIC** membisukan Program dan Preview; naikkan master / fader Preview untuk lanjut.
- Pilihan perangkat disimpan per laptop (`settings.json`), bukan di file show.

## 9. Jika ada masalah

- **Perf** menampilkan grafik frame time, VRAM, dan status decoder per layer.
- Log: `%APPDATA%\EVJ\evj.log`. Crash dump: `%APPDATA%\EVJ\crash`.
- Driver GPU reset (TDR): EVJ memulihkan diri dan memutar ulang clip. Status bar memberi tahu saat ini terjadi.
- Tes ketahanan: `evj.exe --soak 4 show.vjproj` (aksi acak selama 4 jam; log per menit di `%APPDATA%\EVJ\soak-*.csv`).
