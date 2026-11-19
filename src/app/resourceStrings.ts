export const resources = {
  title: 'Storage & performance',
  help: 'Image decode cache: up to 2 images / 32 MiB of estimated RGBA. Large active images remain usable; speculative preload stops above this budget. Undo: up to 100 commands / 64 MiB of raster history.',
  scope:
    'Sizes cover project metadata only. Source media, external datasets, exports, WebView2 data and developer build tools are separate.',
  preserved:
    'Cleanup removes recognized thumbnails and previews that can be rebuilt. It preserves original media, labels, dataset versions, operation history and backups.',
  confirm: 'Rebuild thumbnails and previews when needed',
  clear: 'Clear derived cache',
  refresh: 'Refresh usage',
  category: 'Category',
  size: 'Size',
  budget: 'Cache budget',
  removed: 'Cache removed',
  loading: 'Measuring project storage…',
  database: 'Database, versions & operation history',
  labels: 'Lossless label assets',
  thumbnails: 'Library thumbnails',
  previews: 'Image previews',
  videoPreviews: 'Video previews',
  backups: 'Backups',
  logs: 'Recovery logs',
} as const;
