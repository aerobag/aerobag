# Apps4Av Source Assets

This directory holds raw data assets adopted from the Apps4Av Avare
preprocessing tree. Aerobag rewrites the consuming code, but these source data
files retain their upstream provenance.

Current contents:
- chart cutline GeoJSON files used to crop FAA chart TIFFs before tiling
- `geo/geo.csv`, a one-degree grid whose geoid-height column is currently used
  as a terrain vertical-datum approximation

Origin:
- Upstream project: https://github.com/apps4av/avare
- Upstream README: https://github.com/apps4av/avare/blob/master/README.md
- Upstream license: https://github.com/apps4av/avare/blob/master/LICENSE
- Copyright notice from upstream license: Copyright (c) 2012 Apps4Av Inc.

Keep this notice with these files when redistributing the source tree or
derived source packages.
