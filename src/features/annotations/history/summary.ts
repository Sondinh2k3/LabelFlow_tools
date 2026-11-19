import type { EditDocument } from './session';

/** Only fields that affect library filters/status badges, independent of revision. */
export const librarySummary = (document: EditDocument) =>
  JSON.stringify([
    document.mediaId,
    document.reviewStatus,
    document.eventLabel,
    document.splitGroup,
    !!(document.annotations.length || document.masks.length),
  ]);
