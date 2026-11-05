# Validasi implementasi Prism Black

Tanggal: 5 November 2026. Basis: Amora 0.4.0. Desain acuan: sumber Figcraft
`../Figcraft/output/amora-prism-black-v2/amora-prism-black-v2.figcraft`.

## Hasil implementasi

- Material hitam–grafit dengan aksen lavender, navigasi ikon tanpa nama atau tooltip.
- Revisi setelah masukan screenshot: tampilan tertutup memakai satu kapsul hitam
  solid, mengambang 6 DIP dari tepi atas, tanpa border/blur/lapisan warna terpisah.
  Saat membuka panel, kapsul kembali menyatu dengan tepi atas dan material kaca aktif.
- Ikon navigasi 15 DIP, tombol visual 32 DIP, jarak pusat 54 DIP; area klik tetap
  42 × 38 DIP. Navigasi tetap membungkus bila ruang tidak cukup.
- Setelan transparansi 0–100% dan blur 0–48, pembaruan langsung, simpan saat slider
  dilepas, serta pemulihan tampilan awal melalui halaman Setelan dan menu tray.
- Teks, ikon, dan kontrol memakai kontras tersendiri. Transparansi mengatur tint
  material; lapisan konten tetap memiliki latar kontras agar bisa dibaca.
- Musik, Kontrol, dan Setelan mendapat tata letak baru. Daftar aplikasi menyesuaikan
  tinggi hingga tiga baris sebelum menggulir. Setelan dikelompokkan dan dapat digulir.
- Navigasi membungkus pada lebar sempit; panel dibatasi tinggi monitor.
- Setelan lama tetap kompatibel. Reset tampilan tidak menghapus setelan fitur lain.

## Renderer

Windows Composition menampilkan material pada jendela terpisah di belakang foreground
Direct2D layered. Sumber `CreateBackdropBrush` diproses dengan Gaussian blur;
nilai blur dikalikan skala DPI. Blur nol menyembunyikan material blur.
`CreateHostBackdropBrush` menghasilkan hitam pada lingkungan uji, sehingga tidak dipakai.

Geometri visual dan region HWND dibatasi mengikuti notch. Region HWND diperlukan
agar bagian kosong kanvas tidak menghalangi klik ke aplikasi lain. Alpha minimum
satu byte pada tint menjaga slider tetap dapat ditangkap saat transparansi 100%.

Jika inisialisasi/present material gagal, aplikasi memakai foreground software,
menampilkan alasan, dan menonaktifkan slider blur. Kebijakan transparansi dan kontras
tinggi Windows diperiksa saat inisialisasi. Perubahan kebijakan saat aplikasi berjalan
belum diuji; mulai ulang bila material tidak mengikuti perubahan tersebut.

## Pemeriksaan otomatis

Perintah dari root repositori:

```powershell
cargo test --release --offline --target-dir target-dev/prism-black
cargo build --release --offline --target-dir target-dev/prism-black
git diff --check
```

Hasil: **46 tes lolos, 0 gagal**; build release berhasil. Tes mencakup migrasi dan
reset appearance, pembatalan slider, pembungkusan navigasi, region input pada skala
100/125/200%, serta tes fitur monitor, kecerahan, mixer dan parser yang sudah ada.
`rustfmt` tidak tersedia di toolchain lokal; tidak dipasang selama pekerjaan ini.

Build: `target-dev/prism-black/release/amora.exe`.

Build revisi kapsul floating dan navigasi: `target-dev/prism-refined/release/amora.exe`.
Build ulang ke folder sebelumnya terhalang executable yang sedang digunakan;
build terpisah berhasil tanpa menutup aplikasi tersebut. Sebanyak 46 tes tetap lolos.
Revisi diperiksa langsung: kapsul hitam tunggal mengambang di atas pola uji,
ikon lebih kecil dengan jarak lebih lega, dan klik ikon Setelan berpindah halaman.

### Revisi hover dan jarak vertikal navigasi

Build terbaru: `target-dev/prism-hover/release/amora.exe`.

- Bilah navigasi dipisahkan 12 DIP dari panel isi; tinggi panel dan area klik
  memakai perhitungan gap yang sama, termasuk ketika navigasi membungkus.
- Hover langsung pada kapsul mengalahkan otomatis minggir. Area istirahat kapsul
  tetap dapat membuka panel melalui timer ketika notch sudah tersembunyi.
