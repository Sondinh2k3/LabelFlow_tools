export interface StorageCategory {
  name:
    | 'database'
    | 'labels'
    | 'thumbnails'
    | 'previews'
    | 'videoPreviews'
    | 'backups'
    | 'logs';
  bytes: number;
  files: number;
  budget: number | null;
  rebuildable: boolean;
}
export interface StorageUsage {
  categories: StorageCategory[];
  removableBytes: number;
  removedBytes: number;
  removedFiles: number;
}
