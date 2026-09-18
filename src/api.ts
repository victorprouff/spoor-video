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
  utc_offset_minutes: number;
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
  utc_offset_minutes: number;
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

export type Sequence = {
  id: string;
  trap_id: string;
  trap_name: string;
  started_at: string;
  ended_at: string;
  video_count: number;
  duration_s: number;
  auto_grouped: boolean;
  state: string | null;
  notes: string | null;
  reviewed_at: string | null;
};

export type SequenceVideo = {
  id: string;
  file_name: string;
  file_path: string;
  thumbnail_path: string | null;
  recorded_at: string | null;
  duration_s: number | null;
  file_state: string;
};

export type RegroupReport = {
  sequences_built: number;
  sequences_frozen: number;
  videos_grouped: number;
  videos_undated: number;
  sun_computed: number;
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
  sequences_built: number;
  sequences_frozen: number;
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

export type TileSpecies = {
  id: string;
  common_name: string;
  color: string | null;
  confidence: Confidence;
};

export type GridTile = {
  id: string;
  trap_id: string;
  trap_name: string;
  started_at: string;
  ended_at: string;
  video_count: number;
  duration_s: number;
  state: string | null;
  notes: string | null;
  reviewed: boolean;
  auto_grouped: boolean;
  thumbnails: string[];
  species: TileSpecies[];
  tags: string[];
  unplayable_count: number;
  sun_phase: string | null;
  minutes_from_sunset: number | null;
};

export type GridPage = {
  tiles: GridTile[];
  total: number;
  unreviewed_total: number;
};

export type GridFilter = {
  trap_id: string | null;
  review: 'unreviewed' | 'reviewed' | 'all';
  states: string[];
  species: string[];
  tags: string[];
  confidence_min: Confidence | null;
  from: string | null;
  to: string | null;
  /** Mois retenus, toutes années confondues. */
  months: number[];
  hour_from: number | null;
  hour_to: number | null;
  sun_phases: string[];
  duration_min_s: number | null;
  duration_max_s: number | null;
  query: string | null;
  limit: number;
  offset: number;
};

export const SUN_PHASES: { value: string; label: string }[] = [
  { value: 'day', label: 'Jour' },
  { value: 'dawn', label: 'Aube' },
  { value: 'dusk', label: 'Crépuscule' },
  { value: 'night', label: 'Nuit' },
];

export type Confidence = 'certain' | 'probable' | 'possible';

/** Les états qui ne sont pas une espèce. Nombreux, et ils ne doivent pas polluer
    le référentiel d'espèces. */
export const STATES: { value: string; label: string }[] = [
  { value: 'empty', label: 'Rien / fausse déclenche' },
  { value: 'unidentified', label: 'Indéterminé' },
  { value: 'human', label: 'Humain' },
  { value: 'vehicle', label: 'Véhicule' },
  { value: 'livestock', label: 'Bétail' },
];

export const CONFIDENCES: { value: Confidence; label: string; key: string }[] = [
  { value: 'certain', label: 'Certain', key: '1' },
  { value: 'probable', label: 'Probable', key: '2' },
  { value: 'possible', label: 'Possible', key: '3' },
];

export type SpeciesPick = {
  species_id: string;
  confidence: Confidence;
  count_min: number | null;
  count_max: number | null;
};

export type Annotation = {
  state?: string | null;
  add_species?: SpeciesPick[];
  remove_species?: string[];
  add_tags?: string[];
  remove_tags?: string[];
  notes?: string | null;
  reviewed?: boolean | null;
};

export type AnnotateReport = {
  sequences_touched: number;
  species_added: number;
  species_removed: number;
  tags_added: number;
  tags_removed: number;
};

export type Tag = { id: string; name: string; usage_count: number };

export type HourBucket = { hour: number; count: number };
export type SpeciesHour = {
  species_id: string;
  common_name: string;
  color: string | null;
  hour: number;
  count: number;
};
export type SolarBucket = { bucket: number; count: number };
export type MonthBucket = { month: number; count: number; years: number };
export type TrapStat = {
  trap_id: string;
  name: string;
  sequences: number;
  videos: number;
  species_richness: number;
  first_at: string | null;
  last_at: string | null;
  span_days: number | null;
};
export type SpeciesStat = {
  species_id: string;
  common_name: string;
  color: string | null;
  sequences: number;
  videos: number;
  certain: number;
  traps: number;
  first_at: string | null;
  last_at: string | null;
};

export type Stats = {
  total_sequences: number;
  total_videos: number;
  identified_sequences: number;
  unreviewed_sequences: number;
  without_position: number;
  hours: HourBucket[];
  species_hours: SpeciesHour[];
  solar: SolarBucket[];
  months: MonthBucket[];
  traps: TrapStat[];
  species: SpeciesStat[];
};

export const stats = (filter: GridFilter) => invoke<Stats>('stats', { filter });

export type ExportReport = { path: string; rows: number };
export type CopyReport = {
  dest: string;
  copied: number;
  unavailable: number;
  errors: string[];
  bytes: number;
};

export type PositionGroup = {
  trap_id: string;
  trap_name: string;
  latitude: number | null;
  longitude: number | null;
  video_count: number;
  video_ids: string[];
  first_at: string | null;
  last_at: string | null;
  any_manual: boolean;
};

export const positionGroups = (trapId: string | null, since: string | null) =>
  invoke<PositionGroup[]>('position_groups', { trapId, since });
export const setVideoPositions = (
  videoIds: string[],
  latitude: number | null,
  longitude: number | null,
  altitudeM: number | null,
) => invoke<number>('set_video_positions', { videoIds, latitude, longitude, altitudeM });
export const pendingVideos = () => invoke<number>('pending_videos');

export type DeletePreview = {
  videos: number;
  present_files: number;
  already_gone: number;
  total_bytes: number;
  sequences: number;
  reviewed_sequences: number;
  species_annotations: number;
  outside_root: number;
};

export type DeleteReport = {
  trashed: number;
  rows_removed: number;
  already_gone: number;
  sequences_removed: number;
  annotations_lost: number;
  errors: string[];
};

export const videosOfSequences = (sequenceIds: string[]) =>
  invoke<string[]>('videos_of_sequences', { sequenceIds });
export const previewDeletion = (videoIds: string[]) =>
  invoke<DeletePreview>('preview_deletion', { videoIds });
export const deleteVideosKeepingTrace = (videoIds: string[], reason: string | null) =>
  invoke<DeleteReport>('delete_videos_keeping_trace', { videoIds, reason });
export const deleteVideosWithoutTrace = (videoIds: string[]) =>
  invoke<DeleteReport>('delete_videos_without_trace', { videoIds });

export const exportSequencesCsv = (filter: GridFilter, path: string) =>
  invoke<ExportReport>('export_sequences_csv', { filter, path });
export const exportDetectionsCsv = (filter: GridFilter, path: string) =>
  invoke<ExportReport>('export_detections_csv', { filter, path });
export const copyVideos = (sequenceIds: string[], destDir: string) =>
  invoke<CopyReport>('copy_videos', { sequenceIds, destDir });

export const gridPage = (filter: GridFilter) => invoke<GridPage>('grid_page', { filter });
export const annotateSequences = (sequenceIds: string[], annotation: Annotation) =>
  invoke<AnnotateReport>('annotate_sequences', { sequenceIds, annotation });
export const listTags = () => invoke<Tag[]>('list_tags');

export const listSequences = (trapId: string | null) =>
  invoke<Sequence[]>('list_sequences', { trapId });
export const regroupSequences = () => invoke<RegroupReport>('regroup_sequences');
export const listSequenceVideos = (sequenceId: string) =>
  invoke<SequenceVideo[]>('list_sequence_videos', { sequenceId });
export const splitSequence = (sequenceId: string, atVideoId: string) =>
  invoke<string>('split_sequence', { sequenceId, atVideoId });
export const mergeSequences = (sequenceIds: string[]) =>
  invoke<string>('merge_sequences', { sequenceIds });
