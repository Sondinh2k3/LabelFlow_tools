// Wire types for shared-folder team discussions (Rust: services/team.rs, commands/team.rs).
export interface TeamAuthor {
  id: string;
  name: string;
}
export interface TeamMediaRef {
  sha256: string;
  name: string;
  width: number;
  height: number;
}
export interface TeamLabelShape {
  name: string;
  color: string;
  kind: 'box' | 'polygon';
  points: [number, number][];
}
export interface TeamComment {
  id: string;
  author: TeamAuthor;
  createdAtMs: number;
  body: string;
  pin: { x: number; y: number } | null;
  labels: TeamLabelShape[];
  project: string;
}
export interface TeamThread {
  id: string;
  status: 'open' | 'resolved';
  statusBy: TeamAuthor | null;
  media: TeamMediaRef;
  localMediaId: string | null;
  comments: TeamComment[];
  createdAtMs: number;
  updatedAtMs: number;
  activity: number;
}
export interface TeamFeed {
  threads: TeamThread[];
  warnings: string[];
}
export interface TeamStatus {
  userId: string;
  userName: string;
  folder: string | null;
}
export interface TeamPostInput {
  threadId: string | null;
  mediaId: string | null;
  body: string;
  pin: { x: number; y: number } | null;
  includeLabels: boolean;
}
export interface TeamEntry {
  id: string;
  threadId: string;
}
