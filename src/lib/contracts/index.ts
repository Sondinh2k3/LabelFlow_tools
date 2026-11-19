// Wire names deliberately match Rust serde camelCase. See domain/mod.rs.
import type { StorageUsage } from './resources';
export type * from './resources';
import type { TeamEntry, TeamFeed, TeamPostInput, TeamStatus } from './team';
export type * from './team';
import type {
  DatasetDashboard,
  FingerprintReport,
  DuplicateCandidate,
  FrameCandidate,
  PropagationReport,
  PropagationInfo,
  PropagationOrigin,
  DatasetSnapshot,
  SnapshotOptions,
  SnapshotCheck,
} from './advanced';
export type * from './advanced';
export type ErrorCode =
  | 'io'
  | 'database'
  | 'invalidProject'
  | 'projectLocked'
  | 'unsupportedVersion'
  | 'invalidInput'
  | 'invalidImage'
  | 'noProject'
  | 'internal'
  | 'conflict'
  | 'cancelled';
export interface AppError {
  code: ErrorCode;
  message: string;
}
export interface Project {
  id: string;
  name: string;
  profileId: string;
  schemaVersion: number;
  root: string;
}
export interface ImageInfo {
  id: string;
  name: string;
  path: string;
  width: number;
  height: number;
  orientation: number;
  sha256: string;
}
export interface ImagePreview {
  info: ImageInfo;
  dataUrl: string;
}
export interface CommandMap {
  storage_usage: { args: undefined; result: StorageUsage };
  clear_derived_cache: { args: { confirmed: boolean }; result: StorageUsage };
  quality_dashboard: { args: undefined; result: DatasetDashboard };
  index_fingerprints: { args: { force: boolean }; result: FingerprintReport };
  rank_similar_media: {
    args: { mediaId: string; scope: 'project' | 'group'; maxDistance: number };
    result: DuplicateCandidate[];
  };
  propagation_targets: { args: { anchorId: string }; result: FrameCandidate[] };
  propagate_frames: {
    args: { anchorId: string; targetIds: string[] };
    result: PropagationReport;
  };
  propagation_info: { args: { key: string }; result: PropagationInfo | null };
  verify_propagation: {
    args: { key: string; expectedRevision: number; confirmed: boolean };
    result: PropagationInfo;
  };
  list_snapshots: { args: undefined; result: DatasetSnapshot[] };
  create_snapshot: {
    args: { options: SnapshotOptions };
    result: DatasetSnapshot;
  };
  verify_snapshot: {
    args: { id: string; checkSources: boolean };
    result: SnapshotCheck;
  };
  export_snapshot: { args: { id: string }; result: string | null };
  review_metadata: { args: { mediaId: string }; result: RoadMetadata };
  save_review_metadata: {
    args: { metadata: RoadMetadata };
    result: RoadMetadata;
  };
  list_trips: { args: undefined; result: Trip[] };
  save_trip: { args: { trip: Trip }; result: Trip };
  review_page: {
    args: { filter: ReviewFilter; offset: number; limit: number };
    result: ReviewPage;
  };
  review_predictions: {
    args: { mediaId: string };
    result: ReviewPredictionSummary[];
  };
  review_snapshot: { args: { key: string }; result: ReviewSnapshot };
  import_review_prediction: {
    args: { mediaId: string };
    result: string | null;
  };
  approve_review_prediction: {
    args: { key: string; expectedRevision: number };
    result: AnnotationDocument;
  };
  reject_review_prediction: { args: { key: string }; result: null };
  analyze_review: { args: undefined; result: number };
  export_review_metrics: { args: undefined; result: string | null };
  ai_status: { args: undefined; result: AiStatus };
  ai_configure: { args: undefined; result: AiConfig | null };
  ai_start: { args: undefined; result: { health: unknown; model: AiModel } };
  ai_stop: { args: undefined; result: null };
  ai_cancel: { args: undefined; result: null };
  ai_predict: { args: { mediaId: string; threshold: number }; result: string };
  ai_suggestions: { args: { mediaId: string }; result: AiSuggestion[] };
  ai_preview: { args: { id: string }; result: AiPreview };
  ai_reject: { args: { id: string }; result: null };
  ai_approve: {
    args: { id: string; expectedRevision: number };
    result: AnnotationDocument;
  };
  current_project: { args: undefined; result: Project | null };
  create_project: {
    args: { name: string; profileId?: string; semanticClasses?: string[] };
    result: Project | null;
  };
  open_project: { args: undefined; result: Project | null };
  close_project: { args: undefined; result: null };
  open_image: { args: undefined; result: ImagePreview | null };
  restore_image: { args: undefined; result: ImagePreview | null };
  project_profile: { args: undefined; result: Profile };
  list_media: {
    args: {
      offset: number;
      limit: number;
      query: string;
      status: string;
      filter?: LibraryFilter;
    };
    result: MediaPage;
  };
  select_media: { args: { id: string }; result: ImagePreview };
  media_thumbnail: { args: { id: string }; result: string };
  load_document: { args: { mediaId: string }; result: AnnotationDocument };
  save_document: {
    args: { document: AnnotationDocument; operationId: string };
    result: number;
  };
  import_media: {
    args: { folder: boolean; copied: boolean; paths?: string[] };
    result: ImportReport | null;
  };
  cancel_job: { args: undefined; result: null };
  export_dataset: {
    args: { options: ExportOptions };
    result: ExportReport | null;
  };
  inspect_yolo: { args: undefined; result: YoloInspection | null };
  import_yolo: {
    args: { token: string; mapping: { source: number; target: string }[] };
    result: ImportReport;
  };
  inspect_mask: {
    args: { mediaId: string; layerId: string };
    result: MaskInspection | null;
  };
  apply_mask: {
    args: { token: string; mapping: { source: number; target: number }[] };
    result: AnnotationDocument;
  };
  inspect_mask_folder: {
    args: { layerId: string };
    result: MaskFolderInspection | null;
  };
  apply_mask_folder: {
    args: {
      token: string;
      mapping: { source: number; target: number }[];
      mode: 'fillUnlabeled' | 'replace';
    };
    result: ImportReport;
  };
  inspect_ignore_template: {
    args: { tripId: string | null };
    result: TemplateInspection | null;
  };
  apply_ignore_template: { args: { token: string }; result: ImportReport };
  sam_status: {
    args: undefined;
    result: {
      config: { python: string; manifest: string } | null;
      model: SamModel | null;
    };
  };
  sam_configure: {
    args: undefined;
    result: { python: string; manifest: string } | null;
  };
  sam_start: { args: undefined; result: SamModel };
  sam_stop: { args: undefined; result: null };
  sam_segment: {
    args: {
      mediaId: string;
      points: SamPoint[];
      region: { x: number; y: number; width: number; height: number } | null;
      encodeOnly: boolean;
    };
    result: SamResult;
  };
  backup_project: { args: undefined; result: string | null };
  restore_backup: { args: undefined; result: Project | null };
  video_tools: {
    args: undefined;
    result: {
      available: boolean;
      version: string | null;
      error: string | null;
    };
  };
  configure_video_tools: { args: undefined; result: unknown };
  open_video: { args: undefined; result: VideoInfo | null };
  list_videos: { args: undefined; result: VideoInfo[] };
  video_frames: {
    args: { id: string; offset: number; limit: number };
    result: { video: VideoInfo; frames: FrameInfo[] };
  };
  seek_video: { args: { id: string; timeMs: number }; result: FrameInfo };
  video_stream: { args: { id: string }; result: string };
  preview_frame: {
    args: { id: string; index: number };
    result: { frame: FrameInfo; dataUrl: string };
  };
  extract_frames: {
    args: { options: ExtractionOptions };
    result: ImportReport;
  };
  set_video_group: { args: { id: string; group: string }; result: null };
  inspect_instances: {
    args: { format: 'coco' | 'yoloSeg' };
    result: YoloInspection | null;
  };
  apply_instances: {
    args: { token: string; mapping: { source: number; target: string }[] };
    result: ImportReport;
  };
  dataset_quality: {
    args: { options: ExportOptions; deep: boolean };
    result: QualityReport;
  };
  repair_video: { args: { id: string }; result: boolean };
  repair_media: { args: { id: string }; result: boolean };
  media_origin: { args: { id: string }; result: MediaOrigin | null };
  preload_media: { args: { id: string }; result: ImagePreview };
  project_health: { args: undefined; result: string[] };
  save_class: {
    args: {
      id: string | null;
      name: string;
      color: string;
      namespace?: string;
    };
    result: Profile;
  };
  delete_class: { args: { id: string }; result: Profile };
  team_status: { args: undefined; result: TeamStatus };
  team_configure: {
    args: { userName: string; folder: 'keep' | 'pick' | 'clear' };
    result: TeamStatus | null;
  };
  team_threads: { args: undefined; result: TeamFeed };
  team_post: { args: { input: TeamPostInput }; result: TeamEntry };
  team_set_status: {
    args: { threadId: string; resolved: boolean };
    result: TeamEntry;
  };
  team_preview: { args: { sha256: string }; result: string | null };
}

