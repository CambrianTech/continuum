/**
 * RecipeTeacherPipeline — Sentinel pipeline for recipe-driven Academy sessions
 *
 * Phase 5 of the Academy Implementation Plan:
 * "Feed a recipe to the Academy, it auto-designs the curriculum,
 *  identifies genome gaps, trains only what's missing."
 *
 * The recipe IS the specification. The teacher:
 * 1. Reads the recipe JSON (strategy, rules, tools, pipeline)
 * 2. Queries AdapterStore for the persona's existing genome coverage
 * 3. LLM analyzes recipe + existing adapters → extracts skill gaps
 * 4. Designs a curriculum targeting ONLY the gaps
 * 5. For each gap topic: synthesize data, train, exam, remediate
 *
 * The student pipeline is identical to knowledge mode — it trains on
 * synthesized data and proves mastery through exams. Recipe mode only
 * changes HOW the teacher designs curriculum (recipe-grounded, gap-aware).
 *
 * Shares the exam/grade/remediate loop with TeacherPipeline via
 * buildExamRetryLoop() to eliminate duplication.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { RecipeTeacherPipelineConfig } from '../../genome/shared/AcademyTypes';
import { academyEvent, type AcademyEventAction } from '../../genome/shared/AcademyTypes';
import type { RecipeDefinition } from '../../recipes/shared/RecipeTypes';

/**
 * Build the recipe-driven teacher sentinel pipeline.
 *
 * Step flow:
 *   0: LLM — Analyze recipe + adapters → extract skill gaps → design curriculum
 *   1: Command — data/create (persist curriculum)
 *   2: Emit — curriculum:ready
 *   3: Loop — For each gap topic: synthesize → emit → wait train → exam retry loop
 *   4: Emit — session:complete
 */
export function buildRecipeTeacherPipeline(config: RecipeTeacherPipelineConfig): Pipeline {
  const { sessionId, skill, personaName, personaId, baseModel, recipe, existingAdapters, config: academyConfig } = config;

  const evt = (action: string) => academyEvent(sessionId, action as AcademyEventAction);
  /** Iteration-scoped event: prevents watch from matching previous iteration's events */
  const iterEvt = (action: string) => `${academyEvent(sessionId, action as AcademyEventAction)}:{{input.iteration}}`;

  const steps: PipelineStep[] = [];
  let nextStepIdx = 0;

  // === Step 0: Curriculum Design with Gap Analysis (LLM) ===
  const curriculumStepIdx = nextStepIdx++;
  steps.push(buildRecipeCurriculumStep(
    skill, personaName, recipe, existingAdapters, academyConfig,
  ));

  // === Step 1: Persist Curriculum ===
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
        metadata: {
          mode: 'recipe',
          recipeId: recipe.uniqueId,
          recipeName: recipe.displayName,
          existingAdapterCount: existingAdapters.length,
        },
      },
    },
  });

  // === Step 2: Emit curriculum:ready ===
  nextStepIdx++;
  steps.push({
    type: 'emit',
    event: evt('curriculum:ready'),
    payload: {
      sessionId,
      curriculumId: `{{steps.${persistStepIdx}.data.data.id}}`,
    },
  });

  // === Step 3: Outer Loop — iterate over gap topics ===
  // Matches curriculum prompt's "3-5 topics" request.
  // If LLM returns fewer, extra iterations will no-op (undefined topic references).
  nextStepIdx++;
  steps.push({
    type: 'loop',
    count: 5,
    steps: buildTopicLoopSteps(
      sessionId, skill, personaName, recipe, academyConfig, evt, iterEvt,
      curriculumStepIdx,
    ),
  });

  // === Step 4: Emit session:complete ===
  steps.push({
    type: 'emit',
    event: evt('session:complete'),
    payload: {
      sessionId,
      skill,
      personaName,
      mode: 'recipe',
      recipeId: recipe.uniqueId,
    },
  });

  return {
    name: `academy-recipe-teacher-${recipe.uniqueId}`,
    steps,
    inputs: {
      sessionId,
      skill,
      personaName,
      baseModel,
      recipeId: recipe.uniqueId,
      recipeName: recipe.displayName,
    },
  };
}

// ============================================================================
// Recipe Grounding — Extract structured training context from a recipe
// ============================================================================

