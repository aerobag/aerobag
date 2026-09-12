# SPDX-FileCopyrightText: 2026 Aerobag contributors
#
# SPDX-License-Identifier: AGPL-3.0-or-later

"""Tiny real-GDAL inputs for the publication/cache/quarantine regression."""

import importlib.util
import json
from pathlib import Path
import sys

import numpy as np
from osgeo import gdal, osr

root, checker, mode = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
spec = importlib.util.spec_from_file_location('quality_fixture_checker', checker)
quality = importlib.util.module_from_spec(spec)
spec.loader.exec_module(quality)
gdal.UseExceptions()
osr.UseExceptions()
metadata = root / 'chart-metadata'
sources = root / 'source'
sources.mkdir(exist_ok=True)
srs = osr.SpatialReference()
srs.ImportFromEPSG(3857)
srs.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
wgs = osr.SpatialReference()
wgs.ImportFromEPSG(4326)
wgs.SetAxisMappingStrategy(osr.OAMS_TRADITIONAL_GIS_ORDER)
to_geo = osr.CoordinateTransformation(srs, wgs)

if mode == 'create':
    for family in ('SEC', 'TAC', 'FLY'):
        (metadata / family).mkdir(parents=True, exist_ok=True)
    for name, west, color in [('Healthy SEC', -13500000, (30, 160, 230, 255)),
                              ('Suspect SEC', -13400000, (230, 40, 60, 255))]:
        (sources / (name + '.htm')).write_text('<html></html>')
        # A palette matches actual FAA chart inputs, including the RGB expansion.
        image = gdal.GetDriverByName('GTiff').Create(str(sources / (name + '.tif')), 400, 400, 1, gdal.GDT_Byte)
        image.SetProjection(srs.ExportToWkt())
        image.SetGeoTransform((west, 10, 0, 6000000, 0, -10))
        palette = gdal.ColorTable()
        palette.SetColorEntry(0, color)
        palette.SetColorEntry(1, (245, 245, 220, 255))
        image.GetRasterBand(1).SetColorTable(palette)
        y, x = np.indices((400, 400))
        pixels = ((x % 31 < 10) | (y % 37 < 10)).astype(np.uint8)
        image.GetRasterBand(1).WriteArray(pixels)
        image = None
        document = {'type': 'FeatureCollection', 'crs': {'type': 'name', 'properties': {'name': 'EPSG:3857'}},
                    'features': [{'type': 'Feature', 'properties': {}, 'geometry': {'type': 'Polygon',
                                 'coordinates': [[[west, 6000000], [west+4000, 6000000],
                                                  [west+4000, 5996000], [west, 5996000], [west, 6000000]]]}}]}
        (metadata / 'SEC' / (name + '.geojson')).write_text(json.dumps(document))
        if name.startswith('Suspect'):
            controls = []
            for x, y in ((100, 100), (250, 100), (100, 250), (250, 250)):
                lon, lat, _ = to_geo.TransformPoint(west+x*10, 6000000-y*10)
                controls.append({'kind': 'intersection', 'pixel': [x, y], 'latitude': lat, 'longitude': lon})
            insets = [{'id': family, 'enabled': True, 'target_family': family,
                       'projection_wkt': srs.ExportToWkt(),
                       'boundary': [[70, 70], [280, 70], [280, 280], [70, 280]],
                       'control_points': controls} for family in ('TAC', 'FLY')]
            (metadata / 'SEC' / (name + '.navigable-insets.json')).write_text(json.dumps({
                'schema_version': 2, 'source': name+'.tif', 'source_width': 400, 'source_height': 400,
                'insets': insets}))
    mode = 'approve'

if mode == 'approve':
    output = root / 'fixture-review'
    report = quality.run_check(sources, metadata, 'SEC', output, 'fixture', '2605')
    for region in report['regions']:
        quality.approve(output / 'reports' / report['report_id'] / region['id'] / 'candidate.json',
                        metadata, 'fixture reviewer')
elif mode == 'move':
    # Rewrite rather than modify a cached hard link in place.
    path = sources / 'Suspect SEC.tif'
    image = gdal.Open(str(path))
    replacement = sources / 'moved.tif'
    moved = gdal.GetDriverByName('GTiff').CreateCopy(str(replacement), image)
    moved.GetRasterBand(1).WriteArray(np.roll(image.ReadAsArray(), 13, axis=1))
    moved = image = None
    replacement.replace(path)
else:
    raise ValueError(mode)
