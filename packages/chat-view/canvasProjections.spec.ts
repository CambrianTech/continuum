import { describe, expect, it } from 'vitest';
import { canvasContentBody, type CanvasViewState } from './canvasProjections';

const view = (over: Partial<CanvasViewState>): CanvasViewState => ({
  artifact_title: 'index.html — pricing card',
  artifact_html: '<main><h1>Pricing</h1></main>',
  persona: 'Asha',
  observed_at_ms: 1_700_000_000_000,
  viewport: { width: 1440, height: 900 },
  revision: 3,
  ...over,
});

describe('canvasContentBody', () => {
  // what this catches: the wire→VM fold's honesty rules — no feed = the
  // awaiting frame with feedLive false, a source-less observation folds to
  // NO artifact (a title alone is not a page), an ungraded page carries no
  // score, and a graded one sorts its FAILING checks first with the gate
  // counts derived from the checks themselves — never a fabricated zero.
  it('renders the honest awaiting frame before the feed delivers', () => {
    const body = canvasContentBody(undefined);
    expect(body.feedLive).toBe(false);
    expect(body.artifact).toBeUndefined();
    expect(body.score).toBeUndefined();
  });

  it('folds an observation into a live artifact with its facts', () => {
    const body = canvasContentBody(view({}));
    expect(body.feedLive).toBe(true);
    expect(body.artifact).toEqual({
      title: 'index.html — pricing card',
      html: '<main><h1>Pricing</h1></main>',
    });
    expect(body.persona).toBe('Asha');
    expect(body.observedAtMs).toBe(1_700_000_000_000);
    expect(body.viewport).toEqual({ width: 1440, height: 900 });
    expect(body.revision).toBe(3);
    expect(body.score).toBeUndefined(); // ungraded = unmeasured, honest
  });

  it('a source-less observation is the awaiting frame, not a fake page', () => {
    const body = canvasContentBody(view({ artifact_html: undefined }));
    expect(body.feedLive).toBe(true);
    expect(body.artifact).toBeUndefined();
  });

  it('derives the scorecard from the checks, failures first', () => {
    const body = canvasContentBody(
      view({
        checks: [
          { name: 'structure: h1 present', tier: 'v1', passed: true },
          { name: 'contrast: hero text ≥ 4.5:1', tier: 'v2', passed: false, detail: '3.1:1 measured' },
          { name: 'responsive: no overflow at 360w', tier: 'v2', passed: true },
        ],
        judge: 0.72,
      }),
    );
    const score = body.score!;
    expect(score.passed).toBe(2);
    expect(score.total).toBe(3);
    expect(score.checks[0]).toMatchObject({ passed: false, detail: '3.1:1 measured' });
    expect(score.judge).toBe(0.72);
  });
});