- Timer hover memeriksa posisi kursor saat jatuh tempo agar panel tidak dibuka
  setelah kursor pergi. Preview `--preview-hover` tidak memaksa panel terbuka.
- **47 tes lolos**, termasuk regresi target hover untuk kapsul floating, keadaan
  tersembunyi, koordinat negatif dan skala berbeda. Build release dan diff check lolos.
- Uji native dengan `auto_hide=true`: jendela uji berada di belakang notch;
  kursor di samping kapsul membuatnya tersembunyi, lalu kursor pada posisi kapsul
  membuka panel Musik kembali. Uji ini tidak memakai force-open.
- Halaman Minum diperiksa dengan gap yang terlihat antara kartu dan bilah ikon.

Pengujian ini memakai jendela milik proyek, bukan seluruh aplikasi/game pihak ketiga;
kompatibilitas semua overlay dan aplikasi layar penuh masih belum dipastikan.

### Revisi perpindahan Musik ke Pomodoro

Build terbaru: `target-dev/prism-navigation/release/amora.exe`.

Perubahan tinggi/lebar panel saat navigasi sebelumnya memicu `WM_MOUSELEAVE`
ketika kursor masih diam pada ikon lama. Navigasi lewat klik maupun roda mouse
sekarang menyimpan area panel sebelumnya sebagai jembatan hover. Panel tetap
mengecil mengikuti halaman tujuan, tanpa memaksa halaman pendek menjadi tinggi.
Jembatan dilepas setelah animasi selesai dan kursor masuk panel baru, atau ketika
kursor keluar dari kedua area. Tidak menangkap mouse atau menghalangi klik di luar.

Validasi: **48 tes lolos**, build release berhasil, `git diff --check` lolos.
Tes regresi memeriksa kursor diam selama penyusutan, masuk ke panel baru, keluar
dari kedua area, serta perpindahan ke halaman yang lebih besar. Preview native
tanpa force-open dan dengan auto-hide aktif berhasil berpindah Musik → Fokus
(25:00), tetap terbuka setelah animasi meski kursor di posisi ikon lama, lalu
mengakhiri keadaan terbuka ketika kursor keluar. Masuk kembali ke panel baru
juga diperiksa. Perubahan ini tidak mengubah status atau durasi timer Pomodoro.

### Timer musik dengan pilihan shutdown

Build terbaru: `target-dev/prism-sleep/release/amora.exe`.

Panel Tidur menyediakan durasi 15/30/60 menit, tindakan Musik saja (bawaan) atau
Musik + shutdown, tombol Mulai/Mulai ulang, serta Batalkan timer. Pilihan shutdown
harus diaktifkan dan timer dimulai pengguna; tidak dipersistenkan atau dijadwalkan
di Task Scheduler. Menutup Amora menghapus timer lokal dan memulihkan volume fade.

Urutan selesai: fade 10 detik → kirim Pause → tunggu 800 ms → pulihkan volume →
jika dipilih, worker meminta shutdown. `shutdown.exe` diambil dari direktori sistem
Windows dan memakai `/s /t 0`, tanpa `/f`. Penundaan dilakukan oleh Amora karena
timeout positif Windows mengimplikasikan force-close; lihat [Microsoft](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/shutdown).
Kegagalan worker/perintah ditampilkan di panel. Penerimaan perintah Windows tidak
menjamin perangkat mati bila aplikasi atau kebijakan sistem menghalanginya.

Validasi otomatis: **51 tes lolos**, build release dan diff check berhasil.
Tes baru mencakup pilihan aksi dan jeda pemulihan, susunan tombol pada lebar
320/456/612 DIP, guard preview yang tidak memanggil shutdown, dan propagasi error.

Uji UI: pilihan bawaan Musik saja, pemilihan Musik + shutdown, mulai timer,
pembatalan saat hitung mundur, serta penyelesaian timer dipercepat 15 detik.
Hasil akhir menampilkan “Simulasi selesai: perangkat tidak dimatikan”. Seluruh
pengujian ini memakai preview dengan musik demo: tidak mematikan perangkat,
tidak menjeda pemutar pengguna, dan tidak mengubah volume perangkat.
Shutdown perangkat nyata, kebijakan shutdown, dan aplikasi dengan dokumen belum
disimpan belum diuji secara langsung. Installer dan instalasi pengguna tidak diubah.

### Mode minimal mudah dibuka kembali

Build terbaru: `target-dev/prism-minimal/release/amora.exe`.

