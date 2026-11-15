"""Recorte de un granule GCOV al AOI (Plan Maestro §6.2, script 02).

Rutas y convenciones tomadas de la especificacion, no de memoria (R11):
JPL D-102274 Rev F, NISAR L2 GCOV Product Specification v1.5.0 (8 jul 2026), seccion 5:
  /science/LSAR/identification/trackNumber, frameNumber, orbitPassDirection, listOfFrequencies
  /science/LSAR/GCOV/grids/frequency[A|B]/
      HHHH ...            Float32, _FillValue nan (terminos diagonales, gamma0 en potencia)
      mask                UByte, 0 invalido, 1-5 valido, 255 relleno
      numberOfLooks       Float32, _FillValue nan
      xCoordinates        Float64 (ancho), centro de pixel
      yCoordinates        Float64 (largo), centro de pixel; malla norte-arriba (Y decrece)
      projection          UInt32 escalar = codigo EPSG
      listOfCovarianceTerms
"""
from __future__ import annotations

from pathlib import Path

import h5py
import numpy as np
from rasterio.crs import CRS
from rasterio.transform import from_origin

from .gcov import DS_LOOKS, DS_MASK, GRID_GROUP, parse_granule_name

IDENTIFICATION = "/science/LSAR/identification"


def open_granule(source: str | Path) -> h5py.File:
    """Abre un .h5 local o, si `source` es una URL https, lo lee por partes desde Earthdata
    (solo baja los bloques que se piden). La URL necesita el login de Earthdata en .netrc."""
    if str(source).startswith("http"):
        try:
            import truststore
            truststore.inject_into_ssl()
        except ImportError:
            pass
        import earthaccess
        try:
            auth = earthaccess.login(strategy="netrc")
        except Exception as exc:  # earthaccess lanza si no encuentra .netrc
            raise RuntimeError("Falta el login de Earthdata: correr earthdata_login.py") from exc
        if not getattr(auth, "authenticated", False):
            raise RuntimeError("Falta el login de Earthdata: correr earthdata_login.py")
        session = earthaccess.get_fsspec_https_session()
        return h5py.File(session.open(str(source), block_size=4 * 1024 * 1024,
                                      cache_type="blockcache"), "r")
    return h5py.File(source, "r")


def _text(dataset) -> str:
    value = dataset[()]
    if isinstance(value, np.ndarray):
        value = value.item() if value.shape == () else value.tolist()
    if isinstance(value, bytes):
        value = value.decode("utf-8")
    return str(value)


def _strings(dataset) -> list[str]:
    return [v.decode("utf-8") if isinstance(v, bytes) else str(v)
            for v in np.atleast_1d(dataset[()])]


def describe(h5: h5py.File, frequency: str, term: str) -> dict:
    """Lo que el propio archivo dice de si mismo; se contrasta con el nombre del granule."""
    ident = h5[IDENTIFICATION]
    frequencies = _strings(ident["listOfFrequencies"])
    if frequency not in frequencies:
        raise ValueError(f"El granule no tiene frecuencia {frequency}; tiene {frequencies}")
    group = h5[GRID_GROUP.format(frequency=frequency)]
    terms = _strings(group["listOfCovarianceTerms"])
    if term not in terms:
        raise ValueError(f"El granule no tiene el termino {term}; tiene {terms}")
    x, y = group["xCoordinates"][:], group["yCoordinates"][:]
    return {
        "track": f"{int(ident['trackNumber'][()]):03d}",
        "frame": f"{int(ident['frameNumber'][()]):03d}",
        "orbit_direction": _text(ident["orbitPassDirection"]).lower(),
        "epsg": int(group["projection"][()]),
        "pixel_spacing_m": float(abs(x[1] - x[0])),
        "line_spacing_m": float(abs(y[0] - y[1])),
    }


def check_against_name(info: dict, granule_name: str) -> dict:
    """El nombre del granule y sus metadatos internos deben decir lo mismo (R12)."""
    scene = parse_granule_name(granule_name)
    for key in ("track", "frame", "orbit_direction"):
        if scene[key] != info[key]:
            raise ValueError(f"{granule_name}: {key} es '{scene[key]}' segun el nombre "
                             f"y '{info[key]}' segun el archivo")
    return scene


def read_window(h5: h5py.File, frequency: str, term: str, bounds) -> dict:
    """Lee solo la ventana que cubre `bounds` = (oeste, sur, este, norte) en el CRS del granule."""
    west, south, east, north = bounds
    group = h5[GRID_GROUP.format(frequency=frequency)]
    x, y = group["xCoordinates"][:], group["yCoordinates"][:]
    if not (np.all(np.diff(x) > 0) and np.all(np.diff(y) < 0)):
        raise ValueError("Se esperaba una malla norte-arriba: X creciente e Y decreciente")
    cols = np.flatnonzero((x >= west) & (x < east))     # centros de pixel dentro del AOI
    rows = np.flatnonzero((y > south) & (y <= north))
    if cols.size == 0 or rows.size == 0:
        raise ValueError("El AOI no cae dentro del granule")
    r0, r1, c0, c1 = rows[0], rows[-1] + 1, cols[0], cols[-1] + 1
    dx, dy = float(x[1] - x[0]), float(y[0] - y[1])
    return {
        "gamma": group[term][r0:r1, c0:c1].astype(np.float32),
        "mask": group[DS_MASK][r0:r1, c0:c1].astype(np.uint8),
        "looks": group[DS_LOOKS][r0:r1, c0:c1].astype(np.float32),
        "transform": from_origin(float(x[c0]) - dx / 2, float(y[r0]) + dy / 2, dx, dy),
        "crs": CRS.from_epsg(int(group["projection"][()])),
        "coverage": (rows.size * cols.size * dx * dy) / ((east - west) * (north - south)),
    }
