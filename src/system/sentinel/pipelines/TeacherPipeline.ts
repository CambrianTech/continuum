/**
 * TeacherPipeline — Sentinel pipeline template for the Academy teacher
 *
 * The teacher sentinel is the intelligent half of the Academy Dojo.
 * It uses LLM steps to:
 * 1. (Optional) Explore data sources and extract verified facts
 * 2. Research the skill domain and design a progressive curriculum
 * 3. For each topic: synthesize training data, wait for student to train,
 *    generate exams, grade responses, decide pass/fail/remediate
 * 4. When student fails: generate targeted remedial data, re-train, re-exam
 * 5. Emit events for inter-sentinel coordination with the student
 *
 * All intelligence comes from LLM prompts — the pipeline structure is
 * just control flow. The teacher adapts curriculum based on exam results,
 * generating more data where the student is weak.
 *
 * Knowledge Synthesis Mode (when dataSources provided):
 *   Step 0: Nested sentinel — KnowledgeExplorationPipeline → SourceKnowledge
 *   Step 1: LLM — Design curriculum grounded in extracted facts
 *   Steps 2+: Same as ungrounded mode, shifted by 1
 *
 * Pure Generation Mode (no dataSources — backward compatible):
 *   Step 0: LLM — Design curriculum from scratch
 *   Steps 1+: Same as original
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { TeacherPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent } from '../../genome/shared/AcademyTypes';
import { buildKnowledgeExplorationPipeline } from './KnowledgeExplorationPipeline';

/**
 * Build the teacher sentinel pipeline.
 *
 * When config.dataSources is provided, prepends a knowledge exploration
 * step that extracts facts from the sources. The curriculum LLM and
 * all dataset-synthesize calls receive grounding context from the
 * extracted facts.
 *
 * When config.dataSources is absent, behaves identically to the
 * original pure-generation pipeline.
 */
export function buildTeacherPipeline(config: TeacherPipelineConfig): Pipeline {
  const { sessionId, skill, personaName, baseModel, config: academyConfig } = config;
  const hasKnowledgeSources = config.dataSources && config.dataSources.length > 0;

  const evt = (action: string) => academyEvent(sessionId, action as any);

  const steps: PipelineStep[] = [];

  // Track step indices for interpolation references
  let nextStepIdx = 0;

  // === Optional Step 0: Knowledge Exploration ===
  let knowledgeStepIdx: number | undefined;
  if (hasKnowledgeSources) {
    const explorationPipeline = buildKnowledgeExplorationPipeline({
      dataSources: config.dataSources!,
      model: academyConfig.teacherModel,
      provider: academyConfig.teacherProvider,
    });

    steps.push({
      type: 'sentinel',
      pipeline: explorationPipeline,
    });
    knowledgeStepIdx = nextStepIdx++;
  }

  // === Curriculum Design Step (LLM) ===
  const curriculumStepIdx = nextStepIdx++;
  steps.push(buildCurriculumStep(skill, personaName, academyConfig, knowledgeStepIdx));

  // === Persist Curriculum ===
  const persistStepIdx = nextStepIdx++;
  steps.push({
    type: 'command',
    command: 'data/create',
    params: {
      collection: 'academy_curricula',
      data: {
        sessionId,
        skill,
        topics: `{{steps.${curriculumStepIdx}.output}}`,
        generatedBy: `{{steps.${curriculumStepIdx}.data.model}}`,
        totalTopics: 0,
        completedTopics: 0,
      },
    },
  });

  // === Emit curriculum:ready ===
  const emitCurriculumStepIdx = nextStepIdx++;
  steps.push({
    type: 'emit',
    event: evt('curriculum:ready'),
    payload: {
      sessionId,
      curriculumId: `{{steps.${persistStepIdx}.data.data.id}}`,
    },
  });

  // === Outer Loop: iterate over topics ===
  const loopStepIdx = nextStepIdx++;
  steps.push({
    type: 'loop',
    count: 5,
    steps: buildTopicLoopSteps(
      sessionId, skill, personaName, academyConfig, evt,
      curriculumStepIdx, knowledgeStepIdx,
    ),
  });

  // === Emit session:complete ===
  steps.push({
    type: 'emit',
    event: evt('session:complete'),
    payload: {
      sessionId,
      skill,
      personaName,
    },
  });

  return {
    name: `academy-teacher-${skill}`,
    steps,
    inputs: {
      sessionId,
      skill,
      personaName,
      baseModel,
      ...(hasKnowledgeSources && { knowledgeSynthesis: true }),
    },
  };
}

