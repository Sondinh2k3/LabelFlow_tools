import type { ReviewStatus } from './index';
export interface SnapshotOptions {
  name: string;
  seed: string;
  trainPercent: number;
  valPercent: number;
  approvedOnly: boolean;
}
export interface DatasetSnapshot {
  id: string;
  name: string;
  images: number;
  createdAt: string;
}
export interface SnapshotCheck {
  id: string;
  images: number;
  splitCounts: Record<string, number>;
  sourceChecked: boolean;
}
export interface FrameCandidate {
  mediaId: string;
  name: string;
  timeMs: number;
  deltaMs: number;
  reviewStatus: ReviewStatus;
}
export interface PropagationOrigin {
  trackId: string;
  anchorId: string;
  anchorRevision: number;
  anchorSha256: string;
  dx: number;
  dy: number;
  meanError: number;
  matchMargin: number;
  skippedShapes: number;
}
export interface PropagationInfo extends Omit<
  PropagationOrigin,
  'anchorSha256'
> {
  key: string;
  targetId: string;
  verifiedRevision: number | null;
  verifiedAt: string | null;
}
export interface PropagationReport {
  created: PropagationInfo[];
  skipped: string[];
}
export interface FingerprintReport {
  indexed: number;
  reused: number;
  errors: string[];
}
export interface DuplicateCandidate {
  mediaId: string;
  name: string;
  distance: number;
  sameSourceHash: boolean;
  sameGroup: boolean;
  brightness: number;
  sharpness: number;
}
export interface DatasetDashboard {
  images: number;
  reviewCounts: Record<string, number>;
  eventCounts: Record<string, number>;
  tagCounts: Record<string, number>;
  groupCount: number;
  tripCount: number;
  indexed: number;
  indexErrors: number;
  unindexed: number;
  lowDetailCandidates: number;
  pendingPredictions: number;
  pendingPropagations: number;
  snapshots: number;
}
