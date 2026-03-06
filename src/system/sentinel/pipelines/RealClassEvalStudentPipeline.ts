/**
 * RealClassEvalStudentPipeline — Sentinel pipeline for the student side of
 * RealClassEval benchmark-based training.
 *
 * The student:
 * 1. Watches for curriculum:ready from the teacher
 * 2. In a loop per challenge (initial exam):
 *    a. Watches challenge:ready (receives skeleton + tests)
 *    b. Uses LLM (with LoRA if loaded) to implement the class
 *    c. Emits challenge:attempted with implementation
 *    d. Watches verdict from teacher
 *    e. If verdict includes datasetPath (remediation) → trains LoRA
 * 3. Post-loop: watches session:complete, trains on full remediation dataset
 * 4. If training happened: emits reexam:ready, re-attempts ALL challenges
 *    with the newly trained LoRA, watches reexam:complete for comparison
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { RealClassEvalStudentPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the RealClassEval student sentinel pipeline.
 *
 * Step flow:
 *   0: Watch — curriculum:ready
 *   1: Loop (initial exam):
 *     loop.0:  Watch — challenge:ready
 *     loop.1:  LLM — Implement the class
 *     loop.2:  Emit — challenge:attempted
 *     loop.3:  Watch — verdict:ready
 *     loop.4:  Condition — remediation needed?
 *       Then: [genome/train, Emit training:complete]
 *       Else: []
 *   2: Watch — session:complete (teacher finished, has remediation JSONL)
 *   3: Condition — training data available? (datasetPath truthy)
 *     Then: [
 *       genome/train on full remediation dataset,
 *       Emit reexam:ready,
 *       Re-exam loop (same challenges, LoRA now active),
 *       Watch reexam:complete (teacher comparison)
 *     ]
 *     Else: []
 */
