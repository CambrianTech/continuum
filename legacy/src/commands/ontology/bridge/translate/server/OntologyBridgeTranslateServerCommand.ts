/**
 * ontology/bridge/translate — Server implementation
 *
 * Delegates to OntologyRegistry.bridge (SemanticBridgeService) which is
 * already wired with embedder and generator adapters at startup.
 */

import { CommandBase, type ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../../system/core/types/JTAGTypes';
import type {
  OntologyBridgeTranslateParams,
  OntologyBridgeTranslateResult,
} from '../shared/OntologyBridgeTranslateTypes';
import { OntologyRegistry } from '../../../../../system/ontology/server/OntologyRegistry';
import type { OntologyDomain } from '../../../../../system/ontology/shared/OntologyTypes';
import { ONTOLOGY_CONSTANTS } from '../../../../../system/ontology/shared/OntologyTypes';
import { Events } from '@system/core/shared/Events';

export class OntologyBridgeTranslateServerCommand extends CommandBase<
  OntologyBridgeTranslateParams,
  OntologyBridgeTranslateResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ontology/bridge/translate', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<OntologyBridgeTranslateResult> {
    const p = params as JTAGPayload & OntologyBridgeTranslateParams;
    const {
      content,
      sourceProviderId,
      sourceModelId,
      targetProviderId,
      targetModelId,
      requestId = `translate-${Date.now()}`,
      maxAnchors = ONTOLOGY_CONSTANTS.DEFAULT_MAX_ANCHORS,
      minSimilarity = ONTOLOGY_CONSTANTS.MIN_USABLE_SIMILARITY,
      domains,
    } = p;

    if (!content) {
      return transformPayload(params, {
        success: false,
        error: 'content is required',
        translatedContent: '',
        originalContent: content ?? '',
        anchorsUsed: [],
        realignmentsNeeded: 0,
        translationConfidence: 0,
        hasDriftWarnings: false,
        requestId,
      });
    }

    for (const field of ['sourceProviderId', 'sourceModelId', 'targetProviderId', 'targetModelId'] as const) {
      if (!p[field]) {
        return transformPayload(params, {
          success: false,
          error: `${field} is required`,
          translatedContent: '',
          originalContent: content,
          anchorsUsed: [],
          realignmentsNeeded: 0,
          translationConfidence: 0,
          hasDriftWarnings: false,
          requestId,
        });
      }
    }

    const sourceKey = `${sourceProviderId}/${sourceModelId}`;
    const targetKey = `${targetProviderId}/${targetModelId}`;

    try {
      const result = await OntologyRegistry.sharedInstance().bridge.translate({
        content,
        sourceModel: { providerId: sourceProviderId, modelId: sourceModelId },
        targetModel: { providerId: targetProviderId, modelId: targetModelId },
        domains: domains as OntologyDomain[] | undefined,
        maxAnchors,
        minSimilarity,
        requestId,
      });

      Events.emit(`${ONTOLOGY_CONSTANTS.EVENTS.TRANSLATE_COMPLETE}:${requestId}`, {
        requestId,
        originalContent: result.originalContent,
        translatedContent: result.translatedContent,
        sourceModelKey: sourceKey,
        targetModelKey: targetKey,
        anchorCount: result.anchorsUsed.length,
        translationConfidence: result.translationConfidence,
        hasDriftWarnings: result.hasDriftWarnings,
        passthrough: result.anchorsUsed.length === 0,
      });

      return transformPayload(params, {
        success: true,
        translatedContent: result.translatedContent,
        originalContent: result.originalContent,
        anchorsUsed: result.anchorsUsed,
        realignmentsNeeded: result.realignmentsNeeded,
        translationConfidence: result.translationConfidence,
        hasDriftWarnings: result.hasDriftWarnings,
        requestId,
      });
    } catch (err) {
      return transformPayload(params, {
        success: false,
        error: err instanceof Error ? err.message : String(err),
        translatedContent: content,
        originalContent: content,
        anchorsUsed: [],
        realignmentsNeeded: 0,
        translationConfidence: 0,
        hasDriftWarnings: false,
        requestId,
      });
    }
  }
}
