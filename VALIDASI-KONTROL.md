# Validasi monitor dan halaman Kontrol

Pengujian pengembangan: 29 Oktober 2026. Pratinjau memakai data terpisah di
`target-dev/release/amora-preview-data`.

Pembaruan distribusi: versi aplikasi dan installer dinaikkan ke 0.4.0. Installer
dibangun dengan `tools/build-installer.ps1 -TargetDir target-dev/setup` dan hasilnya
disimpan di `dist/AmoraSetup.exe`. Commit dibuat sesuai permintaan pengguna;
push dan publikasi rilis tetap dilakukan melalui GitHub Desktop oleh pemilik proyek.

## Otomatis

```powershell
cargo test --release --offline --target-dir target-dev
cargo build --release --offline --target-dir target-dev
```

42 tes lulus, termasuk 28 tes sebelumnya dan 14 tes baru. Cakupan baru: pemilihan
monitor dan fallback, koordinat negatif, kompatibilitas setelan lama, penyimpanan
identitas monitor, nilai slider, penggabungan sesi audio, status Campuran, rentang
kecerahan perangkat, tipe parameter WMI, penggabungan permintaan worker, dan hasil
worker yang kedaluwarsa. `git diff --check` juga lulus.

## Perbaikan respons kecerahan dan tinggi panel

- Slider menampilkan posisi pilihan langsung, sambil worker menerapkan nilainya.
- Koneksi WMI, identitas panel, dan signature metode dipakai ulang selama monitor
  yang sama dipilih. Tidak lagi melakukan discovery dua kali per perubahan.
- Penulisan yang diakui driver tidak dianggap gagal hanya karena pembacaan
  sesudahnya belum diperbarui. Kegagalan sementara diperiksa dan dicoba ulang;
  kegagalan nyata tetap ditampilkan dan slider kembali ke nilai terkonfirmasi.
- Posisi terakhir dikirim ulang saat drag selesai. Hasil lama tetap ditolak.
- Tinggi halaman mengikuti jumlah aplikasi (maksimal tiga baris terlihat, lalu
  gulir), dengan ruang pesan tambahan hanya saat ada error. Perubahan jumlah
  aplikasi dianimasikan tanpa menggeser kontrol saat sedang diseret.
- Tes tambahan memeriksa pengakuan perubahan perangkat setelah error dan ukuran
  panel untuk nol, satu, beberapa, serta banyak aplikasi.
- Uji langsung perbaikan di Display 1: 27% → 29% → 27%, tanpa pesan gagal;
  slider langsung mengikuti input dan tetap pada nilai akhir setelah pembacaan ulang.
- Dengan Brave dan Suara sistem, panel mengecil menjadi dua baris. Menambahkan
  aplikasi audio uji membuat panel membesar menjadi tiga baris, kemudian mengecil
  lagi setelah proses uji ditutup. Volume Brave tidak diubah.

## Uji pada perangkat nyata

Diagnostik final yang memakai koordinat fisik mengonfirmasi laptop memakai DPI
125% pada koordinat negatif (-1920 sampai 0), sementara monitor eksternal memakai
DPI 100% dan merupakan monitor utama. Pratinjau diperiksa pada kedua layar melalui
pemulihan ke monitor utama; drag melintasi kedua layar masih perlu uji manual.

| Skenario | Hasil |
|---|---|
| Membaca identitas, posisi, dan kemampuan dua monitor | Berhasil |
| Slider volume utama | Berubah; dikembalikan tepat ke 1.0, tidak mute |
| Slider sesi Suara sistem, termasuk drag | Berubah; dikembalikan ke 100%, tidak mute |
| Kecerahan laptop melalui WMI | 20% → 25% → 20% |
| Kecerahan monitor eksternal melalui DDC/CI | 0% → 5% → 0% |
| Menu kembali ke monitor utama | Notch berpindah dan target kecerahan berubah ke layar tujuan |
| Seret pegangan dalam monitor yang sama | Notch kembali menempel di tengah atas |
| Dua aplikasi uji bersuara hening | Muncul terpisah; volume A tidak memengaruhi B |
| Dua sesi dari aplikasi A | Digabung menjadi satu baris; Campuran muncul saat nilainya berbeda |
| Menyamakan sesi aplikasi A | Campuran hilang setelah slider mengatur seluruh sesi |
| Mute aplikasi B | B mute dan kembali bersuara, A tetap tidak mute |
| Aplikasi uji ditutup | Barisnya hilang dari mixer |

Sesi uji dibuat dengan `examples/audio_session.rs`, tanpa bunyi. Seluruh proses
audio uji sudah ditutup. Pembacaan akhir memastikan laptop 20%, monitor eksternal
0%, volume utama 1.0, dan Suara sistem 100%; semuanya tidak mute.

## Masih perlu uji manual

- Drag melintasi monitor: alat otomatis menolak titik tujuan di luar batas jendela.
- Escape saat menyeret, dan pengambilalihan capture oleh aplikasi lain.
- Cabut/pasang monitor, restart dengan monitor pilihan, serta ganti monitor utama.
- Drag antara monitor dengan DPI berbeda (pemulihan lewat menu sudah diuji).
- Pergantian output default dari speaker ke headphone saat halaman/slider digunakan.
- Monitor tanpa dukungan kecerahan, mode layar duplikat, dan daftar aplikasi melebihi tiga baris.
- Regresi interaktif Shelf, auto-hide, OSD, navigasi tab, dan sleep timer musik.

## Mengulang pratinjau

```powershell
.\target-dev\release\amora.exe --preview-controls
```

Pratinjau selalu terbuka pada halaman Kontrol dan memiliki jendela aplikasi biasa
agar dapat ditemukan alat uji UI. Volume dan kecerahan tetap mengontrol perangkat
nyata. Jalankan tanpa argumen untuk perilaku jendela notch normal.

Untuk diagnostik baca saja:

```powershell
.\target-dev\release\amora.exe --probe-controls
```
