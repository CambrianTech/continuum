/**
 * The SETTINGS activity's neutral `Content` body — the node's operator panel,
 * first face: the genome-commons covenant (the ToS the operator agrees to on
 * ANY surface — this face and the `genome/sharing` terminal verb flip the SAME
 * consent receipt), the HF publishing identity (account status only — the
 * token never rides any wire), and the local gene registry.
 *
 * Shapes only: consumer-neutral, DOM-free. The body mirrors the core's
 * `genome/sharing` + `genome/list` results; the renderer is a pure function of
 * it, and every mutation goes back through those same verbs — one truth,
 * N surfaces.
 */

/** The `Content` purpose key the settings face dispatches on. */
export const SETTINGS_PURPOSE = 'settings';

/** One registered gene's registry row (mirrors `genome/list`). */
export interface SettingsGeneVM {
  readonly gene: string;
  readonly baseModel: string;
  /** Distance-routable: a minted signature is stamped. */
  readonly signed: boolean;
  /** Eval-receipt trials (0 = unmeasured). */
  readonly trials: number;
  /** Age-decayed mean lift, when measured. */
  readonly decayedLift?: number;
}

/** The settings face's body. `loaded: false` renders the awaiting frame
 *  (the face opened, the fetch is in flight); `error` renders the honest
 *  failure WITH the terminal fallback command — never a silent blank. */
export interface SettingsContentBody {
  readonly loaded: boolean;
  readonly error?: string;
  /** Covenant consent: agreed iff the recorded receipt matches the CURRENT
   *  covenant version (stale version = terms changed = re-agree). */
  readonly agreed: boolean;
  readonly covenantVersion: string;
  /** The recorded consent receipt (`<version>@<unix-ms>`), when one exists. */
  readonly receipt?: string;
  /** The covenant text VERBATIM — the one text every surface renders. */
  readonly covenant: string;
  /** HF account the `hf` CLI publishes as; absent = not authenticated. */
  readonly hfAccount?: string;
  /** The local gene registry (mirrors `genome/list`). */
  readonly genes: readonly SettingsGeneVM[];
}
