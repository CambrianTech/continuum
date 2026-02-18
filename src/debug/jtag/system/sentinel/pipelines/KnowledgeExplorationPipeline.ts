/**
 * KnowledgeExplorationPipeline — Builds a sentinel pipeline that explores
 * data sources and produces SourceKnowledge.
 *
 * Each DataSourceConfig type maps to different pipeline step sequences:
 *
 * - git-repo: Shell steps to find files, read git log, read key files,
 *   then LLM to extract facts from the raw content.
 *
 * - web-research: Command steps for interface/web/search and interface/web/fetch,
 *   then LLM to extract facts from fetched content.
 *
 * - conversation-log / document-set: Shell steps to read files,
 *   then LLM to extract facts.
 *
 * - pure-generation: No exploration. Returns minimal SourceKnowledge
 *   with no facts (teacher generates freely).
 *
 * The pipeline builder concatenates source-specific steps, then appends
 * a final LLM step that synthesizes all collected content into a
 * structured SourceKnowledge JSON output.
 *
 * Outlier validation: git-repo (local/filesystem) and web-research (remote/API)
 * are the two outliers proving the source-agnostic interface.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type {
  DataSourceConfig,
  GitRepoSourceConfig,
  WebResearchSourceConfig,
  ConversationLogSourceConfig,
  DocumentSetSourceConfig,
} from '../../genome/shared/KnowledgeTypes';

// ============================================================================
// Pipeline Config
// ============================================================================

export interface KnowledgeExplorationConfig {
  /** Data sources to explore */
  dataSources: DataSourceConfig[];

  /** Maximum total facts to extract (default: 50) */
  maxFacts?: number;

  /** LLM model for fact extraction */
  model?: string;

  /** LLM provider for fact extraction */
  provider?: string;
}

// ============================================================================
// Pipeline Builder
// ============================================================================

/**
 * Build a sentinel pipeline that explores data sources and produces
 * SourceKnowledge as its final output.
 *
 * The pipeline's final step is always an LLM that synthesizes all
 * collected raw content into structured ExtractedFact[] output.
 * The Rust engine stores each step's output, and the final LLM
 * references earlier steps via {{steps.N.output}} interpolation.
 */
export function buildKnowledgeExplorationPipeline(config: KnowledgeExplorationConfig): Pipeline {
  const { dataSources, maxFacts = 50 } = config;

  const steps: PipelineStep[] = [];
  const sourceDescriptions: string[] = [];

  // Track step indices for each source so the final LLM can reference them
  let stepIndex = 0;

  for (const source of dataSources) {
    const startIndex = stepIndex;

    switch (source.type) {
      case 'git-repo':
        stepIndex = appendGitRepoSteps(steps, source, stepIndex);
        sourceDescriptions.push(`Git repo at ${source.repoPath} (steps ${startIndex}-${stepIndex - 1})`);
        break;

      case 'web-research':
        stepIndex = appendWebResearchSteps(steps, source, stepIndex);
        sourceDescriptions.push(`Web research: ${source.searchQueries.length} queries (steps ${startIndex}-${stepIndex - 1})`);
        break;

      case 'conversation-log':
        stepIndex = appendConversationLogSteps(steps, source, stepIndex);
        sourceDescriptions.push(`Conversation logs: ${source.paths.length} files (steps ${startIndex}-${stepIndex - 1})`);
        break;

      case 'document-set':
        stepIndex = appendDocumentSetSteps(steps, source, stepIndex);
        sourceDescriptions.push(`Documents: ${source.paths.length} paths (steps ${startIndex}-${stepIndex - 1})`);
        break;

      case 'pure-generation':
        // No exploration steps needed
        sourceDescriptions.push('Pure generation (no source exploration)');
        break;
    }
  }

  // Final LLM step: synthesize all collected content into SourceKnowledge
  steps.push(buildFactExtractionStep(stepIndex, sourceDescriptions, maxFacts, config.model, config.provider));

  return {
    name: 'knowledge-exploration',
    steps,
    inputs: {
      dataSources: dataSources.map(ds => ds.type),
      maxFacts,
    },
  };
}

// ============================================================================
// Source-Specific Step Builders
// ============================================================================

/**
 * Git repo exploration: find files, read git log, read key files.
 * Returns the next available step index.
 */
function appendGitRepoSteps(
  steps: PipelineStep[],
  source: GitRepoSourceConfig,
  startIndex: number,
): number {
  const maxFiles = source.maxFiles ?? 15;
  const gitLogDepth = source.gitLogDepth ?? 30;
  const globs = source.fileGlobs ?? ['*.ts', '*.md', '*.rs', '*.py'];

  // Build the find command with file globs
  const findArgs = globs.map(g => `-name "${g}"`).join(' -o ');

  // Step N: Find matching files in the repo (must use sh -c for pipe)
  steps.push({
    type: 'shell',
    cmd: 'sh',
    args: ['-c', `find ${source.repoPath} -type f \\( ${findArgs} \\) -not -path "*/node_modules/*" -not -path "*/.git/*" -not -path "*/dist/*" | head -${maxFiles}`],
    timeoutSecs: 30,
    workingDir: source.repoPath,
  });

  // Step N+1: Git log for recent history
  steps.push({
    type: 'shell',
    cmd: 'git',
    args: ['-C', source.repoPath, 'log', `--oneline`, `-${gitLogDepth}`, '--no-decorate'],
    timeoutSecs: 15,
  });

  // Step N+2: Read the key files found in step N.
  // Use xargs + cat to read files from the find output.
  // The Rust shell step captures stdout, so the LLM gets the file contents.
  steps.push({
    type: 'shell',
    cmd: 'sh',
    args: ['-c', `find ${source.repoPath} -type f \\( ${findArgs} \\) -not -path "*/node_modules/*" -not -path "*/.git/*" -not -path "*/dist/*" | head -${maxFiles} | while read f; do echo "=== FILE: $f ==="; head -100 "$f"; echo; done`],
    timeoutSecs: 60,
    workingDir: source.repoPath,
  });

  return startIndex + 3;
}

