# EVJ — Rencana fitur lanjutan

Daftar fitur yang **belum dibuat** (per v0.2.0), untuk dicentang saat selesai. Urutan dalam tiap bagian = saran prioritas. Status fitur yang sudah ada: [PRD-STATUS.md](PRD-STATUS.md); uji manual: [TESTING-CHECKLIST.md](TESTING-CHECKLIST.md).

## 1. Output mapping (projection mapping) — sub-proyek E

Yang sudah ada: output ke monitor mana pun, sumber composition / satu layer, slice (potongan persegi) untuk LED wall.

- [ ] **Corner pin per slice**: tarik 4 sudut gambar agar pas di permukaan miring (proyektor tidak tegak lurus layar / backdrop).
- [ ] **Mesh warp**: grid titik (mis. 4×4 … 16×16) untuk permukaan lengkung (pilar, kubah, backdrop melengkung), dengan interpolasi halus.
- [ ] **Mask**: bentuk (persegi, elips, poligon bebas) untuk menutup bagian yang tidak boleh terkena proyeksi, dengan tepi lembut.
- [ ] **Edge blending**: dua proyektor atau lebih bertumpuk jadi satu gambar lebar; kurva gamma dan lebar overlap per sisi.
- [ ] **Mode kalibrasi di layar output**: grid / garis bantu tampil di proyektor saat mengatur; titik bisa digeser dengan mouse atau panah (presisi 1 px).
- [ ] Simpan / muat preset mapping per venue (terpisah dari show), karena satu venue dipakai banyak show.
- [ ] Uji: pola referensi, corner pin kembali ke persegi = identik dengan tanpa mapping; performa di iGPU.

## 2. Updater

Tujuan: operator tahu ada versi baru dan bisa memasangnya tanpa membuka GitHub, **tidak pernah saat show berjalan**.

Dasar (sudah ada sejak v0.2.0): GitHub Actions menerbitkan `EVJ-Setup-X.Y.Z.exe` + `.sha256` pada setiap tag (lihat [RELEASING.md](RELEASING.md)).

- [ ] **Cek update manual**: menu *Help → Check for updates*; baca GitHub Releases API (`/repos/Imagaa/event-visual-jockey/releases/latest`), bandingkan versi (semver) dengan versi aplikasi, tampilkan "EVJ X.Y.Z tersedia" + catatan rilis, atau "Sudah versi terbaru".
- [ ] **Cek berkala otomatis**: saat EVJ dibuka, paling sering sekali sehari, di latar belakang (tidak menunda start, diam bila offline). Bisa dimatikan di pengaturan (`settings.json`, per laptop).
- [ ] **Unduh & pasang dari aplikasi**: unduh installer ke `%TEMP%` dengan progress bar dan tombol batal, cocokkan SHA-256 dengan file `.sha256` rilis, lalu tanya "Tutup EVJ dan pasang sekarang?". Installer dijalankan diam (`/SILENT /CLOSEAPPLICATIONS`) dan membuka EVJ lagi setelah selesai; show yang terbuka disimpan dulu (atau ditanya bila belum pernah disimpan).
- [ ] **Aturan aman saat live**: tidak ada popup, unduhan, atau pemasangan saat 🔒 Lock aktif atau output sedang live; pemberitahuan cukup berupa titik kecil di status bar dan ditampilkan setelah show selesai.
- [ ] **Lewati versi ini** dan **ingatkan nanti**.
- [ ] Rilis *pre-release* hanya ditawarkan bila operator memilih kanal "beta".
- [ ] Opsi lanjutan (native, tanpa installer): ganti `evj.exe` di tempat (ganti nama exe yang sedang jalan → salin yang baru → restart). Dipertimbangkan hanya bila installer terasa terlalu berat; installer tetap jalur utama karena juga memperbarui DLL FFmpeg, efek, dan dokumen.
- [ ] Uji: perbandingan versi (0.2.0 < 0.10.0, pre-release), checksum salah ditolak, offline, API rate limit, instalasi di atas versi lama tetap membawa `settings.json` dan show.

## 3. Penyempurnaan yang tertunda

- [ ] **Pengaturan audio Program / Preview lebih mudah ditemukan**: tulisan "Preview audio off — Composition › Audio outputs" di panel Preview menjadi tombol yang langsung membuka pengaturannya (atau panel *Audio* sendiri di menu View).
- [ ] **Pre-warm clip sebelum di-trigger** (F-GRID-5): clip di Preview / langkah chain berikutnya dibuka lebih dulu agar trigger langsung tampil.
- [ ] **Pembukaan pertama show lebih ringan**: thumbnail video pertama kali dibuat dengan decode hardware atau prioritas rendah (sekarang ±12 dtk 4–6 core CPU sekali per file; pembukaan berikutnya sudah dari cache).
- [ ] **Performa G2 di iGPU Vega 8** (4 HAP 1080p + 2 efek/layer + 2 output di 60 fps) belum tercapai; butuh profil ulang setelah optimisasi v0.2.0.
- [ ] **Latensi trigger / slide diukur per frame** (NFR-PERF-3).
- [ ] Preview sebuah **scene yang berisi layer chain** menampilkan semua layer chain sekaligus; seharusnya urut seperti saat tayang (atau hanya langkah pertama).
- [ ] Baris info clip **gambar** di Clip settings menulis durasi file 0:00.0, bukan durasi gambarnya.
- [ ] Baris layer di bawah PROGRAM menampilkan nama slide (`slide001`) untuk presentasi; seharusnya nama presentasinya.
- [ ] Presentasi yang sedang di-import ke **scene yang lalu dihapus** mendarat satu kolom bergeser.
- [ ] Output bersumber **layer yang dihapus** lalu menampilkan layer yang mengambil nomornya; seharusnya kembali ke composition (dengan pemberitahuan).
- [ ] **Show dari v0.2.0 dibuka di v0.1.0** lalu disimpan akan terbalik lagi saat dibuka di v0.2.0: tambahkan penanda versi format agar versi lama menolak / memperingatkan.
- [ ] Laporan lama: tombol ✖ *clear layer* tidak berfungsi — belum bisa direproduksi; butuh langkah persisnya bila terjadi lagi.

## 4. Uji manual yang masih terbuka

Bukan fitur baru, tetapi perlu dijalankan sebelum show sungguhan (detail di [TESTING-CHECKLIST.md](TESTING-CHECKLIST.md)):

- [ ] G1 soak 4 jam, G2 bench di laptop RTX.
- [ ] B21–B33 (timeline, audio tempel, chain, scene, presentasi, urutan layer, hapus, durasi gambar, Chroma Key).
- [ ] D8–D11 (panel modular), E6–E9 (audio Preview dengan dua perangkat / soundcard 4 channel), F4 (LibreOffice), F5 (clicker fisik).
