/**
 * CreativeWriteChapterPipeline — Primary-author-with-support writing workflow
 *
 * The outlier validation for the sentinel pipeline engine. Code workflows use
 * divide-and-merge (modular, mechanical merge, binary validation). Creative
 * writing uses primary-author-with-support (single voice, advisor roles,
 * qualitative review). If both fit the same 10 step types, the engine is proven.
 *
 * Pattern: ONE persona holds the voice. Others advise but never write prose.
 *
 * Pipeline shape:
 *   0: LLM — Load story context + plan chapter outline
 *   1: CodingAgent — Write the chapter draft (it writes files — prose is text)
 *   2: Parallel — Advisory reviews (character, structure, continuity)
 *   3: LLM — Synthesize feedback into revision notes
 *   4: Emit — Post revision notes for author review (collaborative mode)
 *   5: Watch — Wait for author input (collaborative mode)
 *   6: CodingAgent — Revise based on feedback
 *   7: Shell — Git commit the chapter
 *   8: Emit — Completion
 *
 * Key difference from code pipelines:
 * - Merge = creative revision (author reads feedback, rewrites)
 * - Validation = qualitative (LLM judges coherence, voice, arc)
 * - Voice coherence is the primary constraint, not API compatibility
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface CreativeWriteChapterConfig {
  /** Chapter number */
  chapter: number;
  /** Chapter title or topic (can be brief — LLM plans the outline) */
  chapterTitle: string;
  /** Project directory containing the manuscript */
  cwd: string;
  /** Persona acting as primary author */
  personaId: string;
  /** Author persona name */
  personaName: string;
  /** Path to story bible / context file (character list, world rules, plot outline) */
  storyBiblePath?: string;
  /** Path to previous chapter(s) for continuity */
  previousChaptersGlob?: string;
  /** Genre (helps tune prompts) */
  genre?: string;
  /** Target word count for the chapter */
  targetWordCount?: number;
  /** Chat room for collaborative feedback */
  roomId?: string;
  /** Provider for advisory LLM steps */
  advisorProvider?: string;
  /** Provider for CodingAgent (writes the prose) */
  writingProvider?: string;
  /** Model for CodingAgent */
  writingModel?: string;
  /** Max CodingAgent turns per attempt */
  maxTurns?: number;
  /** Max budget per writing attempt */
  maxBudgetUsd?: number;
  /** Seconds to wait for author feedback before auto-proceeding */
  feedbackTimeoutSecs?: number;
  /** Skip collaborative checkpoints — author trusts the AI fully */
  autonomous?: boolean;
  /** Character names for character advocate reviews */
  characters?: string[];
}

/**
 * Build a creative/write-chapter pipeline.
 */
