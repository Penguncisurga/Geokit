use anyhow::{bail, Result};
use colored::Colorize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

use crate::gis::shp::read_geojson_wgs84;

fn json_for_script(val: &Value) -> Result<String> {
    let raw = serde_json::to_string(val)?;
    Ok(raw
        .replace("</", "<\\/")
        .replace("<!--", "<\\!--")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029"))
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn run(
    inputs: &[PathBuf],
    out: &Path,
    field: Option<&str>,
    preset: &str,
    title: Option<&str>,
    basemap: &str,
    simplify: f64,
) -> Result<()> {
    if inputs.is_empty() {
        bail!("Provide at least one .shp or .geojson input file.");
    }

    let mut layers_payload: Vec<Value> = Vec::new();
    let mut total_features = 0usize;

    for (idx, input) in inputs.iter().enumerate() {
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
            continue;
        }
        total_features += feat_count;
        let raw_stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Layer");
        let display_name = raw_stem.replace('_', " ");
        layers_payload.push(json!({
            "id": format!("lyr_{idx}"),
            "name": display_name,
            "raw_name": raw_stem.to_ascii_lowercase(),
            "count": feat_count,
            "geojson": fc,
        }));
    }

    if layers_payload.is_empty() {
        bail!("No renderable features found in the provided input file(s).");
    }

    let map_title = title.map(|s| s.to_string()).unwrap_or_else(|| {
        inputs[0]
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("GeoKit Interactive Map")
            .replace('_', " ")
    });
    let layers_json = json_for_script(&Value::Array(layers_payload.clone()))?;
    let field_json = json_for_script(&json!(field.unwrap_or("")))?;
    let preset_json = json_for_script(&json!(preset.to_ascii_lowercase()))?;
    let basemap_json = json_for_script(&json!(basemap.to_ascii_lowercase()))?;
    let layer_count = layers_payload.len();
    let title_esc = escape_html(&map_title);

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
  #header {{ top: 14px; left: 54px; padding: 12px 16px; max-width: 390px; }}
  #header h1 {{ margin: 0 0 4px 0; font-size: 16px; font-weight: 700; letter-spacing: 0.2px; }}
  #header .sub {{ font-size: 12px; color: #475569; }}
  #controls {{ top: 14px; right: 14px; padding: 10px 14px; width: 235px; font-size: 12px; max-height: 45vh; overflow-y: auto; }}
  #controls label.sec {{ display: block; font-weight: 700; margin: 6px 0 4px 0; color: #334155; font-size: 11px; text-transform: uppercase; letter-spacing: 0.4px; }}
  #controls select, #controls input[type=range] {{ width: 100%; margin-bottom: 6px; }}
  .lyr-item {{ display: flex; align-items: center; gap: 7px; margin: 4px 0; cursor: pointer; font-size: 12px; }}
  .lyr-dot {{ width: 11px; height: 11px; border-radius: 3px; flex-shrink: 0; border: 1px solid rgba(15,23,42,0.25); }}
  #legend {{ bottom: 20px; right: 14px; padding: 12px 14px; max-width: 265px; max-height: 44vh; overflow-y: auto; font-size: 12px; }}
  #legend h3 {{ margin: 0 0 8px 0; font-size: 12px; text-transform: uppercase; letter-spacing: 0.6px; color: #334155; }}
  .leg-row {{ display: flex; align-items: center; gap: 8px; margin-bottom: 5px; }}
  .leg-swatch {{ width: 16px; height: 12px; border-radius: 3px; flex-shrink: 0; border: 1px solid rgba(15,23,42,0.2); }}
  .leg-bar-wrap {{ flex: 1; }}
  .leg-label {{ display: flex; justify-content: space-between; font-size: 11.5px; gap: 8px; }}
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
  <div class="sub">{layer_count} layer · {total_features} fitur · GeoKit WebMap</div>
</div>
<div id="controls" class="gk-card">
  <label class="sec" for="bm-sel">Basemap</label>
  <select id="bm-sel">
    <option value="satellite">Citra Satelit (Esri Imagery)</option>
    <option value="osm">OpenStreetMap</option>
    <option value="light">Carto Light (Bersih)</option>
    <option value="topo">OpenTopoMap</option>
    <option value="dark">Carto Dark</option>
  </select>
  <label class="sec" for="op-rng">Transparansi Poligon: <span id="op-val">72%</span></label>
  <input id="op-rng" type="range" min="10" max="100" value="72" />
  <label class="sec">Daftar Layer</label>
  <div id="layer-list"></div>
</div>
<div id="legend" class="gk-card" style="display:none;"></div>

<script>
const LAYERS = {layers_json};
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

function escHtml(v) {{
  return String(v ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}}

function pickColor(idx, total) {{
  const stops = PALETTES[PRESET] || PALETTES["hazard-pastel"];
  if (PRESET === "category") return stops[idx % stops.length];
  if (total <= 1) return stops[0];
  const t = (idx / (total - 1)) * (stops.length - 1);
  return stops[Math.round(t)];
}}

let fillOpacity = 0.72;
const polygonLayers = [];
const allBounds = L.latLngBounds([]);
const lyrListEl = document.getElementById("layer-list");
let legendRendered = false;

LAYERS.forEach((meta, lidx) => {{
  const fc = meta.geojson;
  const sampleProps = (fc.features[0] && fc.features[0].properties) || {{}};
  const propKeys = Object.keys(sampleProps);
  const fieldKey = STYLE_FIELD ? propKeys.find(k => k.toLowerCase() === STYLE_FIELD.toLowerCase()) : null;
  const areaKey = propKeys.find(k => ["area_ha","luas_ha","area_km2","luas"].includes(k.toLowerCase())) || "";
  const g0 = (fc.features[0] && fc.features[0].geometry && fc.features[0].geometry.type) || "";
  const nm = meta.raw_name || "";
  const isBoundary = (nm.includes("batas") || nm.includes("admin") || nm.includes("aoi")) && !fieldKey;
  const isRiver = nm.includes("sungai") || nm.includes("river") || nm.includes("stream");
  const isRoad = nm.includes("jalan") || nm.includes("road");

  const classStats = new Map();
  const colorMap = new Map();
  let orderedClasses = [];
  if (fieldKey) {{
    for (const f of fc.features) {{
      const val = String((f.properties && f.properties[fieldKey]) ?? "(Kosong)");
      const area = areaKey ? (parseFloat(f.properties[areaKey]) || 0) : 0;
      const prev = classStats.get(val) || {{ count: 0, area: 0 }};
      prev.count += 1;
      prev.area += area;
      classStats.set(val, prev);
    }}
    orderedClasses = Array.from(classStats.keys()).sort((a, b) => {{
      const ra = HAZARD_RANK[a.toLowerCase()];
      const rb = HAZARD_RANK[b.toLowerCase()];
      if (ra !== undefined && rb !== undefined) return ra - rb;
      return a.localeCompare(b, undefined, {{ numeric: true }});
    }});
    orderedClasses.forEach((cls, i) => colorMap.set(cls, pickColor(i, orderedClasses.length)));
  }}

  const swatchColor = fieldKey
    ? "#F4A261"
    : (isBoundary ? "#263238" : (isRiver ? "#0277BD" : (isRoad ? "#D84315" : (g0.includes("Point") ? "#1565C0" : "#1E88E5"))));

  function styleFn(feature) {{
    const gtype = (feature.geometry && feature.geometry.type) || "";
    if (gtype.includes("Line")) {{
      return {{ color: isRoad ? "#D84315" : "#0277BD", weight: isRoad ? 2.2 : 1.7, opacity: 0.92 }};
    }}
    if (isBoundary) {{
      return {{ fillOpacity: 0.0, color: "#1e293b", weight: 2.2, dashArray: "8 4 2 4" }};
    }}
    const val = fieldKey ? String((feature.properties && feature.properties[fieldKey]) ?? "") : "";
    const col = colorMap.get(val) || swatchColor;
    return {{ fillColor: col, color: "#1e293b", weight: 0.6, fillOpacity: fillOpacity }};
  }}

  const leafletLyr = L.geoJSON(fc, {{
    style: styleFn,
    pointToLayer: (feat, latlng) => {{
      const val = fieldKey ? String((feat.properties && feat.properties[fieldKey]) ?? "") : "";
      const col = colorMap.get(val) || "#1565C0";
      return L.circleMarker(latlng, {{ radius: 6, fillColor: col, color: "#ffffff", weight: 1.8, fillOpacity: 0.95 }});
    }},
    onEachFeature: (feat, lyr) => {{
      const p = feat.properties || {{}};
      const rows = Object.entries(p).map(([k,v]) => `<tr><td>${{escHtml(k)}}</td><td>${{escHtml(v)}}</td></tr>`).join("");
      if (rows) lyr.bindPopup(`<div style="font-weight:700;margin-bottom:4px;color:#0f172a">${{escHtml(meta.name)}}</div><table class="popup-tbl">${{rows}}</table>`);
    }}
  }}).addTo(map);

  if (g0.includes("Polygon") && !isBoundary) {{
    polygonLayers.push({{ lyr: leafletLyr, styleFn }});
  }}
  try {{
    const b = leafletLyr.getBounds();
    if (b.isValid()) allBounds.extend(b);
  }} catch (_) {{}}

  const row = document.createElement("label");
  row.className = "lyr-item";
  row.innerHTML = `<input type="checkbox" checked /> <span class="lyr-dot" style="background:${{swatchColor}}"></span> <span>${{escHtml(meta.name)}} (${{meta.count}})</span>`;
  row.querySelector("input").addEventListener("change", (ev) => {{
    if (ev.target.checked) leafletLyr.addTo(map);
    else map.removeLayer(leafletLyr);
  }});
  lyrListEl.appendChild(row);

  if (orderedClasses.length > 0 && !legendRendered) {{
    legendRendered = true;
    const leg = document.getElementById("legend");
    const totalArea = Array.from(classStats.values()).reduce((s, v) => s + v.area, 0);
    const totalCnt = fc.features.length;
    let html = `<h3>Keterangan — ${{escHtml(meta.name)}}</h3>`;
    for (const cls of orderedClasses) {{
      const st = classStats.get(cls);
      const col = colorMap.get(cls);
      const pct = totalArea > 0 ? (st.area / totalArea) * 100 : (st.count / totalCnt) * 100;
      const metric = totalArea > 0 ? `${{st.area.toLocaleString('id-ID', {{maximumFractionDigits:1}})}} ha (${{pct.toFixed(1)}}%)` : `${{st.count}} (${{pct.toFixed(1)}}%)`;
      html += `<div class="leg-row"><span class="leg-swatch" style="background:${{col}}"></span><div class="leg-bar-wrap"><div class="leg-label"><span>${{escHtml(cls)}}</span><span>${{metric}}</span></div><div class="leg-bar"><div class="leg-bar-fill" style="width:${{Math.max(4, pct)}}%;background:${{col}}"></div></div></div></div>`;
    }}
    leg.innerHTML = html;
    leg.style.display = "block";
  }}
}});

if (allBounds.isValid()) map.fitBounds(allBounds, {{ padding: [24, 24] }});

document.getElementById("op-rng").addEventListener("input", (e) => {{
  fillOpacity = parseInt(e.target.value, 10) / 100.0;
  document.getElementById("op-val").textContent = e.target.value + "%";
  polygonLayers.forEach(({{ lyr, styleFn }}) => lyr.setStyle(styleFn));
}});
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
        "{} Multi-layer Interactive WebMap ({} layer(s), {} features) -> {}",
        "[OK]".green().bold(),
        layer_count,
        total_features,
        out.display()
    );
    Ok(())
}
