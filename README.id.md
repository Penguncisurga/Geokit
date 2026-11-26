# geokit

[![CI](https://github.com/Penguncisurga/Geokit/actions/workflows/ci.yml/badge.svg)](https://github.com/Penguncisurga/Geokit/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Penguncisurga/Geokit)](https://github.com/Penguncisurga/Geokit/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

> **CLI GIS cepat & MCP Server bawaan berbasis Rust untuk merapikan struktur data spasial, batch geoprocessing (ArcGIS Pro / ArcMap 10.x / GDAL), export layout, raster, dan hidrologi DAS.**

[Read in English (README.md)](README.md) · [Panduan SOP AI Agent (AGENT_GUIDE.md)](AGENT_GUIDE.md) · [Changelog](CHANGELOG.md) · [Contributing](CONTRIBUTING.md)

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

### Opsi 1: Minta AI Kamu yang Pasangkan *(Paling Mudah untuk Pemula!)*
Cukup *copy-paste* prompt ini ke **Claude Code, Antigravity, Cursor, Codex, atau Gemini CLI**:

```text
Tolong pasangkan geokit dari https://github.com/Penguncisurga/Geokit:
1. Jalankan one-line installer (`install.ps1` di Windows atau `install.sh` di Linux/macOS) agar binary terbaru terpasang ke PATH.
2. Jalankan `geokit doctor` untuk mengecek ArcGIS Pro, ArcMap 10.x, atau QGIS/GDAL di komputer saya.
3. Jika ArcMap 10.x terdeteksi, pastikan ArcMap sedang tutup lalu jalankan `geokit live install-addin`.
4. Daftarkan `geokit mcp` di konfigurasi MCP saya supaya kamu bisa langsung mengontrol ArcMap/ArcGIS saya.
```

### Opsi 2: Installer 1-Baris *(Tanpa Install Rust)*

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/Penguncisurga/Geokit/main/install.ps1 | iex
```

**Linux / macOS (Apple Silicon):**
```bash
curl -fsSL https://raw.githubusercontent.com/Penguncisurga/Geokit/main/install.sh | sh
```

### Opsi 3: Unduh Binary Manual
Unduh binary terbaru (`geokit-windows-x86_64.exe`, `geokit-linux-x86_64`, atau `geokit-macos-arm64`) di halaman [GitHub Releases](https://github.com/Penguncisurga/Geokit/releases), ubah namanya menjadi `geokit.exe`, lalu simpan di folder yang masuk `PATH`.

### Opsi 4: Build dari Source
Membutuhkan [Rust](https://rustup.rs/):

```bash
git clone https://github.com/Penguncisurga/Geokit.git
cd Geokit
cargo build --release --locked
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
- **`geokit report <folder> --out <laporan.html|.md|.json>`** — Buat laporan inventaris folder GIS siap kirim dalam format HTML, Markdown, atau JSON (tanpa butuh ArcGIS).

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
- **`geokit gp intersect <input> --with <overlay> --out <out>`** — Batch *intersect* layer vektor dengan layer overlay.
- **`geokit gp erase <input> --mask <mask> --out <out>`** — Batch *erase* fitur yang tumpang tindih dengan poligon mask.
- **`geokit gp spatial-join <input> --join <layer> --out <out> [--match intersect|within|contains|closest]`** — Batch *spatial join* atribut antar layer.
- **`geokit gp xy2shp <tabel.csv> --x-field lon --y-field lat --epsg 4326 --out <titik.shp>`** — Konversi tabel CSV koordinat X/Y (Lon/Lat) langsung menjadi titik `.shp`, `.gpkg`, atau `.gdb` (ArcGIS Pro, ArcMap, atau GDAL).
- **`geokit gp convert <input> --to shp|gpkg|geojson|kml|fgb --out <out>`** — Batch konversi format vektor antar SHP, GeoPackage, GeoJSON, KML, dan FlatGeobuf.
- **`geokit qa <input> [--expected-epsg 32748] [--check-nulls DESA,KELAS] [--check-overlaps]`** — Quality Assurance spasial: cek keseragaman CRS, geometri rusak (*CheckGeometry*), field wajib yang kosong/NULL, dan poligon tumpang tindih (*overlap*).
- **`geokit gp values <input> [--field <kolom>] [--json]`** — Intip daftar *unique values* beserta jumlah fiturnya serta statistik numerik/raster (`min`, `max`, `mean`, `std`).
- **`geokit gp classify <input> --field <asal> --out-field <tujuan> --rules "<aturan>"`** — Skoring & klasifikasi otomatis berbasis kategori (`"Hutan=1;Sawah=3"`) atau rentang angka (`"<=8:1;8-15:2;>15:3"`).
- **`geokit gp calc-field <input> --field <tujuan> --expr "<rumus>"`** — Hitung total skor berbobot/rumus matematika antar kolom dengan aman (`"0.4 * SKOR_LERENG + 0.6 * SKOR_TANAH"`).
- **`geokit gp union <layer1> <layer2> [...] --out <out>`** — Overlay (*Union*) beberapa layer poligon (atau seluruh folder/`.gdb`) sekaligus untuk analisis kerawanan/kesesuaian multi-parameter.

> **Backend GDAL / QGIS:** Gunakan `--engine gdal` (atau otomatis via `--engine auto` jika ArcGIS Pro tidak terpasang) untuk menjalankan `info`, `project`, `clip`, `merge`, `pack` (`.gpkg`), dan `unpack` lewat `ogr2ogr` / `gdalwarp` (otomatis terdeteksi dari `PATH` maupun folder QGIS di Windows).

```bash
# Reproject semua layer ke UTM Zona 48S
geokit gp project ./01_Vector --epsg 32748 --out ./01_Vector_UTM48S

# Clip batch terhadap batas studi
geokit gp clip ./01_Vector_UTM48S --aoi ./batas_studi.shp --out ./hasil.gdb

# Hitung luas (hektar) untuk semua poligon di dalam .gdb
geokit gp calc-geom ./hasil.gdb --unit ha
```

---

### 3. Export Layout & Cek Broken Links *(ArcGIS Pro `.aprx`)*

- **`geokit layout list <path> [--json]`** — Tampilkan daftar layout dan ukuran halaman di satu `.aprx` atau folder proyek.
- **`geokit layout export <path> --out <folder> [--format pdf|png|jpg] [--dpi 300] [--filter <teks>]`** — Batch export semua layout `.aprx` ke PDF/PNG/JPG dalam 1 sesi `arcpy`.
- **`geokit map broken-links <path> [--json]`** — Deteksi layer dengan *broken data source* (tanda seru merah) di proyek `.aprx`.
- **`geokit map fix-links <aprx> --find <lama> --replace <baru> [--out <salinan.aprx>]`** — Perbaiki path data source yang pindah folder/drive secara massal.

```bash
# Export semua layout di folder proyek ke PDF 300 DPI
geokit layout export ./04_Projects --out ./exports --format pdf --dpi 300

# Cek layer rusak sebelum peta dikirim/dicetak
geokit map broken-links ./04_Projects
```

---

### 4. Raster & Hidrologi / DAS (`raster` & `hydro`)

- **`geokit raster mosaic <folder> --out <dem_gabungan.tif>`** — Gabungkan banyak tile DEMNAS/SRTM jadi satu GeoTIFF (ArcGIS Pro atau `--engine gdal`).
- **`geokit raster terrain <dem.tif> --out <folder> [--all | --slope --aspect --hillshade]`** — Hasilkan peta kemiringan lereng (*slope*), arah lereng (*aspect*), dan *hillshade* sekaligus.
- **`geokit raster zonal-stats <raster.tif> --zones <poligon> --zone-field <field> --out <stats.csv>`** — Hitung statistik zonal (min, max, mean, std, sum) per sub-DAS ke CSV/DBF.
- **`geokit hydro delineate <dem.tif> --out <folder> [--threshold 1000] [--pour-point <outlet.shp>]`** — Pipeline delineasi DAS otomatis: `Fill` → `FlowDirection` → `FlowAccumulation` → `Jaringan Sungai (.shp)` → `Poligon DAS (.shp)`.
- **`geokit hydro basin <dem.tif> --out <folder>`** — Delineasi otomatis seluruh poligon *Basin* dari DEM tanpa perlu titik outlet.

```bash
# Buat slope, aspect, dan hillshade cepat lewat GDAL
geokit --engine gdal raster terrain ./02_Raster/dem_das.tif --out ./terrain --all

# Delineasi DAS otomatis dari DEM + titik outlet
geokit hydro delineate ./02_Raster/dem_utm.tif --out ./hasil_das --threshold 500 --pour-point ./outlet.shp
```

---

### 5. Konfigurasi Proyek (`geokit.toml`)

Buat file konfigurasi awal di folder proyek:

```bash
geokit init
```

Kamu bisa mengubah nama folder `organize`, default EPSG untuk `gp project`, default DPI/format untuk `layout export`, dan path `propy.bat` di dalam `geokit.toml`.

---

### 6. Built-in MCP Server (`geokit mcp`) & Dukungan ArcMap 10.x (`.mxd`)

`geokit` sudah memiliki **MCP Server** bawaan (`geokit mcp`) via `stdio` dengan **15 tool GIS** dan **3 SOP Playbook Prompt bawaan** (`peta_kerawanan`, `peta_persebaran`, `analisis_das` — lihat [AGENT_GUIDE.md](AGENT_GUIDE.md)), sehingga AI Agent (Antigravity, Claude Desktop, Cursor, VS Code) bisa mengolah folder bahan mentah menjadi peta analisis lengkap secara otomatis!

Tambahkan ke konfigurasi MCP client kamu:

```json
{
  "mcpServers": {
    "geokit": {
      "command": "geokit",
      "args": ["mcp"]
    }
  }
}
```

Untuk **ArcMap 10.x (`.mxd` / Python 2.7)** — *100% fitur didukung penuh*:
- Di komputer yang **hanya punya ArcMap 10.x** (tanpa ArcGIS Pro), `--engine auto` otomatis memilih **ArcMap 10.x** (`ArcGIS Pro → ArcMap 10.x → GDAL`) untuk semua perintah `gp`, `raster`, `hydro`, `layout`, dan `map`.
- File atau folder `.mxd` pada `geokit layout` dan `geokit map` otomatis dijalankan lewat Python 2.7 ArcMap (`arcpy.mapping`).
- Gunakan `--engine arcmap` (atau set `engine = "arcmap"` di `geokit.toml`) untuk memilih ArcMap 10.x secara eksplisit.

---

### 7. Kontrol Layar Live ArcMap 10.1 – 10.8 (`geokit live` & MCP `geokit_live`)

Kontrol jendela **ArcMap 10.1 – 10.8.2 yang sedang terbuka secara *real-time*** (seperti QGIS MCP) — termasuk **seluruh 800+ tool di ArcToolbox** — lewat Python Add-In `geokit-arcmap.esriaddin` bawaan!

**3 Langkah Setup untuk AI Agent Mana Pun (Claude Desktop, Antigravity, Cursor, Claude Code, Codex):**

1. **Tutup ArcMap**, lalu jalankan sekali:
   ```bash
   geokit live install-addin
   ```
2. **Daftarkan `geokit` di konfigurasi MCP aplikasi AI kamu** (`claude_desktop_config.json`, Cursor, Antigravity, Codex):
   ```json
   {
     "mcpServers": {
       "geokit": {
         "command": "geokit",
         "args": ["mcp"]
       }
     }
   }
   ```
   *(Untuk Claude Code CLI: `claude mcp add geokit -- geokit mcp`)*
3. **Buka ArcMap 10.x** — Live Bridge otomatis nyala di port `9877`, langsung bisa kamu kontrol lewat chat AI!

**Contoh Perintah Live:**

```bash
# Cek koneksi, daftar layer, & baca field + contoh baris atribut
geokit live ping
geokit live layers
geokit live layer-info batas_desa

# Tambah layer, zoom, filter, terapkan simbologi .lyr, nyalakan label, seleksi, export & simpan .mxd
geokit live add ./01_Vector/batas_desa.shp
geokit live zoom batas_desa
geokit live definition-query batas_desa --where "KECAMATAN = 'Boja'"
geokit live symbology batas_desa --lyr ./styles/batas_desa.lyr
geokit live style kerawanan_banjir --field KELAS_RWN --preset hazard --transparency 30
geokit live order sungai --position top
geokit live layout --view layout --title "PETA KERAWANAN BANJIR" --zoom-layer kerawanan_banjir
geokit live labels batas_desa --show true --field NAMOBJ
geokit live select batas_desa --where "KELAS = 'Hutan'"
geokit live export --out ./tampilan_aktif.png --dpi 200
geokit live save --out ./peta_kerja.mxd

# Cari & jalankan SEMUA tool di ArcToolbox (otomatis checkout ekstensi Spatial/3D/Network!)
geokit live tools slope
geokit live tool-usage Slope_sa
geokit live run-gp Buffer_analysis ./sungai.shp ./sungai_buf.shp "100 Meters"
```

---

### 8. Future-Proof ArcGIS Pro Tools, Generator Toolbox ModelBuilder (`.pyt`) & Pipeline

- **Otomatis ikut update versi ArcGIS Pro terbaru:**
  ```bash
  # Cari & jalankan SEMUA tool yang terpasang di versi ArcGIS Pro / ArcMap kamu
  geokit gp tools slope
  geokit gp tool-usage Slope_sa
  geokit gp run Buffer_analysis ./sungai.shp ./sungai_buf.shp "100 Meters"
  ```
- **Generator Python Toolbox (`.pyt`) siap pakai di ModelBuilder:**
  ```bash
  # Buat file .pyt dengan form GUI yang langsung bisa di-drag ke ModelBuilder
  geokit toolbox create GeoKit_DAS --out ./GeoKit_DAS.pyt --template das
  geokit toolbox create Batch_Prep --out ./Batch_Prep.pyt --template batch-prep
  ```
- **Pipeline Resep (`pipeline.toml` — ModelBuilder versi teks/AI):**
  ```bash
  geokit pipeline init --out pipeline.toml
  geokit pipeline run pipeline.toml --dry-run
  geokit pipeline run pipeline.toml
  ```
- **Otomasi `.aprx` Lanjutan:**
  ```bash
  # Export seluruh halaman Spatial Map Series ArcGIS Pro sekaligus
  geokit layout map-series ./peta_desa.aprx --out ./exports/desa_all.pdf --dpi 300

  # Tambah layer + terapkan simbologi .lyrx langsung ke file .aprx
  geokit map add-layer ./proyek.aprx --layer ./batas_desa.shp --lyrx ./style_desa.lyrx
  ```

---

## Lisensi

Proyek ini dirilis dengan [Lisensi MIT](LICENSE).
