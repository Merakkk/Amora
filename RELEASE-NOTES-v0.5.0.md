# Amora 0.5.0 — Prism Black

## Yang baru

- Tampilan Prism Black dengan blur native Windows, transparansi dan kekuatan blur yang bisa diatur, serta pemulihan tampilan awal.
- Kapsul hitam floating saat tertutup dan navigasi ikon yang lebih ringkas.
- Panel Musik, Kontrol, dan Setelan yang lebih rapi; tinggi daftar aplikasi menyesuaikan isinya.
- Sleep timer 15/30/60 menit dengan pilihan **Musik saja** atau **Musik + shutdown**, beserta pembatalan timer.

## Perbaikan interaksi

- Musik ↔ Fokus tetap terbuka saat ukuran panel berubah; ikon tidak lagi sesaat berpindah ke dua baris.
- Mode minimal dibuka dengan mengarahkan kursor ke tepi paling atas layar pada posisi notch.
- Saat otomatis sembunyikan aktif di atas aplikasi lain, akses dari bawah atau samping menyingkirkan notch agar tab di belakang dapat diklik. Tepi paling atas membuka Amora kembali.
- Setelan versi sebelumnya tetap dapat dimuat.

## Memasang dan memperbarui

Unduh **AmoraSetup.exe** dari aset rilis, lalu jalankan. Installer Windows x64 memasang untuk akun pengguna tanpa hak admin. Memperbarui instalasi mempertahankan setelan, tugas, dan jadwal.

Blur native mengikuti dukungan compositor dan setelan efek Windows; bila tidak tersedia, Amora menggunakan tampilan software. Shutdown meminta Windows mematikan perangkat tanpa memaksa aplikasi ditutup, sehingga aplikasi dengan pekerjaan belum disimpan dapat menghalanginya.

## Pemeriksaan

55 tes Rust lolos; aplikasi dan installer release berhasil dibangun. Metadata keduanya menunjukkan versi 0.5.0, dan installer telah diverifikasi membawa executable aplikasi yang baru dibangun. Pengujian perangkat, seluruh kombinasi monitor/DPI, serta shutdown nyata belum lengkap; shutdown diuji melalui simulasi. Installer belum diuji dengan pemasangan ulang pada perangkat ini.

SHA-256 `AmoraSetup.exe`:

```text
8108d3e467f0ca5ab9d4aa103740e06fbaeddb3b02fc9ca472187ae3b700250c
```

File **AmoraSetup.exe.sha256** disertakan pada aset rilis.