export interface Trip {
  id: string;
  name: string;
  route: string;
  cameraId: string;
  distanceKm: number | null;
  notes: string;
  revision: number;
}
export interface RoadMetadata {
  videoTripRevision: number | null;
  mediaId: string;
  revision: number;
  tripId: string | null;
  cameraId: string;
  route: string;
  capturedAt: string;
  latitude: number | null;
  longitude: number | null;
  tags: string[];
}
export interface ReviewPrediction {
  propagation?: PropagationOrigin | null;
  schemaVersion: number;
  profileId: string;
  mediaId: string;
  sourceSha256: string;
  width: number;
  height: number;
  model: {
    id: string;
    name: string;
    version: string;
    sha256: string | null;
    confidence: number | null;
  };
  eventLabel: AnnotationDocument['eventLabel'];
  annotations: Annotation[];
  masks: RasterMask[];
}
export interface ReviewPredictionSummary {
  key: string;
  mediaId: string;
  baseRevision: number;
  status: string;
  modelName: string;
  modelVersion: string;
  eventLabel: AnnotationDocument['eventLabel'];
  createdAt: string;
}
export interface ReviewComparison {
  gtRevision: number;
  gtApproved: boolean;
  gtEvent: AnnotationDocument['eventLabel'];
  predictedEvent: AnnotationDocument['eventLabel'];
  eventMismatch: boolean | null;
  layers: {
    layerId: string;
    evaluatedPixels: number;
    ignoredPixels: number;
    disagreementPixels: number;
    meanIou: number | null;
    classes: {
      classId: number;
      intersection: number;
      union: number;
      iou: number | null;
    }[];
  }[];
  missingGtLayers: string[];
  hasDisagreement: boolean;
  differences: RasterMask[];
  differenceOmitted: boolean;
}
export interface ReviewSnapshot {
  summary: ReviewPredictionSummary;
  prediction: ReviewPrediction;
  groundTruth: AnnotationDocument;
  comparison: ReviewComparison;
}
export interface ReviewFilter {
  status: string;
  event: string;
  tag: string;
  tripId: string;
  disagreementsOnly: boolean;
}
export interface ReviewPage {
  total: number;
  items: {
    mediaId: string;
    name: string;
    reviewStatus: ReviewStatus;
    eventLabel: AnnotationDocument['eventLabel'];
    tripName: string | null;
    predictionKey: string | null;
    modelName: string | null;
    disagreement: boolean | null;
    analyzed: boolean;
  }[];
}

