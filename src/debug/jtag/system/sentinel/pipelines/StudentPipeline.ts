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
import { academyEvent } from '../../genome/shared/AcademyTypes';

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

  const evt = (action: string) => academyEvent(sessionId, action as any);

  const steps: PipelineStep[] = [
    // Step 0: Wait for teacher to publish curriculum
    {
      type: 'watch',
      event: evt('curriculum:ready'),
      timeoutSecs: 300,  // 5 minutes for curriculum design
    },

    // Step 1: Loop over topics (matching teacher's topic count)
    // Intra-loop references use {{loop.N.field}} for stable referencing
    {
      type: 'loop',
      count: 5,  // Max topics (safety limit)
      steps: [
        // loop.0: Wait for training data from teacher
        {
          type: 'watch',
          event: evt('dataset:ready'),
          timeoutSecs: 300,  // 5 minutes for data synthesis
        },

        // loop.1: PRE-TEST — Answer topic questions BEFORE training
        // This establishes a baseline score to compare against post-training.
        // The student uses its current (untrained) capability on this topic.
        {
          type: 'llm',
          prompt: [
            `You are ${personaName}, an AI persona. Answer the following questions about "{{loop.0.data.payload.topicName}}" to the best of your current ability.`,
            'Be thorough but concise. These questions test your knowledge BEFORE any specific training on this topic.',
            '',
            'For each question, provide your best answer.',
            '',
            'Questions (answer ALL of them):',
            '1. What are the key concepts in {{loop.0.data.payload.topicName}}?',
            '2. Explain the most important principle of {{loop.0.data.payload.topicName}}.',
            '3. Give a practical example demonstrating {{loop.0.data.payload.topicName}}.',
            '4. What are common mistakes when applying {{loop.0.data.payload.topicName}}?',
            '5. How does {{loop.0.data.payload.topicName}} relate to other concepts in the field?',
            '',
            'Output ONLY a JSON array of response objects (no markdown, no code fences):',
            '[',
            '  { "questionIndex": 0, "studentAnswer": "Your answer here" }',
            ']',
          ].join('\n'),
          temperature: 0.5,
          maxTokens: 2048,
        },

        // loop.2: Emit training:started
        {
          type: 'emit',
          event: evt('training:started'),
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

        // loop.4: Emit training:complete
        {
          type: 'emit',
          event: evt('training:complete'),
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

        // loop.5: Wait for exam from teacher
        {
          type: 'watch',
          event: evt('exam:ready'),
          timeoutSecs: 300,
        },

        // loop.6: Take the exam via LLM (POST-training)
        // Uses the system default model; future: use base model +
        // trained LoRA adapters via Candle local inference to prove training worked
        {
          type: 'llm',
          prompt: [
            `You are ${personaName}, an AI persona taking an exam.`,
            'Answer each question to the best of your ability.',
            'Be thorough but concise in your answers.',
            '',
            'Questions:',
            '{{loop.5.data.payload.questions}}',
            '',
            'Output ONLY a JSON array of response objects (no markdown, no code fences):',
            '[',
            '  {',
            '    "questionIndex": 0,',
            '    "studentAnswer": "Your answer here"',
            '  }',
            ']',
          ].join('\n'),
          temperature: 0.5,
          maxTokens: 2048,
        },

        // loop.7: Emit exam:responses
        {
          type: 'emit',
          event: evt('exam:responses'),
          payload: {
            sessionId,
            examId: '{{loop.5.data.payload.examId}}',
            topicIndex: '{{input.iteration}}',
            responses: '{{loop.6.output}}',
          },
        },

        // loop.8: Wait for grading results
        {
          type: 'watch',
          event: evt('exam:graded'),
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
              event: evt('inference:demo'),
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
              event: evt('quality:gate:failed'),
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
