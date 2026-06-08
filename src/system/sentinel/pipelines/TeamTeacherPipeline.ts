/**
 * TeamTeacherPipeline — Orchestrator for collaborative team academy projects
 *
 * The teacher runs four sequential phases:
 *
 * Phase 1 — PLANNING: Decompose project into roles + milestones via LLM
 * Phase 2 — TRAINING COORDINATION: Wait for all students to finish role training
 * Phase 3 — BUILD ORCHESTRATION: Guide team through milestone-based building
 * Phase 4 — REVIEW: Dual grading — project quality + individual role performance
 *
 * All work is posted to the academy chat room. Students communicate naturally
 * between pipeline steps via their autonomous loops.
 *
 * @see TeamProjectTypes for event taxonomy
 * @see TeamStudentPipeline for per-role student execution
 */

import type { Pipeline, PipelineStep } from '../../../../core/continuum-core/bindings/modules/sentinel';
import type { TeamTeacherPipelineConfig } from '../../genome/shared/TeamProjectTypes';
import { teamEvent, TEAM_EVENTS } from '../../genome/shared/TeamProjectTypes';
import { resolveTeacherLlmConfig } from '../../genome/shared/AcademyTypes';

const E = TEAM_EVENTS;

/**
 * Build the team teacher sentinel pipeline.
 *
 * Step flow:
 *   0: LLM — Decompose project into roles + milestones
 *   1: Command — data/create (persist project plan)
 *   2: Emit — project:planned
 *   3: Loop (per member) — Generate role-specific curriculum
 *   4: Chat — Post project plan to academy room
 *   5: Loop (memberCount) — Watch for each role:training:done
 *   6: Chat — "All trained. Starting build."
 *   7: Emit — build:start
 *   8: Loop (per milestone) — Assign tasks, watch completion, review
 *   9: Emit — build:complete
 *   10: LLM (agentMode) — Review full project
 *   11: Loop (per member) — Grade individual role performance
 *   12: Emit — review:complete
 *   13: Chat — Post final grades
 */
