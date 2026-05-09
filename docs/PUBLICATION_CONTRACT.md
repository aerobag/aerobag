Both clients start from the static discovery root:

<staticBaseUrl>/current-artifacts.json

The canonical public static base URL is:

https://aerobag.org/packages

So the public discovery entrypoint is:

https://aerobag.org/packages/current-artifacts.json

Use `/packages` because it is human-comprehensible for Android users entering a
host URL: this tree contains downloadable Aerobag data packages, not generic web
assets or implementation-static files. Keep it one path component below `/` so
the public site can serve or redirect the whole product tree independently from
the web app.

That file is the moving alias for “what is current now.” It lists immutable
bundle manifests, currently the cycle bundle and fast bundle, with
relative_path, checksum, size, timestamps, and ids. Historical timestamped
current_artifacts_*.json files may also exist for tests, but production
clients normally start at current-artifacts.json.

From there:

1. Fetch each referenced bundle manifest:
   <staticBaseUrl>/<bundle.relative_path>
2. Read packages[] in each bundle. Each package has id, family_id, region_id,
   relative_path, size_bytes, checksum_sha256, effective_date, and optional
   expiration_date.
3. For offline/package sync, download package zips directly:
   <staticBaseUrl>/<package.relative_path>
   Then verify size/checksum and install atomically.
4. For web’s unpacked/dev-style access, use the staged static paths published
   from those same packages. Examples:
    - /nav-kv/root and /nav-kv/values/<page>
    - /sectional-packages/<package_name>/tiles/...
    - /shaded-relief-products/<package_name>/tiles/...
    - /world-basemap-products/<package_name>/tiles/...
    - /fast-products/<product>/...
5. The nav/HAD KV root is loaded from /nav-kv/root; values are lazy-loaded
   from /nav-kv/values/<page>. HAD then provides semantic catalogs like chart/
   catalog, package metadata, plate indexes, vectors, geometry, etc.

Rule of thumb: clients do not infer filenames. They discover immutable
artifacts from current-artifacts.json, follow relative_path, and use HAD/
catalog metadata for internal paths and tile planning.
