/**
 * StudentPipeline — Sentinel pipeline template for the Academy student
 *
 * The student sentinel is the learning half of the Academy Dojo.
 * It watches for teacher events and responds:
 * 1. Watches for dataset:ready — takes a PRE-TEST baseline, then trains
 * 2. Watches for exam:ready — takes the exam via LLM step
 * 3. Watches for exam:graded — validates phenotype improvement
 * 4. Quality gate: only registers adapter if improvement exceeds threshold
 * 5. Activates adapter via LRU paging (evicts old adapters under memory pressure)
 * 6. Emits inference demo after successful registration
 * 7. Post-loop: composes all trained adapters into a single stacked genome
 *
 * The pre-test → train → post-test → compare cycle is the phenotype
 * validation loop: it proves training actually improved the model.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { StudentPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent, ACADEMY_EVENTS } from '../../genome/shared/AcademyTypes';

const E = ACADEMY_EVENTS;

/**
 * Build the student sentinel pipeline.
 *
 * Step flow:
 *   0: Watch — curriculum:ready (teacher published curriculum)
 *   1: Loop (driven by topic count):
 *     loop.0:  Watch — dataset:ready (teacher synthesized training data)
 *     loop.1:  LLM — Pre-test baseline (answer topic questions BEFORE training)
 *     loop.2:  Emit — training:started
 *     loop.3:  Command — genome/train { datasetPath from event }
 *     loop.4:  Emit — training:complete { layerId, metrics }
 *     loop.5:  Watch — exam:ready (teacher generated exam)
 *     loop.6:  LLM — Answer exam questions as the persona (POST-training)
 *     loop.7:  Emit — exam:responses { answers }
 *     loop.8:  Watch — exam:graded (teacher graded responses)
 *     loop.9:  Command — genome/phenotype-validate (compare pre vs post)
 *     loop.10: Condition — quality gate: register + activate only if improved
 *   2: Command — genome/compose (merge all trained adapters into stacked genome)
 */
