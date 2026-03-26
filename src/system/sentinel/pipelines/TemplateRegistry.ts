/**
 * TemplateRegistry — Name-based lookup for sentinel pipeline templates.
 *
 * Templates are TypeScript builder functions that return Pipeline JSON.
 * They are registered here by name for discovery and invocation.
 *
 * Usage:
 *   const builder = TemplateRegistry.get('dev/build-feature');
 *   const pipeline = builder({ feature: "Add user profiles", ... });
 *   await Commands.execute('sentinel/run', { type: 'pipeline', definition: pipeline });
 *
 * Listing:
 *   const templates = TemplateRegistry.list();
 *   // [{ name: 'dev/build-feature', description: '...', category: 'dev' }, ...]
 */

import type { Pipeline } from '../../../workers/continuum-core/bindings/modules/sentinel';
import { buildDevBuildFeaturePipeline, type DevBuildFeatureConfig } from './DevBuildFeaturePipeline';
import { buildDevFixBugPipeline, type DevFixBugConfig } from './DevFixBugPipeline';
import { buildDevCodeReviewPipeline, type DevCodeReviewConfig } from './DevCodeReviewPipeline';
import { buildDevIntegratePipeline, type DevIntegrateConfig } from './DevIntegratePipeline';
import { buildCreativeWriteChapterPipeline, type CreativeWriteChapterConfig } from './CreativeWriteChapterPipeline';
import { buildResearchInvestigatePipeline, type ResearchInvestigateConfig } from './ResearchInvestigatePipeline';
import { buildPublishPipeline, type PublishConfig } from './PublishPipeline';
import { buildSafeDeployPipeline, type SafeDeployConfig } from './SafeDeployPipeline';
import { buildSemanticTranslatorPipeline, buildSemanticTranslatorListenerPipeline, type SemanticTranslatorConfig } from './SemanticTranslatorPipeline';

// -- Template metadata --------------------------------------------------------