export interface AiConfig {
  python: string;
  manifest: string;
}
export interface AiModel {
  schemaVersion: number;
  id: string;
  name: string;
  version: string;
  license: string;
  file: string;
  sha256: string;
  adapter: string;
  task: 'semantic' | 'boxes';
  layerId: string;
  classMap: string[];
  input: {
    name: string;
    width: number;
    height: number;
    mean: number[];
    std: number[];
  };
  output: { name: string; format: string; resize: string };
}
export interface AiStatus {
  config: AiConfig | null;
  runtime: { running: boolean; busy: boolean; model: AiModel | null };
}
export interface AiSuggestion {
  id: string;
  mediaId: string;
  baseRevision: number;
  status: 'pending' | 'accepted' | 'rejected';
  modelName: string;
  modelVersion: string;
  confidence: number | null;
  createdAt: string;
  acceptedRevision: number | null;
}
export interface AiPreview {
  prediction: {
    model: AiModel;
    width: number;
    height: number;
    confidence: number | null;
    processing: { milliseconds: number; provider: string; device: string };
  };
  annotations: Annotation[];
  masks: RasterMask[];
}

export type ReviewStatus =
  'unlabeled' | 'in_progress' | 'needs_review' | 'approved' | 'rejected';
export const reviewStatuses: ReviewStatus[] = [
  'unlabeled',
  'in_progress',
  'needs_review',
  'approved',
  'rejected',
];
export interface Point {
  x: number;
  y: number;
}
export type Geometry =
  | { type: 'bbox'; x: number; y: number; width: number; height: number }
  | { type: 'polygon'; points: Point[]; holes: Point[][] }
  | { type: 'multiPolygon'; polygons: { points: Point[]; holes: Point[][] }[] }
  | {
      type: 'mask';
      width: number;
      height: number;
      runs: number[];
      iscrowd: boolean;
    };
