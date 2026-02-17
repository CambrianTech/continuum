/**
 * Genome Phenotype Validate Command - Server Implementation
 *
 * Uses LLM-as-judge to score pre-training vs post-training responses.
 * Determines whether training actually improved the model's capability
 * on a specific topic, and whether the improvement meets the quality gate.
 *
 * This is the "selection pressure" step in the Academy Dojo:
 * only adapters that demonstrate measurable improvement get registered.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type {
  GenomePhenotypeValidateParams,
  GenomePhenotypeValidateResult,
  PhenotypeQuestionResult,
} from '../shared/GenomePhenotypeValidateTypes';
import { createGenomePhenotypeValidateResultFromParams } from '../shared/GenomePhenotypeValidateTypes';
import { Commands } from '@system/core/shared/Commands';
import type { AIGenerateParams, AIGenerateResult } from '../../../ai/generate/shared/AIGenerateTypes';

export class GenomePhenotypeValidateServerCommand extends CommandBase<GenomePhenotypeValidateParams, GenomePhenotypeValidateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/phenotype-validate', context, subpath, commander);
  }

  async execute(params: GenomePhenotypeValidateParams): Promise<GenomePhenotypeValidateResult> {
    const { questions, baselineResponses, adaptedResponses } = params;
    const improvementThreshold = params.improvementThreshold ?? 5;

    console.log(`🧬 PHENOTYPE VALIDATE: ${questions.length} questions, threshold=${improvementThreshold}%`);

    if (!questions || questions.length === 0) {
      throw new ValidationError('questions', 'At least one question is required');
    }
    if (!baselineResponses || baselineResponses.length === 0) {
      throw new ValidationError('baselineResponses', 'Baseline responses are required');
    }
    if (!adaptedResponses || adaptedResponses.length === 0) {
      throw new ValidationError('adaptedResponses', 'Adapted responses are required');
    }

    // Build the judging prompt — score both sets of answers in one LLM call
    const judgingPrompt = this._buildJudgingPrompt(questions, baselineResponses, adaptedResponses);

    const generateParams: Partial<AIGenerateParams> = {
      messages: [
        { role: 'system', content: this._buildSystemPrompt() },
        { role: 'user', content: judgingPrompt },
      ],
      ...(params.model && { model: params.model }),
      ...(params.provider && { provider: params.provider as AIGenerateParams['provider'] }),
      maxTokens: 4096,
      temperature: 0.2, // Very low for consistent scoring
    };

    const generateResult = await Commands.execute<AIGenerateParams, AIGenerateResult>(
      'ai/generate',
      generateParams,
    );

    if (!generateResult.success || !generateResult.text) {
      return createGenomePhenotypeValidateResultFromParams(params, {
        success: false,
        error: generateResult.error ?? 'LLM judge failed — no response',
        baselineScore: 0,
        adaptedScore: 0,
        improvement: 0,
        passedQualityGate: false,
        questionResults: [],
        summary: 'Phenotype validation failed: LLM judge unavailable',
        judgedBy: generateResult.model ?? 'unknown',
      });
    }

    // Parse the judge's scores
    const parsed = this._parseJudgeOutput(generateResult.text, questions, baselineResponses, adaptedResponses);

    if (!parsed) {
      return createGenomePhenotypeValidateResultFromParams(params, {
        success: false,
        error: 'Failed to parse LLM judge output',
        baselineScore: 0,
        adaptedScore: 0,
        improvement: 0,
        passedQualityGate: false,
        questionResults: [],
        summary: 'Phenotype validation failed: could not parse judge scores',
        judgedBy: generateResult.model ?? 'unknown',
      });
    }

    const { questionResults, baselineScore, adaptedScore } = parsed;
    const improvement = adaptedScore - baselineScore;
    const passedQualityGate = improvement >= improvementThreshold;

    const summary = passedQualityGate
      ? `Training improved: ${baselineScore.toFixed(1)} → ${adaptedScore.toFixed(1)} (+${improvement.toFixed(1)}pp). Quality gate PASSED.`
      : `Training insufficient: ${baselineScore.toFixed(1)} → ${adaptedScore.toFixed(1)} (+${improvement.toFixed(1)}pp). Quality gate FAILED (need +${improvementThreshold}pp).`;

    console.log(`   ${passedQualityGate ? '✅' : '❌'} ${summary}`);

    return createGenomePhenotypeValidateResultFromParams(params, {
      success: true,
      baselineScore,
      adaptedScore,
      improvement,
      passedQualityGate,
      questionResults,
      summary,
      judgedBy: generateResult.model ?? 'unknown',
    });
  }

  private _buildSystemPrompt(): string {
    return [
      'You are an AI judge evaluating exam responses.',
      'You will be given questions, expected answers, and TWO sets of student responses:',
      '- "Baseline" responses (BEFORE training)',
      '- "Adapted" responses (AFTER training)',
      '',
      'Score each response 0-100 based on accuracy, completeness, and relevance to the expected answer.',
      'Be consistent and fair — score both sets by the same standard.',
      'Output ONLY a JSON object — no markdown, no code fences.',
    ].join('\n');
  }

  private _buildJudgingPrompt(
    questions: Array<{ question: string; expectedAnswer: string }>,
    baselineResponses: Array<{ questionIndex: number; studentAnswer: string }>,
    adaptedResponses: Array<{ questionIndex: number; studentAnswer: string }>,
  ): string {
    const qaPairs = questions.map((q, i) => {
      const baseline = baselineResponses.find(r => r.questionIndex === i);
      const adapted = adaptedResponses.find(r => r.questionIndex === i);
      return [
        `Question ${i + 1}: ${q.question}`,
        `Expected answer: ${q.expectedAnswer}`,
        `Baseline answer: ${baseline?.studentAnswer ?? '(no answer)'}`,
        `Adapted answer: ${adapted?.studentAnswer ?? '(no answer)'}`,
      ].join('\n');
    });

    return [
      'Score each question for both baseline and adapted responses:',
      '',
      ...qaPairs,
      '',
      'Output a JSON object:',
      '{',
      '  "scores": [',
      '    { "questionIndex": 0, "baselineScore": <0-100>, "adaptedScore": <0-100> }',
      '  ]',
      '}',
    ].join('\n');
  }

  private _parseJudgeOutput(
    text: string,
    questions: Array<{ question: string; expectedAnswer: string }>,
    baselineResponses: Array<{ questionIndex: number; studentAnswer: string }>,
    adaptedResponses: Array<{ questionIndex: number; studentAnswer: string }>,
  ): { questionResults: PhenotypeQuestionResult[]; baselineScore: number; adaptedScore: number } | null {
    try {
      const cleaned = text.trim();
      const jsonMatch = cleaned.match(/\{[\s\S]*\}/);
      if (!jsonMatch) return null;

      const parsed = JSON.parse(jsonMatch[0]);
      if (!parsed.scores || !Array.isArray(parsed.scores)) return null;

      const questionResults: PhenotypeQuestionResult[] = [];
      let totalBaseline = 0;
      let totalAdapted = 0;

      for (let i = 0; i < questions.length; i++) {
        const score = parsed.scores.find((s: any) => s.questionIndex === i) ?? parsed.scores[i];
        const baseline = baselineResponses.find(r => r.questionIndex === i);
        const adapted = adaptedResponses.find(r => r.questionIndex === i);

        const bScore = Number(score?.baselineScore ?? 0);
        const aScore = Number(score?.adaptedScore ?? 0);

        questionResults.push({
          question: questions[i].question,
          expectedAnswer: questions[i].expectedAnswer,
          baselineAnswer: baseline?.studentAnswer ?? '',
          adaptedAnswer: adapted?.studentAnswer ?? '',
          baselineScore: bScore,
          adaptedScore: aScore,
        });

        totalBaseline += bScore;
        totalAdapted += aScore;
      }

      const count = questions.length || 1;
      return {
        questionResults,
        baselineScore: totalBaseline / count,
        adaptedScore: totalAdapted / count,
      };
    } catch (err) {
      console.warn(`   PHENOTYPE VALIDATE: Failed to parse judge output: ${err}`);
      return null;
    }
  }
}
