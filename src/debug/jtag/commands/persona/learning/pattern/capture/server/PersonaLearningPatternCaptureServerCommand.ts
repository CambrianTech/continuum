/**
 * Persona Learning Pattern Capture Command - Server Implementation
 *
 * Capture a successful pattern for cross-AI learning. When an AI discovers a working solution, they share it with the team.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { PersonaLearningPatternCaptureParams, PersonaLearningPatternCaptureResult } from '../shared/PersonaLearningPatternCaptureTypes';
import { createPersonaLearningPatternCaptureResultFromParams } from '../shared/PersonaLearningPatternCaptureTypes';
import { FeedbackEntity, FeedbackStatus, FeedbackType, FeedbackDomain } from '@system/data/entities/FeedbackEntity';
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';

// Map string inputs to FeedbackType enum
function mapToFeedbackType(type: string): FeedbackType {
  const typeMap: Record<string, FeedbackType> = {
    'debugging': FeedbackType.DEBUGGING,
    'tool-use': FeedbackType.TOOL_USE,
    'optimization': FeedbackType.OPTIMIZATION,
    'architecture': FeedbackType.EXPERTISE,       // Map to EXPERTISE
    'communication': FeedbackType.COLLABORATION,  // Map to COLLABORATION
    'response-strategy': FeedbackType.RESPONSE_STRATEGY,
    'error-handling': FeedbackType.ERROR_HANDLING,
    'collaboration': FeedbackType.COLLABORATION,
    'expertise': FeedbackType.EXPERTISE,
    'other': FeedbackType.EXPERTISE               // Default for 'other'
  };
  return typeMap[type.toLowerCase()] || FeedbackType.EXPERTISE;
}

// Map string inputs to FeedbackDomain enum
function mapToFeedbackDomain(domain: string): FeedbackDomain {
  const domainMap: Record<string, FeedbackDomain> = {
    'chat': FeedbackDomain.CHAT,
    'code': FeedbackDomain.CODE,
    'tools': FeedbackDomain.TOOLS,
    'web': FeedbackDomain.GENERAL,                // Map 'web' to GENERAL
    'general': FeedbackDomain.GENERAL,
    'coordination': FeedbackDomain.COORDINATION
  };
  return domainMap[domain.toLowerCase()] || FeedbackDomain.GENERAL;
}

export class PersonaLearningPatternCaptureServerCommand extends CommandBase<PersonaLearningPatternCaptureParams, PersonaLearningPatternCaptureResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/pattern/capture', context, subpath, commander);
  }

  async execute(params: PersonaLearningPatternCaptureParams): Promise<PersonaLearningPatternCaptureResult> {
    console.log('🧬 PATTERN CAPTURE: Capturing pattern for cross-AI learning');
    console.log(`   Name: ${params.name}`);
    console.log(`   Type: ${params.type}, Domain: ${params.domain}`);

    // Validate required parameters
    if (!params.name?.trim()) {
      throw new ValidationError('name', 'Pattern name is required');
    }
    if (!params.type?.trim()) {
      throw new ValidationError('type', 'Pattern type is required (debugging, tool-use, optimization, architecture, communication, other)');
    }
    if (!params.domain?.trim()) {
      throw new ValidationError('domain', 'Pattern domain is required (chat, code, tools, web, general)');
    }
    if (!params.problem?.trim()) {
      throw new ValidationError('problem', 'Problem description is required');
    }
    if (!params.solution?.trim()) {
      throw new ValidationError('solution', 'Solution description is required');
    }

    // Determine source persona (from params or context)
    const sourcePersonaId = (params as { sourcePersonaId?: UUID }).sourcePersonaId ||
                           (params as { userId?: UUID }).userId;
    if (!sourcePersonaId) {
      throw new ValidationError('sourcePersonaId', 'Could not determine source persona. Provide sourcePersonaId or ensure userId is set.');
    }

    // Map string params to proper enum types
    const feedbackType = mapToFeedbackType(params.type);
    const feedbackDomain = mapToFeedbackDomain(params.domain);

    // Convert string[] applicableWhen to proper object structure
    const applicableWhen = params.applicableWhen
      ? { keywords: params.applicableWhen }
      : undefined;

    // Convert string[] examples to proper object structure
    const examples = params.examples
      ? params.examples.map(ex => ({ input: ex, output: '', explanation: 'Example usage' }))
      : undefined;

    // Create the FeedbackEntity using the factory method
    const entity = FeedbackEntity.createPattern({
      sourcePersonaId,
      name: params.name,
      description: params.description || params.solution,
      type: feedbackType,
      domain: feedbackDomain,
      problem: params.problem,
      solution: params.solution,
      tags: params.tags,
      applicableWhen,
      examples
    });

    // Make public if requested
    if (params.makePublic) {
      entity.isPublic = true;
      entity.status = FeedbackStatus.ACTIVE;
    }

    // Store in database
    const storeResult = await Commands.execute<DataCreateParams, DataCreateResult<FeedbackEntity>>('data/create', {
      collection: FeedbackEntity.collection,
      data: entity,
      context: params.context,
      sessionId: params.sessionId,
      backend: 'server'
    });

    if (!storeResult.success) {
      throw new Error(`Failed to store pattern: ${storeResult.error || 'Unknown error'}`);
    }

    // Emit event for pattern capture
    Events.emit('persona:learning:pattern:captured', {
      patternId: entity.id,
      name: entity.name,
      type: entity.type,
      domain: entity.domain,
      sourcePersonaId,
      isPublic: entity.isPublic
    });

    console.log(`✅ Pattern captured: ${entity.name} (${entity.id})`);

    return createPersonaLearningPatternCaptureResultFromParams(params, {
      success: true,
      patternId: entity.id,
      name: entity.name,
      status: entity.status,
      confidence: entity.confidence,
      message: entity.isPublic
        ? 'Pattern is now visible to all AIs. They can use it immediately.'
        : 'Pattern is pending validation. Use it successfully 3+ times to auto-validate.'
    });
  }
}
