use anyhow::{bail, Result};
use colored::Colorize;
use std::fs;
use std::path::Path;

use crate::gis::shp::read_geojson_wgs84;

pub fn run(
    input: &Path,
    out: &Path,
    field: Option<&str>,
    preset: &str,
    title: Option<&str>,
    basemap: &str,
    simplify: f64,
) -> Result<()> {
    if !input.is_file() {
        bail!("Input vector file does not exist: {}", input.display());
    }
    let fc = read_geojson_wgs84(input, simplify)?;
    let feat_count = fc
        .get("features")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    if feat_count == 0 {
        bail!("No renderable features found in {}", input.display());
    }

    let map_title = title.map(|s| s.to_string()).unwrap_or_else(|| {
        input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("GeoKit Interactive Map")
            .replace('_', " ")
    });
    let geojson_str = serde_json::to_string(&fc)?;
    let field_json = serde_json::to_string(&field.unwrap_or(""))?;
    let preset_json = serde_json::to_string(&preset.to_ascii_lowercase())?;
    let basemap_json = serde_json::to_string(&basemap.to_ascii_lowercase())?;
    let title_esc = map_title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");

    let html = format!(
        r##"<!doctype html>
<html lang="id">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width,initial-scale=1.0" />
<title>{title_esc} — GeoKit WebMap</title>
<link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
<style>
  * {{ box-sizing: border-box; }}
  body, html {{ margin: 0; padding: 0; height: 100%; font-family: 'Segoe UI', system-ui, sans-serif; background: #0f172a; }}
  #map {{ width: 100%; height: 100%; }}
  .gk-card {{
    position: absolute; z-index: 1000; background: rgba(255,255,255,0.94);
    backdrop-filter: blur(8px); border-radius: 12px; box-shadow: 0 8px 28px rgba(15,23,42,0.18);
    border: 1px solid rgba(148,163,184,0.35); color: #0f172a;
  }}
  #header {{ top: 14px; left: 54px; padding: 12px 16px; max-width: 380px; }}
  #header h1 {{ margin: 0 0 4px 0; font-size: 16px; font-weight: 700; letter-spacing: 0.2px; }}
  #header .sub {{ font-size: 12px; color: #475569; }}
  #controls {{ top: 14px; right: 14px; padding: 10px 14px; width: 220px; font-size: 12px; }}
  #controls label {{ display: block; font-weight: 600; margin-bottom: 4px; color: #334155; }}
  #controls select, #controls input[type=range] {{ width: 100%; margin-bottom: 8px; }}
  #legend {{ bottom: 20px; right: 14px; padding: 12px 14px; max-width: 260px; max-height: 52vh; overflow-y: auto; font-size: 12px; }}
  #legend h3 {{ margin: 0 0 8px 0; font-size: 12px; text-transform: uppercase; letter-spacing: 0.6px; color: #334155; }}
  .leg-row {{ display: flex; align-items: center; gap: 8px; margin-bottom: 5px; }}
  .leg-swatch {{ width: 16px; height: 12px; border-radius: 3px; flex-shrink: 0; border: 1px solid rgba(15,23,42,0.2); }}
  .leg-bar-wrap {{ flex: 1; }}
  .leg-label {{ display: flex; justify-content: space-between; font-size: 11.5px; }}
  .leg-bar {{ height: 4px; border-radius: 2px; background: #e2e8f0; margin-top: 2px; overflow: hidden; }}
  .leg-bar-fill {{ height: 100%; border-radius: 2px; }}
  .popup-tbl {{ border-collapse: collapse; font-size: 12px; min-width: 190px; }}
  .popup-tbl td {{ padding: 3px 6px; border-bottom: 1px solid #e2e8f0; }}
  .popup-tbl td:first-child {{ font-weight: 600; color: #475569; }}
</style>
</head>
<body>
<div id="map"></div>
<div id="header" class="gk-card">
  <h1>{title_esc}</h1>
  <div class="sub" id="meta-sub">{feat_count} fitur · Dibuat dengan GeoKit WebMap</div>
</div>
<div id="controls" class="gk-card">
  <label for="bm-sel">Basemap</label>
  <select id="bm-sel">
    <option value="satellite">Citra Satelit (Esri Imagery)</option>
    <option value="osm">OpenStreetMap</option>
    <option value="light">Carto Light (Bersih)</option>
    <option value="topo">OpenTopoMap</option>
    <option value="dark">Carto Dark</option>
  </select>
  <label for="op-rng">Transparansi Layer: <span id="op-val">75%</span></label>
  <input id="op-rng" type="range" min="10" max="100" value="75" />
</div>
<div id="legend" class="gk-card" style="display:none;"></div>

<script>
const GEOJSON = {geojson_str};
const STYLE_FIELD = {field_json};
const PRESET = {preset_json};
const INIT_BASEMAP = {basemap_json};

const PALETTES = {{
  "hazard-pastel": ["#52B788","#B7E4C7","#FFE66D","#F4A261","#E63946"],
  "hazard":        ["#2E7D32","#81C784","#FFF176","#FFA726","#D32F2F"],
  "viridis":       ["#440154","#3B528B","#21918C","#5EC962","#FDE725"],
  "blue":          ["#E3F2FD","#90CAF9","#42A5F5","#1976D2","#0D47A1"],
  "category":      ["#1F77B4","#FF7F0E","#2CA02C","#D62728","#9467BD","#8C564B","#E377C2","#17BECF"]
}};
const HAZARD_RANK = {{
  "sangat rendah": 1, "rendah": 2, "sedang": 3, "tinggi": 4, "sangat tinggi": 5,
  "very low": 1, "low": 2, "medium": 3, "moderate": 3, "high": 4, "very high": 5
}};

const map = L.map("map", {{ zoomControl: true }}).setView([-2.5, 118.0], 5);
const basemaps = {{
  satellite: L.tileLayer("https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{{z}}/{{y}}/{{x}}", {{ attribution: "Esri World Imagery" }}),
  osm:       L.tileLayer("https://{{s}}.tile.openstreetmap.org/{{z}}/{{x}}/{{y}}.png", {{ attribution: "&copy; OpenStreetMap" }}),
  light:     L.tileLayer("https://{{s}}.basemaps.cartocdn.com/light_all/{{z}}/{{x}}/{{y}}{{r}}.png", {{ attribution: "&copy; CARTO" }}),
  topo:      L.tileLayer("https://{{s}}.tile.opentopomap.org/{{z}}/{{x}}/{{y}}.png", {{ attribution: "&copy; OpenTopoMap" }}),
  dark:      L.tileLayer("https://{{s}}.basemaps.cartocdn.com/dark_all/{{z}}/{{x}}/{{y}}{{r}}.png", {{ attribution: "&copy; CARTO" }})
}};
let activeBm = basemaps[INIT_BASEMAP] || basemaps.satellite;
activeBm.addTo(map);
document.getElementById("bm-sel").value = basemaps[INIT_BASEMAP] ? INIT_BASEMAP : "satellite";
document.getElementById("bm-sel").addEventListener("change", (e) => {{
  map.removeLayer(activeBm);
  activeBm = basemaps[e.target.value] || basemaps.osm;
  activeBm.addTo(map);
}});

// Detect actual field name (case-insensitive) + optional AREA_HA field
const sampleProps = (GEOJSON.features[0] && GEOJSON.features[0].properties) || {{}};
const propKeys = Object.keys(sampleProps);
const fieldKey = STYLE_FIELD ? (propKeys.find(k => k.toLowerCase() === STYLE_FIELD.toLowerCase()) || STYLE_FIELD) : "";
const areaKey = propKeys.find(k => ["area_ha","luas_ha","area_km2","luas"].includes(k.toLowerCase())) || "";

// Collect ordered unique classes + summary stats
const classStats = new Map();
if (fieldKey) {{
  for (const f of GEOJSON.features) {{
    const val = String((f.properties && f.properties[fieldKey]) ?? "(Kosong)");
    const area = areaKey ? (parseFloat(f.properties[areaKey]) || 0) : 0;
    const prev = classStats.get(val) || {{ count: 0, area: 0 }};
    prev.count += 1;
    prev.area += area;
    classStats.set(val, prev);
  }}
}}
const orderedClasses = Array.from(classStats.keys()).sort((a, b) => {{
  const ra = HAZARD_RANK[a.toLowerCase()];
  const rb = HAZARD_RANK[b.toLowerCase()];
  if (ra !== undefined && rb !== undefined) return ra - rb;
  return a.localeCompare(b, undefined, {{ numeric: true }});
}});

function pickColor(idx, total) {{
  const stops = PALETTES[PRESET] || PALETTES["hazard-pastel"];
  if (PRESET === "category") return stops[idx % stops.length];
  if (total <= 1) return stops[0];
  const t = (idx / (total - 1)) * (stops.length - 1);
  return stops[Math.round(t)];
}}
const colorMap = new Map();
orderedClasses.forEach((cls, i) => colorMap.set(cls, pickColor(i, orderedClasses.length)));

let fillOpacity = 0.75;
function featureStyle(feature) {{
  const val = fieldKey ? String((feature.properties && feature.properties[fieldKey]) ?? "") : "";
  const col = colorMap.get(val) || "#0288D1";
  const gtype = (feature.geometry && feature.geometry.type) || "";
  if (gtype.includes("Line")) return {{ color: col, weight: 2.2, opacity: 0.9 }};
  return {{ fillColor: col, color: "#1e293b", weight: 0.8, fillOpacity: fillOpacity }};
}}

const layer = L.geoJSON(GEOJSON, {{
  style: featureStyle,
  pointToLayer: (feat, latlng) => {{
    const val = fieldKey ? String((feat.properties && feat.properties[fieldKey]) ?? "") : "";
    const col = colorMap.get(val) || "#D32F2F";
    return L.circleMarker(latlng, {{ radius: 6, fillColor: col, color: "#fff", weight: 1.5, fillOpacity: 0.9 }});
  }},
  onEachFeature: (feat, lyr) => {{
    const p = feat.properties || {{}};
    const rows = Object.entries(p).map(([k,v]) => `<tr><td>${{k}}</td><td>${{v ?? ""}}</td></tr>`).join("");
    if (rows) lyr.bindPopup(`<table class="popup-tbl">${{rows}}</table>`);
    lyr.on("mouseover", () => {{ if (lyr.setStyle) lyr.setStyle({{ weight: 2.2 }}); }});
    lyr.on("mouseout", () => {{ if (layer.resetStyle) layer.resetStyle(lyr); }});
  }}
}}).addTo(map);

try {{ map.fitBounds(layer.getBounds(), {{ padding: [24, 24] }}); }} catch (_) {{}}

document.getElementById("op-rng").addEventListener("input", (e) => {{
  fillOpacity = parseInt(e.target.value, 10) / 100.0;
  document.getElementById("op-val").textContent = e.target.value + "%";
  layer.setStyle(featureStyle);
}});

if (orderedClasses.length > 0) {{
  const leg = document.getElementById("legend");
  const totalArea = Array.from(classStats.values()).reduce((s, v) => s + v.area, 0);
  const totalCnt = GEOJSON.features.length;
  let html = `<h3>Keterangan (${{fieldKey}})</h3>`;
  for (const cls of orderedClasses) {{
    const st = classStats.get(cls);
    const col = colorMap.get(cls);
    const pct = totalArea > 0 ? (st.area / totalArea) * 100 : (st.count / totalCnt) * 100;
    const metric = totalArea > 0 ? `${{st.area.toLocaleString('id-ID', {{maximumFractionDigits:1}})}} ha (${{pct.toFixed(1)}}%)` : `${{st.count}} (${{pct.toFixed(1)}}%)`;
    html += `<div class="leg-row"><span class="leg-swatch" style="background:${{col}}"></span><div class="leg-bar-wrap"><div class="leg-label"><span>${{cls}}</span><span>${{metric}}</span></div><div class="leg-bar"><div class="leg-bar-fill" style="width:${{Math.max(4, pct)}}%;background:${{col}}"></div></div></div></div>`;
  }}
  leg.innerHTML = html;
  leg.style.display = "block";
}}
</script>
</body>
</html>
"##
    );

    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(out, html)?;
    println!(
        "{} Interactive WebMap ({} features) -> {}",
        "[OK]".green().bold(),
        feat_count,
        out.display()
    );
    Ok(())
}