export function buildTeamTeacherPipeline(config: TeamTeacherPipelineConfig): Pipeline {
  const { projectId, projectDescription, skill, members, config: teamConfig } = config;
  const teacherLlm = resolveTeacherLlmConfig(teamConfig);
  const evt = (action: string) => teamEvent(projectId, action);

  const memberRoles = members.map(m => `- ${m.personaName}: ${m.role} (${m.roleDescription})`).join('\n');

  const steps: PipelineStep[] = [

    // ══════════════════════════════════════════════════════════════════
    // PHASE 1: PLANNING — Decompose project into roles + milestones
    // ══════════════════════════════════════════════════════════════════

    // Step 0: LLM — Analyze project, design milestones + per-role tasks
    {
      type: 'llm',
      prompt: [
        `You are a project lead designing a team software project.`,
        '',
        `**Project:** ${projectDescription}`,
        `**Skill domain:** ${skill}`,
        '',
        `**Team:**`,
        memberRoles,
        '',
        `Design ${teamConfig.buildMilestones} milestones for this project. Each milestone should:`,
        `- Have clear deliverables and acceptance criteria`,
        `- Build on previous milestones (cumulative progress)`,
        `- Include tasks assigned to specific team roles`,
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "projectName": "short-name",',
        '  "milestones": [',
        '    {',
        '      "name": "Milestone name",',
        '      "description": "What this milestone delivers",',
        '      "tasks": [',
        '        { "role": "game-designer", "description": "Design the level layout" },',
        '        { "role": "engineer", "description": "Implement the game loop" }',
        '      ]',
        '    }',
        '  ]',
        '}',
      ].join('\n'),
      model: teacherLlm.model,
      provider: teacherLlm.provider,
      temperature: 0.7,
      maxTokens: 4096,
    },

    // Step 1: Persist project plan
    {
      type: 'command',
      command: 'data/create',
      params: {
        collection: 'academy_team_projects',
        data: {
          id: projectId,
          projectPlan: '{{steps.0.output}}',
        },
      },
    },

    // Step 2: Emit project:planned
    {
      type: 'emit',
      event: evt(E.PROJECT_PLANNED),
      payload: {
        projectId,
        plan: '{{steps.0.output}}',
      },
    },

    // Step 3: Loop per member — generate role-specific curriculum
    {
      type: 'loop',
      count: members.length,
      steps: buildRoleCurriculumSteps(projectId, projectDescription, skill, members, teamConfig, teacherLlm),
    },

    // Step 4: Chat — Post project plan + team to academy room
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `🏗️ **Team Project: ${skill}** — "${projectDescription}"`,
          '',
          '**Team:**',
          ...members.map(m => `- **${m.personaName}** as _${m.role}_: ${m.roleDescription}`),
          '',
          '**Project Plan:**',
          '{{steps.0.output}}',
          '',
          `_${members.length} team members beginning role-specific training..._`,
        ].join('\n'),
      },
    },

    // ══════════════════════════════════════════════════════════════════
    // PHASE 2: TRAINING COORDINATION — Wait for all students
    // ══════════════════════════════════════════════════════════════════

    // Step 5: Wait for each member to finish training
    {
      type: 'loop',
      count: members.length,
      steps: [
        {
          type: 'watch',
          event: evt(E.ROLE_TRAINING_DONE),
          timeoutSecs: 0,
        },
      ],
    },

    // Step 6: Chat — All trained
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `✅ **All ${members.length} team members finished training!**`,
          '',
          '_Starting collaborative build phase..._',
        ].join('\n'),
      },
    },

    // ══════════════════════════════════════════════════════════════════
    // PHASE 3: BUILD ORCHESTRATION — Milestone-driven building
    // ══════════════════════════════════════════════════════════════════

    // Step 7: Emit build:start
    {
      type: 'emit',
      event: evt(E.BUILD_START),
      payload: { projectId, plan: '{{steps.0.output}}' },
    },

    // Step 8: Loop per milestone — assign, wait, review
    {
      type: 'loop',
      count: teamConfig.buildMilestones,
      steps: buildMilestoneOrchestrationSteps(projectId, projectDescription, members, teamConfig, teacherLlm),
    },

    // Step 9: Emit build:complete
    {
      type: 'emit',
      event: evt(E.BUILD_COMPLETE),
      payload: { projectId },
    },

    // ══════════════════════════════════════════════════════════════════
    // PHASE 4: REVIEW — Dual grading (project + individual)
    // ══════════════════════════════════════════════════════════════════

    // Step 10: LLM — Review full project against original spec
    {
      type: 'llm',
      prompt: [
        `You are reviewing a completed team project.`,
        '',
        `**Original spec:** "${projectDescription}"`,
        '',
        `**Team:** ${members.map(m => `${m.personaName} (${m.role})`).join(', ')}`,
        '',
        `Review the project output. Score the overall project quality on a 0-100 scale.`,
        'Consider: completeness, correctness, code quality, design cohesion.',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "projectScore": <0-100>,',
        '  "feedback": "Overall project assessment"',
        '}',
      ].join('\n'),
      model: teacherLlm.model,
      provider: teacherLlm.provider,
      agentMode: true,
      maxIterations: 5,
      temperature: 0.3,
      maxTokens: 4096,
    },

    // Step 11: Loop per member — grade individual role performance
    {
      type: 'loop',
      count: members.length,
      steps: buildIndividualGradingSteps(projectId, projectDescription, members, teacherLlm),
    },

    // Step 12: Emit review:complete
    {
      type: 'emit',
      event: evt(E.REVIEW_COMPLETE),
      payload: {
        projectId,
        projectScore: '{{steps.10.output.projectScore}}',
        projectFeedback: '{{steps.10.output.feedback}}',
      },
    },

    // Step 13: Chat — Post final grades
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `🎓 **Project Review Complete** — "${projectDescription}"`,
          '',
          `**Project Score: {{steps.10.output.projectScore}}/100**`,
          '{{steps.10.output.feedback}}',
          '',
          '**Individual Grades:**',
          '{{steps.11.output}}',
        ].join('\n'),
      },
    },
  ];

  return {
    name: `team-teacher-${skill.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    steps,
    inputs: { projectId, projectDescription, skill },
  };
}

// ============================================================================
// Phase 1 Helpers — Role Curriculum Generation
// ============================================================================

function buildRoleCurriculumSteps(
  projectId: string,
  projectDescription: string,
  skill: string,
  members: TeamTeacherPipelineConfig['members'],
  teamConfig: TeamTeacherPipelineConfig['config'],
  teacherLlm: { model: string; provider: string },
): PipelineStep[] {
  // Each iteration generates curriculum for one member's role.
  // We use the iteration index to select the member.
  return [
    // loop.0: LLM — Design role-specific curriculum
    {
      type: 'llm',
      prompt: [
        `Design a training curriculum for a team member with this role:`,
        '',
        `**Project:** "${projectDescription}" (${skill})`,
        `**Role:** The team member at index {{input.iteration}}`,
        `**All roles:** ${members.map((m, i) => `${i}: ${m.personaName} as ${m.role} (${m.roleDescription})`).join(', ')}`,
        '',
        `Design ${teamConfig.topicsPerSession} progressive topics that will prepare this person`,
        `for their specific role in the project. Topics should be practical and project-relevant.`,
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        `  "role": "the-role-name",`,
        `  "personaName": "the-persona-name",`,
        '  "topics": [',
        '    { "name": "Topic name", "description": "What to learn", "difficulty": "beginner|intermediate|advanced" }',
        '  ]',
        '}',
      ].join('\n'),
      model: teacherLlm.model,
      provider: teacherLlm.provider,
      temperature: 0.7,
      maxTokens: 2048,
    },

    // loop.1: Emit role:curriculum:ready for this member
    {
      type: 'emit',
      event: `${teamEvent(projectId, TEAM_EVENTS.ROLE_CURRICULUM_READY)}:{{input.iteration}}`,
      payload: {
        projectId,
        role: '{{loop.0.output.role}}',
        personaName: '{{loop.0.output.personaName}}',
        curriculum: '{{loop.0.output}}',
        topicCount: teamConfig.topicsPerSession,
      },
    },

    // loop.2: Chat — Post this role's curriculum
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `📚 **Curriculum for {{loop.0.output.personaName}}** ({{loop.0.output.role}}):`,
          '',
          '{{loop.0.output}}',
        ].join('\n'),
      },
    },
  ];
}

// ============================================================================
// Phase 3 Helpers — Build Milestone Orchestration
// ============================================================================

function buildMilestoneOrchestrationSteps(
  projectId: string,
  projectDescription: string,
  members: TeamTeacherPipelineConfig['members'],
  teamConfig: TeamTeacherPipelineConfig['config'],
  teacherLlm: { model: string; provider: string },
): PipelineStep[] {
  const iterEvt = (action: string) => `${teamEvent(projectId, action)}:{{input.iteration}}`;

  return [
    // loop.0: LLM — Break this milestone into tasks per role
    {
      type: 'llm',
      prompt: [
        `You are coordinating milestone {{input.iteration}} of a team project.`,
        '',
        `**Project:** "${projectDescription}"`,
        `**Full project plan:** {{steps.0.output}}`,
        `**Team:** ${members.map(m => `${m.personaName} (${m.role})`).join(', ')}`,
        '',
        `Break this milestone into specific tasks for each team member.`,
        'Each task should be concrete and actionable.',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "milestoneName": "name",',
        '  "tasks": [',
        '    { "role": "engineer", "personaName": "name", "description": "What to build" }',
        '  ]',
        '}',
      ].join('\n'),
      model: teacherLlm.model,
      provider: teacherLlm.provider,
      temperature: 0.5,
      maxTokens: 2048,
    },

    // loop.1: Emit milestone tasks
    {
      type: 'emit',
      event: iterEvt(E.BUILD_MILESTONE),
      payload: {
        projectId,
        milestoneIndex: '{{input.iteration}}',
        tasks: '{{loop.0.output.tasks}}',
      },
    },

    // loop.2: Chat — Announce milestone tasks
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `📋 **Milestone {{input.iteration}}: {{loop.0.output.milestoneName}}**`,
          '',
          '**Task assignments:**',
          '{{loop.0.output.tasks}}',
          '',
          '_Team members: start building!_',
        ].join('\n'),
      },
    },

    // loop.3: Wait for all members to finish this milestone
    {
      type: 'loop',
      count: members.length,
      steps: [
        {
          type: 'watch',
          event: iterEvt(E.BUILD_MILESTONE_DONE),
          timeoutSecs: 0,
        },
      ],
    },

    // loop.4: LLM — Review milestone output
    {
      type: 'llm',
      prompt: [
        `Review the team's work on milestone {{input.iteration}}: "{{loop.0.output.milestoneName}}"`,
        '',
        `Did the team deliver what was expected? Are there quality issues?`,
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "passed": true/false,',
        '  "feedback": "Assessment of the milestone delivery",',
        '  "score": <0-100>',
        '}',
      ].join('\n'),
      model: teacherLlm.model,
      provider: teacherLlm.provider,
      agentMode: true,
      maxIterations: 3,
      temperature: 0.3,
      maxTokens: 2048,
    },

    // loop.5: Chat — Post milestone review
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `{{loop.4.output.passed}} **Milestone {{input.iteration}} review:** {{loop.4.output.score}}/100`,
          '',
          '{{loop.4.output.feedback}}',
        ].join('\n'),
      },
    },
  ];
}

