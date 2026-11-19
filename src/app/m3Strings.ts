export const m3 = {
  milestone: 'M3 · Local AI assistance',
  title: 'AI assistance',
  optional: 'Optional local Python worker · ONNX Runtime CPU',
  configure: 'Choose Python & model manifest',
  start: 'Start / restart worker',
  stop: 'Stop / unload worker',
  predict: 'Generate suggestion',
  cancel: 'Cancel AI request',
  approve: 'Accept suggestion',
  reject: 'Reject suggestion',
  overlay: 'Show suggestion overlay',
  empty: 'No suggestions for this image.',
  protection:
    'Accept appends boxes and fills only semantic ignore (255) pixels. Existing labels are preserved. Approved images must be reopened for review first.',
  prompt: 'Click-to-segment unavailable: no prompt-capable model is loaded.',
  unavailable:
    'AI is off. Manual labeling remains available without Python or models.',
  stale:
    'Document changed since prediction. Reject this suggestion and generate a new one.',
} as const;