export interface Annotation {
  id: string;
  layerId: string;
  classId: string | null;
  geometry: Geometry;
  source: 'manual' | 'imported' | 'model_suggestion' | 'model_approved';
}
export interface RasterMask {
  layerId: string;
  width: number;
  height: number;
  runs: number[];
}
export interface AnnotationDocument {
  mediaId: string;
  revision: number;
  reviewStatus: ReviewStatus;
  eventLabel: 'positive' | 'negative' | 'uncertain' | null;
  notes: string;
  splitGroup: string;
  annotations: Annotation[];
  masks: RasterMask[];
}
export interface ClassDefinition {
  id: string;
  namespace: string;
  stableKey: string;
  exportId: number;
  displayName: string;
  color: string;
  isIgnore: boolean;
}
export interface LayerDefinition {
  id: string;
  stableKey: string;
  layerType: 'instance' | 'semantic' | 'region';
  displayName: string;
  namespace: string | null;
  color: string;
}
export interface Profile {
  id: string;
  classes: ClassDefinition[];
  layers: LayerDefinition[];
}
export interface MediaRecord extends ImageInfo {
  reviewStatus: ReviewStatus;
  importMode: 'linked' | 'copied';
}
export interface MediaPage {
  items: MediaRecord[];
  total: number;
}
export interface ImportReport {
  imported: number;
  duplicates: number;
  warnings: string[];
  cancelled: boolean;
}
export interface Progress {
  completed: number;
  total: number;
  message: string;
}
export interface ExportOptions {
  format: 'yolo' | 'semantic' | 'all' | 'yoloSeg' | 'coco';
  skipUnsupported?: boolean;
  approvedOnly: boolean;
  seed: string;
  trainPercent: number;
  valPercent: number;
}
export interface ExportReport {
  path: string;
  images: number;
  annotations: number;
  warnings: string[];
}
export interface YoloInspection {
  names?: Record<number, string>;
  token: string;
  classes: number[];
  files: number;
  objects: number;
}
export interface MaskInspection {
  token: string;
  values: number[];
  width: number;
  height: number;
}
export interface MaskFolderInspection {
  token: string;
  values: number[];
  matched: number;
  approvedSkipped: number;
  warnings: string[];
}
export interface TemplateInspection {
  token: string;
  width: number;
  height: number;
  ignorePixels: number;
  images: number;
  approvedSkipped: number;
}
export interface SamModel {
  id: string;
  name: string;
  version: string;
  license: string;
  kind: 'sam_prompt';
  inputSize: number;
}
export interface SamPoint {
  x: number;
  y: number;
  positive: boolean;
}
/** Binary mask at canonical resolution; `cached` is set for encode-only calls. */
export interface SamResult {
  width?: number;
  height?: number;
  runs?: number[];
  score?: number | null;
  area?: number | null;
  milliseconds?: number | null;
  cached?: boolean;
}

export interface VideoInfo {
  id: string;
  name: string;
  path: string;
  sha256: string;
  codec: string;
  width: number;
  height: number;
  rotation: number;
  timeBaseNum: number;
  timeBaseDen: number;
  startPts: number;
  durationMs: number;
  averageFps: number;
  frameCount: number;
  splitGroup: string;
}
export interface FrameInfo {
  index: number;
  pts: number;
  timeMs: number;
  durationMs: number;
}
export interface ExtractionOptions {
  videoId: string;
  indices: number[];
  startMs: number;
  endMs: number;
  intervalMs: number | null;
  format: 'png' | 'jpeg';
  quality: number;
}
export interface MediaOrigin {
  videoId: string;
  frameIndex: number;
  timeMs: number;
  pts: number;
  timeBase: [number, number];
}
export interface QualityReport {
  images: number;
  groups: { group: string; images: number; split: string }[];
  reviewCounts: Record<string, number>;
  splitCounts: Record<string, number>;
  errors: string[];
  warnings: string[];
  classMapping: ClassDefinition[];
}

export interface LibraryFilter {
  event: string;
  group: string;
  annotated: boolean | null;
}
