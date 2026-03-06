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
    // Teacher LLM may take several minutes (DeepSeek can be slow under load)
    {
      type: 'watch',
      event: evt('curriculum:ready'),
      timeoutSecs: 600,
    },

    // Step 1: Challenge loop — one iteration per challenge
    {
      type: 'loop',
      count: academyConfig.questionsPerExam,
      steps: [
        // loop.0: Watch for challenge:ready (teacher presents skeleton + tests)
        {
          type: 'watch',
          event: evt('challenge:ready'),
          timeoutSecs: 300,
        },

        // loop.1: LLM — Implement the Python class
        // Uses studentModel for inference (cloud model with adequate context),
        // while baseModel is the LoRA training target.
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
          model: academyConfig.studentModel ?? baseModel,
          ...(academyConfig.studentProvider && { provider: academyConfig.studentProvider }),
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

        // loop.3: Watch for teacher's verdict.
        // Teacher always emits verdict:ready with result + optional datasetPath.
        {
          type: 'watch',
          event: evt('verdict:ready'),
          timeoutSecs: 600,
        },

        // loop.4: Condition — did we get dataset:ready (need training)?
        // If the event was dataset:ready, payload contains datasetPath (truthy).
        // If the event was topic:passed, datasetPath is absent (empty = falsy).
        {
          type: 'condition',
          if: '{{loop.3.data.payload.datasetPath}}',
          then: [
            // Then sub-step 0: Train on remediation data
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
            // Then sub-step 1: Emit training:complete
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
        layers: '{{steps.1.data.iterations.*.4.data.layerId}}',
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
