# -*- coding: utf-8 -*-
"""
Core COM / ctypes bindings, verified ArcObjects 10.x GUIDs, locale-safe number helpers,
and basic MxDocument/layer/geometry utilities for ArcMap 10.1-10.8 (32-bit Python 2.7).
"""

import ctypes
from ctypes import (
    POINTER,
    Structure,
    byref,
    c_double,
    c_long,
    c_short,
    c_ulong,
    c_ushort,
    c_void_p,
    c_wchar_p,
)
import uuid

ole32 = ctypes.windll.ole32
oleaut32 = ctypes.windll.oleaut32
oleaut32.SysAllocString.argtypes = [c_wchar_p]
oleaut32.SysAllocString.restype = c_void_p
oleaut32.SysFreeString.argtypes = [c_void_p]

CLSCTX_ALL = 0x17


class GUID(Structure):
    _fields_ = [
        ("Data1", c_ulong),
        ("Data2", c_ushort),
        ("Data3", c_ushort),
        ("Data4", ctypes.c_ubyte * 8),
    ]


class VARIANT(Structure):
    _fields_ = [
        ("vt", c_ushort),
        ("wReserved1", c_ushort),
        ("wReserved2", c_ushort),
        ("wReserved3", c_ushort),
        ("val", c_void_p),
        ("pad", c_void_p),
    ]


def _guid(s):
    g = GUID()
    ctypes.memmove(byref(g), uuid.UUID(s).bytes_le, 16)
    return g


def _to_float(val, default=0.0):
    """Locale-safe float parser (handles Windows Indonesian decimal commas like '1,5')."""
    if val is None:
        return float(default)
    if isinstance(val, (int, float)):
        return float(val)
    try:
        s = unicode(val).strip()
    except Exception:
        s = str(val).strip()
    if not s:
        return float(default)
    try:
        return float(s)
    except ValueError:
        pass
    # Handle single comma decimal separator ("1,5" -> "1.5") or Indonesian thousands ("1.250,5")
    if "," in s and "." in s:
        if s.rfind(",") > s.rfind("."):
            s = s.replace(".", "").replace(",", ".")
        else:
            s = s.replace(",", "")
    elif "," in s:
        s = s.replace(",", ".")
    try:
        return float(s)
    except ValueError:
        return float(default)


# Verified ArcObjects 10.x CLSIDs & IIDs
CLSID_AppROT = _guid("{FABC30FB-D273-11D2-9F36-00C04F6BC61A}")
IID_IAppROT = _guid("{FABC30F9-D273-11D2-9F36-00C04F6BC61A}")
IID_IMxDocument = _guid("{0522A5F1-487C-11D0-98BD-00805F7CED21}")
IID_ILayer = _guid("{34C20002-4D3C-11D0-92D8-00805F7C28B0}")
IID_IGeoFeatureLayer = _guid("{E156D7E5-22AF-11D3-9F99-00C04F6BC78E}")
IID_IFeatureRenderer = _guid("{40A9E884-5533-11D0-98BE-00805F7CED21}")
IID_ILayerEffects = _guid("{09998199-1FB8-11D3-AB4E-000000000000}")
IID_IRasterLayer = _guid("{D02371C7-35F7-11D2-B1F2-00C04F8EDEFF}")
IID_IRasterRenderer = _guid("{6F5322D1-3790-11D2-8D26-0000F8780535}")
IID_IActiveView = _guid("{DD94D76B-836D-11D0-87EC-080009EC732A}")
IID_ILegend = _guid("{3141F307-38E2-11D1-8809-080009EC732A}")
IID_ILegend3 = _guid("{15ABE7F7-49E8-4012-BE72-D14E5A755ACE}")
IID_ILegendFormat = _guid("{52AA5BC1-CB21-11D0-87F9-080009EC732A}")
IID_ILegendClassFormat = _guid("{52AA5BC2-CB21-11D0-87F9-080009EC732A}")
IID_ITransform2D = _guid("{3BE78AD0-886E-11D1-BC78-0000F875BCCE}")

CLSID_RgbColor = _guid("{7EE9C496-D123-11D0-8383-080009B996CC}")
IID_IColor = _guid("{20CD40B0-8F3D-11D0-8590-0800091A2A72}")
IID_IRgbColor = _guid("{20CD40B1-8F3D-11D0-8590-0800091A2A72}")
IID_ISymbol = _guid("{F3435802-5779-11D0-98BF-00805F7CED21}")
IID_ILineSymbol = _guid("{E6BDAA7D-4D35-11D0-98BE-00805F7CED21}")
IID_IColorRamp = _guid("{BEB87091-C0B4-11D0-8379-080009B996CC}")

