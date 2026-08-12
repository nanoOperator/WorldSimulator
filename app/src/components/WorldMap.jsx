import { useMemo, useRef, useEffect } from "react";
import DeckGL from "@deck.gl/react";
import { GeoJsonLayer } from "@deck.gl/layers";
import { Map } from "react-map-gl/maplibre";
import "maplibre-gl/dist/maplibre-gl.css";

// Blank offline style (no external tiles needed).
const BLANK_STYLE = {
  version: 8,
  sources: {},
  layers: [
    { id: "bg", type: "background", paint: { "background-color": "#0a0e14" } },
  ],
};

export default function WorldMap({ geojson, selected, onSelect }) {
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
      lineWidthMinPixels: 0.5,
      getFillColor: (f) => hexToRgb(f.properties.color || "#888888"),
      getLineColor: [20, 24, 32],
      getElevation: (f) => {
        // 2.5D extrusion: bigger nations pop more.
        const pop = Number(f.properties.population || 0);
        return Math.min(800000, pop / 200000) + 20;
      },
      material: { ambient: 0.6, diffuse: 0.6, shininess: 32, specularColor: [60, 64, 80] },
      onClick: (info) => {
        if (info.object && onSelect) onSelect(info.object.properties);
      },
      updateTriggers: { getFillColor: geojson },
    });
  }, [geojson, onSelect]);

  useEffect(() => {
    if (mapRef.current) mapRef.current.resize();
  }, []);

  return (
    <DeckGL
      initialViewState={{ longitude: 10, latitude: 30, zoom: 1.3, pitch: 45, bearing: 0 }}
      controller={true}
      layers={layer ? [layer] : []}
      style={{ position: "absolute", inset: 0 }}
    >
      <Map ref={mapRef} mapStyle={BLANK_STYLE} attributionControl={false} />
    </DeckGL>
  );
}

function hexToRgb(hex) {
  const m = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
  if (!m) return [136, 136, 136];
  return [parseInt(m[1], 16), parseInt(m[2], 16), parseInt(m[3], 16), 220];
}
