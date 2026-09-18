import { useEffect, useRef, useState } from 'react';
import L from 'leaflet';
import 'leaflet/dist/leaflet.css';

/**
 * Carte pour poser un point.
 *
 * ⚠️ **C'est le seul endroit où l'application touche au réseau.** Les fonds de carte
 * viennent d'OpenStreetMap et ne sont pas embarqués : sans connexion, la carte reste
 * grise. La saisie des coordonnées au clavier fonctionne toujours, elle — on ne doit
 * jamais être empêché de renseigner une position parce qu'un serveur distant ne
 * répond pas.
 */

// Les icônes par défaut de Leaflet sont chargées depuis un CDN, ce qui casserait la
// promesse de fonctionner hors ligne. On dessine le marqueur nous-mêmes.
const MARKER = L.divIcon({
  className: 'map__marker',
  html: '<span></span>',
  iconSize: [18, 18],
  iconAnchor: [9, 9],
});

export function MapPicker({
  latitude,
  longitude,
  onChange,
  height = 320,
}: {
  latitude: number | null;
  longitude: number | null;
  onChange: (lat: number, lng: number) => void;
  height?: number;
}) {
  const container = useRef<HTMLDivElement>(null);
  const map = useRef<L.Map | null>(null);
  const marker = useRef<L.Marker | null>(null);
  const [offline, setOffline] = useState(false);

  useEffect(() => {
    if (!container.current || map.current) return;

    // Centre par défaut : la France, à défaut de mieux. Le premier clic recentre.
    const start: [number, number] = [latitude ?? 46.6, longitude ?? 2.5];
    const instance = L.map(container.current, {
      center: start,
      zoom: latitude != null ? 15 : 5,
      attributionControl: true,
    });

    const tiles = L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', {
      maxZoom: 19,
      attribution: '© OpenStreetMap',
    });
    tiles.on('tileerror', () => setOffline(true));
    tiles.addTo(instance);

    instance.on('click', (e: L.LeafletMouseEvent) => {
      onChange(Number(e.latlng.lat.toFixed(6)), Number(e.latlng.lng.toFixed(6)));
    });

    map.current = instance;

    return () => {
      instance.remove();
      map.current = null;
      marker.current = null;
    };
    // Volontairement monté une seule fois : recréer la carte à chaque frappe dans le
    // champ latitude la ferait clignoter et perdrait le niveau de zoom.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Le marqueur suit les coordonnées, d'où qu'elles viennent — clic ou clavier.
  useEffect(() => {
    const instance = map.current;
    if (!instance) return;

    if (latitude == null || longitude == null) {
      marker.current?.remove();
      marker.current = null;
      return;
    }
    const position: [number, number] = [latitude, longitude];
    if (marker.current) {
      marker.current.setLatLng(position);
    } else {
      marker.current = L.marker(position, { icon: MARKER, draggable: true })
        .on('dragend', (e) => {
          const p = (e.target as L.Marker).getLatLng();
          onChange(Number(p.lat.toFixed(6)), Number(p.lng.toFixed(6)));
        })
        .addTo(instance);
      instance.setView(position, Math.max(instance.getZoom(), 15));
    }
  }, [latitude, longitude, onChange]);

  return (
    <div className="map">
      <div ref={container} className="map__canvas" style={{ height }} />
      {offline && (
        <p className="muted small">
          Fond de carte indisponible — pas de connexion. Les coordonnées restent
          saisissables au clavier, et le point est bien enregistré.
        </p>
      )}
      <p className="muted small">
        Clique sur la carte pour poser le point, ou fais glisser le marqueur.
      </p>
    </div>
  );
}
