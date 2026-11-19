export const team = {
  title: 'Team discussion',
  section: 'DISCUSSION',
  setupTitle: 'Shared team folder',
  setupHelp:
    'Pick a folder everyone can reach: a LAN share, or a OneDrive / Google Drive / Dropbox / Syncthing folder. Each person writes only their own files there, so sync tools never conflict. Images are matched by content, so teammates can use their own project copies.',
  yourName: 'Your display name',
  chooseFolder: 'Choose shared folder',
  changeFolder: 'Change folder',
  disconnect: 'Disconnect',
  saveName: 'Save name',
  notConnected: 'Not connected to a team folder.',
  connectHint:
    'Connect a shared team folder to ask colleagues about hard cases.',
  setUp: 'Set up team folder',
  open: 'Open',
  resolved: 'Resolved',
  all: 'All',
  mine: 'Mine',
  thisImage: 'This image only',
  noThreads: 'No discussions here yet.',
  ask: 'Ask the team about this image',
  question: 'What is unclear? e.g. "Is this a street stall or goods cluster?"',
  reply: 'Write a reply…',
  post: 'Post question',
  send: 'Reply',
  shareLabels: 'Share my current labels',
  pin: 'Pin a spot',
  pinActive: 'Click the image to place the pin…',
  pinSet: 'Pin placed · click to move',
  clearPin: 'Remove pin',
  resolve: 'Mark resolved',
  reopen: 'Reopen',
  openImage: 'Open image in editor',
  notInProject:
    'This image is not in your project; the shared preview is shown.',
  showLabels: 'Show shared labels',
  noPreview: 'Preview not synced yet.',
  refresh: 'Refresh',
  newCount: (n: number) => `${n} new`,
  replies: (n: number) => (n === 1 ? '1 comment' : `${n} comments`),
  resolvedBy: (name: string) => `Resolved by ${name}`,
  close: 'Close',
  cancel: 'Cancel',
  syncNote:
    'Updates appear when your sync tool delivers teammates’ files (checked every 20 s).',
};
export const classStrings = {
  edit: 'Edit classes',
  title: 'Object classes',
  help: 'Classes for boxes and polygons in this project. Renaming or recoloring is always safe; export IDs never change. A class can be deleted only while no saved label uses it.',
  name: 'Class name',
  color: 'Color',
  add: 'Add class',
  newName: 'New class name',
  save: 'Save',
  delete: 'Delete',
  confirmDelete: (name: string) => `Delete class “${name}”?`,
  close: 'Done',
  semanticTitle: 'Semantic classes',
  semanticHelp:
    'Pixel classes of this project; the number is the value written to exported masks. Renaming or recoloring is always safe; IDs never change. A class can be deleted only while no saved mask contains it. 255 = Ignore is fixed.',
  semanticLayer: 'Semantic',
  semanticSet: 'Semantic classes',
  cityscapes: 'Cityscapes (19 train IDs)',
  custom: 'Custom list',
  customHelp:
    'One class per line; IDs follow the order (first line = 0). Optional color: “Road #804080”. 255 = Ignore is added automatically. Use the same list in every annotator’s project so IDs match.',
};
export const prelabel = {
  folder: 'Import mask folder',
  apply: 'Import masks',
  folderHelp:
    'Pre-labels for many images at once: a folder of class-ID PNGs named after each image (same file name or media ID), same size. Layer = the selector on the left.',
  template: 'Ignore template',
  templateHelp:
    'Force Ignore (255) on a fixed area, e.g. a bus hood: a PNG where white = ignore, applied to every unapproved image of the same size.',
  confirmTemplate: (v: {
    width: number;
    height: number;
    ignorePixels: number;
    images: number;
    approvedSkipped: number;
  }) =>
    `Set ${v.ignorePixels.toLocaleString()} px to Ignore on ${v.images} images of ${v.width}×${v.height}?` +
    (v.approvedSkipped
      ? ` (${v.approvedSkipped} approved images are left unchanged.)`
      : '') +
    ' Each image can be undone only by editing it again.',
  matched: (n: number, approved: number) =>
    `${n} masks match images.` +
    (approved ? ` ${approved} approved images are skipped.` : ''),
  mode: 'Merge with existing labels',
  fillUnlabeled: 'Fill only unlabeled pixels (keep human labels)',
  replace: 'Replace the layer',
  warnings: (n: number) => `${n} files skipped`,
};
export const samStrings = {
  section: 'Click-to-mask (SAM)',
  choose: 'Choose SAM model…',
  change: 'Change model',
  start: 'Start SAM',
  stop: 'Stop',
  apply: 'Apply mask',
  applyUnlabeled: 'Apply to unlabeled only',
  clear: 'Clear',
  states: {
    unconfigured: 'Not set up. Needs Python + a SAM ONNX export (see README).',
    stopped: 'Stopped',
    starting: 'Loading model…',
    ready: 'Ready',
    encoding: 'Reading image…',
    thinking: 'Segmenting…',
  },
  help: 'S: SAM tool. Click = include, right-click or Alt+click = exclude, drag = box. Enter = write mask with the chosen class · Shift+Enter = only unlabeled pixels · Esc = clear.',
};
export const workflow = {
  done: 'Done → next',
  doneHelp: 'Ctrl+Enter: mark “needs review” and open the next image',
  approve: 'Approve → next',
  approveHelp: 'Ctrl+Shift+Enter: mark “approved” and open the next image',
  shortcuts: 'Keyboard shortcuts',
  shortcutHint: 'Press ? for shortcuts',
  list: [
    ['V / B / P', 'Select · Bounding box · Polygon'],
    ['M / E / H', 'Brush · Eraser · Pan'],
    ['S', 'SAM click-to-mask (right-click = exclude, drag = box)'],
    ['Enter / Shift+Enter', 'SAM: apply mask / apply to unlabeled pixels only'],
    ['1 – 9', 'Choose class'],
    ['C', 'Search classes'],
    ['[ / ]', 'Brush radius'],
    ['Enter / Backspace / Esc', 'Close polygon · remove vertex · cancel'],
    ['Delete', 'Delete selected annotation'],
    ['← / →', 'Previous / next image'],
    ['Ctrl+Enter', 'Mark needs review → next image'],
    ['Ctrl+Shift+Enter', 'Approve → next image'],
    ['Ctrl+Z / Ctrl+Shift+Z', 'Undo / redo'],
    ['Space-drag, wheel', 'Pan, zoom at pointer'],
    ['0 / =', 'Fit / 100%'],
    ['L / Tab', 'Toggle overlays / panels'],
    ['Ctrl+E', 'Export dataset'],
  ] as const,
};