export function buildRealClassEvalStudentPipeline(config: RealClassEvalStudentPipelineConfig): Pipeline {
  const {
    sessionId,
    personaId,
    personaName,
    baseModel,
    datasetDir,
    config: academyConfig,
  } = config;

  const evt = (action: string) => academyEvent(sessionId, action as any);
  // Per-iteration event name — matches teacher's iterEvt pattern
  const iterEvt = (action: string) => `${academyEvent(sessionId, action as any)}:{{input.iteration}}`;
  const reexamIterEvt = (action: string) => `${academyEvent(sessionId, `reexam:${action}` as any)}:{{input.iteration}}`;

  // LLM config — only pass model/provider when explicitly set to avoid
  // "Model Not Exist" errors with cloud providers.
  const llmConfig = {
    ...(academyConfig.studentModel ? { model: academyConfig.studentModel } : !academyConfig.studentProvider ? { model: baseModel } : {}),
    ...(academyConfig.studentProvider && { provider: academyConfig.studentProvider }),
  };

  const steps: PipelineStep[] = [
    // Step 0: Wait for teacher to publish curriculum
    {
      type: 'watch',
      event: evt('curriculum:ready'),
      timeoutSecs: 600,
    },

    // Step 1: Initial exam — challenge loop
    {
      type: 'loop',
      count: academyConfig.questionsPerExam,
      steps: buildInitialChallengeSteps(sessionId, personaId, personaName, baseModel, academyConfig, iterEvt, evt, llmConfig),
    },

    // Step 2: Watch for session:complete (teacher finished grading all challenges)
    {
      type: 'watch',
      event: evt('session:complete'),
      timeoutSecs: 60,
    },

    // Step 3: Condition — training data available? Train + re-exam if so.
    {
      type: 'condition',
      if: '{{steps.2.data.payload.datasetPath}}',
      then: [
        // Then sub-step 0: Train on the full remediation dataset
        {
          type: 'command',
          command: 'genome/train',
          params: {
            personaId,
            personaName,
            traitType: `realclasseval-${sessionId.slice(0, 8)}`,
            baseModel,
            datasetPath: '{{steps.2.data.payload.datasetPath}}',
            rank: academyConfig.rank,
            epochs: academyConfig.epochs,
            learningRate: academyConfig.learningRate,
            batchSize: academyConfig.batchSize,
          },
        },

        // Then sub-step 1: Signal teacher that training is done, ready for re-exam
        {
          type: 'emit',
          event: evt('reexam:ready'),
          payload: {
            sessionId,
            personaId,
            trained: true,
          },
        },

        // Then sub-step 2: Re-exam loop — same challenges, LoRA now hot-loaded
        {
          type: 'loop',
          count: academyConfig.questionsPerExam,
          steps: buildReexamChallengeSteps(sessionId, personaId, reexamIterEvt, llmConfig),
        },

        // Then sub-step 3: Watch for teacher's re-exam comparison results
        {
          type: 'watch',
          event: evt('reexam:complete'),
          timeoutSecs: 120,
        },
      ],
      else: [],
    },
  ];

  return {
    name: `realclasseval-student-${personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    steps,
    inputs: {
      sessionId,
      personaId,
      personaName,
      baseModel,
      datasetDir,
    },
  };
}

/**
 * Build initial exam challenge loop steps.
 *
 * Each iteration:
 *   0: Watch challenge:ready
 *   1: LLM implement
 *   2: Emit challenge:attempted
 *   3: Watch verdict:ready
 *   4: Condition — train on remediation if provided
 */
function buildInitialChallengeSteps(
  sessionId: string,
  personaId: string,
  personaName: string,
  baseModel: string,
  academyConfig: RealClassEvalStudentPipelineConfig['config'],
  iterEvt: (action: string) => string,
  evt: (action: string) => string,
  llmConfig: Record<string, string>,
): PipelineStep[] {
  return [
    // loop.0: Watch for challenge:ready (iteration-scoped)
    {
      type: 'watch',
      event: iterEvt('challenge:ready'),
      timeoutSecs: 300,
    },

    // loop.1: LLM — Implement the Python class
    {
      type: 'llm',
      prompt: [
        'You are an expert Python developer. Implement the following class based on the skeleton and tests.',
        '',
        '=== CLASS SKELETON ===',
        '{{loop.0.data.payload.skeleton}}',
        '',
        '=== TEST CODE ===',
        '{{loop.0.data.payload.testCode}}',
        '',
        'INSTRUCTIONS:',
        '- Output ONLY the complete Python class implementation',
        '- Do NOT include markdown code fences or explanations',
        '- The implementation must pass all the tests',
        '- Tests marked @pytest.mark.xfail(strict=True) EXPECT the code to RAISE an exception.',
        '  If such a test passes without error, pytest counts it as XPASS (failure).',
        '  Make sure your implementation raises appropriate errors for invalid inputs.',
        '- Implement ALL methods from the skeleton',
        '- Keep the same class name and method signatures',
        '- Import any needed standard library modules at the top',
      ].join('\n'),
      ...llmConfig,
      temperature: 0.2,
      maxTokens: 4096,
    },

    // loop.2: Emit challenge:attempted (iteration-scoped)
    {
      type: 'emit',
      event: iterEvt('challenge:attempted'),
      payload: {
        sessionId,
        personaId,
        implementation: '{{loop.1.output}}',
        challengeIndex: '{{loop.0.data.payload.challengeIndex}}',
        round: '{{input.iteration}}',
      },
    },

    // loop.3: Watch for teacher's verdict (iteration-scoped)
    {
      type: 'watch',
      event: iterEvt('verdict:ready'),
      timeoutSecs: 600,
    },

    // loop.4: Condition — did we get remediation data? (datasetPath truthy)
    {
      type: 'condition',
      if: '{{loop.3.data.payload.datasetPath}}',
      then: [
        // Train on remediation data
        {
          type: 'command',
          command: 'genome/train',
          params: {
            personaId,
            personaName,
            traitType: `realclasseval-${sessionId.slice(0, 8)}-round-{{input.iteration}}`,
            baseModel,
            datasetPath: '{{loop.3.data.payload.datasetPath}}',
            rank: academyConfig.rank,
            epochs: academyConfig.epochs,
            learningRate: academyConfig.learningRate,
            batchSize: academyConfig.batchSize,
          },
        },
        // Emit training:complete
        {
          type: 'emit',
          event: evt('training:complete'),
          payload: {
            sessionId,
            personaId,
            layerId: '{{loop.4.data.layerId}}',
            metrics: {
              finalLoss: '{{loop.4.data.metrics.finalLoss}}',
              trainingTime: '{{loop.4.data.metrics.trainingTime}}',
              examplesProcessed: '{{loop.4.data.metrics.examplesProcessed}}',
              epochs: '{{loop.4.data.metrics.epochs}}',
            },
          },
        },
      ],
      else: [],
    },
  ];
}

/**
 * Build re-exam challenge loop steps (simplified — no mid-loop training).
 *
 * The LoRA is already loaded from the post-loop training. This loop just
 * re-attempts all challenges to measure improvement.
 *
 * Each iteration:
 *   0: Watch reexam:challenge:ready
 *   1: LLM implement (same prompt, but LoRA active via LimbicSystem hot-load)
 *   2: Emit reexam:challenge:attempted
 *   3: Watch reexam:verdict:ready
 */
function buildReexamChallengeSteps(
  sessionId: string,
  personaId: string,
  reexamIterEvt: (action: string) => string,
  llmConfig: Record<string, string>,
): PipelineStep[] {
  return [
    // loop.0: Watch for re-exam challenge
    {
      type: 'watch',
      event: reexamIterEvt('challenge:ready'),
      timeoutSecs: 300,
    },

    // loop.1: LLM — Re-implement (LoRA now active for local model inference)
    {
      type: 'llm',
      prompt: [
        'You are an expert Python developer. Implement the following class based on the skeleton and tests.',
        '',
        '=== CLASS SKELETON ===',
        '{{loop.0.data.payload.skeleton}}',
        '',
        '=== TEST CODE ===',
        '{{loop.0.data.payload.testCode}}',
        '',
        'INSTRUCTIONS:',
        '- Output ONLY the complete Python class implementation',
        '- Do NOT include markdown code fences or explanations',
        '- The implementation must pass all the tests',
        '- Tests marked @pytest.mark.xfail(strict=True) EXPECT the code to RAISE an exception.',
        '  If such a test passes without error, pytest counts it as XPASS (failure).',
        '  Make sure your implementation raises appropriate errors for invalid inputs.',
        '- Implement ALL methods from the skeleton',
        '- Keep the same class name and method signatures',
        '- Import any needed standard library modules at the top',
      ].join('\n'),
      ...llmConfig,
      temperature: 0.2,
      maxTokens: 4096,
    },

    // loop.2: Emit re-exam attempt
    {
      type: 'emit',
      event: reexamIterEvt('challenge:attempted'),
      payload: {
        sessionId,
        personaId,
        implementation: '{{loop.1.output}}',
        challengeIndex: '{{loop.0.data.payload.challengeIndex}}',
        round: '{{input.iteration}}',
      },
    },

    // loop.3: Watch for re-exam verdict
    {
      type: 'watch',
      event: reexamIterEvt('verdict:ready'),
      timeoutSecs: 600,
    },
  ];
}
