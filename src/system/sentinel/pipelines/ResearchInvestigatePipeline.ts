/**
 * ResearchInvestigatePipeline — Gather-and-synthesize research workflow
 *
 * Third workflow pattern validation. Parallel investigation, deduplication,
 * synthesis into a coherent document.
 *
 * Pattern:
 *   0: LLM — Break research question into investigation aspects
 *   1: Parallel — Investigate each aspect independently
 *   2: LLM — Deduplicate findings, resolve contradictions
 *   3: LLM — Synthesize into structured document
 *   4: Emit — Post for review (collaborative mode)
 *   5: Watch — Wait for feedback (collaborative mode)
 *   6: Shell — Save final document + git commit
 *   7: Emit — Completion
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface ResearchInvestigateConfig {
  /** Research question or topic */
  question: string;
  /** Output file path */
  outputPath: string;
  /** Project directory */
  cwd: string;
  /** Persona running the research */
  personaId: string;
  /** Persona display name */
  personaName: string;
  /** Number of parallel investigation branches (default: 3) */
  branches?: number;
  /** Provider for LLM steps */
  provider?: string;
  /** Chat room for collaborative review */
  roomId?: string;
  /** Seconds to wait for review feedback */
  reviewTimeoutSecs?: number;
  /** Skip collaborative checkpoints */
  autonomous?: boolean;
  /** Specific aspects to investigate (auto-generated if omitted) */
  aspects?: string[];
}