CLSID_SimpleLineSymbol = _guid("{7914E5F9-C892-11D0-8BB6-080009EE4E41}")
IID_ISimpleLineSymbol = _guid("{A9360291-5828-11D0-98BF-00805F7CED21}")

CLSID_CartographicLineSymbol = _guid("{7914E5FB-C892-11D0-8BB6-080009EE4E41}")
IID_ICartographicLineSymbol = _guid("{B04BC359-C36E-11D0-BFA1-0080C7E24280}")
IID_ILineProperties = _guid("{B04BC357-C36E-11D0-BFA1-0080C7E24280}")
CLSID_Template = _guid("{41093A71-CCE1-11D0-BFAA-0080C7E24280}")
IID_ITemplate = _guid("{7914E5F4-C892-11D0-8BB6-080009EE4E41}")

CLSID_TextSymbol = _guid("{B65A3E74-2993-11D1-9A43-0080C7EC5C96}")
IID_ITextSymbol = _guid("{A80B5E91-7F9C-11D0-9410-080009EEBECB}")
IID_IMask = _guid("{6A7EF984-6924-11D2-980D-0080C7E04196}")

IID_IFillSymbol = _guid("{E6BDAA7E-4D35-11D0-98BE-00805F7CED21}")
IID_ILabelEngineLayerProperties = _guid("{01004143-0D1C-11D2-A26F-080009B6F22B}")

IID_IPageLayout = _guid("{DD94D76A-836D-11D0-87EC-080009EC732A}")
IID_IPage = _guid("{2AB4981F-9406-11D0-87EF-080009EC732A}")
IID_IGraphicsContainer = _guid("{D09411D1-4BB4-11D1-B6CC-080009B996CC}")
IID_IElement = _guid("{9B001319-9ED7-11D0-859E-0800091A2A72}")
IID_IElementProperties = _guid("{86F32B61-7667-11D2-A2D3-080009B6F22B}")
IID_ITextElement = _guid("{204034D2-F6EA-11D0-83AD-080009B996CC}")
CLSID_TextElement = _guid("{204034D3-F6EA-11D0-83AD-080009B996CC}")
CLSID_RectangleElement = _guid("{3A9767C2-F253-11D0-83A4-080009B996CC}")
IID_IFillShapeElement = _guid("{3E150C0D-19F5-11D3-9F9E-00C04F6BC8DD}")
IID_IMapFrame = _guid("{9B00131B-9ED7-11D0-859E-0800091A2A72}")
IID_IMapSurroundFrame = _guid("{9B00131C-9ED7-11D0-859E-0800091A2A72}")
IID_IMapGrids = _guid("{08E7D005-9AAE-11D2-AE7B-080009EC732A}")
CLSID_MeasuredGrid = _guid("{03762C90-F4D0-11D1-ADE8-080009EC732A}")
IID_IMeasuredGrid = _guid("{4F69021F-A5C1-11D2-AE82-080009EC732A}")
IID_IMapGrid = _guid("{03762C8B-F4D0-11D1-ADE8-080009EC732A}")

CLSID_Envelope = _guid("{30707212-52D5-11D0-A8F2-00608C85EDE5}")
IID_IEnvelope = _guid("{1B077914-38C5-11D0-92D2-00805F7C28B0}")
CLSID_Point = _guid("{00A5CB41-52DA-11D0-A8F2-00608C85EDE5}")
IID_IPoint = _guid("{1B077915-38C5-11D0-92D2-00805F7C28B0}")
IID_IGeometry = _guid("{1B07790D-38C5-11D0-92D2-00805F7C28B0}")

CLSID_UID = _guid("{78FF7FA1-FB2F-11D1-94A2-080009EEBECB}")
IID_IUID = _guid("{1714D59B-FB22-11D1-94A2-080009EEBECB}")