Penyebab: target hover ikut mengecil menjadi garis 5 DIP. Pemantauan posisi kursor
sebelumnya hanya aktif bila otomatis sembunyikan mendeteksi jendela di belakang;
area transparan dekat garis tidak menerima `WM_MOUSEMOVE`. `WM_MOUSELEAVE` juga
dapat membatalkan pembukaan sebelum panel sempat membesar melewati kursor.

Perbaikan: garis tetap 5 DIP, tetapi target logis mode minimal diperluas menjadi
24 DIP dengan skala monitor. Timer pointer aktif selama mode minimal, termasuk
ketika auto-hide nonaktif dan ketika status jendela di belakang tidak berubah.
Keluar dari piksel garis tidak membatalkan hover bila kursor masih di target logis.
Timer yang sama mendeteksi keluarnya kursor, sehingga panel dapat menutup kembali.
Area tambahan hanya dipakai membaca posisi kursor; tidak membuat overlay penahan klik.

Validasi: **52 tes lolos**, build release berhasil dan `git diff --check` lolos.
Regresi otomatis memeriksa band 24 DIP pada skala 100/125/200%, koordinat negatif,
batas vertikal/horizontal, dan target mode biasa yang tidak diperbesar.

Perbandingan UI tanpa force-open dengan data uji terpisah: build sebelumnya tetap
berupa garis ketika kursor berada 18 DIP dari tepi atas. Build baru pada posisi
yang sama membuka Musik. Keluar dari panel mengembalikannya ke garis, kemudian
area yang sama membuka panel kembali. Pembukaan juga diperiksa dengan minimal
dan auto-hide sama-sama aktif. Pemeriksaan langsung memakai monitor utama dan
jendela latar milik proyek; seluruh konfigurasi perangkat/DPI belum diuji langsung.

## Pemeriksaan langsung yang dilakukan

Preview memakai direktori data terpisah, bukan data instalasi pengguna. Latar uji
lokal berupa pola kotak dan teks disediakan oleh `examples/prism_backdrop.rs`.

| Pemeriksaan | Hasil |
|---|---|
| Blur native 0, 24, 48 | Latar di dalam notch berubah dari tajam menjadi buram; area luar tetap tajam |
| Transparansi 0 dan 100% | Tint berubah; teks, ikon dan slider tetap terlihat |
| Pulihkan tampilan awal | Kembali ke 80% / 24 dan tersimpan pada data preview |
| Restart preview | Nilai tersimpan dimuat kembali |
| Setelan Umum dibuka | Panel membesar, sembilan sakelar dan kelompok berikutnya tidak bertumpuk |
| Klik di luar notch | Diterima jendela latar uji, bukan terhalang kanvas material |
| Kontrol | Dua lalu tiga baris sesi audio tampil; tinggi adaptif dan ikon mute terlihat |
| Kecerahan | Nilai monitor eksternal DISPLAY2 terbaca; tidak melakukan perubahan perangkat |
| Musik | Data musik contoh, tombol dan progress tampil tanpa bertumpuk |
| Fokus | Tombol Mulai menjalankan hitung mundur; Jeda menghentikannya |
| Release native | Halaman Setelan menampilkan blur aktif dan nilai 80% / 24 |
| Release software | Keterangan mode software tampil dan slider blur nonaktif |

## Belum diverifikasi langsung

- Seluruh kombinasi monitor/DPI, posisi negatif, hotplug, sleep/resume, dan pemindahan
  notch dengan material baru. Tes geometri otomatis bukan pengganti uji perangkat.
- Tulisan volume/mute/mixer dan kecerahan ke perangkat nyata pada build ini.
- Regresi drag file Shelf, riwayat Clipboard, OSD, sleep timer musik, serta seluruh
  halaman pada monitor sangat sempit. Logika fitur tersebut dipertahankan, tetapi
  belum semuanya diuji melalui UI setelah pergantian material.
- Performa GPU, pemakaian memori jangka panjang, dan kompatibilitas semua versi Windows.

Tidak membuat installer, memasang aplikasi, commit, push, atau rilis. Amora yang
terpasang tidak diganti. Screenshot desktop pribadi tidak disimpan untuk validasi;
pemeriksaan visual memakai jendela latar uji milik proyek.

## Glitch ikon saat Musik ↔ Fokus

Build terbaru: `target-dev/prism-transition/release/amora.exe`.

