#!/usr/bin/env python3
"""
GeoKit Automated Python Validation & Golden Algorithmic Checks (CI + Local).
Verifies:
  1. Syntax compilation (py_compile) of Python 3 runners & add-in builder.
  2. Core HANDLERS parity between arcpy_runner.py and arcmap_runner.py.
  3. Golden Reference tests for AST Safe Math Evaluator (weighted overlay + RCE blocking).
  4. Golden Reference tests for classify interval boundary semantics & NULL handling.
  5. ArcMap Add-In packaging integrity (Config.xml + Install/arcobjects.py + Install/geokit_addin.py).
"""

import ast
import math
import os
import py_compile
import re
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def check_syntax():
    py3_files = [
        "scripts/arcpy_runner.py",
        "scripts/arcgis_pro_live_bridge.py",
        "addin/arcmap/build_addin.py",
    ]
    for rel in py3_files:
        full = os.path.join(ROOT, rel)
        py_compile.compile(full, doraise=True)
    print("[OK] Python 3 py_compile passed (%d files)" % len(py3_files))


def check_handlers_parity():
    def extract_handlers(rel):
        src = open(os.path.join(ROOT, rel), "r", encoding="utf-8").read()
        m = re.search(r"HANDLERS\s*=\s*\{(.*?)\}", src, re.S)
        assert m, f"HANDLERS dict not found in {rel}"
        return set(re.findall(r'"([a-z0-9_]+)"\s*:', m.group(1)))

    pro_h = extract_handlers("scripts/arcpy_runner.py")
    am_h = extract_handlers("scripts/arcmap_runner.py")
    # Every ArcMap handler must exist in ArcGIS Pro runner
    missing_in_pro = am_h - pro_h
    assert not missing_in_pro, f"Handlers in arcmap_runner missing from arcpy_runner: {missing_in_pro}"
    # Core GIS/raster/hydro/QA handlers must exist in BOTH
    core_required = {
        "info", "project", "clip", "merge", "pack", "unpack", "repair",
        "buffer", "dissolve", "calc_geom", "intersect", "erase", "spatial_join",
        "gp_tools", "gp_tool_usage", "gp_run", "xy2shp", "convert",
        "values", "classify", "calc_field", "union", "qa_check",
        "raster_mosaic", "raster_terrain", "raster_zonal_stats", "raster_reclass",
        "raster_weighted_sum", "raster_interpolate", "raster_index", "raster_contour",
        "hydro_delineate", "hydro_basin",
    }
    assert core_required.issubset(pro_h), f"Missing core handlers in Pro: {core_required - pro_h}"
    assert core_required.issubset(am_h), f"Missing core handlers in ArcMap: {core_required - am_h}"
    print(f"[OK] HANDLERS parity verified (Pro={len(pro_h)}, ArcMap={len(am_h)}, Core={len(core_required)})")


def check_golden_safe_math_and_classify():
    src = open(os.path.join(ROOT, "scripts/arcpy_runner.py"), "r", encoding="utf-8").read()

    # 1. AST Safe Math Golden Reference
    s1 = src.index("def _compile_safe_math(")
    e1 = src.index("def sanitize_gdb_name(")
    ns = {}
    exec(src[s1:e1], ns)

    funcs = {
        "abs": abs, "min": min, "max": max, "round": round,
        "int": int, "float": float, "str": str, "len": len,
        "sqrt": math.sqrt, "log": math.log, "pow": math.pow,
    }
    # Golden weighted overlay: 0.30*4 + 0.50*3 + 0.20*3.5 = 1.2 + 1.5 + 0.7 = 3.4
    tree = ns["_compile_safe_math"]("0.30 * S_LERENG + 0.50 * S_HUJAN + 0.20 * S_TANAH")
    res = ns["_eval_safe_math_node"](tree, {"S_LERENG": 4, "S_HUJAN": 3, "S_TANAH": 3.5}, funcs)
    assert abs(res - 3.4) < 1e-9, f"Expected 3.4, got {res}"

    # Golden conditional expression
    t2 = ns["_compile_safe_math"]("1 if X < 0 else round(sqrt(X), 2)")
    assert ns["_eval_safe_math_node"](t2, {"X": -5}, funcs) == 1
    assert ns["_eval_safe_math_node"](t2, {"X": 9}, funcs) == 3.0

    # Security: RCE payloads MUST raise ValueError
    for bad in [
        "().__class__.__base__.__subclasses__()",
        "__import__('os').system('echo pwn')",
        "open('/etc/passwd')",
        "_hidden + 1",
    ]:
        try:
            ns["_eval_safe_math_node"](ns["_compile_safe_math"](bad), {}, funcs)
            raise AssertionError(f"Unsafe expression was NOT blocked: {bad}")
        except ValueError:
            pass

    # 2. Classify Boundary Golden Reference
    s2 = src.index("def _cast_value(")
    e2 = src.index("def cmd_values(")
    ns2 = {}
    exec(src[s2:e2], ns2)

    rules, ftype = ns2["_parse_classify_rules"]("<=8:1;8-15:2;15-25:3;>25:4", None)
    assert ftype == "LONG"
    assert ns2["_match_classify_rule"](-2.0, rules) == (1, True)
    assert ns2["_match_classify_rule"](8.0, rules) == (1, True)     # exact boundary: first rule wins
    assert ns2["_match_classify_rule"](8.01, rules) == (2, True)
    assert ns2["_match_classify_rule"](15.0, rules) == (2, True)
    assert ns2["_match_classify_rule"](25.0, rules) == (3, True)
    assert ns2["_match_classify_rule"](25.1, rules) == (4, True)
    assert ns2["_match_classify_rule"](None, rules) == (None, False)
    assert ns2["_match_classify_rule"]("not-a-number", rules) == (None, False)

    cat_rules, cat_type = ns2["_parse_classify_rules"]("Hutan=1;Sawah=3;Permukiman=5", None)
    assert cat_type == "LONG"
    assert ns2["_match_classify_rule"]("  hutan ", cat_rules) == (1, True)
    assert ns2["_match_classify_rule"]("Sawah", cat_rules) == (3, True)
    assert ns2["_match_classify_rule"]("Unknown", cat_rules) == (None, False)
    print("[OK] Golden AST Safe Math & Classify boundary tests passed")