/**
 * Web research exploration: search queries + fetch top results.
 * Returns the next available step index.
 */
function appendWebResearchSteps(
  steps: PipelineStep[],
  source: WebResearchSourceConfig,
  startIndex: number,
): number {
  const maxPagesPerQuery = source.maxPagesPerQuery ?? 3;
  let idx = startIndex;

  // For each search query: one search command + one fetch command for top result
  for (const query of source.searchQueries) {
    // Search step
    steps.push({
      type: 'command',
      command: 'interface/web/search',
      params: {
        query,
        maxResults: maxPagesPerQuery,
        ...(source.domains && { domains: source.domains }),
      },
    });
    const searchStepIdx = idx++;

    // Fetch the top result from this search
    // Uses interpolation to get the first result URL
    steps.push({
      type: 'command',
      command: 'interface/web/fetch',
      params: {
        url: `{{steps.${searchStepIdx}.data.results.0.url}}`,
        format: 'text',
        maxLength: 30000,
      },
    });
    idx++;
  }

  return idx;
}

/**
 * Conversation log exploration: read files via shell.
 * Returns the next available step index.
 */
function appendConversationLogSteps(
  steps: PipelineStep[],
  source: ConversationLogSourceConfig,
  startIndex: number,
): number {
  // Read all conversation files in one shell step
  const catCmd = source.paths.map(p => `echo "=== FILE: ${p} ==="; head -500 "${p}"; echo`).join('; ');

  steps.push({
    type: 'shell',
    cmd: 'sh',
    args: ['-c', catCmd],
    timeoutSecs: 30,
  });

  return startIndex + 1;
}

/**
 * Document set exploration: read files/directories via shell.
 * Returns the next available step index.
 */
function appendDocumentSetSteps(
  steps: PipelineStep[],
  source: DocumentSetSourceConfig,
  startIndex: number,
): number {
  // For each path: if it's a directory, read files in it; if file, read it
  const readCmd = source.paths.map(p =>
    `if [ -d "${p}" ]; then find "${p}" -type f -name "*.md" -o -name "*.txt" -o -name "*.ts" | head -20 | while read f; do echo "=== FILE: $f ==="; head -200 "$f"; echo; done; else echo "=== FILE: ${p} ==="; head -500 "${p}"; echo; fi`
  ).join('; ');

  steps.push({
    type: 'shell',
    cmd: 'sh',
    args: ['-c', readCmd],
    timeoutSecs: 60,
  });

  return startIndex + 1;
}

// ============================================================================
// Fact Extraction — Final LLM Step
// ============================================================================

/**
 * Build the final LLM step that extracts structured facts from all
 * preceding step outputs. This is the heart of the knowledge exploration:
 * raw content goes in, ExtractedFact[] comes out.
 */
function buildFactExtractionStep(
  totalPrecedingSteps: number,
  sourceDescriptions: string[],
  maxFacts: number,
  model?: string,
  provider?: string,
): PipelineStep {
  // Build references to all preceding step outputs
  const stepRefs = [];
  for (let i = 0; i < totalPrecedingSteps; i++) {
    stepRefs.push(`--- Output from step ${i} ---\n{{steps.${i}.output}}`);
  }

  const prompt = [
    'You are a knowledge extraction specialist. Analyze the following source material',
    'and extract verified facts as structured JSON.',
    '',
    `Sources explored: ${sourceDescriptions.join('; ')}`,
    '',
    'SOURCE MATERIAL:',
    '',
    ...stepRefs,
    '',
    '---',
    '',
    `Extract up to ${maxFacts} distinct, verifiable facts from the source material above.`,
    'Each fact should be a clear, specific statement that could be tested with a question.',
    '',
    'Output ONLY a JSON object with this structure (no markdown, no code fences):',
    '{',
    '  "summary": "A 2-3 sentence summary of what was learned from the sources",',
    '  "facts": [',
    '    {',
    '      "statement": "A specific, verifiable fact (e.g., The CEO of Acme is Jane Smith)",',
    '      "confidence": 0.95,',
    '      "source": {',
    '        "sourceType": "git-repo|web-research|conversation-log|document-set",',
    '        "location": "file path or URL where this fact was found",',
    '        "excerpt": "The relevant quote from the source"',
    '      },',
    '      "category": "A category like architecture, api, history, people, config"',
    '    }',
    '  ]',
    '}',
  ].join('\n');

  return {
    type: 'llm',
    prompt,
    ...(model && { model }),
    ...(provider && { provider }),
    temperature: 0.3,
    maxTokens: 8192,
  };
}
