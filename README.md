## Amora 0.3.0

### Important fixes
- **Table tidy no longer loops.** In 0.2.0, copying a Markdown table made Amora rewrite the clipboard over and over (about 7 times a second) until you copied something else. Tables are now tidied once.
- **Safer data.** Settings, to-dos, schedule and Shelf are saved safely: the old file stays intact if Amora stops in the middle of a save. A damaged file is set aside as `.rusak` instead of being replaced with defaults. Files edited in Notepad (with a BOM) are now read correctly.
- **Crash log.** If Amora stops unexpectedly, the cause is written to `%LOCALAPPDATA%\Amora\log.txt`. It stays on your computer and is never sent anywhere.
- Changing screen resolution or scaling is now picked up right away by "move aside for tabs behind the notch".

### Polish
- The notch no longer opens when the cursor merely passes over it; the cursor has to rest for a moment (150 ms).
- Long lyric lines in the closed notch scroll slowly to the end instead of being cut off with "…". Each line scrolls only once, so CPU use stays low.
- Shelf thumbnails load in the background, so the notch no longer stutters on large files or files on a network drive.
- The top corners of the notch now curve inward and blend into the top edge of the screen.

**Install / update:** download `AmoraSetup.exe` below and run it. Updating keeps your settings, tasks and schedule. No admin rights needed.

The installer isn't code-signed yet, so SmartScreen may warn you: **More info → Run anyway**.

SHA-256 of `AmoraSetup.exe`:
`e8251424f4001d4534b41417b07b07c6481990fe81f39b3f3a5138e6120fc118`

---

<details>
<summary>Bahasa Indonesia</summary>

### Perbaikan penting
- **Rapikan tabel tidak lagi berulang tanpa henti.** Di 0.2.0, menyalin tabel Markdown membuat Amora menulis ulang clipboard terus-menerus (sekitar 7× per detik) sampai kamu menyalin hal lain. Sekarang tabel hanya dirapikan sekali.
- **Data lebih aman.** Setelan, to-do, jadwal, dan Shelf disimpan dengan cara yang aman: file lama tetap utuh bila Amora mati di tengah penyimpanan. File yang rusak disisihkan sebagai `.rusak`, tidak langsung diganti nilai bawaan. File yang diedit di Notepad (ber-BOM) kini tetap terbaca.
- **Log crash.** Bila Amora berhenti mendadak, penyebabnya tercatat di `%LOCALAPPDATA%\Amora\log.txt` (hanya disimpan di komputermu, tidak dikirim ke mana pun).
- Ganti resolusi atau skala layar kini langsung dikenali oleh fitur "minggir untuk tab di belakang".

### Polesan
- Notch tidak lagi terbuka saat kursor hanya lewat; kursor perlu diam sebentar (150 ms).
- Lirik yang terlalu panjang di notch tertutup bergeser pelan sampai ujung, tidak lagi terpotong "…". Bergeser hanya sekali per baris, jadi tetap hemat CPU.
- Thumbnail Shelf dimuat di latar belakang, jadi notch tidak tersendat saat membuka file besar atau file di jaringan.
- Sudut atas notch kini melengkung ke dalam, menyatu dengan tepi atas layar.

**Pasang / perbarui:** unduh `AmoraSetup.exe` lalu jalankan. Setelan, tugas, dan jadwal tetap tersimpan. Tidak perlu hak admin.

Installer belum ditandatangani, jadi Windows SmartScreen mungkin memberi peringatan: klik **More info → Run anyway**.

</details>
