import { useMemo } from 'react';
import type {
  AnnotationDocument,
  Profile,
  ReviewSnapshot,
} from '../../lib/contracts';
import { decodeDocument } from '../annotations/history/session';
import { SceneRenderer } from '../annotations/canvas/renderer';
export type ReviewView = 'groundTruth' | 'prediction' | 'both' | 'difference';
export function useReviewOverlay(
  snapshot: ReviewSnapshot | null,
  profile: Profile,
  view: ReviewView,
  layerId: string,
  opacity: number,
  groundTruth: AnnotationDocument | null,
) {
  const scenes = useMemo(() => {
    const source = snapshot?.groundTruth ?? groundTruth;
    if (!source) return null;
    const gt = decodeDocument(source);
    const predicted = decodeDocument({
      ...source,
      annotations: snapshot?.prediction.annotations ?? [],
      masks: snapshot?.prediction.masks ?? [],
    });
    const difference = decodeDocument({
      ...source,
      annotations: [],
      masks: snapshot?.comparison.differences ?? [],
    });
    const diffProfile: Profile = {
      ...profile,
      classes: [],
      layers: profile.layers.map((l) => ({
        ...l,
        layerType: 'region',
        namespace: null,
        color: '#ff4f8b',
      })),
    };
    return {
      gt,
      predicted,
      difference,
      diffProfile,
      gtRenderer: new SceneRenderer(),
      predRenderer: new SceneRenderer(),
      diffRenderer: new SceneRenderer(),
    };
  }, [snapshot, profile, groundTruth]);
  return useMemo(() => {
    if (!scenes) return undefined;
    const visible = new Set(
      layerId ? [layerId] : profile.layers.map((l) => l.id),
    );
    return (ctx: CanvasRenderingContext2D, zoom: number) => {
      const draw = (
        renderer: SceneRenderer,
        document: typeof scenes.gt,
        p: Profile,
        alpha: number,
      ) =>
        renderer.draw(
          ctx,
          document,
          p,
          zoom,
          visible,
          alpha,
          null,
          null,
          [],
          null,
          0,
          false,
          null,
          null,
        );
      if (view === 'groundTruth' || view === 'both')
        draw(
          scenes.gtRenderer,
          scenes.gt,
          profile,
          view === 'both' ? opacity * 0.6 : opacity,
        );
      if (view === 'prediction' || view === 'both') {
        ctx.save();
        if (view === 'both') ctx.setLineDash([6 / zoom, 4 / zoom]);
        draw(scenes.predRenderer, scenes.predicted, profile, opacity);
        ctx.restore();
      }
      if (view === 'difference')
        draw(
          scenes.diffRenderer,
          scenes.difference,
          scenes.diffProfile,
          opacity,
        );
    };
  }, [scenes, view, layerId, opacity, profile]);
}