/**
 * Extract a rich grounding context from a recipe for use in dataset synthesis.
 *
 * This is the bridge between a recipe's behavioral specification and the
 * training data the LLM generates. The better the grounding, the more
 * the trained adapter will actually follow the recipe's patterns.
 */
function buildRecipeGroundingContext(recipe: RecipeDefinition): string {
  const sections: string[] = [];

  sections.push(`# Recipe: ${recipe.displayName}`);
  sections.push(`${recipe.description}`);
  sections.push('');

  // Strategy — the core behavioral spec
  sections.push('## Conversation Pattern');
  sections.push(`Pattern: ${recipe.strategy.conversationPattern}`);
  sections.push('');

  sections.push('## Response Rules (MUST follow)');
  for (const rule of recipe.strategy.responseRules) {
    sections.push(`- ${rule}`);
  }
  sections.push('');

  if (recipe.strategy.feedbackLoopRules?.length) {
    sections.push('## Feedback Loop Rules (MANDATORY verification behavior)');
    for (const rule of recipe.strategy.feedbackLoopRules) {
      sections.push(`- ${rule}`);
    }
    sections.push('');
  }

  sections.push('## Decision Criteria (when to engage)');
  for (const criterion of recipe.strategy.decisionCriteria) {
    sections.push(`- ${criterion}`);
  }
  sections.push('');

  // Tools — the persona needs to know how to use these
  if (recipe.tools?.length) {
    sections.push('## Available Tools');
    for (const tool of recipe.tools) {
      sections.push(`- ${tool.name}: ${tool.description}`);
    }
    sections.push('');
  }

  return sections.join('\n');
}

// ============================================================================
// Curriculum Design Step — Recipe Analysis + Gap Detection
// ============================================================================

/**
 * Build the curriculum design LLM step.
 *
 * The LLM receives:
 * - The full recipe (strategy, rules, tools, pipeline)
 * - The persona's existing adapters (typed as AdapterManifest via config)
 * - Instructions to identify GAPS and design curriculum for gaps ONLY
 */
function buildRecipeCurriculumStep(
  skill: string,
  personaName: string,
  recipe: RecipeTeacherPipelineConfig['recipe'],
  existingAdapters: RecipeTeacherPipelineConfig['existingAdapters'],
  academyConfig: RecipeTeacherPipelineConfig['config'],
): PipelineStep {
  const adapterList = existingAdapters.length > 0
    ? existingAdapters.map(a =>
      `  - "${a.name}" (domain: ${a.traitType}, model: ${a.baseModel}, created: ${a.createdAt}${a.trainingMetadata?.performance !== undefined ? `, performance: ${a.trainingMetadata.performance}` : ''})`
    ).join('\n')
    : '  (none — persona has no trained adapters)';

  const recipeContext = buildRecipeGroundingContext(recipe);

  const prompt = [
    `You are designing a targeted training curriculum for the AI persona "${personaName}".`,
    '',
    '## Recipe Specification',
    'The persona must master the following recipe to perform its role effectively:',
    '',
    recipeContext,
    '',
    '## Existing Genome (Trained Adapters)',
    'The persona already has these LoRA adapters trained:',
    adapterList,
    '',
    '## Your Task',
    '',
    'Analyze the recipe and identify the SPECIFIC SKILLS required to execute it well.',
    'Then compare against the existing adapters to find GAPS — skills the persona',
    'needs but does NOT have trained adapters for.',
    '',
    'Consider these skill dimensions:',
    '- **Behavioral**: Following conversation patterns, response rules, decision criteria',
    '- **Verification**: Feedback loop rules, mandatory checks, iterative fixing',
    '- **Tool Usage**: Proficiency with the tools highlighted in the recipe',
    '- **Domain Knowledge**: Subject matter expertise the recipe context requires',
    '- **Coordination**: Multi-agent interaction patterns (if collaborative/competitive)',
    '',
    'Design a curriculum with 3-5 topics targeting ONLY the gaps.',
    'If the persona already has strong coverage for a skill, SKIP it.',
    'If no gaps exist, return an empty topics array.',
    '',
    `The overall skill domain is "${skill}".`,
    '',
    'Output ONLY a JSON object with this structure (no markdown, no code fences):',
    '{',
    '  "skill": "the-skill-name",',
    '  "recipeAnalysis": {',
    '    "requiredSkills": ["skill1", "skill2", ...],',
    '    "coveredByExisting": ["skill-already-trained", ...],',
    '    "gaps": ["skill-not-yet-trained", ...]',
    '  },',
    '  "topics": [',
    '    {',
    '      "name": "Topic Name (targeting a specific gap)",',
    '      "description": "What this topic covers and why the recipe needs it",',
    '      "difficulty": "beginner|intermediate|advanced",',
    '      "targetedGap": "Which gap this topic addresses"',
    '    }',
    '  ]',
    '}',
  ].join('\n');

  return {
    type: 'llm',
    prompt,
    ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
    ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
    temperature: 0.7,
    maxTokens: 4096,
  };
}

