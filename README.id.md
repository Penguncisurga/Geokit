# geokit

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

> **CLI GIS cepat berbasis Rust untuk merapikan struktur data spasial dan menjalankan batch geoprocessing ArcGIS Pro.**

[Read in English (README.md)](README.md)

---

## Kenapa `geokit`?

Proyek GIS sering dipenuhi folder yang berantakan, shapefile yang komponennya hilang, nama file penuh spasi/simbol, file `.lock` nyangkut, serta pekerjaan *geoprocessing* yang berulang-ulang (*reproject*, *clip*, *repair geometry*, *pack/unpack* `.gdb`, hitung luas/panjang).

`geokit` menggabungkan keduanya dalam **satu file `.exe` ringan**:

1. **Rapihin Struktur Data (100% Native Rust — jalan tanpa ArcGIS)**
   - Eksekusi sangat cepat tanpa butuh Python/ArcGIS.
   - Memperlakukan **paket shapefile** (`.shp`, `.shx`, `.dbf`, `.prj`, `.cpg`, `.shp.xml`, dll.), **sidecar raster** (`.tfw`, `.ovr`, `.aux.xml`, dll.), dan **File Geodatabase (`.gdb`)** sebagai satu kesatuan agar tidak pecah saat dipindah atau diganti namanya.
2. **Batch Geoprocessing (ArcGIS Pro / `arcpy`)**
   - Otomatis mendeteksi `propy.bat` di Windows (atau bisa diarahkan lewat `--propy` / `GEOKIT_PROPY`).
   - **Optimasi single-startup**: `import arcpy` hanya dijalankan **1× per batch** (bukan per file), sehingga jauh lebih cepat untuk puluhan/ratusan layer.
   - Skrip Python helper tertanam langsung di dalam binary — cukup simpan **1 file `geokit.exe`**.

---

## Instalasi

### Opsi 1: Unduh Binary Siap Pakai
Unduh `geokit-windows-x86_64.exe` terbaru di halaman [GitHub Releases](https://github.com/Penguncisurga/geokit/releases), ubah namanya menjadi `geokit.exe`, lalu simpan di folder yang masuk `PATH`.

### Opsi 2: Build dari Source
Membutuhkan [Rust](https://rustup.rs/):

```bash
git clone https://github.com/Penguncisurga/geokit.git
cd geokit
cargo build --release
```

Hasil kompilasi berada di `target/release/geokit.exe`.

---

## Mulai Cepat

Cek deteksi *environment* terlebih dahulu:

```bash
geokit doctor
```

---

## Daftar Perintah

### 1. Rapihin Struktur Data *(Tanpa ArcGIS)*

- **`geokit audit <folder> [--json]`** — Pindai folder GIS, rekap semua dataset, deteksi shapefile tidak lengkap (`.prj`/`.dbf`/`.shx` hilang), nama file bermasalah, dan file `.lock`.
- **`geokit organize <folder> [--output <folder>] [--dry-run]`** — Rapikan file ke folder standar (`01_Vector`, `02_Raster`, `03_Geodatabase`, `04_Projects`, `05_Tabular`) dengan aman.
- **`geokit sanitize <folder> [--dry-run]`** — Ubah nama file GIS menjadi `snake_case` aman untuk ArcGIS/GDB (tanpa spasi/simbol, awalan angka ditambah `l_`).
- **`geokit clean <folder> [--include-xml] [--dry-run]`** — Hapus file `.lock`, `.tmp`, `.bak` (dan opsional `.shp.xml` / `.aux.xml`).

```bash
# Audit folder proyek
geokit audit ./proyek_das

# Pratinjau dulu sebelum memindahkan/mengubah nama
geokit sanitize ./proyek_das --dry-run
geokit organize ./proyek_das --dry-run

# Jalankan langsung
geokit sanitize ./proyek_das
geokit organize ./proyek_das
geokit clean ./proyek_das
```

---

### 2. Batch Geoprocessing *(ArcGIS Pro `arcpy`)*

- **`geokit gp info <path> [--json]`** — Lihat daftar layer, CRS/EPSG, tipe geometri, jumlah fitur, dan *field*.
- **`geokit gp project <input> --epsg <kode> --out <out>`** — Batch *reproject* vektor & raster.
- **`geokit gp clip <input> --aoi <batas> --out <out>`** — Batch *clip* vektor & raster terhadap poligon AOI.
- **`geokit gp merge <folder> --out <out>`** — Gabungkan beberapa shapefile/feature class sejenis.
- **`geokit gp pack <folder> --out <target.gdb>`** — Kemas semua shapefile (dan opsional `--include-rasters`) ke dalam `.gdb`.
- **`geokit gp unpack <input.gdb> --out <folder>`** — Ekstrak semua *feature class* dari `.gdb` ke shapefile.
- **`geokit gp repair <input>`** — Batch *Repair Geometry*.
- **`geokit gp buffer <input> --dist "50 Meters" --out <out> [--dissolve]`** — Batch *buffer*.
- **`geokit gp dissolve <input> --field <field> --out <out>`** — Batch *dissolve* berdasarkan atribut.
- **`geokit gp calc-geom <input> [--unit ha|m2|km2|km|m]`** — Batch hitung luas poligon (`AREA_HA`, `AREA_M2`, `AREA_KM2`) atau panjang garis (`LEN_KM`, `LEN_M`).

```bash
# Reproject semua layer ke UTM Zona 48S
geokit gp project ./01_Vector --epsg 32748 --out ./01_Vector_UTM48S

# Clip batch terhadap batas studi
geokit gp clip ./01_Vector_UTM48S --aoi ./batas_studi.shp --out ./hasil.gdb

# Hitung luas (hektar) untuk semua poligon di dalam .gdb
geokit gp calc-geom ./hasil.gdb --unit ha
```

---

## Lisensi

Proyek ini dirilis dengan [Lisensi MIT](LICENSE).