def check_addin_zip():
    addin = os.path.join(ROOT, "addin/arcmap/geokit-arcmap.esriaddin")
    assert os.path.isfile(addin), f"Missing add-in archive: {addin}"
    with zipfile.ZipFile(addin, "r") as zf:
        names = set(zf.namelist())
    required = {"Config.xml", "Install/geokit_addin.py", "Install/arcobjects.py"}
    missing = required - names
    assert not missing, f"Add-in ZIP missing required files: {missing} (found: {names})"
    print(f"[OK] ArcMap Add-In ZIP verified ({len(names)} files)")


def check_jenks_klhk_and_perkabig_papers():
    src = open(os.path.join(ROOT, "addin/arcmap/Install/arcobjects.py"), "r", encoding="utf-8").read()
    # Extract pure-Python helper functions (KLHK + Jenks + PAPER_SPECS + _select_scalebar_division)
    # Note: arcobjects.py uses unicode/basestring for Py2.7, shim them in Py3 test namespace
    ns = {"unicode": str, "basestring": str}
    s_pal = src.index("PALETTES = {")
    e_pal = src.index("LINE_STYLES = {")
    exec(src[s_pal:e_pal], ns)

    s_spec = src.index("PAPER_SPECS = {")
    e_spec = src.index("def build_perkabig_layout(")
    exec(src[s_spec:e_spec], ns)

    # 1. Verify A4/A3/A1/A0 paper specs
    for p in ("a4", "a3", "a1", "a0"):
        assert p in ns["PAPER_SPECS"], f"Missing PerKaBIG paper size: {p}"
    assert ns["PAPER_SPECS"]["a1"][:2] == (84.1, 59.4)
    assert ns["PAPER_SPECS"]["a0"][:2] == (118.9, 84.1)

    # 2. Verify physical ScaleBar division selection fits slot cleanly
    use_m, div_val, bar_cm = ns["_select_scalebar_division"](50000, 5.8)
    assert not use_m and div_val == 1.0 and abs(bar_cm - 4.0) < 0.05, (use_m, div_val, bar_cm)

    # 3. Verify KLHK 23-class official palette & semantic matching
    assert len(ns["PALETTES"]["klhk"]) == 23
    assert ns["match_klhk_color"]("Hutan Lahan Kering Primer") == (0, 68, 27)
    assert ns["match_klhk_color"]("Sawah") == (204, 255, 102)
    assert ns["match_klhk_color"]("Permukiman") == (227, 26, 28)
    assert ns["match_klhk_color"]("Badan Air") == (33, 113, 181)

    # 4. Verify Fisher-Jenks Natural Breaks finds natural clusters [1..3], [10..12], [50..53]
    vals = [1, 2, 2.5, 3, 10, 11, 12, 50, 51, 53]
    brks = ns["compute_jenks_breaks"](vals, 3)
    assert len(brks) == 3 and brks == [3.0, 12.0, 53.0], f"Unexpected Jenks breaks: {brks}"
    print("[OK] PerKaBIG A4/A3/A1/A0, ScaleBar centering math, KLHK 23-class, & Jenks Natural Breaks passed")


if __name__ == "__main__":
    check_syntax()
    check_handlers_parity()
    check_golden_safe_math_and_classify()
    check_addin_zip()
    check_jenks_klhk_and_perkabig_papers()
    print("ALL PYTHON & GOLDEN CHECKS PASSED!")
