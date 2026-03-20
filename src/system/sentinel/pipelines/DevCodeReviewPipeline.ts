/**
 * DevCodeReviewPipeline — AI team reviews a branch/PR collaboratively
 *
 * This is the most collaborative template — multiple AIs analyze code,
 * post findings to chat, and the team discusses/votes on issues.
 *
 * Flow:
 *   0: Shell — Get diff (branch vs base)
 *   1: Shell — Get file list and stats
 *   2: LLM — Architecture review (high-level: patterns, coupling, design)
 *   3: LLM — Security review (OWASP, injection, auth issues)
 *   4: LLM — Quality review (bugs, edge cases, error handling)
 *   5: Emit — Post consolidated review to chat
 *   6: Watch — Wait for team discussion and author response
 *   7: LLM — Synthesize verdict from all reviews
 *   8: Emit — Post final verdict
 *
 * Usage:
 *   const pipeline = buildDevCodeReviewPipeline({
 *     branch: "feature/user-profiles",
 *     personaId: "...",
 *     personaName: "CodeReview AI",
 *     cwd: "/path/to/project",
 *   });
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';

export interface DevCodeReviewConfig {
  /** Branch to review */
  branch?: string;
  /** Or: specific files to review */
  files?: string[];
  /** Persona executing the review */
  personaId: string;
  /** Persona display name */
  personaName: string;
  /** Project working directory */
  cwd: string;
  /** Git repo path — for future workspace support */
  repoPath?: string;
  /** Base branch for diff */
  baseBranch?: string;
  /** Chat room for collaborative discussion */
  roomId?: string;
  /** Provider for LLM review steps */
  reviewProvider?: string;
  /** Seconds to wait for team discussion */
  discussionTimeoutSecs?: number;
  /** Skip collaborative checkpoints */
  autonomous?: boolean;
}