IID_IScaleBar = _guid("{6DFAE8E2-ECAA-11D0-8682-0000F8751720}")
IID_INorthArrow = _guid("{7A3F91F1-B9E3-11D1-8756-0000F8751720}")
IID_ILegendItem = _guid("{52AA5BC0-CB21-11D0-87F9-080009EC732A}")
CLSID_FormattedGridLabel = _guid("{CE41C507-9DF9-11D2-AADE-000000000000}")
IID_IFormattedGridLabel = _guid("{CE41C505-9DF9-11D2-AADE-000000000000}")
IID_IGridLabel = _guid("{CE41C501-9DF9-11D2-AADE-000000000000}")
CLSID_NumericFormat = _guid("{7E4F4719-8E54-11D2-AAD8-000000000000}")
IID_INumericFormat = _guid("{7E4F4710-8E54-11D2-AAD8-000000000000}")
IID_INumberFormat = _guid("{7E4F470D-8E54-11D2-AAD8-000000000000}")
IID_IFont = _guid("{BEF6E002-A874-101A-8BBA-00AA00300CAB}")
CLSID_MapFrame = _guid("{DD94D770-836D-11D0-87EC-080009EC732A}")
CLSID_Map = _guid("{E6BDAA76-4D35-11D0-98BE-00805F7CED21}")
IID_IMap = _guid("{E6BDAA75-4D35-11D0-98BE-00805F7CED21}")
IID_IMaps = _guid("{DFD9FF9F-3080-11D2-94C9-080009EEBECB}")
CLSID_LocatorRectangle = _guid("{83FFCAE2-EDCA-11D0-8683-0000F8751720}")
IID_ILocatorRectangle = _guid("{41D9C095-A403-11D2-87E8-0000F8751720}")
CLSID_SymbolBorder = _guid("{A5D0F017-62DD-11D2-87BE-0000F8751720}")
IID_ISymbolBorder = _guid("{E4240308-DD41-11D2-B868-00600802E603}")
IID_IBorder = _guid("{E4240307-DD41-11D2-B868-00600802E603}")
IID_IFrameProperties = _guid("{D1B1B855-5885-4D7F-A444-A02E3E1078C3}")
CLSID_PngPictureElement = _guid("{85CD6330-D45A-11D3-A414-0004AC1B1D86}")
IID_IPictureElement = _guid("{C0A341C2-CFE2-11D1-9140-0000F87808EE}")
CLSID_Graticule = _guid("{03762C8F-F4D0-11D1-ADE8-080009EC732A}")
CLSID_DMSGridLabel = _guid("{CE41C506-9DF9-11D2-AADE-000000000000}")
IID_IDMSGridLabel = _guid("{CE41C503-9DF9-11D2-AADE-000000000000}")
CLSID_PolygonElement = _guid("{3A9767C7-F253-11D0-83A4-080009B996CC}")
CLSID_Polygon = _guid("{00A5CB42-52DA-11D0-A8F2-00608C85EDE5}")
IID_IPointCollection = _guid("{DE08A214-4FC2-11D1-834E-0000F8775BE9}")

CLSID_SimpleFillSymbol = _guid("{7914E603-C892-11D0-8BB6-080009EE4E41}")
IID_ISimpleFillSymbol = _guid("{A9360292-5828-11D0-98BF-00805F7CED21}")

CLSID_SimpleMarkerSymbol = _guid("{7914E5FE-C892-11D0-8BB6-080009EE4E41}")
IID_ISimpleMarkerSymbol = _guid("{A9360290-5828-11D0-98BF-00805F7CED21}")

CLSID_SimpleRenderer = _guid("{F3435801-5779-11D0-98BF-00805F7CED21}")
IID_ISimpleRenderer = _guid("{F3435800-5779-11D0-98BF-00805F7CED21}")

CLSID_UniqueValueRenderer = _guid("{C3346D29-B2BC-11D1-8817-080009EC732A}")
IID_IUniqueValueRenderer = _guid("{C3346D2A-B2BC-11D1-8817-080009EC732A}")

CLSID_ClassBreaksRenderer = _guid("{AE5F7EA2-8B48-11D0-8356-080009B996CC}")
IID_IClassBreaksRenderer = _guid("{AE5F7EA1-8B48-11D0-8356-080009B996CC}")

CLSID_AlgorithmicColorRamp = _guid("{BEB8709B-C0B4-11D0-8379-080009B996CC}")
IID_IAlgorithmicColorRamp = _guid("{BEB87096-C0B4-11D0-8379-080009B996CC}")

CLSID_MultiPartColorRamp = _guid("{BEB87099-C0B4-11D0-8379-080009B996CC}")
IID_IMultiPartColorRamp = _guid("{BEB87098-C0B4-11D0-8379-080009B996CC}")

CLSID_RasterStretchColorRampRenderer = _guid("{A301A3B2-74D7-11D2-9F29-00C04F8ED1D7}")
IID_IRasterStretchColorRampRenderer = _guid("{A301A3B1-74D7-11D2-9F29-00C04F8ED1D7}")

SURROUND_LEGEND_CLSID = u"{7A3F91E4-B9E3-11D1-8756-0000F8751720}"
SURROUND_NORTH_CLSID = u"{7A3F91DD-B9E3-11D1-8756-0000F8751720}"
SURROUND_SCALEBAR_CLSID = u"{6589F147-F7F7-11D2-B872-00600802E603}"