export interface TemplateInfo {
  /** Template name (e.g., 'dev/build-feature') */
  name: string;
  /** Short description */
  description: string;
  /** Category for grouping */
  category: string;
  /** Required config fields */
  requiredFields: string[];
  /** Optional config fields with defaults */
  optionalFields: string[];
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type BuilderFn = (config: any) => Pipeline;

interface TemplateEntry {
  info: TemplateInfo;
  builder: BuilderFn;
}

// -- Registry -----------------------------------------------------------------

const _templates = new Map<string, TemplateEntry>();

function register(info: TemplateInfo, builder: BuilderFn): void {
  _templates.set(info.name, { info, builder });
}

// -- Register built-in templates ----------------------------------------------

register(
  {
    name: 'dev/build-feature',
    description: 'End-to-end feature development: plan → review → implement → build → test → commit',
    category: 'dev',
    requiredFields: ['feature', 'personaId', 'personaName', 'cwd'],
    optionalFields: [
      'repoPath', 'roomId', 'branchName', 'baseBranch', 'planProvider', 'codingProvider',
      'codingModel', 'maxBudgetUsd', 'maxTurns', 'buildCommand', 'testCommand',
      'planReviewTimeoutSecs', 'qaReviewTimeoutSecs', 'autonomous', 'captureTraining',
    ],
  },
  buildDevBuildFeaturePipeline as BuilderFn,
);

register(
  {
    name: 'dev/fix-bug',
    description: 'Diagnose root cause → team review diagnosis → implement fix → verify → commit',
    category: 'dev',
    requiredFields: ['bug', 'personaId', 'personaName', 'cwd'],
    optionalFields: [
      'repoPath', 'roomId', 'codingProvider', 'codingModel', 'maxBudgetUsd', 'maxTurns',
      'buildCommand', 'testCommand', 'diagnosisReviewTimeoutSecs', 'autonomous', 'captureTraining',
    ],
  },
  buildDevFixBugPipeline as BuilderFn,
);

register(
  {
    name: 'dev/code-review',
    description: 'Parallel architecture + security + quality review → team discussion → verdict',
    category: 'dev',
    requiredFields: ['personaId', 'personaName', 'cwd'],
    optionalFields: [
      'repoPath', 'branch', 'files', 'baseBranch', 'roomId', 'reviewProvider',
      'discussionTimeoutSecs', 'autonomous',
    ],
  },
  buildDevCodeReviewPipeline as BuilderFn,
);

register(
  {
    name: 'dev/integrate',
    description: 'Merge persona branches into feature branch: sequential merge → conflict resolution → build → test',
    category: 'dev',
    requiredFields: ['featureBranch', 'personaId', 'personaName', 'cwd'],
    optionalFields: [
      'repoPath', 'branches', 'baseBranch', 'roomId', 'codingProvider', 'codingModel',
      'maxBudgetUsd', 'buildCommand', 'testCommand', 'autonomous',
    ],
  },
  buildDevIntegratePipeline as BuilderFn,
);

register(
  {
    name: 'creative/write-chapter',
    description: 'Primary-author-with-support: outline → draft → parallel advisory reviews → revise → commit',
    category: 'creative',
    requiredFields: ['chapter', 'chapterTitle', 'personaId', 'personaName', 'cwd'],
    optionalFields: [
      'storyBiblePath', 'previousChaptersGlob', 'genre', 'targetWordCount',
      'roomId', 'advisorProvider', 'writingProvider', 'writingModel',
      'maxTurns', 'maxBudgetUsd', 'feedbackTimeoutSecs', 'autonomous', 'characters',
    ],
  },
  buildCreativeWriteChapterPipeline as BuilderFn,
);

register(
  {
    name: 'research/investigate',
    description: 'Gather-and-synthesize: decompose question → parallel investigation → deduplicate → synthesize → publish',
    category: 'research',
    requiredFields: ['question', 'outputPath', 'personaId', 'personaName', 'cwd'],
    optionalFields: [
      'branches', 'provider', 'roomId', 'reviewTimeoutSecs', 'autonomous', 'aspects',
    ],
  },
  buildResearchInvestigatePipeline as BuilderFn,
);

register(
  {
    name: 'dev/publish',
    description: 'Push branch → create PR → wait for CI → approve → merge (with human gates)',
    category: 'dev',
    requiredFields: ['branch', 'title', 'personaId', 'personaName', 'cwd'],
    optionalFields: [
      'baseBranch', 'body', 'remote', 'mergeStrategy', 'ciTimeoutSecs',
      'ciPollIntervalSecs', 'autoApprovePr', 'autoApproveMerge',
      'approvalTimeoutSecs', 'roomId', 'deploy', 'deployHealthTimeout',
    ],
  },
  buildPublishPipeline as BuilderFn,
);

register(
  {
    name: 'deploy/safe-deploy',
    description: 'Safe deployment: compile → deploy → health check → auto-revert on failure',
    category: 'deploy',
    requiredFields: ['cwd', 'personaId', 'personaName'],
    optionalFields: [
      'healthTimeoutSecs', 'healthPollIntervalSecs', 'requireAiHealthy', 'roomId',
    ],
  },
  buildSafeDeployPipeline as BuilderFn,
);

register(
  {
    name: 'ontology/translate',
    description: 'Translate content between two AI model semantic spaces using the shared ontology',
    category: 'ontology',
    requiredFields: ['content', 'sourceProviderId', 'sourceModelId', 'targetProviderId', 'targetModelId'],
    optionalFields: ['requestId', 'maxAnchors', 'minSimilarity', 'announceResult'],
  },
  buildSemanticTranslatorPipeline as BuilderFn,
);

register(
  {
    name: 'ontology/translate-listener',
    description: 'Long-running sentinel that listens for translate:request events and processes them continuously',
    category: 'ontology',
    requiredFields: ['sourceProviderId', 'sourceModelId', 'targetProviderId', 'targetModelId'],
    optionalFields: ['maxTranslations'],
  },
  buildSemanticTranslatorListenerPipeline as BuilderFn,
);

// -- Public API ---------------------------------------------------------------

export class TemplateRegistry {
  /**
   * Get a template builder by name.
   * Returns undefined if not found.
   */
  static get(name: string): BuilderFn | undefined {
    return _templates.get(name)?.builder;
  }

  /**
   * Get template info by name.
   */
  static info(name: string): TemplateInfo | undefined {
    return _templates.get(name)?.info;
  }

  /**
   * List all registered templates.
   */
  static list(): TemplateInfo[] {
    return Array.from(_templates.values()).map(entry => entry.info);
  }

  /**
   * List templates filtered by category.
   */
  static listByCategory(category: string): TemplateInfo[] {
    return TemplateRegistry.list().filter(t => t.category === category);
  }

  /**
   * Check if a template exists.
   */
  static has(name: string): boolean {
    return _templates.has(name);
  }

  /**
   * Build a pipeline from a template name and config.
   * Throws if template not found.
   */
  static build(name: string, config: Record<string, unknown>): Pipeline {
    const builder = TemplateRegistry.get(name);
    if (!builder) {
      const available = TemplateRegistry.list().map(t => t.name).join(', ');
      throw new Error(`Template '${name}' not found. Available: ${available}`);
    }
    return builder(config);
  }

  /**
   * Register a custom template at runtime.
   * Personas can extend the registry with their own templates.
   */
  static register(info: TemplateInfo, builder: BuilderFn): void {
    register(info, builder);
  }
}