Penyebab terukur: pegas lebar dari 612 menuju 456 DIP sempat mencapai 455,826 DIP
pada frame ke-15 dalam simulasi 60 Hz. Perhitungan kolom dari lebar sesaat membagi
delapan ikon menjadi dua baris, sementara panel isi masih mencadangkan satu baris.
Saat pegas kembali, ikon melompat ke satu baris. Progres opacity juga sebelumnya
menggunakan tinggi tujuan baru sehingga pergantian halaman dapat meredupkan isi.

Susunan kolom kini memakai lebar tujuan, sementara posisi bilah tetap mengikuti
animasi ukuran panel. Lebar minimum halaman mencadangkan seluruh ikon bila monitor
memungkinkan; monitor sempit tetap membungkus. Progres buka/tutup dipertahankan
penuh selama navigasi, sehingga perubahan ukuran halaman tidak dianggap penutupan.

Validasi: **54 tes lolos**, build release dan diff check berhasil. Tes baru menyapu
120 langkah pegas untuk delapan/sembilan ikon, memastikan satu baris dan jarak
antarikon tetap; opacity diperiksa untuk perpindahan besar→kecil dan kecil→besar.
Preview hover biasa diperiksa langsung untuk Musik → Fokus dan Fokus → Musik:
ikon tetap satu baris selama dan sesudah animasi, isi tidak meredup karena resize,
dan perpindahan tetap terbuka ketika kursor tertinggal di posisi ikon lama.
Pengujian langsung seluruh refresh rate, DPI, dan kombinasi GPU belum dilakukan.

## Pemicu mode minimal hanya di tepi atas

Build terbaru: `target-dev/prism-top-edge/release/amora.exe`.

Mengikuti preferensi terbaru, area 24 DIP diganti dengan satu baris piksel fisik
paling atas monitor, terbatas pada lebar notch. Seluruh garis 5 DIP dan area di
bawahnya tidak lagi menjadi pemicu. Saat panel sudah terbuka, hover isi panel
tetap normal. Pemantauan pointer dan jeda 150 ms dipertahankan agar pembukaan
tidak bergantung pada jendela yang berada di belakang notch.

Validasi: **54 tes lolos**, build release dan diff check berhasil. Tes pemicu
memastikan posisi y=top diterima, posisi top+1/3/18/24 ditolak, batas horizontal
tetap berlaku, serta koordinat negatif dan DPI berbeda memakai satu piksel fisik.
Uji UI mode minimal tanpa force-open: y=18 dan y=3 tidak membuka notch setelah
jeda; y=0 membuka Musik, lalu kursor dapat turun ke isi panel. Pemeriksaan langsung
memakai monitor utama; seluruh konfigurasi monitor belum diuji ulang.

## Prioritas tab aplikasi di belakang notch normal

Build terbaru: `target-dev/prism-avoidance/release/amora.exe`.

Jika auto-hide aktif dan ada jendela di belakang notch tertutup, pembukaan dibatasi
ke baris piksel paling atas. Pendekatan dari bawah maupun samping ke area kapsul
menjalankan penghindaran, bukan membuka panel. Hover biasa tetap berlaku di desktop
kosong, saat auto-hide nonaktif, dan di dalam panel yang sudah terbuka. Mode minimal
tetap memakai pemicu tepi atas yang terakhir disepakati.

Hover tertunda dibatalkan bila kursor berpindah ke area aplikasi belakang. Pemicu
tepi atas dipertahankan selama kapsul floating membesar agar `WM_MOUSELEAVE` dari
celah di atas kapsul tidak membatalkannya. Timer pointer juga mengurus penutupan
setelah kursor meninggalkan panel; penangkapan slider dan drag notch tetap didahulukan.

Validasi: **55 tes lolos**, build release dan diff check berhasil. Tes baru mencakup
normal tertutup di atas aplikasi (area kapsul ditolak, tepi atas diterima), area
desktop/panel terbuka yang tetap interaktif, serta monitor berkoordinat negatif.
Uji native tanpa force-open, dengan minimal=false dan auto_hide=true: kursor dari
bawah menuju tengah kapsul membuatnya tersembunyi; klik pada posisi yang sama
diterima jendela latar uji (penghitung klik bertambah). Setelah kursor dipindahkan
ke tepi atas, Musik terbuka kembali dan bertahan. Seluruh browser/game dan susunan
monitor belum diuji langsung; pemeriksaan ini memakai jendela lokal milik proyek.