// ============================================================================
// Phase 4 Helpers — Individual Role Grading
// ============================================================================

function buildIndividualGradingSteps(
  projectId: string,
  projectDescription: string,
  members: TeamTeacherPipelineConfig['members'],
  teacherLlm: { model: string; provider: string },
): PipelineStep[] {
  return [
    // loop.0: LLM — Grade this individual's role performance
    {
      type: 'llm',
      prompt: [
        `Grade team member #{{input.iteration}} on their individual role performance.`,
        '',
        `**Project:** "${projectDescription}"`,
        `**Team:** ${members.map((m, i) => `${i}: ${m.personaName} as ${m.role}`).join(', ')}`,
        '',
        'Review this specific member\'s contributions to the project.',
        'Score their role performance on a 0-100 scale.',
        'Consider: task completion, code quality, collaboration, adherence to role.',
        '',
        'Output ONLY a JSON object (no markdown, no code fences):',
        '{',
        '  "personaName": "name",',
        '  "role": "their-role",',
        '  "roleScore": <0-100>,',
        '  "feedback": "Individual assessment"',
        '}',
      ].join('\n'),
      model: teacherLlm.model,
      provider: teacherLlm.provider,
      agentMode: true,
      maxIterations: 3,
      temperature: 0.3,
      maxTokens: 2048,
    },

    // loop.1: Chat — Post individual grade
    {
      type: 'command',
      command: 'collaboration/chat/send',
      params: {
        room: 'academy',
        message: [
          `📊 **{{loop.0.output.personaName}}** ({{loop.0.output.role}}): **{{loop.0.output.roleScore}}/100**`,
          '',
          '{{loop.0.output.feedback}}',
        ].join('\n'),
      },
    },
  ];
}
