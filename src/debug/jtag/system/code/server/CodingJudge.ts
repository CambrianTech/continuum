/**
 * CodingJudge - AI evaluation of coding challenge attempts
 *
 * Uses a reasoning-class model to evaluate challenge solutions against rubric criteria.
 * Returns a score (0-100) and detailed feedback.
 *
 * Evaluation considers:
 * - Correctness: Does the code do what was asked?
 * - Completeness: Were all requirements met?
 * - Code quality: Is the code clean and idiomatic?
 * - Efficiency: Were resources (tool calls, time) used well?
 */

import { Logger } from '../../core/logging/Logger';
import { AIProviderDaemon } from '../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import type { CodingChallengeEntity } from '../../data/entities/CodingChallengeEntity';
import type { CodingResult } from '../shared/CodingTypes';

const log = Logger.create('CodingJudge', 'code');

export interface JudgeEvaluation {
  /** Score from 0 to 100 */
  score: number;
  /** Whether the challenge is considered passed (score >= 70) */
  passed: boolean;
  /** Detailed feedback */
  feedback: string;
  /** Per-criterion scores */
  criteriaScores: Array<{ criterion: string; score: number; comment: string }>;
  /** Strengths identified */
  strengths: string[];
  /** Weaknesses identified */
  weaknesses: string[];
}

/** Minimum score to pass a challenge */
const PASS_THRESHOLD = 70;

export class CodingJudge {

  /**
   * Evaluate a coding challenge attempt.
   *
   * Sends the challenge spec, result files, and execution metrics to a
   * reasoning model that scores the attempt against the rubric.
   */
  async evaluate(
    challenge: CodingChallengeEntity,
    resultFiles: Record<string, string>,
    executionResult: CodingResult,
  ): Promise<JudgeEvaluation> {
    log.info(`Judging challenge "${challenge.name}" — ${Object.keys(resultFiles).length} result files`);

    const prompt = this.buildJudgePrompt(challenge, resultFiles, executionResult);

    try {
      const response = await AIProviderDaemon.generateText({
        messages: [{ role: 'user', content: prompt }],
        systemPrompt: JUDGE_SYSTEM_PROMPT,
        preferredProvider: 'anthropic',
        model: 'claude-sonnet-4-5-20250514',
        temperature: 0.2,
        maxTokens: 2000,
      });

      return this.parseJudgeResponse(response.text, challenge);

    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      log.error(`Judge evaluation failed: ${message}`);

      // Fallback: simple heuristic scoring when LLM unavailable
      return this.heuristicScore(challenge, resultFiles, executionResult);
    }
  }

  /**
   * Build the evaluation prompt for the judge model.
   */
  private buildJudgePrompt(
    challenge: CodingChallengeEntity,
    resultFiles: Record<string, string>,
    executionResult: CodingResult,
  ): string {
    const setupFilesStr = Object.entries(challenge.setupFiles)
      .map(([path, content]) => `### ${path} (BEFORE)\n\`\`\`\n${content}\n\`\`\``)
      .join('\n\n');

    const resultFilesStr = Object.entries(resultFiles)
      .map(([path, content]) => `### ${path} (AFTER)\n\`\`\`\n${content}\n\`\`\``)
      .join('\n\n');

    const expectedFilesStr = challenge.expectedFiles
      ? Object.entries(challenge.expectedFiles)
          .map(([path, content]) => `### ${path} (EXPECTED)\n\`\`\`\n${content}\n\`\`\``)
          .join('\n\n')
      : 'No expected files provided — evaluate based on description and criteria.';

    const criteriaList = challenge.evaluationCriteria
      .map((c, i) => `${i + 1}. ${c}`)
      .join('\n');

    return `## Challenge: ${challenge.name}
**Difficulty**: ${challenge.difficulty}
**Category**: ${challenge.category}

## Task Description
${challenge.description}

## Expected Outcome
${challenge.expectedOutcome}

## Evaluation Criteria
${criteriaList}

## Setup Files (Initial State)
${setupFilesStr}

## Result Files (After Execution)
${resultFilesStr}

## Expected Files (Reference Solution)
${expectedFilesStr}

## Execution Metrics
- Status: ${executionResult.status}
- Steps completed: ${executionResult.stepResults.filter(s => s.status === 'completed').length}/${executionResult.stepResults.length}
- Tool calls used: ${executionResult.totalToolCalls}
- Duration: ${executionResult.totalDurationMs}ms
- Files modified: ${executionResult.filesModified.join(', ') || 'none'}
- Files created: ${executionResult.filesCreated.join(', ') || 'none'}
- Errors: ${executionResult.errors.join('; ') || 'none'}

## Instructions
Evaluate this coding challenge attempt. Score each criterion from 0-100, then provide an overall score. Respond with valid JSON matching this schema:

\`\`\`json
{
  "score": <number 0-100>,
  "feedback": "<overall assessment>",
  "criteriaScores": [
    { "criterion": "<criterion text>", "score": <0-100>, "comment": "<specific feedback>" }
  ],
  "strengths": ["<strength 1>", ...],
  "weaknesses": ["<weakness 1>", ...]
}
\`\`\``;
  }