export function buildStudentPipeline(config: StudentPipelineConfig): Pipeline {
  const { sessionId, personaId, personaName, baseModel, config: academyConfig } = config;

  const evt = (action: string) => academyEvent(sessionId, action);
  /** Iteration-scoped event: prevents watch from matching previous iteration's events */
  const iterEvt = (action: string) => `${academyEvent(sessionId, action)}:{{input.iteration}}`;


  const steps: PipelineStep[] = [
    // Step 0: Wait for teacher to publish curriculum
    {
      type: 'watch',
      event: evt(E.CURRICULUM_READY),
      timeoutSecs: 300,  // 5 minutes for curriculum design
    },

    // Step 1: Loop over topics (matching teacher's topic count)
    // Intra-loop references use {{loop.N.field}} for stable referencing
    {
      type: 'loop',
      count: academyConfig.topicsPerSession,
      steps: [
        // loop.0: Wait for training data from teacher (iteration-scoped)
        {
          type: 'watch',
          event: iterEvt(E.DATASET_READY),
          timeoutSecs: 300,  // 5 minutes for data synthesis
        },

        // loop.1: PRE-TEST — Answer topic questions BEFORE training
        // Uses Candle (local model) WITHOUT adapters for baseline.
        // This establishes the pre-training score that post-test compares against.
        {
          type: 'llm',
          prompt: [
            `You are ${personaName}. Answer about "{{loop.0.data.payload.topicName}}":`,
            '1. Key concepts?',
            '2. Most important principle?',
            '3. Practical example?',
            'Reply as JSON: [{"questionIndex":0,"studentAnswer":"..."}]',
          ].join('\n'),
          model: baseModel,
          provider: 'candle',
          temperature: 0.5,
          maxTokens: 1024,
        },

        // loop.2: Emit training:started (iteration-scoped)
        {
          type: 'emit',
          event: iterEvt(E.TRAINING_STARTED),
          payload: {
            sessionId,
            personaId,
            topicIndex: '{{input.iteration}}',
            datasetPath: '{{loop.0.data.payload.datasetPath}}',
          },
        },

        // loop.3: Train on the synthesized dataset
        {
          type: 'command',
          command: 'genome/train',
          params: {
            personaId,
            personaName,
            traitType: '{{loop.0.data.payload.topicName}}',
            baseModel,
            datasetPath: '{{loop.0.data.payload.datasetPath}}',
            rank: academyConfig.rank,
            epochs: academyConfig.epochs,
            learningRate: academyConfig.learningRate,
            batchSize: academyConfig.batchSize,
          },
        },

        // loop.4: Emit training:complete (iteration-scoped)
        {
          type: 'emit',
          event: iterEvt(E.TRAINING_COMPLETE),
          payload: {
            sessionId,
            personaId,
            topicIndex: '{{input.iteration}}',
            layerId: '{{loop.3.data.layerId}}',
            metrics: {
              finalLoss: '{{loop.3.data.metrics.finalLoss}}',
              trainingTime: '{{loop.3.data.metrics.trainingTime}}',
              examplesProcessed: '{{loop.3.data.metrics.examplesProcessed}}',
              epochs: '{{loop.3.data.metrics.epochs}}',
            },
          },
        },

        // loop.5: Wait for exam from teacher (iteration-scoped)
        {
          type: 'watch',
          event: iterEvt(E.EXAM_READY),
          timeoutSecs: 300,
        },

        // loop.6: Take the exam via LLM (POST-training)
        // Uses Candle WITH trained LoRA adapter — this is the whole point.
        // Comparing loop.1 (no adapter) vs loop.6 (with adapter) proves training worked.
        {
          type: 'llm',
          prompt: [
            `You are ${personaName}. Answer these exam questions:`,
            '{{loop.5.data.payload.questions}}',
            'Reply as JSON: [{"questionIndex":0,"studentAnswer":"..."}]',
          ].join('\n'),
          model: baseModel,
          provider: 'candle',
          temperature: 0.5,
          maxTokens: 1024,
          activeAdapters: [{
            name: '{{loop.3.data.layerId}}',
            path: '{{loop.3.data.adapterPath}}',
            domain: '{{loop.0.data.payload.topicName}}',
            scale: 1.0,
          }],
        },

        // loop.7: Emit exam:responses (iteration-scoped)
        {
          type: 'emit',
          event: iterEvt(E.EXAM_RESPONSES),
          payload: {
            sessionId,
            examId: '{{loop.5.data.payload.examId}}',
            topicIndex: '{{input.iteration}}',
            responses: '{{loop.6.output}}',
          },
        },

        // loop.8: Wait for grading results (iteration-scoped)
        {
          type: 'watch',
          event: iterEvt(E.EXAM_GRADED),
          timeoutSecs: 300,
        },

        // loop.9: Phenotype validation — compare pre-test (loop.1) vs exam (loop.6)
        // Uses LLM-as-judge to score both sets of answers against the exam questions
        {
          type: 'command',
          command: 'genome/phenotype-validate',
          params: {
            questions: '{{loop.5.data.payload.questions}}',
            baselineResponses: '{{loop.1.output}}',
            adaptedResponses: '{{loop.6.output}}',
            improvementThreshold: 5,
          },
        },

        // loop.10: Quality gate — only register adapter if phenotype improved
        {
          type: 'condition',
          if: '{{loop.9.data.passedQualityGate}}',
          then: [
            // Write phenotype metrics back to adapter manifest
            {
              type: 'command',
              command: 'genome/adapter-update-metrics',
              params: {
                adapterPath: '{{loop.3.data.adapterPath}}',
                phenotypeScore: '{{loop.9.data.adaptedScore}}',
                phenotypeImprovement: '{{loop.9.data.improvement}}',
              },
            },
            // Register the trained adapter in the paging registry
            {
              type: 'command',
              command: 'genome/paging-adapter-register',
              params: {
                layerId: '{{loop.3.data.layerId}}',
                adapterId: '{{loop.3.data.layerId}}',
                name: `${personaName}-${sessionId.slice(0, 8)}-topic-{{input.iteration}}`,
                domain: '{{loop.0.data.payload.topicName}}',
                sizeMB: 0,
              },
            },
            // Activate adapter on persona — triggers LRU eviction under memory pressure
            {
              type: 'command',
              command: 'genome/paging-activate',
              params: {
                personaId,
                adapterId: '{{loop.3.data.layerId}}',
              },
            },
            // Emit inference demo — showcase what the adapted model learned
            {
              type: 'emit',
              event: iterEvt(E.INFERENCE_DEMO),
              payload: {
                sessionId,
                personaId,
                topicIndex: '{{input.iteration}}',
                topicName: '{{loop.0.data.payload.topicName}}',
                baselineScore: '{{loop.9.data.baselineScore}}',
                adaptedScore: '{{loop.9.data.adaptedScore}}',
                improvement: '{{loop.9.data.improvement}}',
                summary: '{{loop.9.data.summary}}',
                // Include a sample Q&A to demonstrate learning
                sampleQuestion: '{{loop.9.data.questionResults.0.question}}',
                sampleBaselineAnswer: '{{loop.9.data.questionResults.0.baselineAnswer}}',
                sampleAdaptedAnswer: '{{loop.9.data.questionResults.0.adaptedAnswer}}',
              },
            },
          ],
          else: [
            // Training didn't help enough — emit quality gate failure
            {
              type: 'emit',
              event: iterEvt(E.QUALITY_GATE_FAILED),
              payload: {
                sessionId,
                personaId,
                topicIndex: '{{input.iteration}}',
                topicName: '{{loop.0.data.payload.topicName}}',
                baselineScore: '{{loop.9.data.baselineScore}}',
                adaptedScore: '{{loop.9.data.adaptedScore}}',
                improvement: '{{loop.9.data.improvement}}',
                summary: '{{loop.9.data.summary}}',
              },
            },
          ],
        },
      ],
    },

    // Step 2: Post-loop composition — merge all successfully trained adapters
    // into a single stacked genome for the persona.
    // Uses the layerIds collected from each loop iteration's training step.
    // The sentinel engine tracks step results across iterations, so we reference
    // the training results from all loop iterations.
    {
      type: 'command',
      command: 'genome/compose',
      params: {
        personaId,
        baseModel,
        name: `${personaName}-academy-${sessionId.slice(0, 8)}`,
        // Layers are collected from all successful training iterations.
        // Each loop.3.data.layerId contains the trained adapter's UUID.
        // The Rust engine expands {{steps.1.iterations}} to the loop result array.
        layers: '{{steps.1.iterations.*.3.data.layerId}}',
        strategy: 'weighted-merge',
        activate: true,  // Auto-activate with LRU eviction
      },
    },
  ];

  return {
    name: `academy-student-${personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    steps,
    inputs: {
      sessionId,
      personaId,
      personaName,
      baseModel,
    },
  };
}
