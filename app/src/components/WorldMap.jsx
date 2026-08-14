import { useMemo, useRef, useEffect } from "react";
import DeckGL from "@deck.gl/react";
import { GeoJsonLayer } from "@deck.gl/layers";
import { Map } from "react-map-gl/maplibre";
import "maplibre-gl/dist/maplibre-gl.css";

// World terrain beneath the political polygons: free Amazon Terrain Tiles
// (terrarium encoding) rendered as hillshade + real 3D terrain. Falls back
// to a flat ocean when offline or tiles are unavailable.
const MAP_STYLE = {
  version: 8,
  sources: {
    dem: {
      type: "raster-dem",
      tiles: ["https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png"],
      encoding: "terrarium",
      tileSize: 256,
      maxzoom: 15,
      attribution: "Terrain: © Amazon Web Services",
    },
    graticule: { type: "geojson", data: graticuleGeoJSON() },
  },
  layers: [
    { id: "ocean", type: "background", paint: { "background-color": "#0a1420" } },
    {
      id: "hillshade",
      type: "hillshade",
      source: "dem",
      paint: {
        "hillshade-exaggeration": 0.5,
        "hillshade-shadow-color": "#04070d",
        "hillshade-highlight-color": "#3d4d68",
        "hillshade-accent-color": "#182741",
      },
    },
    {
      id: "graticule",
      type: "line",
      source: "graticule",
      paint: { "line-color": "#243248", "line-width": 0.5, "line-opacity": 0.3 },
    },
  ],
};

export default function WorldMap({ geojson, selected, onSelect, onJumpToFirst, focusId }) {
  const mapRef = useRef(null);

  const layer = useMemo(() => {
    if (!geojson || !geojson.features) return null;
    return new GeoJsonLayer({
      id: "territories",
      data: geojson,
      stroked: true,
      filled: true,
      extruded: true,
      pickable: true,
      lineWidthMinPixels: 1.2,
      getFillColor: (f) => hexToRgb(f.properties.color || "#888888"),
      getLineColor: [10, 15, 25, 255],
      getElevation: (f) => {
        // 2.5D extrusion: bigger nations pop more.
        const pop = Number(f.properties.population || 0);
        return Math.min(600000, pop / 300000) + 10;
      },
      material: { ambient: 0.7, diffuse: 0.6, shininess: 32, specularColor: [60, 64, 80] },
      onClick: (info) => {
        if (info.object && onSelect) onSelect(info.object.properties);
      },
      updateTriggers: { getFillColor: geojson, getElevation: geojson },
    });
  }, [geojson, onSelect]);

  // White outline around the focused nation so map clicks read back visually.
  const focusLayer = useMemo(() => {
    if (!geojson || !geojson.features || !focusId) return null;
    return new GeoJsonLayer({
      id: "focus",
      data: {
        ...geojson,
        features: geojson.features.filter((f) => f.properties.owner === focusId),
      },
      stroked: true,
      filled: false,
      lineWidthMinPixels: 2.5,
      getLineColor: [255, 255, 255],
      pickable: false,
    });
  }, [geojson, focusId]);

  useEffect(() => {
    if (mapRef.current) mapRef.current.resize();
  }, []);

  const empty = !geojson || !geojson.features || geojson.features.length === 0;

  return (
    <div style={{ position: "absolute", inset: 0 }}>
      <DeckGL
        initialViewState={{ longitude: 10, latitude: 30, zoom: 1.3, pitch: 45, bearing: 0 }}
        controller={true}
        layers={layer ? [layer, focusLayer].filter(Boolean) : []}
        style={{ position: "absolute", inset: 0 }}
      >
        <Map
          ref={mapRef}
          mapStyle={MAP_STYLE}
          attributionControl={false}
          terrain={{ source: "dem", exaggeration: 0.25 }}
        />
      </DeckGL>
      {empty && onJumpToFirst && (
        <div className="map-empty">
          <div className="map-empty-title">No nations yet</div>
          <div className="map-empty-text">
            Human civilizations appear around 3200 BCE. Move the timeline forward to see the world
            take shape.
          </div>
          <button onClick={onJumpToFirst}>Jump to 3200 BCE</button>
        </div>
      )}
    </div>
  );
}

function hexToRgb(hex) {
  const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  if (!m) return [136, 136, 136];
  return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16), 220];
}

function graticuleGeoJSON() {
  const features = [];
  for (let lon = -180; lon <= 180; lon += 30) {
    features.push({
      type: "Feature",
      properties: {},
      geometry: {
        type: "LineString",
        coordinates: Array.from({ length: 37 }, (_, i) => [lon, -90 + i * 5]),
      },
    });
  }
  for (let lat = -90; lat <= 90; lat += 30) {
    features.push({
      type: "Feature",
      properties: {},
      geometry: {
        type: "LineString",
        coordinates: Array.from({ length: 145 }, (_, i) => [-180 + i * 2.5, lat]),
      },
    });
  }
  return { type: "FeatureCollection", features };
}