export function buildResearchInvestigatePipeline(config: ResearchInvestigateConfig): Pipeline {
  const {
    question,
    outputPath,
    cwd,
    personaId,
    personaName,
    branches = 3,
    provider = 'deepseek',
    roomId = 'general',
    reviewTimeoutSecs = 120,
    autonomous = false,
    aspects,
  } = config;

  const runId = `research-${Date.now().toString(36)}`;

  const steps: PipelineStep[] = [
    // ──────────────────────────────────────────────────
    // Step 0: Decompose the research question
    // ──────────────────────────────────────────────────
    {
      type: 'llm',
      prompt: [
        '=== RESEARCH QUESTION ===',
        question,
        '',
        aspects
          ? `Investigate these specific aspects:\n${aspects.map((a, i) => `${i + 1}. ${a}`).join('\n')}`
          : `Break this research question into ${branches} distinct investigation aspects.`,
        '',
        'For each aspect, provide:',
        '1. A clear sub-question to investigate',
        '2. What kind of evidence or information to look for',
        '3. Potential sources or approaches',
        '',
        'Output a numbered list. Each aspect should be independent — no overlap.',
      ].join('\n'),
      provider,
      temperature: 0.4,
      maxTokens: 1500,
      systemPrompt: 'You are a research methodology expert. You decompose complex questions into orthogonal investigation angles that together cover the full space.',
    },

    // ──────────────────────────────────────────────────
    // Step 1: Parallel investigation
    // ──────────────────────────────────────────────────
    {
      type: 'parallel',
      branches: Array.from({ length: branches }, (_, i) => [{
        type: 'llm',
        prompt: [
          `=== INVESTIGATION BRANCH ${i + 1} ===`,
          '',
          '=== ORIGINAL QUESTION ===',
          question,
          '',
          '=== YOUR ASSIGNED ASPECT ===',
          `{{steps.0.output}}`,
          '',
          `Focus on aspect ${i + 1} from the decomposition above.`,
          '',
          'Investigate thoroughly:',
          '- Present findings with supporting evidence',
          '- Note areas of uncertainty or disagreement',
          '- Cite sources where possible',
          '- Flag any surprising or counterintuitive findings',
          '',
          'Be specific and factual. Avoid generalities.',
        ].join('\n'),
        provider,
        temperature: 0.5,
        maxTokens: 3000,
        systemPrompt: [
          'You are a thorough research investigator.',
          'You present findings clearly, distinguish fact from inference,',
          'and flag uncertainty honestly. Depth over breadth.',
        ].join(' '),
      } as PipelineStep]),
      failFast: false,
    },

    // ──────────────────────────────────────────────────
    // Step 2: Deduplicate and resolve contradictions
    // ──────────────────────────────────────────────────
    {
      type: 'llm',
      prompt: [
        '=== DEDUPLICATE AND RECONCILE ===',
        '',
        'You received parallel investigation results on:',
        question,
        '',
        ...Array.from({ length: branches }, (_, i) => [
          `=== BRANCH ${i + 1} FINDINGS ===`,
          `{{steps.1.output.${i}}}`,
          '',
        ]).flat(),
        'Tasks:',
        '1. Remove duplicate findings (keep the more detailed version)',
        '2. Identify and resolve contradictions between branches',
        '3. Flag unresolved contradictions that need further investigation',
        '4. Note gaps — important aspects that no branch covered',
        '',
        'Output the deduplicated, reconciled findings organized by theme.',
      ].join('\n'),
      provider,
      temperature: 0.3,
      maxTokens: 3000,
      systemPrompt: 'You are a research synthesizer. You reconcile findings from multiple investigators, resolve contradictions, and produce a coherent unified view.',
    },

    // ──────────────────────────────────────────────────
    // Step 3: Synthesize into structured document
    // ──────────────────────────────────────────────────
    {
      type: 'llm',
      prompt: [
        '=== SYNTHESIZE FINAL DOCUMENT ===',
        '',
        `Research question: ${question}`,
        '',
        '=== RECONCILED FINDINGS ===',
        '{{steps.2.output}}',
        '',
        'Write a structured research document in markdown:',
        '1. **Executive Summary** (2-3 paragraphs)',
        '2. **Key Findings** (organized by theme, with evidence)',
        '3. **Analysis** (implications, connections, patterns)',
        '4. **Open Questions** (what we still don\'t know)',
        '5. **Recommendations** (actionable next steps)',
        '',
        'Write for a knowledgeable audience. Be precise, cite evidence, distinguish certainty levels.',
      ].join('\n'),
      provider,
      temperature: 0.4,
      maxTokens: 4096,
      systemPrompt: 'You are a senior research analyst producing a publication-quality document. Clear structure, rigorous analysis, honest about limitations.',
    },

    // ──────────────────────────────────────────────────
    // Steps 4-5: Collaborative review (skip in autonomous mode)
    // ──────────────────────────────────────────────────
    ...(autonomous ? [] : [
      {
        type: 'command',
        command: 'collaboration/chat/send',
        params: {
          room: roomId,
          message: [
            `**${personaName}** — Research complete: "${question}"`,
            '',
            '{{steps.3.output}}',
            '',
            `Reply with feedback or corrections. Auto-saving in ${reviewTimeoutSecs}s.`,
          ].join('\n'),
        },
      } as PipelineStep,
      {
        type: 'watch',
        event: `research:${runId}:reviewed`,
        timeoutSecs: reviewTimeoutSecs,
      } as PipelineStep,
    ]),

    // ──────────────────────────────────────────────────
    // Step 6: Save document + git commit
    // ──────────────────────────────────────────────────
    {
      type: 'shell',
      cmd: [
        `cd "${cwd}"`,
        `mkdir -p "$(dirname "${outputPath}")"`,
        // Write synthesized document to file via env var (safe from shell metacharacters)
        `echo "$RESEARCH_CONTENT" > "${outputPath}"`,
        `git add "${outputPath}"`,
        `git diff --cached --quiet && echo "Nothing to commit" || git commit -m "$(cat <<'COMMITMSG'`,
        `research: ${question.length > 80 ? question.slice(0, 80) + '...' : question}`,
        '',
        `Investigated by ${personaName} via sentinel pipeline.`,
        'COMMITMSG',
        `)"`,
      ].join('\n'),
      workingDir: cwd,
      timeoutSecs: 30,
      allowFailure: true,
      env: {
        RESEARCH_CONTENT: '{{steps.3.output}}',
      },
    },

    // ──────────────────────────────────────────────────
    // Step 7: Completion
    // ──────────────────────────────────────────────────
    {
      type: 'emit',
      event: `research:${runId}:complete`,
      payload: {
        runId,
        question,
        outputPath,
        personaId,
        personaName,
      },
    },
  ];

  return {
    name: `${personaName}: research — ${question.length > 60 ? question.slice(0, 60) + '...' : question}`,
    steps,
    workingDir: cwd,
    timeoutSecs: 1800,
    inputs: {
      runId,
      question,
      outputPath,
      personaId,
      personaName,
      roomId,
    },
  };
}