export function buildDevCodeReviewPipeline(config: DevCodeReviewConfig): Pipeline {
  const {
    branch,
    files,
    personaId,
    personaName,
    cwd,
    baseBranch = 'main',
    roomId = 'general',
    reviewProvider = 'deepseek',
    discussionTimeoutSecs = 180,
    autonomous = false,
  } = config;

  const runId = `review-${personaId.slice(0, 8)}-${Date.now().toString(36)}`;
  const target = branch || (files ? files.join(', ') : 'HEAD');

  const diffCommand = branch
    ? `git diff ${baseBranch}...${branch} 2>/dev/null || git diff ${baseBranch}..${branch}`
    : files
      ? `git diff HEAD -- ${files.join(' ')}`
      : `git diff ${baseBranch}...HEAD`;

  const fileListCommand = branch
    ? `git diff --stat ${baseBranch}...${branch} 2>/dev/null || git diff --stat ${baseBranch}..${branch}`
    : `git diff --stat ${baseBranch}...HEAD`;

  const steps: PipelineStep[] = [
    // Step 0: Get the diff
    {
      type: 'shell',
      cmd: `cd "${cwd}" && ${diffCommand}`,
      timeoutSecs: 30,
    },

    // Step 1: File stats
    {
      type: 'shell',
      cmd: `cd "${cwd}" && ${fileListCommand}`,
      timeoutSecs: 10,
    },

    // Step 2-4: Three parallel review perspectives
    {
      type: 'parallel',
      branches: [
        // Architecture review
        [{
          type: 'llm',
          prompt: [
            '=== CODE DIFF ===',
            '{{steps.0.output}}',
            '',
            '=== FILE SUMMARY ===',
            '{{steps.1.output}}',
            '',
            'Review this diff for ARCHITECTURE concerns:',
            '- Does it follow existing patterns and conventions?',
            '- Does it introduce coupling between modules that should be independent?',
            '- Are abstractions at the right level?',
            '- Is there code duplication that should be extracted?',
            '- Are there naming issues?',
            '',
            'Format: List issues as [ARCH-1], [ARCH-2], etc. with severity (critical/warning/note).',
            'If the code is clean, say so explicitly.',
          ].join('\n'),
          provider: reviewProvider,
          temperature: 0.2,
          maxTokens: 3000,
          systemPrompt: 'You are a senior architect reviewing code changes. Be specific and constructive.',
        }],

        // Security review
        [{
          type: 'llm',
          prompt: [
            '=== CODE DIFF ===',
            '{{steps.0.output}}',
            '',
            'Review this diff for SECURITY concerns:',
            '- Command injection, SQL injection, XSS',
            '- Authentication/authorization gaps',
            '- Secrets or credentials in code',
            '- Unsafe deserialization',
            '- Missing input validation at system boundaries',
            '- Insecure defaults',
            '',
            'Format: List issues as [SEC-1], [SEC-2], etc. with severity.',
            'If no security issues found, say "No security concerns identified."',
          ].join('\n'),
          provider: reviewProvider,
          temperature: 0.1,
          maxTokens: 2000,
          systemPrompt: 'You are a security engineer reviewing code for vulnerabilities. Focus on OWASP top 10.',
        }],

        // Quality review
        [{
          type: 'llm',
          prompt: [
            '=== CODE DIFF ===',
            '{{steps.0.output}}',
            '',
            'Review this diff for CODE QUALITY:',
            '- Potential bugs or logic errors',
            '- Missing error handling',
            '- Edge cases not covered',
            '- Race conditions or concurrency issues',
            '- Resource leaks',
            '- Test coverage gaps',
            '',
            'Format: List issues as [BUG-1], [BUG-2], etc. with severity.',
            'If the code quality is good, acknowledge what was done well.',
          ].join('\n'),
          provider: reviewProvider,
          temperature: 0.2,
          maxTokens: 3000,
          systemPrompt: 'You are a QA engineer reviewing code for bugs and quality issues. Be thorough.',
        }],
      ],
      failFast: false,
    },

    // Step 3: Post reviews to chat for team discussion
    ...(autonomous ? [] : [
      {
        type: 'command',
        command: 'collaboration/chat/send',
        params: {
          room: roomId,
          message: [
            `**${personaName}** reviewed: ${target}`,
            '',
            '---',
            '**Architecture Review:**',
            '{{steps.2.output.0}}',
            '',
            '**Security Review:**',
            '{{steps.2.output.1}}',
            '',
            '**Quality Review:**',
            '{{steps.2.output.2}}',
            '---',
            '',
            `Team: discuss findings. Voting in ${discussionTimeoutSecs}s.`,
          ].join('\n'),
        },
      } as PipelineStep,

      // Wait for discussion
      {
        type: 'watch',
        event: `dev:${runId}:discussion-done`,
        timeoutSecs: discussionTimeoutSecs,
      } as PipelineStep,
    ]),

    // Step 4: Synthesize final verdict
    {
      type: 'llm',
      prompt: [
        '=== ARCHITECTURE REVIEW ===',
        '{{steps.2.output.0}}',
        '',
        '=== SECURITY REVIEW ===',
        '{{steps.2.output.1}}',
        '',
        '=== QUALITY REVIEW ===',
        '{{steps.2.output.2}}',
        '',
        'Synthesize these three reviews into a FINAL VERDICT:',
        '1. Overall assessment: APPROVE / REQUEST CHANGES / NEEDS DISCUSSION',
        '2. Critical issues that MUST be fixed (if any)',
        '3. Suggestions (nice to have)',
        '4. What was done well',
      ].join('\n'),
      provider: reviewProvider,
      temperature: 0.2,
      maxTokens: 2000,
      systemPrompt: 'Synthesize multiple code reviews into a single clear verdict. Be fair and constructive.',
    },

    // Step 5: Post verdict
    {
      type: 'emit',
      event: `dev:${runId}:verdict`,
      payload: {
        runId,
        target,
        personaId,
        personaName,
        verdict: '{{steps[-1].output}}',
      },
    },

    ...(autonomous ? [] : [
      {
        type: 'command',
        command: 'collaboration/chat/send',
        params: {
          room: roomId,
          message: [
            `**${personaName}** — Final Verdict for ${target}:`,
            '',
            '{{steps[-2].output}}',
          ].join('\n'),
        },
      } as PipelineStep,
    ]),
  ];

  return {
    name: `${personaName}: review ${target}`,
    steps,
    workingDir: cwd,
    timeoutSecs: 1200,
    inputs: {
      runId,
      target,
      personaId,
      personaName,
      roomId,
      baseBranch,
    },
  };
}
