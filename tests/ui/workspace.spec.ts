import { test, expect, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type {
  AnnotationDocument,
  ImagePreview,
  Profile,
  Project,
  ExportOptions,
  ExtractionOptions,
  LibraryFilter,
  AiSuggestion,
  RoadMetadata,
  Trip,
  ReviewPredictionSummary,
  ReviewFilter,
  DatasetSnapshot,
  SnapshotOptions,
  PropagationInfo,
  TeamThread,
  TeamPostInput,
} from '../../src/lib/contracts';
import {
  fitImage,
  imageToScreen,
} from '../../src/features/annotations/geometry/viewport';
import { decodeRuns } from '../../src/features/annotations/geometry/raster';
interface TestState {
  commandCounts?: Record<string, number>;
  fail: boolean;
  cancel: boolean;
  failSave: boolean;
  documents: Record<string, AnnotationDocument>;
  lastExport?: ExportOptions;
  lastExtraction?: ExtractionOptions;
  lastFilter?: LibraryFilter;
  qualityError?: boolean;
  videoReady?: boolean;
  instanceImported?: boolean;
  aiConfigured?: boolean;
  aiReady?: boolean;
  samReady?: boolean;
  samCalls?: Record<string, unknown>[];
  aiHold?: boolean;
  aiSuggestions?: AiSuggestion[];
  roadMetadata?: RoadMetadata;
  trips?: Trip[];
  reviewPredictions?: ReviewPredictionSummary[];
  reviewAnalyzed?: boolean;
  reviewFilter?: ReviewFilter;
  snapshots?: DatasetSnapshot[];
  snapshotOptions?: SnapshotOptions;
  fingerprintIndexed?: boolean;
  propagationEnabled?: boolean;
  propagationInfo?: PropagationInfo;
  teamFolder?: string | null;
  teamThreads?: TeamThread[];
  lastTeamPost?: TeamPostInput;
}
declare global {
  interface Window {
    __TEST_STATE__: TestState;
  }
}
const project: Project = {
  id: 'ui-road-project',
  name: 'Road calibration',
  profileId: 'road-encroachment-v1',
  schemaVersion: 6,
  root: 'C:\\Projects\\Road calibration',
};
const image: ImagePreview = {
  info: {
    id: 'test-image',
    name: 'calibration-1920x1080.png',
    path: 'C:\\fixtures\\calibration-1920x1080.png',
    width: 1920,
    height: 1080,
    orientation: 1,
    sha256: '0'.repeat(64),
  },
  dataUrl: `data:image/png;base64,${readFileSync('tests/fixtures/calibration-1920x1080.png').toString('base64')}`,
};
const names = [
  'road',
  'sidewalk',
  'building',
  'wall',
  'fence',
  'pole',
  'traffic light',
  'traffic sign',
  'vegetation',
  'terrain',
  'sky',
  'person',
  'rider',
  'car',
  'truck',
  'bus',
  'train',
  'motorcycle',
  'bicycle',
];
const profile: Profile = {
  id: project.profileId,
  classes: [
    ...names.map((name, i) => ({
      id: `cityscapes-${i}`,
      namespace: 'cityscapes',
      stableKey: name,
      displayName: name,
      exportId: i,
      color: i === 0 ? '#804080' : '#00ccaa',
      isIgnore: false,
    })),
    {
      id: 'cityscapes-ignore',
      namespace: 'cityscapes',
      stableKey: 'ignore',
      displayName: 'Ignore',
      exportId: 255,
      color: '#888888',
      isIgnore: true,
    },
    ...[
      'material pile',
      'goods cluster',
      'street stall',
      'cart',
      'standalone object',
    ].map((name, i) => ({
      id: `candidate-${i}`,
      namespace: 'candidate',
      stableKey: name,
      displayName: name,
      exportId: i,
      color: '#ffb86b',
      isIgnore: false,
    })),
  ],
  layers: [
    {
      id: 'candidate_objects',
      stableKey: 'candidate_objects',
      layerType: 'instance',
      displayName: 'Candidate objects',
      namespace: 'candidate',
      color: '#ffb86b',
    },
    {
      id: 'cityscapes_semantic',
      stableKey: 'cityscapes_semantic',
      layerType: 'semantic',
      displayName: 'Cityscapes semantic',
      namespace: 'cityscapes',
      color: '#804080',
    },
    ...['road_extent', 'traffic_exclusion', 'suspected_region'].map((id) => ({
      id,
      stableKey: id,
      layerType: 'region' as const,
      displayName: id,
      namespace: null,
      color: '#52d8ff',
    })),
  ],
};
test.beforeEach(async ({ page }) => {
  await page.addInitScript(
    ({ project, image, profile }) => {
      let stored = null as {
        remembered: boolean;
        activeId: string;
        documents: Record<string, AnnotationDocument>;
        operations: Record<string, number>;
      } | null;
      let remembered = stored?.remembered ?? false,
        activeId = stored?.activeId ?? image.info.id,
        opened = false;
      const operations = stored?.operations ?? {};
      let initialized = false;
      let cancelAi: (() => void) | null = null;
      const state: TestState = {
        fail: false,
        cancel: false,
        failSave: false,
        documents: stored?.documents ?? {},
      };
      const doc = (id: string) =>
        state.documents[id] ??
        ({
          mediaId: id,
          revision: 0,
          reviewStatus: 'unlabeled',
          eventLabel: null,
          notes: '',
          splitGroup: 'trip-a',
          annotations: [],
          masks: [],
        } as AnnotationDocument);
      const persist = () =>
        localStorage.setItem(
          'test-rust-state',
          JSON.stringify({
            remembered,
            activeId,
            documents: state.documents,
            operations,
          }),
        );
      const preview = (id: string) => ({
        ...image,
        info: {
          ...image.info,
          id,
          name: id === image.info.id ? image.info.name : 'next-frame.png',
        },
      });
      Object.assign(window, {
        __TEST_STATE__: state,
        __TAURI_INTERNALS__: {
          invoke: async (
            command: string,
            args: Record<string, unknown> = {},
          ) => {
            state.commandCounts ??= {};
            state.commandCounts[command] =
              (state.commandCounts[command] ?? 0) + 1;
            if (!initialized) {
              initialized = true;
              stored = JSON.parse(
                localStorage.getItem('test-rust-state') ?? 'null',
              ) as typeof stored;
              if (stored) {
                remembered = stored.remembered;
                activeId = stored.activeId;
                Object.assign(state.documents, stored.documents);
                Object.assign(operations, stored.operations);
              }
            }
            if (state.fail)
              throw {
                code: 'projectLocked',
                message: 'This project is open in another process.',
              };
            if (state.cancel) return null;
            if (command === 'current_project') return null;
            if (
              command === 'create_project' ||
              command === 'open_project' ||
              command === 'restore_backup'
            ) {
              opened = true;
              return project;
            }
            if (command === 'close_project') {
              opened = false;
              return null;
            }
            if (!opened) throw { code: 'noProject', message: 'No project' };
            if (command === 'media_origin')
              return state.propagationEnabled
                ? {
                    videoId: 'video-1',
                    frameIndex: 0,
                    timeMs: 0,
                    pts: 0,
                    timeBase: [1, 1000],
                  }
                : null;
            if (command === 'quality_dashboard')
              return {
                images: 2,
                reviewCounts: { approved: 1, unlabeled: 1 },
                eventCounts: { positive: 1, unassessed: 1 },
                tagCounts: { 'hard_negative:median': 1 },
                groupCount: 1,
                tripCount: 0,
                indexed: state.fingerprintIndexed ? 2 : 0,
                unindexed: state.fingerprintIndexed ? 0 : 2,
                indexErrors: 0,
                lowDetailCandidates: 0,
                pendingPredictions: state.propagationInfo ? 1 : 0,
                pendingPropagations: state.propagationInfo ? 1 : 0,
                snapshots: state.snapshots?.length ?? 0,
              };
            if (command === 'list_snapshots') return state.snapshots ?? [];
            if (
              command === 'storage_usage' ||
              command === 'clear_derived_cache'
            ) {
              if (command === 'clear_derived_cache' && !args.confirmed)
                throw Error('Explicit confirmation required');
              return {
                categories: [
                  {
                    name: 'thumbnails',
                    bytes: command === 'storage_usage' ? 4096 : 0,
                    files: 2,
                    budget: 32 * 1024 * 1024,
                    rebuildable: true,
                  },
                ],
                removableBytes: command === 'storage_usage' ? 4096 : 0,
                removedBytes: command === 'clear_derived_cache' ? 4096 : 0,
                removedFiles: command === 'clear_derived_cache' ? 2 : 0,
              };
            }
            if (command === 'index_fingerprints') {
              state.fingerprintIndexed = true;
              return { indexed: 2, reused: 0, errors: [] };
            }
            if (command === 'rank_similar_media') {
              if (!state.fingerprintIndexed)
                throw {
                  code: 'invalidInput',
                  message: 'Index this image first',
                };
              return [
                {
                  mediaId: 'test-image-2',
                  name: 'next-frame.png',
                  distance: 2,
                  sameSourceHash: false,
                  sameGroup: true,
                  brightness: 100,
                  sharpness: 30,
                },
              ];
            }
            if (command === 'create_snapshot') {
              state.snapshotOptions = structuredClone(
                args.options as SnapshotOptions,
              );
              if (!state.snapshotOptions.seed)
                throw {
                  code: 'invalidInput',
                  message: 'Snapshot requires a split seed',
                };
              state.snapshots ??= [
                {
                  id: 'b'.repeat(64),
                  name: state.snapshotOptions.name,
                  images: 2,
                  createdAt: '2026-11-07T00:00:00Z',
                },
              ];
              return state.snapshots[0];
            }
            if (command === 'verify_snapshot')
              return {
                id: args.id,
                images: 2,
                splitCounts: { train: 2 },
                sourceChecked: true,
              };
            if (command === 'export_snapshot')
              return 'C:\\exports\\frozen-version';
            if (command === 'propagation_targets')
              return [
                {
                  mediaId: 'test-image-2',
                  name: 'next-frame.png',
                  timeMs: 100,
                  deltaMs: 100,
                  reviewStatus: 'unlabeled',
                },
              ];
            if (command === 'propagate_frames') {
              state.propagationInfo = {
                key: 'review:propagation',
                trackId: 'track-1',
                anchorId: image.info.id,
                anchorRevision: doc(image.info.id).revision,
                targetId: 'test-image-2',
                dx: 3,
                dy: 2,
                meanError: 0,
                matchMargin: 30,
                skippedShapes: 0,
                verifiedRevision: null,
                verifiedAt: null,
              };
              state.reviewPredictions = [
                {
                  key: 'review:propagation',
                  mediaId: 'test-image-2',
                  baseRevision: doc('test-image-2').revision,
                  status: 'pending',
                  modelName: 'Translation propagation (experimental)',
                  modelVersion: '1',
                  eventLabel: null,
                  createdAt: '2026-11-07',
                },
              ];
              return { created: [state.propagationInfo], skipped: [] };
            }
            if (command === 'propagation_info')
              return state.propagationInfo ?? null;
            if (command === 'verify_propagation') {
              if (!args.confirmed)
                throw {
                  code: 'invalidInput',
                  message: 'Visual verification required',
                };
              state.propagationInfo!.verifiedRevision = Number(
                args.expectedRevision,
              );
              state.propagationInfo!.verifiedAt = '2026-11-07';
              return structuredClone(state.propagationInfo);
            }
            if (command === 'review_metadata')
              return (
                state.roadMetadata ?? {
                  mediaId: String(args.mediaId),
                  revision: 0,
                  videoTripRevision: null,
                  tripId: null,
                  cameraId: '',
                  route: '',
                  capturedAt: '',
                  latitude: null,
                  longitude: null,
                  tags: [],
                }
              );
            if (command === 'save_review_metadata') {
              const metadata = structuredClone(args.metadata as RoadMetadata);
              metadata.revision++;
              state.roadMetadata = metadata;
              return metadata;
            }
            if (command === 'list_trips') return state.trips ?? [];
            if (command === 'save_trip') {
              const trip = structuredClone(args.trip as Trip);
              trip.id ||= 'trip-1';
              trip.revision++;
              state.trips = [trip];
              return trip;
            }
            if (command === 'review_predictions')
              return state.reviewPredictions ?? [];
            if (command === 'import_review_prediction') {
              const row: ReviewPredictionSummary = {
                key: 'review:fixture',
                mediaId: String(args.mediaId),
                baseRevision: doc(String(args.mediaId)).revision,
                status: 'pending',
                modelName: 'Road fixture',
                modelVersion: '1',
                eventLabel: 'positive',
                createdAt: new Date().toISOString(),
              };
              state.reviewPredictions = [row];
              return row.key;
            }
            if (command === 'review_snapshot') {
              const row = state.reviewPredictions![0]!;
              return {
                summary: row,
                prediction: {
                  ...(state.propagationInfo
                    ? {
                        propagation: {
                          ...state.propagationInfo,
                          anchorSha256: image.info.sha256,
                        },
                      }
                    : {}),
                  schemaVersion: 1,
                  profileId: project.profileId,
                  mediaId: row.mediaId,
                  sourceSha256: image.info.sha256,
                  width: image.info.width,
                  height: image.info.height,
                  model: {
                    id: 'fixture',
                    name: 'Road fixture',
                    version: '1',
                    sha256: null,
                    confidence: 0.8,
                  },
                  eventLabel: 'positive',
                  annotations: [],
                  masks: [
                    {
                      layerId: 'road_extent',
                      width: image.info.width,
                      height: image.info.height,
                      runs: [
                        0,
                        100,
                        1,
                        100,
                        0,
                        image.info.width * image.info.height - 200,
                      ],
                    },
                  ],
                },
                groundTruth: doc(row.mediaId),
                comparison: {
                  gtRevision: doc(row.mediaId).revision,
                  gtApproved: doc(row.mediaId).reviewStatus === 'approved',
                  gtEvent: doc(row.mediaId).eventLabel,
                  predictedEvent: 'positive',
                  eventMismatch: true,
                  layers: [],
                  missingGtLayers: ['road_extent'],
                  hasDisagreement: true,
                  differences: [],
                  differenceOmitted: false,
                },
              };
            }
            if (command === 'reject_review_prediction') {
              state.reviewPredictions![0]!.status = 'rejected';
              return null;
            }
            if (
              command === 'approve_review_prediction' &&
              state.propagationInfo
            ) {
              if (state.propagationInfo.verifiedRevision === null)
                throw {
                  code: 'invalidInput',
                  message: 'Verification required',
                };
              const next = structuredClone(doc(state.propagationInfo.targetId));
              next.reviewStatus = 'needs_review';
              next.revision++;
              state.documents[next.mediaId] = next;
              state.reviewPredictions![0]!.status = 'accepted';
              return next;
            }
            if (command === 'approve_review_prediction')
              throw {
                code: 'invalidInput',
                message: 'Approved ground truth is protected',
              };
            if (command === 'analyze_review') {
              state.reviewAnalyzed = true;
              return 1;
            }
            if (command === 'review_page') {
              state.reviewFilter = args.filter as ReviewFilter;
              const items =
                state.reviewFilter.disagreementsOnly && !state.reviewAnalyzed
                  ? []
                  : [
                      {
                        mediaId: image.info.id,
                        name: image.info.name,
                        reviewStatus: doc(image.info.id).reviewStatus,
                        eventLabel: doc(image.info.id).eventLabel,
                        tripName: state.trips?.[0]?.name ?? null,
                        predictionKey:
                          state.reviewPredictions?.[0]?.key ?? null,
                        modelName: 'Road fixture',
                        disagreement: state.reviewAnalyzed ? true : null,
                        analyzed: state.reviewAnalyzed ?? false,
                      },
                    ];
              return { items, total: items.length };
            }
            if (command === 'export_review_metrics')
              return 'C:\\exports\\review-fixture';
            const aiModel = {
              schemaVersion: 1,
              id: 'ui-model',
              name: 'UI fixture',
              version: '1',
              license: 'test fixture',
              file: 'ui.onnx',
              sha256: 'a'.repeat(64),
              adapter: 'onnx_boxes',
              task: 'boxes',
              layerId: 'candidate_objects',
              classMap: ['candidate-0'],
              input: {
                name: 'image',
                width: 4,
                height: 2,
                mean: [0, 0, 0],
                std: [1, 1, 1],
              },
              output: {
                name: 'output',
                format: 'boxes_xyxy_score_class',
                resize: 'scale_xyxy',
              },
            };
            if (command === 'ai_status')
              return {
                config: state.aiConfigured
                  ? {
                      python: 'C:\\ai\\python.exe',
                      manifest: 'C:\\models\\model.json',
                    }
                  : null,
                runtime: {
                  running: state.aiReady ?? false,
                  busy: false,
                  model: state.aiReady ? aiModel : null,
                },
              };
            if (command === 'ai_configure') {
              state.aiConfigured = true;
              return { python: 'python', manifest: 'model.json' };
            }
            if (command === 'ai_start') {
              state.aiReady = true;
              return { health: {}, model: aiModel };
            }
            if (command === 'ai_stop') {
              state.aiReady = false;
              return null;
            }
            if (command === 'ai_cancel') {
              state.aiReady = false;
              cancelAi?.();
              return null;
            }
            if (command === 'ai_suggestions')
              return structuredClone(state.aiSuggestions ?? []);
            if (command === 'ai_predict') {
              if (state.aiHold)
                await new Promise((_, reject) => {
                  cancelAi = () =>
                    reject({
                      code: 'cancelled',
                      message: 'AI request cancelled',
                    });
                });
              const rows = (state.aiSuggestions ??= []);
              const id = `suggestion-${rows.length}`;
              rows.unshift({
                id,
                mediaId: String(args.mediaId),
                baseRevision: doc(String(args.mediaId)).revision,
                status: 'pending',
                modelName: aiModel.name,
                modelVersion: '1',
                confidence: 0.9,
                createdAt: new Date().toISOString(),
                acceptedRevision: null,
              });
              return id;
            }
            const aiAnnotation = {
              id: 'ai-box',
              layerId: 'candidate_objects',
              classId: 'candidate-0',
              geometry: {
                type: 'bbox',
                x: 100,
                y: 100,
                width: 300,
                height: 200,
              },
              source: 'model_suggestion',
            };
            if (command === 'ai_preview')
              return {
                prediction: {
                  model: aiModel,
                  width: image.info.width,
                  height: image.info.height,
                  confidence: 0.9,
                  processing: {
                    milliseconds: 12,
                    provider: 'CPUExecutionProvider',
                    device: 'cpu',
                  },
                },
                annotations: [aiAnnotation],
                masks: [],
              };
            if (command === 'ai_reject') {
              state.aiSuggestions!.find((s) => s.id === args.id)!.status =
                'rejected';
              return null;
            }
            if (command === 'ai_approve') {
              const row = state.aiSuggestions!.find((s) => s.id === args.id)!;
              const next = structuredClone(doc(row.mediaId));
              if (
                next.revision !== args.expectedRevision ||
                next.revision !== row.baseRevision
              )
                throw { code: 'conflict', message: 'stale prediction' };
              if (next.reviewStatus === 'approved')
                throw {
                  code: 'invalidInput',
                  message: 'Approved labels are protected',
                };
              next.annotations.push({
                ...aiAnnotation,
                geometry: {
                  type: 'bbox',
                  x: 100,
                  y: 100,
                  width: 300,
                  height: 200,
                },
                source: 'model_approved',
              });
              next.reviewStatus = 'needs_review';
              next.revision++;
              state.documents[row.mediaId] = next;
              row.status = 'accepted';
              row.acceptedRevision = next.revision;
              persist();
              return structuredClone(next);
            }
            if (command === 'preload_media') return preview(String(args.id));
            const video = {
              id: 'video-one',
              name: 'VFR fixture.mp4',
              path: 'C:\\fixtures\\vfr.mp4',
              sha256: '0'.repeat(64),
              codec: 'h264',
              width: 64,
              height: 48,
              rotation: 0,
              timeBaseNum: 1,
              timeBaseDen: 1000,
              startPts: 0,
              durationMs: 500,
              averageFps: 8,
              frameCount: 4,
              splitGroup: 'trip-one',
            };
            if (command === 'video_tools')
              return {
                available: state.videoReady ?? false,
                version: null,
                error: state.videoReady ? null : 'No FFmpeg tools',
              };
            if (command === 'list_videos')
              return state.videoReady ? [video] : [];
            if (command === 'video_stream') return '';
            if (command === 'video_frames')
              return {
                video,
                frames: [{ index: 0, pts: 0, timeMs: 0, durationMs: 40 }],
              };
            if (command === 'preview_frame')
              return {
                frame: {
                  index: Number(args.index),
                  pts: Number(args.index) * 40,
                  timeMs: Number(args.index) * 40,
                  durationMs: 40,
                },
                dataUrl: image.dataUrl,
              };
            if (command === 'extract_frames') {
              state.lastExtraction = args.options as ExtractionOptions;
              return {
                imported: 1,
                duplicates: 0,
                warnings: [],
                cancelled: false,
              };
            }
            if (command === 'inspect_instances')
              return {
                token: 'instance',
                classes: [17],
                names: { 17: 'source vehicle' },
                files: 1,
                objects: 1,
              };
            if (command === 'apply_instances') {
              state.instanceImported = true;
              return {
                imported: 1,
                duplicates: 0,
                warnings: [],
                cancelled: false,
              };
            }
            if (command === 'repair_media') return true;
            if (command === 'dataset_quality')
              return {
                images: 2,
                groups: [{ group: 'trip-one', images: 2, split: 'train' }],
                reviewCounts: { unlabeled: 2 },
                splitCounts: { train: 2 },
                errors: state.qualityError ? ['Missing source image'] : [],
                warnings: ['val is empty; whole groups are preserved'],
                classMapping: profile.classes,
              };
            if (command === 'project_profile') return profile;
            if (command === 'project_health') return [];
            if (command === 'team_status')
              return {
                userId: 'ui-user',
                userName: 'Tester',
                folder: state.teamFolder ?? null,
              };
            if (command === 'team_configure') {
              if (args.folder === 'pick') state.teamFolder = 'T:\\Team';
              if (args.folder === 'clear') state.teamFolder = null;
              return {
                userId: 'ui-user',
                userName: args.userName,
                folder: state.teamFolder ?? null,
              };
            }
            if (command === 'team_threads')
              return { threads: state.teamThreads ?? [], warnings: [] };
            if (command === 'team_preview') return null;
            if (command === 'team_post') {
              const input = args.input as TeamPostInput;
              state.lastTeamPost = structuredClone(input);
              state.teamThreads ??= [];
              const comment = {
                id: `c-${state.teamThreads.length}-${Date.now()}`,
                author: { id: 'ui-user', name: 'Tester' },
                createdAtMs: Date.now(),
                body: input.body,
                pin: input.pin,
                labels: [],
                project: project.name,
              };
              const existing = state.teamThreads.find(
                (t) => t.id === input.threadId,
              );
              if (existing) {
                existing.comments.push(comment);
                existing.activity++;
              } else
                state.teamThreads.push({
                  id: `thread-${state.teamThreads.length}`,
                  status: 'open',
                  statusBy: null,
                  media: {
                    sha256: image.info.sha256,
                    name: image.info.name,
                    width: 1920,
                    height: 1080,
                  },
                  localMediaId: image.info.id,
                  comments: [comment],
                  createdAtMs: comment.createdAtMs,
                  updatedAtMs: comment.createdAtMs,
                  activity: 1,
                });
              return { id: comment.id, threadId: input.threadId };
            }
            if (command === 'team_set_status') {
              const t = state.teamThreads?.find((x) => x.id === args.threadId);
              if (t) {
                t.status = args.resolved ? 'resolved' : 'open';
                t.activity++;
              }
              return { id: 'status', threadId: args.threadId };
            }
            if (command === 'save_class') {
              const id = (args.id as string | null) ?? `custom-${Date.now()}`;
              const found = profile.classes.find((c) => c.id === id);
              if (found) {
                found.displayName = args.name as string;
                found.color = args.color as string;
              } else
                profile.classes.push({
                  id,
                  namespace: 'candidate',
                  stableKey: id,
                  displayName: args.name as string,
                  exportId: 5,
                  color: args.color as string,
                  isIgnore: false,
                });
              return structuredClone(profile);
            }
            if (command === 'restore_image')
              return remembered ? preview(activeId) : null;
            if (command === 'open_image') {
              remembered = true;
              activeId = image.info.id;
              persist();
              return preview(activeId);
            }
            if (command === 'import_media') {
              remembered = true;
              persist();
              return {
                imported: 2,
                duplicates: 0,
                warnings: [],
                cancelled: false,
              };
            }
            if (command === 'list_media') {
              state.lastFilter = args.filter as LibraryFilter;
              const records = remembered
                ? [image.info.id, 'test-image-2'].map((id) => ({
                    ...preview(id).info,
                    reviewStatus: doc(id).reviewStatus,
                    importMode: 'linked',
                  }))
                : [];
              return {
                items: records
                  .filter(
                    (r) =>
                      String(args.status ?? '') === '' ||
                      r.reviewStatus === args.status,
                  )
                  .slice(
                    Number(args.offset),
                    Number(args.offset) + Number(args.limit),
                  ),
                total: records.length,
              };
            }
            if (command === 'media_thumbnail') return image.dataUrl;
            if (command === 'select_media') {
              activeId = String(args.id);
              persist();
              return preview(activeId);
            }
            if (command === 'load_document')
              return structuredClone(doc(String(args.mediaId)));
            if (command === 'save_document') {
              if (state.failSave) throw { code: 'io', message: 'disk full' };
              const document = structuredClone(
                  args.document as AnnotationDocument,
                ),
                id = String(args.operationId);
              if (operations[id]) return operations[id];
              if (doc(document.mediaId).revision !== document.revision)
                throw { code: 'conflict', message: 'stale revision' };
              document.revision++;
              state.documents[document.mediaId] = document;
              operations[id] = document.revision;
              persist();
              return document.revision;
            }
            if (command === 'export_dataset') {
              state.lastExport = args.options as ExportOptions;
              return {
                path: 'C:\\exports\\test',
                images: 2,
                annotations: 1,
                warnings: [],
              };
            }
            if (command === 'inspect_yolo')
              return { token: 'yolo', classes: [0], files: 1, objects: 1 };
            if (command === 'import_yolo')
              return {
                imported: 1,
                duplicates: 0,
                warnings: [],
                cancelled: false,
              };
            if (command === 'inspect_mask')
              return {
                token: 'mask',
                values: [0, 255],
                width: 1920,
                height: 1080,
              };
            if (command === 'apply_mask') {
              const d = structuredClone(doc(activeId));
              d.masks = [
                {
                  layerId: 'cityscapes_semantic',
                  width: 1920,
                  height: 1080,
                  runs: [0, 1, 255, 1920 * 1080 - 1],
                },
              ];
              d.revision++;
              state.documents[activeId] = d;
              persist();
              return d;
            }
            if (command === 'inspect_mask_folder')
              return {
                token: 'folder',
                values: [0, 1, 40],
                matched: 12,
                approvedSkipped: 2,
                warnings: ['x.png: no imported image with this name.'],
              };
            if (command === 'apply_mask_folder') {
              (state.samCalls ??= []).push(structuredClone(args));
              return {
                imported: 12,
                duplicates: 0,
                warnings: [],
                cancelled: false,
              };
            }
            if (command === 'sam_status')
              return {
                config: state.samReady
                  ? { python: 'py', manifest: 'sam.json' }
                  : null,
                model: state.samReady
                  ? {
                      id: 'sam',
                      name: 'Test SAM',
                      version: '1',
                      license: 'test',
                      kind: 'sam_prompt',
                      inputSize: 1024,
                    }
                  : null,
              };
            if (command === 'sam_segment') {
              (state.samCalls ??= []).push(structuredClone(args));
              if (args.encodeOnly) return { cached: false };
              const first = (
                args.points as { x: number; y: number; positive: boolean }[]
              ).find((p) => p.positive);
              const w = 1920,
                h = 1080,
                pixels = new Uint8Array(w * h);
              if (first)
                for (let y = Math.max(0, first.y - 50); y < first.y + 50; y++)
                  for (let x = Math.max(0, first.x - 50); x < first.x + 50; x++)
                    pixels[Math.floor(y) * w + Math.floor(x)] = 1;
              const runs: number[] = [];
              let v = pixels[0]!,
                n = 0;
              for (const p of pixels)
                if (p === v) n++;
                else {
                  runs.push(v, n);
                  v = p;
                  n = 1;
                }
              runs.push(v, n);
              return {
                width: w,
                height: h,
                runs,
                score: 0.9,
                milliseconds: 12,
              };
            }
            if (command === 'backup_project') return 'C:\\backup';
            if (command === 'cancel_job') return null;
            throw Error(`Unexpected command ${command}`);
          },
        },
      });
    },
    { project, image, profile },
  );
  await page.goto('/', { waitUntil: 'domcontentloaded' });
});
async function openImage(page: Page) {
  await page.getByRole('button', { name: 'Open project', exact: true }).click();
  await page.getByRole('button', { name: 'Open image', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Zoom in' })).toBeEnabled();
}
async function point(page: Page, x: number, y: number) {
  const box = await page.getByTestId('canvas-host').boundingBox();
  if (!box) throw Error('No canvas');
  const p = imageToScreen({ x, y }, fitImage(image.info, box));
  return { x: box.x + p.x, y: box.y + p.y };
}
async function drag(
  page: Page,
  from: { x: number; y: number },
  to: { x: number; y: number },
) {
  const a = await point(page, from.x, from.y),
    b = await point(page, to.x, to.y);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 8 });
  await page.mouse.up();
}
async function document(page: Page) {
  return page.evaluate(() => window.__TEST_STATE__.documents['test-image']);
}
async function drawBox(page: Page) {
  await page.getByRole('button', { name: 'Bounding box', exact: true }).click();
  await drag(page, { x: 250, y: 250 }, { x: 650, y: 550 });
}
test('M5 dashboard, similarity ranking and reproducible snapshot controls preserve live labels', async ({
  page,
}) => {
  await openImage(page);
  await drawBox(page);
  await expect.poll(() => document(page)).toMatchObject({ revision: 1 });
  const before = await document(page);
  await page
    .getByRole('button', { name: 'Dataset tools', exact: true })
    .click();
  const modal = page.getByRole('dialog', {
    name: 'Dataset tools',
    exact: true,
  });
  await expect(
    modal.getByText('Indexed sources', { exact: true }),
  ).toBeVisible();
  await modal
    .getByRole('button', { name: 'Similar images', exact: true })
    .click();
  await modal
    .getByRole('button', { name: 'Index image fingerprints', exact: true })
    .click();
  await modal
    .getByRole('button', { name: 'Rank candidates', exact: true })
    .click();
  await expect(
    modal.getByRole('cell', { name: '2 / 64', exact: true }),
  ).toBeVisible();
  await modal
    .getByRole('button', { name: 'next-frame.png', exact: true })
    .click();
  await expect(
    modal.getByRole('img', { name: 'next-frame.png', exact: true }),
  ).toBeVisible();
  await modal
    .getByRole('button', { name: 'Dataset versions', exact: true })
    .click();
  await modal
    .getByLabel('Version name', { exact: true })
    .fill('Reviewed version');
  await modal.getByLabel('Split seed', { exact: true }).fill('held-out-trip');
  await modal
    .getByRole('button', { name: 'Create immutable version', exact: true })
    .click();
  await expect(
    modal.getByRole('button', {
      name: 'Verify labels and sources',
      exact: true,
    }),
  ).toBeVisible();
  await modal
    .getByRole('button', { name: 'Create immutable version', exact: true })
    .click();
  await expect(modal.locator('tbody tr')).toHaveCount(1);
  await modal
    .getByRole('button', { name: 'Verify labels and sources', exact: true })
    .click();
  await expect(modal.getByLabel('Operation result')).toContainText(
    'sourceChecked',
  );
  await modal
    .getByRole('button', { name: 'Export frozen dataset', exact: true })
    .click();
  await expect(modal.getByLabel('Operation result')).toContainText(
    'frozen-version',
  );
  await modal.getByLabel('Split seed', { exact: true }).fill('');
  await modal
    .getByRole('button', { name: 'Create immutable version', exact: true })
    .click();
  await expect(modal.getByRole('alert')).toContainText('split seed');
  await modal.getByRole('button', { name: 'Close', exact: true }).click();
  expect(await document(page)).toEqual(before);
});
test('M5 propagation navigation requires explicit visual verification before acceptance', async ({
  page,
}) => {
  await page.evaluate(() => {
    window.__TEST_STATE__.propagationEnabled = true;
    window.__TEST_STATE__.documents['test-image-2'] = {
      mediaId: 'test-image-2',
      revision: 0,
      reviewStatus: 'in_progress',
      eventLabel: 'negative',
      notes: 'Human event',
      splitGroup: 'trip-a',
      annotations: [],
      masks: [],
    };
  });
  await openImage(page);
  await page
    .getByRole('button', { name: 'Dataset tools', exact: true })
    .click();
  const modal = page.getByRole('dialog', {
    name: 'Dataset tools',
    exact: true,
  });
  await modal
    .getByRole('button', { name: 'Video propagation', exact: true })
    .click();
  await modal.getByRole('checkbox', { name: /next-frame.png/ }).check();
  await modal
    .getByRole('button', {
      name: 'Generate translation suggestions',
      exact: true,
    })
    .click();
  await modal
    .getByRole('button', { name: 'Open target for visual review', exact: true })
    .click();
  const review = page.getByRole('dialog', {
    name: 'Road / dataset review',
    exact: true,
  });
  const accept = review.getByRole('button', {
    name: 'Accept pending prediction',
    exact: true,
  });
  await expect(accept).toBeDisabled();
  await review
    .getByRole('combobox', { name: 'Overlay view', exact: true })
    .selectOption('prediction');
  const verify = review.getByRole('button', {
    name: 'Mark visually verified',
    exact: true,
  });
  await expect(verify).toBeDisabled();
  await review
    .getByRole('checkbox', {
      name: 'I inspected the target overlay, boundaries and classes',
      exact: true,
    })
    .check();
  await verify.click();
  await expect(
    review.getByText('Verified for the current target revision', {
      exact: true,
    }),
  ).toBeVisible();
  await expect(accept).toBeEnabled();
  await accept.click();
  await expect
    .poll(() =>
      page.evaluate(() => window.__TEST_STATE__.documents['test-image-2']),
    )
    .toMatchObject({
      reviewStatus: 'needs_review',
      eventLabel: 'negative',
      revision: 1,
    });
  await expect(accept).toBeDisabled();
});
test('home and empty workspace', async ({ page }) => {
  await expect(
    page.getByRole('heading', { name: 'A workspace for your vision.' }),
  ).toBeVisible();
  await page.screenshot({ path: 'docs/milestones/screenshots/m1-home.png' });
  await page.getByLabel('Project name').fill('Đường phố');
  await page.getByRole('button', { name: 'Create project' }).click();
  await expect(page.getByRole('heading', { name: project.name })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Zoom in' })).toBeDisabled();
});

test('M3 optional AI overlay, accept, undo and reject preserve manual labels', async ({
  page,
}) => {
  await openImage(page);
  await drawBox(page);
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  const before = structuredClone(await document(page));
  if (!before) throw Error('Manual document was not saved');
  await page
    .getByRole('button', { name: 'AI assistance', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'AI assistance',
    exact: true,
  });
  await expect(dialog.getByText(/AI is off/)).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Generate suggestion', exact: true }),
  ).toBeDisabled();
  await dialog
    .getByRole('button', {
      name: 'Choose Python & model manifest',
      exact: true,
    })
    .click();
  await dialog
    .getByRole('button', { name: 'Start / restart worker', exact: true })
    .click();
  await dialog
    .getByRole('button', { name: 'Generate suggestion', exact: true })
    .click();
  await expect(
    dialog.getByRole('button', { name: 'Accept suggestion', exact: true }),
  ).toBeEnabled();
  const fit = await dialog.getByLabel('Zoom', { exact: true }).textContent();
  await dialog.getByRole('button', { name: '100%', exact: true }).click();
  await expect(dialog.getByLabel('Zoom', { exact: true })).toHaveText('100%');
  await dialog.getByTestId('canvas-host').focus();
  await page.keyboard.press('0');
  await expect(dialog.getByLabel('Zoom', { exact: true })).toHaveText(fit!);
  await page.keyboard.press('b');
  await page.keyboard.press('Delete');
  expect((await document(page))?.annotations).toEqual(before.annotations);
  await dialog.getByLabel('Show suggestion overlay', { exact: true }).uncheck();
  await dialog.getByLabel('Show suggestion overlay', { exact: true }).check();
  await dialog
    .getByRole('button', { name: 'Accept suggestion', exact: true })
    .click();
  await expect(dialog.getByText('accepted', { exact: true })).toBeVisible();
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  expect((await document(page))?.annotations[0]).toEqual(before.annotations[0]);
  expect((await document(page))?.annotations[1]?.source).toBe('model_approved');
  await page.keyboard.press('Control+z');
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  await page
    .getByRole('button', { name: 'AI assistance', exact: true })
    .click();
  await dialog
    .getByRole('button', { name: 'Generate suggestion', exact: true })
    .click();
  await dialog
    .getByRole('button', { name: 'Reject suggestion', exact: true })
    .click();
  await expect(dialog.getByText('rejected', { exact: true })).toBeVisible();
  expect((await document(page))?.annotations).toEqual(before.annotations);
});

test('M3 cancellation returns to manual editing without a late suggestion', async ({
  page,
}) => {
  await openImage(page);
  await page.evaluate(() => {
    window.__TEST_STATE__.aiConfigured = true;
    window.__TEST_STATE__.aiReady = true;
    window.__TEST_STATE__.aiHold = true;
  });
  await page
    .getByRole('button', { name: 'AI assistance', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'AI assistance',
    exact: true,
  });
  await dialog
    .getByRole('button', { name: 'Generate suggestion', exact: true })
    .click();
  await dialog
    .getByRole('button', { name: 'Cancel AI request', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toContainText('cancelled');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  expect(
    await page.evaluate(() => window.__TEST_STATE__.aiSuggestions ?? []),
  ).toEqual([]);
  await drawBox(page);
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
});

test('M4 event, independent overlays, metadata, trip and disagreement queue', async ({
  page,
}) => {
  await openImage(page);
  await drawBox(page);
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  const manual = (await document(page))!.annotations;
  await page
    .getByRole('button', { name: 'Road / dataset review', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'Road / dataset review',
    exact: true,
  });
  await dialog
    .getByLabel('Human event assessment', { exact: true })
    .selectOption('negative');
  await expect
    .poll(async () => (await document(page))?.eventLabel)
    .toBe('negative');
  await dialog
    .getByLabel('Human review status', { exact: true })
    .selectOption('approved');
  await dialog
    .getByRole('button', { name: 'Import prediction JSON', exact: true })
    .click();
  for (const view of ['groundTruth', 'prediction', 'both', 'difference'])
    await dialog.getByLabel('Overlay view', { exact: true }).selectOption(view);
  await dialog
    .getByRole('button', { name: 'Accept pending prediction', exact: true })
    .click();
  await expect(dialog.getByRole('alert')).toContainText('protected');
  expect((await document(page))!.annotations).toEqual(manual);
  await dialog
    .getByRole('button', { name: 'Reject pending prediction', exact: true })
    .click();
  await dialog.getByRole('button', { name: 'Trips', exact: true }).click();
  await dialog.getByLabel('Trip name', { exact: true }).fill('Saigon route');
  await dialog
    .getByLabel('Measured trip distance (km, optional)', { exact: true })
    .fill('2.5');
  await dialog.getByRole('button', { name: 'Save trip', exact: true }).click();
  await dialog
    .getByRole('button', { name: 'Tags & capture', exact: true })
    .click();
  await dialog
    .getByLabel('Capture trip', { exact: true })
    .selectOption('trip-1');
  await dialog
    .getByLabel('hard negative:stopped vehicle', { exact: true })
    .check();
  await dialog
    .getByRole('button', { name: 'Save capture metadata & tags', exact: true })
    .click();
  expect(
    await page.evaluate(() => window.__TEST_STATE__.roadMetadata?.tags),
  ).toEqual(['hard_negative:stopped_vehicle']);
  expect((await document(page))?.eventLabel).toBe('negative');
  await dialog
    .getByRole('button', { name: 'Review queue', exact: true })
    .click();
  await dialog.getByLabel('Disagreements only', { exact: true }).check();
  await expect(
    dialog.getByText('0 images match.', { exact: false }),
  ).toBeVisible();
  await dialog
    .getByRole('button', { name: 'Analyze comparisons', exact: true })
    .click();
  await expect(
    dialog.getByRole('cell', { name: 'Disagreement', exact: true }),
  ).toBeVisible();
  await dialog
    .getByRole('button', { name: 'Export review metrics', exact: true })
    .click();
  await expect(dialog.getByRole('status')).toContainText('review-fixture');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  expect((await document(page))!.annotations).toEqual(manual);
});

test('M4 unsaved metadata requires deliberate discard and modal shortcuts stay scoped', async ({
  page,
}) => {
  await openImage(page);
  await drawBox(page);
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  await page
    .getByRole('button', { name: 'Road / dataset review', exact: true })
    .click();
  const dialog = page.getByRole('dialog', {
    name: 'Road / dataset review',
    exact: true,
  });
  await dialog
    .getByRole('button', { name: 'Tags & capture', exact: true })
    .click();
  await dialog.getByLabel('Camera ID', { exact: true }).fill('unsaved camera');
  await page.keyboard.press('Delete');
  expect((await document(page))?.annotations.length).toBe(1);
  page.once('dialog', (d) => void d.dismiss());
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toBeVisible();
  page.once('dialog', (d) => void d.accept());
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).toHaveCount(0);
});
test('zoom/pan, fit and 100% retain canonical rendering', async ({ page }) => {
  await openImage(page);
  const zoom = page.getByLabel('Zoom', { exact: true });
  const initial = await zoom.textContent();
  await page.mouse.move(
    ...(Object.values(await point(page, 960, 540)) as [number, number]),
  );
  await page.mouse.wheel(0, -150);
  await expect(zoom).not.toHaveText(initial!);
  await page.getByRole('button', { name: '100%', exact: true }).click();
  await expect(zoom).toHaveText('100%');
  await page.keyboard.press('0');
  await expect(zoom).toHaveText(initial!);
  const data = await page.locator('canvas').evaluate((c) => {
    const canvas = c as HTMLCanvasElement;
    return canvas
      .getContext('2d')!
      .getImageData(
        Math.floor(canvas.width / 2),
        Math.floor(canvas.height / 2),
        1,
        1,
      ).data[3];
  });
  expect(data).toBe(255);
});
test('bbox classification, undo/redo, save and reopen', async ({ page }) => {
  await openImage(page);
  await drawBox(page);
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  await page.keyboard.press('2');
  await expect
    .poll(async () => (await document(page))?.annotations[0]?.classId)
    .toBe('candidate-1');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect
    .poll(async () => (await document(page))?.annotations[0]?.classId)
    .toBe('candidate-0');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(0);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  const before = await document(page);
  await page.getByRole('button', { name: 'Close project' }).click();
  await page.getByRole('button', { name: 'Open project', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Zoom in' })).toBeEnabled();
  expect((await document(page))?.annotations).toEqual(before?.annotations);
});
test('polygon validation and deliberate deletion', async ({ page }) => {
  await openImage(page);
  await page.getByRole('button', { name: 'Polygon', exact: true }).click();
  for (const p of [
    [250, 200],
    [600, 500],
    [250, 500],
    [600, 200],
  ]) {
    const q = await point(page, p[0]!, p[1]!);
    await page.mouse.click(q.x, q.y);
  }
  await page.keyboard.press('Enter');
  await expect(page.getByRole('alert')).toContainText('Polygon must be simple');
  await page.keyboard.press('Escape');
  for (const p of [
    [250, 200],
    [600, 200],
    [450, 500],
  ]) {
    const q = await point(page, p[0]!, p[1]!);
    await page.mouse.click(q.x, q.y);
  }
  await page.keyboard.press('Enter');
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  page.once('dialog', (d) => d.accept());
  await page
    .getByRole('button', { name: 'Delete selected annotation' })
    .click();
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(0);
});

test('select moves and resizes boxes without changing image geometry during pan', async ({
  page,
}) => {
  await openImage(page);
  await drawBox(page);
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  await page.getByRole('button', { name: 'Select', exact: true }).click();
  const initial = (await document(page))!.annotations[0]!.geometry;
  if (initial.type !== 'bbox') throw Error('Expected bbox');
  await drag(
    page,
    { x: initial.x + 100, y: initial.y + 100 },
    { x: initial.x + 220, y: initial.y + 160 },
  );
  await expect.poll(async () => (await document(page))?.revision).toBe(2);
  const moved = (await document(page))!.annotations[0]!.geometry;
  if (moved.type !== 'bbox') throw Error('Expected bbox');
  expect(Math.abs(moved.x - initial.x - 120)).toBeLessThan(4);
  expect(Math.abs(moved.y - initial.y - 60)).toBeLessThan(4);
  await drag(
    page,
    { x: moved.x + moved.width, y: moved.y + moved.height },
    { x: moved.x + moved.width + 70, y: moved.y + moved.height + 40 },
  );
  await expect.poll(async () => (await document(page))?.revision).toBe(3);
  const resized = (await document(page))!.annotations[0]!.geometry;
  if (resized.type !== 'bbox') throw Error('Expected bbox');
  expect(Math.abs(resized.width - moved.width - 70)).toBeLessThan(4);
  const saved = await document(page);
  await page.getByTestId('canvas-host').focus();
  await page.keyboard.down('Space');
  await drag(page, { x: 1000, y: 600 }, { x: 1100, y: 650 });
  await page.keyboard.up('Space');
  expect(await document(page)).toEqual(saved);
});

test('panel widths restore and separator arrows do not navigate images', async ({
  page,
}) => {
  await openImage(page);
  const separator = page.getByRole('separator', { name: 'Resize media panel' });
  await separator.focus();
  await page.keyboard.press('ArrowRight');
  await expect(separator).toHaveAttribute('aria-valuenow', '270');
  await expect(page.locator('.filename')).toHaveText(image.info.name);
  await page
    .getByRole('button', { name: 'Close project', exact: true })
    .click();
  await page.getByRole('button', { name: 'Open project', exact: true }).click();
  await expect(
    page.getByRole('separator', { name: 'Resize media panel' }),
  ).toHaveAttribute('aria-valuenow', '270');
});
test('semantic brush, eraser, separate region layer and review', async ({
  page,
}) => {
  await openImage(page);
  await page
    .getByLabel('Active layer', { exact: true })
    .selectOption('cityscapes_semantic');
  await page.getByRole('button', { name: 'Brush', exact: true }).click();
  await drag(page, { x: 150, y: 150 }, { x: 200, y: 150 });
  await expect.poll(async () => (await document(page))?.masks.length).toBe(1);
  let doc = (await document(page))!;
  expect(decodeRuns(doc.masks[0]!)[150 * 1920 + 170]).toBe(0);
  await page.getByRole('button', { name: 'Eraser', exact: true }).click();
  await drag(page, { x: 150, y: 150 }, { x: 200, y: 150 });
  await expect
    .poll(
      async () =>
        decodeRuns((await document(page))!.masks[0]!)[150 * 1920 + 170],
    )
    .toBe(255);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect
    .poll(
      async () =>
        decodeRuns((await document(page))!.masks[0]!)[150 * 1920 + 170],
    )
    .toBe(0);
  await page
    .getByLabel('Active layer', { exact: true })
    .selectOption('road_extent');
  await page.getByRole('button', { name: 'Brush', exact: true }).click();
  await drag(page, { x: 300, y: 300 }, { x: 600, y: 300 });
  await expect.poll(async () => (await document(page))?.masks.length).toBe(2);
  await page.getByLabel('Notes', { exact: true }).fill('Offline smoke test');
  await page.getByLabel('Notes', { exact: true }).press('ArrowRight');
  await expect(page.getByRole('button', { name: 'Zoom in' })).toBeEnabled();
  await page
    .getByLabel('Image event', { exact: true })
    .selectOption('uncertain');
  await page
    .getByLabel('Review state', { exact: true })
    .selectOption('approved');
  await expect
    .poll(async () => (await document(page))?.reviewStatus)
    .toBe('approved');
  doc = (await document(page))!;
  expect(doc.eventLabel).toBe('uncertain');
  expect(doc.notes).toBe('Offline smoke test');
  expect(
    decodeRuns(doc.masks.find((m) => m.layerId === 'road_extent')!)[
      300 * 1920 + 400
    ],
  ).toBe(1);
  await page.screenshot({
    path: 'docs/milestones/screenshots/m1-workspace.png',
  });
});
test('semantic polygon fills pixels with the chosen class and undoes as one step', async ({
  page,
}) => {
  await openImage(page);
  await page
    .getByLabel('Active layer', { exact: true })
    .selectOption('cityscapes_semantic');
  await page.getByRole('button', { name: 'Polygon', exact: true }).click();
  await page.getByRole('button', { name: /^1 · sidewalk/ }).click();
  for (const p of [
    [200, 200],
    [600, 200],
    [600, 500],
    [200, 500],
  ]) {
    const q = await point(page, p[0]!, p[1]!);
    await page.mouse.click(q.x, q.y);
  }
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await document(page))?.masks.length).toBe(1);
  const doc = (await document(page))!;
  const pixels = decodeRuns(doc.masks[0]!);
  expect(doc.annotations).toHaveLength(0);
  expect(pixels[300 * 1920 + 400]).toBe(1);
  expect(pixels[100 * 1920 + 400]).toBe(255);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect
    .poll(async () => {
      const masks = (await document(page))!.masks;
      return masks.length ? decodeRuns(masks[0]!)[300 * 1920 + 400] : 255;
    })
    .toBe(255);
});
test('SAM click-to-mask previews, excludes with right click and writes the chosen class', async ({
  page,
}) => {
  await page.evaluate(() => {
    (
      window as unknown as { __TEST_STATE__: TestState }
    ).__TEST_STATE__.samReady = true;
  });
  await openImage(page);
  await expect(page.getByText(/Ready · Test SAM/)).toBeVisible();
  await page
    .getByLabel('Active layer', { exact: true })
    .selectOption('cityscapes_semantic');
  await page.getByRole('button', { name: /^1 · sidewalk/ }).click();
  await page.keyboard.press('s');
  await expect(
    page.getByRole('button', { name: 'SAM', exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
  const calls = () =>
    page.evaluate(
      () =>
        (window as unknown as { __TEST_STATE__: TestState }).__TEST_STATE__
          .samCalls ?? [],
    );
  await expect.poll(async () => (await calls())[0]?.encodeOnly).toBe(true);
  const q = await point(page, 400, 300);
  await page.mouse.click(q.x, q.y);
  await expect(page.getByRole('button', { name: 'Apply mask' })).toBeEnabled();
  const r = await point(page, 700, 700);
  await page.mouse.click(r.x, r.y, { button: 'right' });
  await expect
    .poll(async () => {
      const last = (await calls()).at(-1) as {
        points: { positive: boolean }[];
      };
      return last.points.map((p) => p.positive);
    })
    .toEqual([true, false]);
  await page.keyboard.press('Enter');
  await expect.poll(async () => (await document(page))?.masks.length).toBe(1);
  const pixels = decodeRuns((await document(page))!.masks[0]!);
  expect(pixels[300 * 1920 + 400]).toBe(1);
  expect(pixels[100 * 1920 + 100]).toBe(255);
  await expect(page.getByRole('button', { name: 'Apply mask' })).toBeDisabled();
});
test('mask folder pre-labels prefill identity mapping and send the merge mode', async ({
  page,
}) => {
  await openImage(page);
  await page
    .getByLabel('Import class-ID PNG', { exact: true })
    .selectOption('cityscapes_semantic');
  await page.getByRole('button', { name: 'Import mask folder' }).click();
  await expect(page.getByText('12 masks match images.')).toBeVisible();
  const apply = page.getByRole('button', { name: 'Import masks' });
  // 0 and 1 are project IDs and map to themselves; 40 needs a decision.
  await expect(page.getByLabel('Source ID 0', { exact: true })).toHaveValue(
    '0',
  );
  await expect(apply).toBeDisabled();
  await page.getByLabel('Source ID 40', { exact: true }).selectOption('255');
  await page
    .getByLabel('Merge with existing labels', { exact: true })
    .selectOption('replace');
  await apply.click();
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          (window as unknown as { __TEST_STATE__: TestState }).__TEST_STATE__
            .samCalls?.[0],
      ),
    )
    .toEqual({
      token: 'folder',
      mode: 'replace',
      mapping: [
        { source: 0, target: 0 },
        { source: 1, target: 1 },
        { source: 40, target: 255 },
      ],
    });
});
test('failed save blocks navigation and retry persists the draft', async ({
  page,
}) => {
  await openImage(page);
  await page.evaluate(() => {
    window.__TEST_STATE__.failSave = true;
  });
  await drawBox(page);
  await expect(page.getByRole('button', { name: 'Retry save' })).toBeVisible();
  await page.getByRole('button', { name: 'Next image', exact: true }).click();
  await expect(page.getByRole('button', { name: 'Retry save' })).toBeVisible();
  expect((await document(page))?.annotations.length ?? 0).toBe(0);
  await page.evaluate(() => {
    window.__TEST_STATE__.failSave = false;
  });
  await page.getByRole('button', { name: 'Retry save' }).click();
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  await page.getByRole('button', { name: 'Next image', exact: true }).click();
  await expect(page.locator('.filename')).toHaveText('next-frame.png');
});
test('reload recovers the durable outbox before loading the saved image', async ({
  page,
}) => {
  await openImage(page);
  await page.evaluate(() => {
    window.__TEST_STATE__.failSave = true;
  });
  await drawBox(page);
  await expect(page.getByRole('button', { name: 'Retry save' })).toBeVisible();
  page.on('dialog', (d) => d.accept());
  await page.reload();
  await page.getByRole('button', { name: 'Open project', exact: true }).click();
  await expect
    .poll(async () => (await document(page))?.annotations.length)
    .toBe(1);
  await expect(page.getByRole('button', { name: 'Zoom in' })).toBeEnabled();
});
test('explicit class mapping and export options', async ({ page }) => {
  await openImage(page);
  await page.getByRole('button', { name: 'Import YOLO labels' }).click();
  await expect(
    page.getByRole('button', { name: 'Append mapped labels' }),
  ).toBeDisabled();
  await page
    .getByLabel('Source ID 0', { exact: true })
    .selectOption('candidate-2');
  await page.getByRole('button', { name: 'Append mapped labels' }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: 'Close', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Export dataset', exact: true })
    .click();
  await page.getByLabel('Approved images only').check();
  await page
    .getByRole('button', { name: 'Choose destination and export' })
    .click();
  await expect(page.getByRole('dialog')).toContainText('C:\\exports\\test');
  expect(
    await page.evaluate(() => window.__TEST_STATE__.lastExport?.approvedOnly),
  ).toBe(true);
});
test.describe('HiDPI', () => {
  test.use({ deviceScaleFactor: 2 });
  test('backing store follows device pixels', async ({ page }) => {
    await openImage(page);
    const sizes = await page.locator('canvas').evaluate((el) => {
      const c = el as HTMLCanvasElement;
      return {
        backing: c.width,
        css: c.getBoundingClientRect().width,
        dpr: devicePixelRatio,
      };
    });
    expect(sizes.dpr).toBe(2);
    expect(sizes.backing).toBe(Math.round(sizes.css * 2));
  });
});

test('M2 tools unavailable stays recoverable and modal shortcuts cannot edit labels', async ({
  page,
}) => {
  await openImage(page);
  await page
    .getByRole('button', { name: 'Video workspace', exact: true })
    .click();
  const dialog = page.getByRole('dialog', { name: 'Video workspace' });
  await expect(dialog.getByText(/Video tools are unavailable/)).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Import video', exact: true }),
  ).toBeDisabled();
  await page.keyboard.press('b');
  await page.keyboard.press('Delete');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(
    page.getByRole('button', { name: 'Select', exact: true }),
  ).toHaveAttribute('aria-pressed', 'true');
});
test('M2 exact frame stepping and extraction sends indexed frame and explicit format', async ({
  page,
}) => {
  await openImage(page);
  await page.evaluate(() => {
    window.__TEST_STATE__.videoReady = true;
  });
  await page
    .getByRole('button', { name: 'Video workspace', exact: true })
    .click();
  const dialog = page.getByRole('dialog', { name: 'Video workspace' });
  await expect(dialog.getByText(/h264/)).toBeVisible();
  await dialog.getByRole('button', { name: 'Next frame', exact: true }).click();
  await expect(dialog.getByAltText('Frame index 1')).toBeVisible();
  await expect(dialog.getByText(/PTS 40/)).toBeVisible();
  await dialog.getByLabel('Frame format').selectOption('jpeg');
  await dialog
    .getByRole('button', { name: 'Extract selected frame', exact: true })
    .click();
  await expect
    .poll(() => page.evaluate(() => window.__TEST_STATE__.lastExtraction))
    .toMatchObject({
      videoId: 'video-one',
      indices: [1],
      intervalMs: null,
      format: 'jpeg',
      quality: 95,
    });
});
test('M2 COCO import requires explicit source mapping', async ({ page }) => {
  await openImage(page);
  await page.getByRole('button', { name: 'Import COCO', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Confirm class mapping' });
  await expect(
    dialog.getByText('source vehicle', { exact: false }),
  ).toBeVisible();
  await expect(
    dialog.getByRole('button', { name: 'Append mapped labels', exact: true }),
  ).toBeDisabled();
  await dialog.getByLabel('Source ID 17').selectOption('candidate-2');
  await dialog
    .getByRole('button', { name: 'Append mapped labels', exact: true })
    .click();
  await expect
    .poll(() => page.evaluate(() => window.__TEST_STATE__.instanceImported))
    .toBe(true);
});
test('M2 export previews whole groups and quality errors block publication', async ({
  page,
}) => {
  await openImage(page);
  await page.evaluate(() => {
    window.__TEST_STATE__.qualityError = true;
  });
  await page
    .getByRole('button', { name: 'Export dataset', exact: true })
    .click();
  const dialog = page.getByRole('dialog', { name: 'Export dataset' });
  await dialog
    .getByRole('button', { name: 'Preview grouped splits', exact: true })
    .click();
  await expect(
    dialog.getByText('Missing source image', { exact: true }),
  ).toBeVisible();
  await expect(
    dialog.getByRole('button', {
      name: 'Choose destination and export',
      exact: true,
    }),
  ).toBeDisabled();
  await dialog.getByRole('combobox').selectOption('yoloSeg');
  await expect(
    dialog.getByLabel(/Allow skipping unsupported/),
  ).not.toBeChecked();
});
test('ordinary redraws reuse the canvas backing store and note saves avoid library refresh/preload', async ({
  page,
}) => {
  await openImage(page);
  await page.waitForTimeout(350); // Let the single deferred neighbor preload settle.
  await page.evaluate(() => {
    const canvas = window.document.querySelector(
      '.canvas-host canvas',
    ) as HTMLCanvasElement;
    const counts = { width: 0, height: 0 };
    for (const key of ['width', 'height'] as const) {
      const descriptor = Object.getOwnPropertyDescriptor(
        HTMLCanvasElement.prototype,
        key,
      )!;
      Object.defineProperty(canvas, key, {
        get: () => descriptor.get!.call(canvas),
        set: (value: number) => {
          counts[key]++;
          descriptor.set!.call(canvas, value);
        },
        configurable: true,
      });
    }
    Object.assign(window, { __CANVAS_ALLOCATION__: counts });
  });
  const before = await page.evaluate(() => ({
    ...window.__TEST_STATE__.commandCounts,
  }));
  for (let i = 0; i < 12; i++) await page.mouse.move(500 + i * 3, 400 + i * 2);
  for (const note of ['first note', 'another note', 'final note']) {
    await page.getByLabel('Notes', { exact: true }).fill(note);
    await expect(page.locator('.editor-status .save-saved')).toBeVisible();
  }
  const result = await page.evaluate(() => ({
    counts: window.__TEST_STATE__.commandCounts,
    allocations: (
      window as unknown as {
        __CANVAS_ALLOCATION__: { width: number; height: number };
      }
    ).__CANVAS_ALLOCATION__,
  }));
  expect(result.allocations).toEqual({ width: 0, height: 0 });
  expect(result.counts?.list_media).toBe(before.list_media);
  expect(result.counts?.preload_media).toBe(before.preload_media);
  expect(result.counts?.save_document).toBeGreaterThan(
    before.save_document ?? 0,
  );
});

test('storage cleanup requires an explicit choice and reports reclaimable cache separately', async ({
  page,
}) => {
  await openImage(page);
  await page
    .getByRole('button', { name: 'Dataset tools', exact: true })
    .click();
  await page
    .getByRole('button', { name: 'Storage & performance', exact: true })
    .click();
  const panel = page.getByRole('region', { name: 'Storage & performance' });
  await expect(
    panel.getByText('Library thumbnails', { exact: true }),
  ).toBeVisible();
  await expect(
    panel.getByRole('button', { name: 'Clear derived cache', exact: true }),
  ).toBeDisabled();
  await panel
    .getByLabel('Rebuild thumbnails and previews when needed', { exact: true })
    .check();
  await panel
    .getByRole('button', { name: 'Clear derived cache', exact: true })
    .click();
  await expect(panel.getByRole('status')).toContainText('4.0 KiB');
});

test('M2 review event/group/annotation filters are passed to paginated backend', async ({
  page,
}) => {
  await openImage(page);
  await page
    .getByLabel('Filter event', { exact: true })
    .selectOption('uncertain');
  await page
    .getByLabel('Filter annotation presence', { exact: true })
    .selectOption('true');
  await page
    .getByLabel('Filter source / trip group', { exact: true })
    .fill('trip-2026');
  await expect
    .poll(() => page.evaluate(() => window.__TEST_STATE__.lastFilter))
    .toEqual({ event: 'uncertain', group: 'trip-2026', annotated: true });
});

test('custom classes, Done → next and team discussion with a pin', async ({
  page,
}) => {
  await openImage(page);
  // Custom class: add from the inspector and use it immediately.
  await page.getByRole('button', { name: 'Edit classes' }).click();
  await page.getByLabel('New class name').fill('Helmet');
  await page.getByRole('button', { name: 'Add class' }).click();
  await expect(page.getByLabel('Class name Helmet')).toBeVisible();
  await page.getByRole('button', { name: 'Done', exact: true }).click();
  await page.getByRole('button', { name: /5 · Helmet/ }).click();
  await drawBox(page);
  await expect
    .poll(async () => (await document(page))?.annotations[0]?.classId)
    .toMatch(/^custom-/);
  // Shortcut help and keyboard review workflow.
  await page.keyboard.press('?');
  await expect(
    page.getByRole('dialog', { name: 'Keyboard shortcuts' }),
  ).toBeVisible();
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await page.keyboard.press('Control+Enter');
  await expect
    .poll(async () => (await document(page))?.reviewStatus)
    .toBe('needs_review');
  await expect(page.locator('.filename')).toHaveText('next-frame.png');
  await page.getByRole('button', { name: 'Previous image' }).click();
  await expect(page.locator('.filename')).toHaveText(image.info.name);
  // Team: connect a folder, then ask about this image with a pin.
  await page.getByRole('button', { name: 'Set up team folder' }).click();
  await page.getByLabel('Your display name').fill('Lan');
  await page.getByRole('button', { name: 'Choose shared folder' }).click();
  await page.getByRole('button', { name: 'Close', exact: true }).click();
  await page
    .getByRole('button', { name: 'Ask the team about this image' })
    .click();
  await page.getByLabel('Ask the team about this image').fill('Helmet or hat?');
  await page.getByRole('button', { name: 'Pin a spot' }).click();
  const spot = await point(page, 400, 300);
  await page.mouse.click(spot.x, spot.y);
  await expect(page.getByRole('button', { name: /Pin placed/ })).toBeVisible();
  await page.getByRole('button', { name: 'Post question' }).click();
  await expect
    .poll(() => page.evaluate(() => window.__TEST_STATE__.lastTeamPost))
    .toMatchObject({
      body: 'Helmet or hat?',
      mediaId: 'test-image',
      includeLabels: true,
      threadId: null,
    });
  await expect(
    page.getByRole('button', { name: /Helmet or hat\?/ }),
  ).toBeVisible();
  await page.screenshot({
    path: 'docs/milestones/screenshots/team-ask-desktop.png',
  });
  const pin = await page.evaluate(
    () => window.__TEST_STATE__.lastTeamPost?.pin,
  );
  expect(pin!.x).toBeCloseTo(400, -1);
  expect(pin!.y).toBeCloseTo(300, -1);
  // The thread appears in the inspector; reply and resolve in the panel.
  await page.getByRole('button', { name: /Helmet or hat\?/ }).click();
  const dialog = page.getByRole('dialog', { name: 'Team discussion' });
  await dialog.getByLabel('Write a reply…').fill('A hard hat: helmet.');
  await dialog.getByRole('button', { name: 'Reply' }).click();
  await expect(dialog.getByText('A hard hat: helmet.')).toBeVisible();
  await dialog.getByRole('button', { name: 'Mark resolved' }).click();
  await expect(dialog.getByRole('button', { name: 'Reopen' })).toBeVisible();
  await page.screenshot({
    path: 'docs/milestones/screenshots/team-discussion-desktop.png',
  });
});