export function buildCreativeWriteChapterPipeline(config: CreativeWriteChapterConfig): Pipeline {
  const {
    chapter,
    chapterTitle,
    cwd,
    personaId,
    personaName,
    storyBiblePath = 'story-bible.md',
    previousChaptersGlob = 'chapters/chapter-*.md',
    genre = 'fiction',
    targetWordCount = 3000,
    roomId = 'general',
    advisorProvider = 'deepseek',
    writingProvider = 'claude-code',
    writingModel = 'sonnet',
    maxTurns = 20,
    maxBudgetUsd = 3.0,
    feedbackTimeoutSecs = 120,
    autonomous = false,
    characters = [],
  } = config;

  const chapterFile = `chapters/chapter-${String(chapter).padStart(2, '0')}.md`;
  const runId = `creative-ch${chapter}-${Date.now().toString(36)}`;

  const steps: PipelineStep[] = [
    // ──────────────────────────────────────────────────
    // Step 0: Plan the chapter outline
    // ──────────────────────────────────────────────────
    {
      type: 'llm',
      prompt: [
        `=== CHAPTER ${chapter}: ${chapterTitle} ===`,
        '',
        `Read the story bible at "${storyBiblePath}" and previous chapters matching "${previousChaptersGlob}".`,
        '',
        'Create a detailed chapter outline including:',
        '1. Opening scene and hook',
        '2. Key scenes (3-5) with purpose (advance plot, develop character, build tension)',
        '3. Character arcs progressed in this chapter',
        '4. Emotional trajectory (how does the reader feel at start vs end?)',
        '5. Closing beat / chapter ending (cliffhanger, resolution, transition)',
        '6. Continuity notes (references to prior chapters that must be consistent)',
        '',
        `Target: ~${targetWordCount} words. Genre: ${genre}.`,
        characters.length > 0
          ? `Key characters in this chapter: ${characters.join(', ')}`
          : '',
      ].join('\n'),
      provider: advisorProvider,
      temperature: 0.7,
      maxTokens: 2048,
      systemPrompt: [
        `You are an experienced ${genre} novelist and story architect.`,
        'You plan chapters with strong narrative structure, emotional pacing, and character depth.',
        'Your outlines are specific enough to guide writing but leave room for discovery.',
      ].join(' '),
    },

    // ──────────────────────────────────────────────────
    // Step 1: Write the chapter draft
    // ──────────────────────────────────────────────────
    {
      type: 'codingagent',
      prompt: [
        `=== WRITE CHAPTER ${chapter}: ${chapterTitle} ===`,
        '',
        '=== CHAPTER OUTLINE (from planning phase) ===',
        '{{steps.0.output}}',
        '',
        `Write the full chapter to "${chapterFile}".`,
        `Target: ~${targetWordCount} words. Genre: ${genre}.`,
        '',
        'Guidelines:',
        `- Read "${storyBiblePath}" for character voices, world rules, and plot outline`,
        `- Read previous chapters (${previousChaptersGlob}) for continuity and voice consistency`,
        '- Show, don\'t tell. Use dialogue, action, and sensory detail',
        '- Maintain consistent voice throughout — this is the author\'s style, not a report',
        '- End the chapter with a compelling beat that makes readers turn the page',
        '- Use markdown formatting: # for chapter title, ## for scene breaks',
        '',
        `Create the file: ${chapterFile}`,
      ].join('\n'),
      provider: writingProvider,
      model: writingModel,
      workingDir: cwd,
      maxTurns,
      maxBudgetUsd,
      permissionMode: 'bypassPermissions',
      personaId,
    },

    // ──────────────────────────────────────────────────
    // Step 2: Parallel advisory reviews
    // ──────────────────────────────────────────────────
    {
      type: 'parallel',
      branches: [
        // Branch A: Structural review
        [{
          type: 'llm',
          prompt: [
            '=== STRUCTURAL REVIEW ===',
            '',
            `Review chapter ${chapter} ("${chapterTitle}") at "${chapterFile}".`,
            '',
            'Evaluate:',
            '1. Does the chapter advance the plot meaningfully?',
            '2. Is the pacing appropriate? Any scenes that drag or rush?',
            '3. Is there sufficient tension/conflict to sustain interest?',
            '4. Does the ending work? Does it create forward momentum?',
            '5. Are scene transitions smooth?',
            '',
            'Give specific, actionable feedback. Quote problematic passages.',
            'Rate overall structure 1-10 and explain why.',
          ].join('\n'),
          provider: advisorProvider,
          temperature: 0.4,
          maxTokens: 1500,
          systemPrompt: 'You are a structural editor. You focus on narrative architecture, pacing, and story mechanics. Be direct and specific.',
        }],

        // Branch B: Line-level prose review
        [{
          type: 'llm',
          prompt: [
            '=== PROSE QUALITY REVIEW ===',
            '',
            `Review chapter ${chapter} at "${chapterFile}" for prose quality.`,
            '',
            'Check for:',
            '1. Show vs tell violations (flagging specific passages)',
            '2. Word repetition (same word/phrase used too close together)',
            '3. Passive voice overuse',
            '4. Dialogue that sounds unnatural or expository',
            '5. Purple prose or overwriting',
            '6. Missed opportunities for sensory detail',
            '',
            'Quote specific passages that need work. Suggest concrete improvements.',
          ].join('\n'),
          provider: advisorProvider,
          temperature: 0.3,
          maxTokens: 1500,
          systemPrompt: 'You are a line editor specializing in prose craft. You catch weak writing and suggest stronger alternatives. Be specific — quote and fix.',
        }],

        // Branch C: Character advocate review (for each named character)
        ...(characters.length > 0 ? [[{
          type: 'llm',
          prompt: [
            '=== CHARACTER CONSISTENCY REVIEW ===',
            '',
            `Review chapter ${chapter} at "${chapterFile}".`,
            `Characters to check: ${characters.join(', ')}`,
            '',
            `Read "${storyBiblePath}" for canonical character descriptions.`,
            '',
            'For each character appearing in this chapter:',
            '1. Does their dialogue match their established voice?',
            '2. Are their actions consistent with their personality/motivation?',
            '3. Do they have agency, or are they just reacting?',
            '4. Is their emotional state consistent with what just happened to them?',
            '5. Would this character actually do/say this?',
            '',
            'Flag specific lines where a character feels "off" and explain why.',
          ].join('\n'),
          provider: advisorProvider,
          temperature: 0.4,
          maxTokens: 1500,
          systemPrompt: `You are a character advocate. You know these characters deeply and protect their authenticity. If a character does something out of character, you catch it.`,
        } as PipelineStep]] : []),

        // Branch D: Continuity check
        [{
          type: 'llm',
          prompt: [
            '=== CONTINUITY CHECK ===',
            '',
            `Review chapter ${chapter} at "${chapterFile}" for continuity with prior chapters.`,
            `Previous chapters: ${previousChaptersGlob}`,
            `Story bible: ${storyBiblePath}`,
            '',
            'Check for:',
            '1. Contradictions with established facts (locations, timelines, character descriptions)',
            '2. Dangling references to events/characters not yet introduced',
            '3. Timeline consistency (day/night, seasons, character ages)',
            '4. Object continuity (items mentioned earlier that should/shouldn\'t be present)',
            '5. Previously established rules or constraints being violated',
            '',
            'Be specific. Quote contradicting passages from different chapters.',
          ].join('\n'),
          provider: advisorProvider,
          temperature: 0.2,
          maxTokens: 1500,
          systemPrompt: 'You are a continuity editor with a photographic memory. You track every detail across the manuscript and catch contradictions. Nothing slips past you.',
        }],
      ],
      failFast: false,
    },

    // ──────────────────────────────────────────────────
    // Step 3: Synthesize feedback into revision notes
    // ──────────────────────────────────────────────────
    {
      type: 'llm',
      prompt: [
        '=== SYNTHESIZE EDITORIAL FEEDBACK ===',
        '',
        'You received parallel reviews of the chapter. Synthesize them into clear revision notes.',
        '',
        '=== STRUCTURAL REVIEW ===',
        '{{steps.2.output.0}}',
        '',
        '=== PROSE QUALITY REVIEW ===',
        '{{steps.2.output.1}}',
        '',
        characters.length > 0 ? [
          '=== CHARACTER REVIEW ===',
          '{{steps.2.output.2}}',
          '',
          '=== CONTINUITY REVIEW ===',
          '{{steps.2.output.3}}',
        ].join('\n') : [
          '=== CONTINUITY REVIEW ===',
          '{{steps.2.output.2}}',
        ].join('\n'),
        '',
        'Create a prioritized revision list:',
        '1. MUST FIX — continuity errors, character inconsistencies (factual problems)',
        '2. SHOULD FIX — structural pacing issues, show-vs-tell violations',
        '3. CONSIDER — stylistic suggestions, optional improvements',
        '',
        'For each item, be specific: what to change, where (quote the passage), and why.',
      ].join('\n'),
      provider: advisorProvider,
      temperature: 0.3,
      maxTokens: 2048,
      systemPrompt: 'You are a developmental editor synthesizing feedback from multiple reviewers. Prioritize ruthlessly. The author\'s time is precious — only flag what truly matters.',
    },

    // ──────────────────────────────────────────────────
    // Steps 4-5: Collaborative feedback (skip in autonomous mode)
    // ──────────────────────────────────────────────────
    ...(autonomous ? [] : [
      // Post revision notes to chat for author input
      {
        type: 'command',
        command: 'collaboration/chat/send',
        params: {
          room: roomId,
          message: [
            `**${personaName}** — Chapter ${chapter} draft complete: "${chapterTitle}"`,
            '',
            '**Editorial Feedback Summary:**',
            '{{steps.3.output}}',
            '',
            `Reply with additional notes or react to approve revision. Auto-proceeding in ${feedbackTimeoutSecs}s.`,
          ].join('\n'),
        },
      } as PipelineStep,

      // Wait for author feedback
      {
        type: 'watch',
        event: `creative:${runId}:feedback`,
        timeoutSecs: feedbackTimeoutSecs,
      } as PipelineStep,
    ]),

    // ──────────────────────────────────────────────────
    // Step 6: Revise based on feedback
    // ──────────────────────────────────────────────────
    {
      type: 'codingagent',
      prompt: [
        `=== REVISE CHAPTER ${chapter}: ${chapterTitle} ===`,
        '',
        '=== EDITORIAL REVISION NOTES ===',
        '{{steps.3.output}}',
        '',
        ...(autonomous ? [] : [
          '=== AUTHOR FEEDBACK ===',
          '{{steps.5.output}}',
          '',
        ]),
        `Revise "${chapterFile}" based on the editorial feedback above.`,
        '',
        'Revision guidelines:',
        '- Fix all MUST FIX items (continuity errors, character inconsistencies)',
        '- Address SHOULD FIX items where they improve the chapter',
        '- Consider suggestions but keep the author\'s voice — don\'t over-polish',
        '- Maintain the chapter\'s emotional arc — don\'t flatten the pacing to fix details',
        '- After revision, the prose should read naturally — no visible seams',
      ].join('\n'),
      provider: writingProvider,
      model: writingModel,
      workingDir: cwd,
      maxTurns: 15,
      maxBudgetUsd: Math.min(maxBudgetUsd, 2.0),
      permissionMode: 'bypassPermissions',
      personaId,
    },

    // ──────────────────────────────────────────────────
    // Step 7: Git commit the chapter
    // ──────────────────────────────────────────────────
    {
      type: 'shell',
      cmd: [
        `cd "${cwd}"`,
        `git add "${chapterFile}"`,
        `git diff --cached --quiet && echo "Nothing to commit" || git commit -m "$(cat <<'COMMITMSG'`,
        `chapter ${chapter}: ${chapterTitle}`,
        '',
        `Written and revised by ${personaName}.`,
        'COMMITMSG',
        `)"`,
      ].join('\n'),
      workingDir: cwd,
      timeoutSecs: 30,
      allowFailure: true,
    },

    // ──────────────────────────────────────────────────
    // Step 8: Completion event
    // ──────────────────────────────────────────────────
    {
      type: 'emit',
      event: `creative:${runId}:complete`,
      payload: {
        runId,
        chapter,
        chapterTitle,
        chapterFile,
        personaId,
        personaName,
      },
    },
  ];

  return {
    name: `${personaName}: write chapter ${chapter} — ${chapterTitle}`,
    steps,
    workingDir: cwd,
    timeoutSecs: 3600,
    inputs: {
      runId,
      chapter,
      chapterTitle,
      chapterFile,
      personaId,
      personaName,
      roomId,
    },
  };
}