/**
 * Build the curriculum design LLM step.
 * When knowledgeStepIdx is provided, includes extracted facts for grounding.
 */
function buildCurriculumStep(
  skill: string,
  personaName: string,
  academyConfig: TeacherPipelineConfig['config'],
  knowledgeStepIdx?: number,
): PipelineStep {
  const promptLines = [
    `You are designing a training curriculum to teach the skill "${skill}" to an AI persona named "${personaName}".`,
    '',
  ];

  if (knowledgeStepIdx !== undefined) {
    promptLines.push(
      'You have access to verified source knowledge extracted from real data sources.',
      'Use these facts to design a curriculum that teaches THIS SPECIFIC knowledge:',
      '',
      `{{steps.${knowledgeStepIdx}.output}}`,
      '',
      'Design topics that cover the key areas found in the source knowledge.',
      'Each topic should teach a distinct subset of the extracted facts.',
      '',
    );
  }

  promptLines.push(
    'Design a curriculum with 3-5 progressive topics, ordered from foundational to advanced.',
    'Each topic should build on the previous one.',
    '',
    'Output ONLY a JSON object with this structure (no markdown, no code fences):',
    '{',
    '  "skill": "the-skill-name",',
    '  "topics": [',
    '    {',
    '      "name": "Topic Name",',
    '      "description": "What this topic covers and why it matters",',
    '      "difficulty": "beginner|intermediate|advanced"',
    '    }',
    '  ]',
    '}',
  );

  return {
    type: 'llm',
    prompt: promptLines.join('\n'),
    ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
    ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
    temperature: 0.7,
    maxTokens: 2048,
  };
}

/**
 * Build the outer loop steps for iterating over curriculum topics.
 *
 * Each iteration: synthesize data → emit → wait for training → exam loop.
 * When knowledgeStepIdx is provided, passes groundingContext to synthesis.
 */
function buildTopicLoopSteps(
  sessionId: string,
  skill: string,
  personaName: string,
  academyConfig: TeacherPipelineConfig['config'],
  evt: (action: string) => string,
  curriculumStepIdx: number,
  knowledgeStepIdx?: number,
): PipelineStep[] {
  // Build the synthesize params — conditionally include groundingContext
  const synthesizeParams: Record<string, unknown> = {
    topic: `{{steps.${curriculumStepIdx}.output.topics.{{input.iteration}}.name}}`,
    skill,
    personaName,
    exampleCount: academyConfig.examplesPerTopic,
    difficulty: `{{steps.${curriculumStepIdx}.output.topics.{{input.iteration}}.difficulty}}`,
    ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
    ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
  };

  if (knowledgeStepIdx !== undefined) {
    // Pass the extracted facts as grounding context for synthesis
    synthesizeParams.groundingContext = `{{steps.${knowledgeStepIdx}.output}}`;
  }

  return [
    // outer.0: Synthesize initial training data for current topic
    {
      type: 'command',
      command: 'genome/dataset-synthesize',
      params: synthesizeParams,
    },

    // outer.1: Emit dataset:ready for student (initial training)
    {
      type: 'emit',
      event: evt('dataset:ready'),
      payload: {
        sessionId,
        datasetPath: '{{loop.0.data.datasetPath}}',
        topicIndex: '{{input.iteration}}',
        topicName: `{{steps.${curriculumStepIdx}.output.topics.{{input.iteration}}.name}}`,
        exampleCount: '{{loop.0.data.exampleCount}}',
      },
    },

    // outer.2: Wait for student to finish initial training
    {
      type: 'watch',
      event: evt('training:complete'),
      timeoutSecs: 600,
    },

    // outer.3: Inner loop — exam/grade/remediate cycle
    {
      type: 'loop',
      until: '{{loop.4.output.passed}}',
      maxIterations: academyConfig.maxTopicAttempts,
      steps: buildExamRetrySteps(
        sessionId, skill, personaName, academyConfig, evt,
        curriculumStepIdx, knowledgeStepIdx,
      ),
    },
  ];
}

