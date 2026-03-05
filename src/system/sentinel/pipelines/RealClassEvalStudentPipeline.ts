/**
 * RealClassEvalStudentPipeline — Sentinel pipeline for the student side of
 * RealClassEval benchmark-based training.
 *
 * The student:
 * 1. Watches for curriculum:ready from the teacher
 * 2. In a loop per challenge:
 *    a. Watches challenge:ready (receives skeleton + tests)
 *    b. Uses local LLM (with LoRA if loaded) to implement the class
 *    c. Emits challenge:attempted with implementation
 *    d. If topic:passed → next challenge
 *    e. If dataset:ready (remediation) → trains LoRA, emits training:complete
 *    f. If topic:remediate → loop retries with improved LoRA
 * 3. Post-loop: compose all trained adapters
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { RealClassEvalStudentPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';

/**
 * Build the RealClassEval student sentinel pipeline.
 *
 * Step flow:
 *   0: Watch — curriculum:ready
 *   1: Loop (maxTopicAttempts iterations):
 *     loop.0:  Watch — challenge:ready (skeleton + tests)
 *     loop.1:  LLM — Implement the class (uses baseModel = local, LoRA helps)
 *     loop.2:  Emit — challenge:attempted { implementation }
 *     loop.3:  Watch — topic:passed OR dataset:ready (teacher decides)
 *     loop.4:  Condition — is this a dataset:ready event? (remediation path)
 *       Then: [genome/train, Emit training:complete]
 *       Else: [] (topic passed, no training needed)
 *   2: Command — genome/compose (merge all trained adapters)
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

  const steps: PipelineStep[] = [
    // Step 0: Wait for teacher to publish curriculum
    {
      type: 'watch',
      event: evt('curriculum:ready'),
      timeoutSecs: 300,
    },

    // Step 1: Challenge attempt loop
    {
      type: 'loop',
      count: academyConfig.maxTopicAttempts,
      steps: [
        // loop.0: Watch for challenge:ready (teacher presents skeleton + tests)
        {
          type: 'watch',
          event: evt('challenge:ready'),
          timeoutSecs: 300,
        },

        // loop.1: LLM — Implement the Python class
        // Uses baseModel (local model) so LoRA training improves this step
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
            '- Do NOT include markdown code fences',
            '- Do NOT include explanations or comments',
            '- The implementation must pass all the tests',
            '- Implement ALL methods from the skeleton',
            '- Keep the same class name and method signatures',
          ].join('\n'),
          model: baseModel,
          temperature: 0.2,
          maxTokens: 4096,
        },

        // loop.2: Emit challenge:attempted with the implementation
        {
          type: 'emit',
          event: evt('challenge:attempted'),
          payload: {
            sessionId,
            personaId,
            implementation: '{{loop.1.output}}',
            challengeIndex: '{{loop.0.data.payload.challengeIndex}}',
            round: '{{input.iteration}}',
          },
        },

        // loop.3: Watch for teacher's verdict — either dataset:ready (need training)
        // or topic:passed (move on). We watch for dataset:ready which means we need
        // remediation. If topic:passed happens instead, the watch will timeout and
        // we break out of the loop naturally.
        {
          type: 'watch',
          event: evt('dataset:ready'),
          timeoutSecs: 120,
        },

        // loop.4: Train on remediation data if received
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

        // loop.5: Emit training:complete
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
    },

    // Step 2: Post-loop — compose all trained adapters into stacked genome
    {
      type: 'command',
      command: 'genome/compose',
      params: {
        personaId,
        baseModel,
        name: `${personaName}-realclasseval-${sessionId.slice(0, 8)}`,
        layers: '{{steps.1.iterations.*.4.data.layerId}}',
        strategy: 'weighted-merge',
        activate: true,
      },
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