  /**
   * Parse the LLM judge response into a JudgeEvaluation.
   */
  private parseJudgeResponse(text: string, challenge: CodingChallengeEntity): JudgeEvaluation {
    try {
      // Extract JSON from response (may be wrapped in markdown code block)
      const jsonMatch = text.match(/\{[\s\S]*\}/);
      if (!jsonMatch) {
        throw new Error('No JSON found in judge response');
      }

      const parsed = JSON.parse(jsonMatch[0]);
      const score = Math.max(0, Math.min(100, Math.round(parsed.score ?? 0)));

      return {
        score,
        passed: score >= PASS_THRESHOLD,
        feedback: parsed.feedback ?? 'No feedback provided',
        criteriaScores: Array.isArray(parsed.criteriaScores) ? parsed.criteriaScores : [],
        strengths: Array.isArray(parsed.strengths) ? parsed.strengths : [],
        weaknesses: Array.isArray(parsed.weaknesses) ? parsed.weaknesses : [],
      };
    } catch (error) {
      log.warn(`Failed to parse judge response: ${error instanceof Error ? error.message : String(error)}`);
      return {
        score: 0,
        passed: false,
        feedback: `Judge response parsing failed: ${text.slice(0, 200)}`,
        criteriaScores: [],
        strengths: [],
        weaknesses: [],
      };
    }
  }

  /**
   * Simple heuristic scoring when LLM judge is unavailable.
   * Based on execution success, file presence, and basic content checks.
   */
  private heuristicScore(
    challenge: CodingChallengeEntity,
    resultFiles: Record<string, string>,
    executionResult: CodingResult,
  ): JudgeEvaluation {
    let score = 0;
    const strengths: string[] = [];
    const weaknesses: string[] = [];

    // Base score from execution status
    if (executionResult.status === 'completed') {
      score += 30;
      strengths.push('All plan steps completed');
    } else if (executionResult.status === 'partial') {
      score += 15;
      weaknesses.push('Only partial execution completed');
    } else {
      weaknesses.push(`Execution ${executionResult.status}`);
    }

    // File presence check (30 points)
    if (challenge.expectedFiles) {
      const expectedPaths = Object.keys(challenge.expectedFiles);
      const foundPaths = expectedPaths.filter(p => resultFiles[p] !== undefined);
      const fileScore = expectedPaths.length > 0
        ? Math.round((foundPaths.length / expectedPaths.length) * 30)
        : 0;
      score += fileScore;
      if (foundPaths.length === expectedPaths.length) {
        strengths.push('All expected files present');
      } else {
        weaknesses.push(`Missing ${expectedPaths.length - foundPaths.length} expected files`);
      }
    } else {
      // No expected files — award points if any files were created/modified
      if (executionResult.filesCreated.length > 0 || executionResult.filesModified.length > 0) {
        score += 20;
        strengths.push('Files were created/modified');
      }
    }

    // Content match check (30 points)
    if (challenge.expectedFiles) {
      let contentMatches = 0;
      let totalChecks = 0;
      for (const [filePath, expectedContent] of Object.entries(challenge.expectedFiles)) {
        if (resultFiles[filePath]) {
          totalChecks++;
          const actual = resultFiles[filePath].trim();
          const expected = expectedContent.trim();
          if (actual === expected) {
            contentMatches++;
          } else if (actual.includes(expected.split('\n')[0])) {
            contentMatches += 0.5;
          }
        }
      }
      if (totalChecks > 0) {
        score += Math.round((contentMatches / totalChecks) * 30);
      }
    }

    // Efficiency bonus (10 points)
    const toolEfficiency = challenge.toolCallLimit > 0
      ? 1 - (executionResult.totalToolCalls / challenge.toolCallLimit)
      : 0;
    if (toolEfficiency > 0.5) {
      score += 10;
      strengths.push('Efficient tool call usage');
    } else if (toolEfficiency > 0.2) {
      score += 5;
    }

    score = Math.min(100, Math.max(0, score));

    return {
      score,
      passed: score >= PASS_THRESHOLD,
      feedback: `Heuristic evaluation (LLM judge unavailable): score=${score}`,
      criteriaScores: challenge.evaluationCriteria.map(c => ({
        criterion: c,
        score: score,
        comment: 'Heuristic scoring — LLM judge unavailable',
      })),
      strengths,
      weaknesses,
    };
  }
}

const JUDGE_SYSTEM_PROMPT = `You are a coding challenge evaluator. You assess AI-generated code solutions against specific criteria.

Be strict but fair:
- Score 90-100: Excellent — meets all criteria, clean code, efficient
- Score 70-89: Good — meets most criteria, minor issues
- Score 50-69: Partial — some criteria met, significant gaps
- Score 30-49: Poor — major issues, few criteria met
- Score 0-29: Failed — solution doesn't address the task

Always respond with valid JSON matching the requested schema. Be specific in feedback.`;
