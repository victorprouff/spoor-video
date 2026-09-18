import { invoke } from '@tauri-apps/api/core';

/** Miroirs des types Rust. */

export type Trap = {
  id: string;
  name: string;
  folder_name: string | null;
  video_count: number;
};

export type AppState = {
  root_path: string | null;
  ffmpeg_available: boolean;
  traps: Trap[];
  videos_count: number;
  videos_missing: number;
  videos_no_date: number;
  last_scan: string | null;
};

export type ScanReport = {
  root_path: string;
  files_seen: number;
  files_added: number;
  files_known: number;
  files_moved: number;
  files_repurged: number;
  files_missing: number;
  files_recovered: number;
  files_no_date: number;
  files_error: number;
  unknown_folders: string[];
  errors: string[];
  ffmpeg_available: boolean;
};

export const appState = () => invoke<AppState>('app_state');
export const setRootPath = (path: string) => invoke<void>('set_root_path', { path });
export const scanRoot = () => invoke<ScanReport>('scan_root');

export const linkFolderToTrap = (folderName: string, trapId: string | null) =>
  invoke<string>('link_folder_to_trap', { folderName, trapId });