/**
 * Build the inner exam retry loop steps.
 *
 * Each iteration of this inner loop:
 * 1. Generates exam questions
 * 2. Emits exam:ready
 * 3. Watches for student responses
 * 4. Grades responses
 * 5. Emits exam:graded
 * 6. If passed: emits topic:passed (loop will terminate via `until`)
 *    If failed: synthesizes targeted remedial data, emits dataset:ready,
 *               waits for re-training to complete
 *
 * Inner loop step references use {{loop.N}} within the inner loop context.
 * The outer loop's topic index is still available via parent context.
 */
function buildExamRetrySteps(
  sessionId: string,
  skill: string,
  personaName: string,
  academyConfig: TeacherPipelineConfig['config'],
  evt: (action: string) => string,
  curriculumStepIdx: number,
  knowledgeStepIdx?: number,
): PipelineStep[] {
  // Build remedial synthesize params
  const remediationSynthesizeParams: Record<string, unknown> = {
    topic: `{{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.name}}`,
    skill,
    personaName,
    exampleCount: academyConfig.examplesPerTopic,
    difficulty: `{{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.difficulty}}`,
    remediationFeedback: '{{loop.4.output.feedback}}',
    weakAreas: '{{loop.4.output.weakAreas}}',
    ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
    ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
  };

  if (knowledgeStepIdx !== undefined) {
    remediationSynthesizeParams.groundingContext = `{{steps.${knowledgeStepIdx}.output}}`;
  }

  return [
    // inner.0: Generate exam questions via LLM
    {
      type: 'llm',
      prompt: [
        `Generate ${academyConfig.questionsPerExam} exam questions to test mastery of the topic: "{{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.name}}"`,
        `This is part of the "${skill}" curriculum for persona "${personaName}".`,
        `Difficulty: {{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.difficulty}}`,
        `This is exam attempt {{input.iteration}} (0-indexed).`,
        '',
        '{{#if input.iteration}}',
        'The student failed the previous attempt. Focus questions on weak areas.',
        '{{/if}}',
        '',
        'Output ONLY a JSON array of question objects (no markdown, no code fences):',
        '[',
        '  {',
        '    "question": "The question text",',
        '    "expectedAnswer": "The ideal answer",',
        '    "category": "Sub-category within the topic"',
        '  }',
        ']',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      temperature: 0.7,
      maxTokens: 2048,
    },

    // inner.1: Persist exam to database
    {
      type: 'command',
      command: 'data/create',
      params: {
        collection: 'academy_examinations',
        data: {
          sessionId,
          topicIndex: '{{input.parent_iteration}}',
          round: '{{input.iteration}}',
          questions: '{{loop.0.output}}',
          responses: [],
          overallScore: 0,
          passed: false,
        },
      },
    },

    // inner.2: Emit exam:ready for student
    {
      type: 'emit',
      event: evt('exam:ready'),
      payload: {
        sessionId,
        examId: '{{loop.1.data.data.id}}',
        topicIndex: '{{input.parent_iteration}}',
        questions: '{{loop.0.output}}',
      },
    },

    // inner.3: Wait for student responses
    {
      type: 'watch',
      event: evt('exam:responses'),
      timeoutSecs: 300,
    },

    // inner.4: Grade responses via LLM
    {
      type: 'llm',
      prompt: [
        `Grade the following exam responses for the topic "{{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.name}}".`,
        `Passing score: ${academyConfig.passingScore}/100`,
        `This is attempt {{input.iteration}} (0-indexed).`,
        '',
        'Questions and expected answers:',
        '{{loop.0.output}}',
        '',
        'Student responses:',
        '{{loop.3.data.payload.responses}}',
        '',
        'For each response, evaluate accuracy and completeness.',
        'If the student fails, provide specific feedback on weak areas to guide remediation.',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "overallScore": <0-100>,',
        '  "passed": <true/false>,',
        '  "feedback": "Overall feedback summary with specific weak areas",',
        '  "weakAreas": ["area1", "area2"],',
        '  "responses": [',
        '    { "questionIndex": 0, "score": <0-100>, "feedback": "Per-question feedback" }',
        '  ]',
        '}',
      ].join('\n'),
      ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
      ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      temperature: 0.3,
      maxTokens: 2048,
    },

    // inner.5: Persist grades to database
    {
      type: 'command',
      command: 'data/update',
      params: {
        collection: 'academy_examinations',
        id: '{{loop.1.data.data.id}}',
        data: {
          responses: '{{loop.4.output.responses}}',
          overallScore: '{{loop.4.output.overallScore}}',
          passed: '{{loop.4.output.passed}}',
          gradedBy: '{{loop.4.data.model}}',
          feedback: '{{loop.4.output.feedback}}',
          weakAreas: '{{loop.4.output.weakAreas}}',
        },
      },
    },

    // inner.6: Emit exam:graded
    {
      type: 'emit',
      event: evt('exam:graded'),
      payload: {
        sessionId,
        examId: '{{loop.1.data.data.id}}',
        topicIndex: '{{input.parent_iteration}}',
        round: '{{input.iteration}}',
        overallScore: '{{loop.4.output.overallScore}}',
        passed: '{{loop.4.output.passed}}',
        feedback: '{{loop.4.output.feedback}}',
      },
    },

    // inner.7: Pass/remediate decision
    {
      type: 'condition',
      if: '{{loop.4.output.passed}}',
      then: [
        // Student passed — emit topic:passed
        {
          type: 'emit',
          event: evt('topic:passed'),
          payload: {
            sessionId,
            topicIndex: '{{input.parent_iteration}}',
            round: '{{input.iteration}}',
            overallScore: '{{loop.4.output.overallScore}}',
          },
        },
      ],
      else: [
        // Student failed — synthesize targeted remedial data
        {
          type: 'emit',
          event: evt('topic:remediate'),
          payload: {
            sessionId,
            topicIndex: '{{input.parent_iteration}}',
            round: '{{input.iteration}}',
            feedback: '{{loop.4.output.feedback}}',
            weakAreas: '{{loop.4.output.weakAreas}}',
          },
        },

        // Generate remedial training data targeting the weak areas
        {
          type: 'command',
          command: 'genome/dataset-synthesize',
          params: remediationSynthesizeParams,
        },

        // Emit remedial dataset:ready for student to re-train
        {
          type: 'emit',
          event: evt('dataset:ready'),
          payload: {
            sessionId,
            datasetPath: '{{loop.9.data.datasetPath}}',
            topicIndex: '{{input.parent_iteration}}',
            topicName: `{{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.name}}`,
            exampleCount: '{{loop.9.data.exampleCount}}',
            isRemediation: true,
            round: '{{input.iteration}}',
          },
        },

        // Wait for student to finish remedial training
        {
          type: 'watch',
          event: evt('training:complete'),
          timeoutSecs: 600,
        },
      ],
    },
  ];
}