def _vcall(ptr, idx, *argtypes):
    vtbl = ctypes.cast(ptr, POINTER(c_void_p))[0]
    fn = ctypes.cast(vtbl, POINTER(c_void_p))[idx]
    return ctypes.WINFUNCTYPE(c_long, c_void_p, *argtypes)(fn)


def _release(ptr):
    if ptr and getattr(ptr, "value", None):
        try:
            _vcall(ptr, 2)(ptr)
        except Exception:
            pass


def _qi(ptr, iid):
    out = c_void_p()
    hr = _vcall(ptr, 0, POINTER(GUID), POINTER(c_void_p))(ptr, byref(iid), byref(out))
    if hr != 0:
        return None
    return out


def _co(clsid, iid):
    out = c_void_p()
    hr = ole32.CoCreateInstance(byref(clsid), None, CLSCTX_ALL, byref(iid), byref(out))
    if hr != 0:
        raise RuntimeError("ArcObjects CoCreateInstance failed hr=0x%08X" % (hr & 0xFFFFFFFF))
    return out


def _bstr(s):
    return oleaut32.SysAllocString(unicode(s))


def _free_bstr(b):
    if b:
        oleaut32.SysFreeString(b)


def get_current_mxdoc():
    rot = _co(CLSID_AppROT, IID_IAppROT)
    cnt = c_long(0)
    _vcall(rot, 5, POINTER(c_long))(rot, byref(cnt))
    if cnt.value <= 0:
        _release(rot)
        raise RuntimeError("No running ArcMap instance found in in-process AppROT.")
    app = c_void_p()
    _vcall(rot, 6, c_long, POINTER(c_void_p))(rot, 0, byref(app))
    doc = c_void_p()
    _vcall(app, 8, POINTER(c_void_p))(app, byref(doc))
    mxdoc = _qi(doc, IID_IMxDocument)
    _release(doc)
    _release(app)
    _release(rot)
    if not mxdoc:
        raise RuntimeError("Active ArcGIS document is not an ArcMap MxDocument.")
    return mxdoc


def find_com_layer(mxdoc, layer_name):
    fmap = c_void_p()
    _vcall(mxdoc, 9, POINTER(c_void_p))(mxdoc, byref(fmap))
    cnt = c_long(0)
    _vcall(fmap, 8, POINTER(c_long))(fmap, byref(cnt))
    target = layer_name.lower()
    partial = None
    for i in range(cnt.value):
        lyr = c_void_p()
        _vcall(fmap, 9, c_long, POINTER(c_void_p))(fmap, i, byref(lyr))
        bn = c_void_p()
        _vcall(lyr, 3, POINTER(c_void_p))(lyr, byref(bn))
        nm = ctypes.wstring_at(bn) if bn.value else u""
        _free_bstr(bn)
        if nm.lower() == target:
            if partial is not None:
                _release(partial[0])
            _release(fmap)
            return lyr, nm
        if target in nm.lower() and partial is None:
            partial = (lyr, nm)
        else:
            _release(lyr)
    _release(fmap)
    if partial is not None:
        return partial
    return None, None


def refresh_mxdoc(mxdoc):
    try:
        _vcall(mxdoc, 16)(mxdoc)  # UpdateContents()
    except Exception:
        pass
    try:
        av = c_void_p()
        _vcall(mxdoc, 5, POINTER(c_void_p))(mxdoc, byref(av))
        if av.value:
            _vcall(av, 31)(av)  # Refresh()
            _release(av)
    except Exception:
        pass


def set_layer_transparency(lyr, transparency):
    if transparency is None:
        return
    eff = _qi(lyr, IID_ILayerEffects)
    if eff:
        _vcall(eff, 9, c_short)(eff, int(transparency))
        _release(eff)


def _make_envelope(xmin, ymin, xmax, ymax):
    env = _co(CLSID_Envelope, IID_IEnvelope)
    _vcall(env, 57, c_double, c_double, c_double, c_double)(
        env, _to_float(xmin), _to_float(ymin), _to_float(xmax), _to_float(ymax)
    )
    return env


def _make_point(x, y):
    pt = _co(CLSID_Point, IID_IPoint)
    _vcall(pt, 16, c_double, c_double)(pt, _to_float(x), _to_float(y))
    return pt


def _make_uid(guid_str):
    uid = _co(CLSID_UID, IID_IUID)
    bs = _bstr(guid_str)
    v = VARIANT()
    v.vt = 8  # VT_BSTR
    v.val = bs
    _vcall(uid, 8, VARIANT)(uid, v)  # put_Value
    _free_bstr(bs)
    return uid