// ============================================================================
// Topic Loop Steps — Synthesize → Train → Exam (same as knowledge mode)
// ============================================================================

/**
 * Build the topic loop steps for recipe-driven curriculum.
 *
 * Each iteration: synthesize training data → emit → wait for training → exam retry loop.
 * The synthesis step includes the recipe context as grounding, so training data
 * teaches the persona to behave according to the recipe's patterns.
 */
function buildTopicLoopSteps(
  sessionId: string,
  skill: string,
  personaName: string,
  recipe: RecipeTeacherPipelineConfig['recipe'],
  academyConfig: RecipeTeacherPipelineConfig['config'],
  evt: (action: string) => string,
  iterEvt: (action: string) => string,
  curriculumStepIdx: number,
): PipelineStep[] {
  const recipeGrounding = buildRecipeGroundingContext(recipe);

  return [
    // outer.0: Synthesize training data for current gap topic
    {
      type: 'command',
      command: 'genome/dataset-synthesize',
      params: {
        topic: `{{steps.${curriculumStepIdx}.output.topics.{{input.iteration}}.name}}`,
        skill,
        personaName,
        exampleCount: academyConfig.examplesPerTopic,
        difficulty: `{{steps.${curriculumStepIdx}.output.topics.{{input.iteration}}.difficulty}}`,
        groundingContext: recipeGrounding,
        ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
        ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
      },
    },

    // outer.1: Emit dataset:ready for student (iteration-scoped)
    {
      type: 'emit',
      event: iterEvt('dataset:ready'),
      payload: {
        sessionId,
        datasetPath: '{{loop.0.data.datasetPath}}',
        topicIndex: '{{input.iteration}}',
        topicName: `{{steps.${curriculumStepIdx}.output.topics.{{input.iteration}}.name}}`,
        exampleCount: '{{loop.0.data.exampleCount}}',
      },
    },

    // outer.2: Wait for student to finish training (iteration-scoped)
    {
      type: 'watch',
      event: iterEvt('training:complete'),
      timeoutSecs: 600,
    },

    // outer.3: Inner loop — exam/grade/remediate cycle
    {
      type: 'loop',
      until: '{{loop.4.output.passed}}',
      maxIterations: academyConfig.maxTopicAttempts,
      steps: buildExamRetrySteps(
        sessionId, skill, personaName, recipe, academyConfig, evt, iterEvt,
        curriculumStepIdx,
      ),
    },
  ];
}

/**
 * Build the inner exam retry loop steps.
 *
 * Same structure as knowledge mode but exam questions are grounded in the recipe.
 * Questions test the persona's ability to follow the recipe's patterns,
 * not just abstract knowledge.
 */
