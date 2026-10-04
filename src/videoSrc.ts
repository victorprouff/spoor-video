import { convertFileSrc, invoke } from '@tauri-apps/api/core';

/**
 * Adresse à donner à une balise `<video>`.
 *
 * Sous Linux, WebKitGTK confie la vidéo à GStreamer, qui ne sait pas lire `asset://` :
 * l'application y sert les vidéos par un serveur local (voir `stream.rs`). Ailleurs,
 * `video_base_url` répond `null` et l'on garde `asset://`. Les images n'ont pas ce
 * problème et continuent de passer par `convertFileSrc`.
 */
let base: string | null = null;

export async function initVideoSrc(): Promise<void> {
  try {
    base = await invoke<string | null>('video_base_url');
  } catch {
    base = null;
  }
}

export function videoSrc(path: string): string {
  return base ? `${base}?path=${encodeURIComponent(path)}` : convertFileSrc(path);
}
