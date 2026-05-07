import { latLonToWorld, worldToScreen, type MapViewportState } from "./mapViewport";

export type PointVectorRecord = {
  id: string;
  kind: string;
  lat: number;
  lon: number;
  label: string;
  style_class: string;
  towered?: boolean;
  fuel_available?: boolean;
  has_paved_runway?: boolean | null;
  heliport?: boolean | null;
  has_water_runway?: boolean | null;
  longest_runway_heading_true_deg?: number | null;
  obstacle?: {
    height_agl_ft: number;
    elevation_msl_ft: number;
    top_msl_ft: number;
    is_tall: boolean;
  } | null;
};

export type PointTilePayload = {
  schema_version: number;
  layer: string;
  z: number;
  x: number;
  y: number;
  records: PointVectorRecord[];
};

export type VisiblePointRecord = {
  id: string;
  label: string;
  x: number;
  y: number;
};

type VisibleTile = {
  z: number;
  x: number;
  y: number;
  key: string;
};

const WORLD_SIZE = 256;

export function visibleTileWindow(
  zoom: number,
  viewport: MapViewportState,
  width: number,
  height: number,
): VisibleTile[] {
  if (width <= 0 || height <= 0) {
    return [];
  }
  const scale = 2 ** viewport.zoom;
  const minWorldX = viewport.centerWorldX - width / 2 / scale;
  const maxWorldX = viewport.centerWorldX + width / 2 / scale;
  const minWorldY = viewport.centerWorldY - height / 2 / scale;
  const maxWorldY = viewport.centerWorldY + height / 2 / scale;
  const tileWorldSize = WORLD_SIZE / (2 ** zoom);
  const xStart = Math.max(0, Math.floor(minWorldX / tileWorldSize));
  const xEnd = Math.min((2 ** zoom) - 1, Math.floor(maxWorldX / tileWorldSize));
  const yStart = Math.max(0, Math.floor(minWorldY / tileWorldSize));
  const yEnd = Math.min((2 ** zoom) - 1, Math.floor(maxWorldY / tileWorldSize));
  const tiles: VisibleTile[] = [];

  for (let y = yStart; y <= yEnd; y += 1) {
    for (let x = xStart; x <= xEnd; x += 1) {
      tiles.push({ z: zoom, x, y, key: tileKey(zoom, x, y) });
    }
  }

  return tiles;
}

export function visiblePointRecords(
  payloads: PointTilePayload[],
  viewport: MapViewportState,
  width: number,
  height: number,
): VisiblePointRecord[] {
  const visible: VisiblePointRecord[] = [];
  for (const payload of payloads) {
    for (const record of payload.records) {
      const point = worldToScreen(viewport, latLonToWorld(record.lat, record.lon), width, height);
      visible.push({
        id: record.id,
        label: record.label.trim().toUpperCase(),
        x: point.x,
        y: point.y,
      });
    }
  }
  return visible;
}

type MetarFastProductManifestInfo = {
  manifestJson: string;
  tilePathTemplate: string;
};

let metarFastProductManifestPromise: Promise<MetarFastProductManifestInfo> | null = null;

async function loadMetarFastProductManifest(signal?: AbortSignal): Promise<MetarFastProductManifestInfo> {
  if (!metarFastProductManifestPromise) {
    metarFastProductManifestPromise = fetch("/fast-products/metars/manifest.json", {
      cache: "no-cache",
      signal,
    })
      .then(async (response) => {
        if (!response.ok) {
          throw new Error(`failed to load METAR manifest: ${response.status}`);
        }
        const manifestJson = await response.text();
        const manifest = JSON.parse(manifestJson) as {
          map_view?: {
            tile_path_template?: unknown;
          };
        };
        const template = manifest.map_view?.tile_path_template;
        if (typeof template !== "string" || template.length === 0) {
          throw new Error("METAR manifest missing map_view.tile_path_template");
        }
        return { manifestJson, tilePathTemplate: template };
      })
      .catch((error) => {
        metarFastProductManifestPromise = null;
        throw error;
      });
  }
  return metarFastProductManifestPromise;
}

export async function loadMetarFastProductManifestJson(signal?: AbortSignal): Promise<string> {
  return (await loadMetarFastProductManifest(signal)).manifestJson;
}

export async function loadMetarTilePathTemplate(signal?: AbortSignal): Promise<string> {
  return (await loadMetarFastProductManifest(signal)).tilePathTemplate;
}

export function metarTileUrl(tilePathTemplate: string, zoom: number, x: number, y: number) {
  const relativePath = tilePathTemplate
    .replaceAll("{z}", String(zoom))
    .replaceAll("{x}", String(x))
    .replaceAll("{y}", String(y));
  return `/fast-products/metars/${relativePath}`;
}

export function tileKey(z: number, x: number, y: number) {
  return `${z}/${x}/${y}`;
}