function buildExamRetrySteps(
  sessionId: string,
  skill: string,
  personaName: string,
  recipe: RecipeTeacherPipelineConfig['recipe'],
  academyConfig: RecipeTeacherPipelineConfig['config'],
  evt: (action: string) => string,
  iterEvt: (action: string) => string,
  curriculumStepIdx: number,
): PipelineStep[] {
  // Inner loop events scoped by OUTER topic iteration (parent_iteration)
  const parentIterEvt = (action: string) => `${academyEvent(sessionId, action as AcademyEventAction)}:{{input.parent_iteration}}`;

  const recipeGrounding = buildRecipeGroundingContext(recipe);

  const remediationSynthesizeParams: Record<string, unknown> = {
    topic: `{{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.name}}`,
    skill,
    personaName,
    exampleCount: academyConfig.examplesPerTopic,
    difficulty: `{{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.difficulty}}`,
    remediationFeedback: '{{loop.4.output.feedback}}',
    weakAreas: '{{loop.4.output.weakAreas}}',
    groundingContext: recipeGrounding,
    ...(academyConfig.teacherModel && { model: academyConfig.teacherModel }),
    ...(academyConfig.teacherProvider && { provider: academyConfig.teacherProvider }),
  };

  return [
    // inner.0: Generate exam questions grounded in the recipe
    {
      type: 'llm',
      prompt: [
        `Generate ${academyConfig.questionsPerExam} exam questions to test mastery of the topic: "{{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.name}}"`,
        `This is part of the "${skill}" curriculum for persona "${personaName}".`,
        `Difficulty: {{steps.${curriculumStepIdx}.output.topics.{{input.parent_iteration}}.difficulty}}`,
        `This is exam attempt {{input.iteration}} (0-indexed).`,
        '',
        'IMPORTANT: Questions must test the persona\'s ability to follow this recipe:',
        `Recipe: ${recipe.displayName}`,
        `Conversation pattern: ${recipe.strategy.conversationPattern}`,
        `Key rules: ${recipe.strategy.responseRules.slice(0, 5).join('; ')}`,
        ...(recipe.strategy.feedbackLoopRules?.length
          ? [`Verification rules: ${recipe.strategy.feedbackLoopRules.slice(0, 3).join('; ')}`]
          : []),
        '',
        'Questions should be SCENARIO-BASED — present a situation and ask how the',
        'persona should respond according to the recipe rules and patterns.',
        'Include scenarios testing tool usage, decision-making, and verification behavior.',
        '',
        '{{#if input.iteration}}',
        'The student failed the previous attempt. Focus questions on weak areas.',
        '{{/if}}',
        '',
        'Output ONLY a JSON array of question objects (no markdown, no code fences):',
        '[',
        '  {',
        '    "question": "The question text (scenario-based)",',
        '    "expectedAnswer": "The ideal answer following the recipe",',
        '    "category": "behavioral|tool-usage|domain|coordination|verification"',
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

    // inner.2: Emit exam:ready for student (scoped by outer topic iteration)
    {
      type: 'emit',
      event: parentIterEvt('exam:ready'),
      payload: {
        sessionId,
        examId: '{{loop.1.data.data.id}}',
        topicIndex: '{{input.parent_iteration}}',
        questions: '{{loop.0.output}}',
      },
    },

    // inner.3: Wait for student responses (scoped by outer topic iteration)
    {
      type: 'watch',
      event: parentIterEvt('exam:responses'),
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
        'IMPORTANT: Grade against the recipe\'s behavioral expectations:',
        `Recipe: ${recipe.displayName} (${recipe.strategy.conversationPattern})`,
        '',
        'Questions and expected answers:',
        '{{loop.0.output}}',
        '',
        'Student responses:',
        '{{loop.3.data.payload.responses}}',
        '',
        'For each response, evaluate how well the persona follows the recipe patterns.',
        'Score verification behavior and tool usage knowledge, not just factual recall.',
        'If the student fails, provide specific feedback on recipe-relevant weak areas.',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "overallScore": <0-100>,',
        '  "passed": <true/false>,',
        '  "feedback": "Overall feedback with specific recipe-relevant weak areas",',
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

    // inner.5: Persist grades
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

    // inner.6: Emit exam:graded (scoped by outer topic iteration)
    {
      type: 'emit',
      event: parentIterEvt('exam:graded'),
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
        {
          type: 'emit',
          event: parentIterEvt('topic:passed'),
          payload: {
            sessionId,
            topicIndex: '{{input.parent_iteration}}',
            round: '{{input.iteration}}',
            overallScore: '{{loop.4.output.overallScore}}',
          },
        },
      ],
      else: [
        {
          type: 'emit',
          event: parentIterEvt('topic:remediate'),
          payload: {
            sessionId,
            topicIndex: '{{input.parent_iteration}}',
            round: '{{input.iteration}}',
            feedback: '{{loop.4.output.feedback}}',
            weakAreas: '{{loop.4.output.weakAreas}}',
          },
        },
        {
          type: 'command',
          command: 'genome/dataset-synthesize',
          params: remediationSynthesizeParams,
        },
        {
          type: 'emit',
          event: parentIterEvt('dataset:ready'),
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
        {
          type: 'watch',
          event: parentIterEvt('training:complete'),
          timeoutSecs: 600,
        },
      ],
    },
  ];
}
