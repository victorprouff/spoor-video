import { invoke } from '@tauri-apps/api/core';

/** Miroirs des types Rust. */

export type Trap = {
  id: string;
  name: string;
  folder_name: string | null;
  camera_name: string | null;
  latitude: number | null;
  longitude: number | null;
  altitude_m: number | null;
  clock_offset_minutes: number;
  notes: string | null;
  active: boolean;
  video_count: number;
  first_video_at: string | null;
  last_video_at: string | null;
};

export type TrapInput = {
  name: string;
  folder_name: string | null;
  camera_name: string | null;
  latitude: number | null;
  longitude: number | null;
  altitude_m: number | null;
  clock_offset_minutes: number;
  notes: string | null;
  active: boolean;
};

export type SpeciesGroup = 'mammifere' | 'oiseau' | 'autre';

export type Species = {
  id: string;
  common_name: string;
  scientific_name: string | null;
  species_group: SpeciesGroup;
  color: string | null;
  sort_order: number;
  shortcut_key: string | null;
  builtin: boolean;
  usage_count: number;
};

export type SpeciesInput = {
  common_name: string;
  scientific_name: string | null;
  species_group: SpeciesGroup;
  color: string | null;
  sort_order: number;
  shortcut_key: string | null;
};

export type RootFolder = {
  name: string;
  trap_name: string | null;
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
export const listRootFolders = () => invoke<RootFolder[]>('list_root_folders');

export const linkFolderToTrap = (folderName: string, trapId: string | null) =>
  invoke<string>('link_folder_to_trap', { folderName, trapId });

export const listTraps = () => invoke<Trap[]>('list_traps');
export const createTrap = (input: TrapInput) => invoke<string>('create_trap', { input });
export const updateTrap = (id: string, input: TrapInput) => invoke<void>('update_trap', { id, input });
export const deleteTrap = (id: string) => invoke<void>('delete_trap', { id });

export const listSpecies = () => invoke<Species[]>('list_species');
export const createSpecies = (input: SpeciesInput) => invoke<string>('create_species', { input });
export const updateSpecies = (id: string, input: SpeciesInput) =>
  invoke<void>('update_species', { id, input });
export const deleteSpecies = (id: string) => invoke<void>('delete_species', { id });
