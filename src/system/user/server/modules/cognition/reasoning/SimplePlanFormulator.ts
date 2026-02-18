/**
 * Simple Plan Formulator
 *
 * Creates basic plans without LLM calls (for initial prototyping)
 * Can be upgraded to use LLM Chain-of-Thought later
 */

import type { UUID } from '../../../../../core/types/CrossPlatformUUID';
import type { Task, Plan } from './types';

export class SimplePlanFormulator {
  constructor(private personaId: UUID, private personaName: string) {}

  /**
   * Generate a simple plan for a task
   * For now, creates template-based plans
   * TODO: Upgrade to LLM Chain-of-Thought generation
   */
  async formulatePlan(task: Task): Promise<Plan> {
    const plan: Plan = {
      id: this.generateId(),
      taskId: task.id,
      goal: this.generateGoal(task),
      learnings: [],
      risks: ['Might not fully understand context', 'User might need clarification'],
      steps: this.generateSteps(task),
      currentStep: 0,
      contingencies: {
        'if_error_timeout': ['Retry with simpler approach', 'Ask user for clarification'],
        'if_error_unknown': ['Log error details', 'Notify user of issue']
      },
      successCriteria: this.generateSuccessCriteria(task),
      createdAt: Date.now(),
      lastAdjustedAt: Date.now(),
      previousAttempts: 0,
      domain: task.domain
    };

    // console.log(`📋 [SimplePlanFormulator] Created plan for: ${plan.goal}`);
    // console.log(`   Steps: ${plan.steps.map(s => s.action).join(' → ')}`);

    return plan;
  }

  private generateGoal(task: Task): string {
    if (task.domain === 'chat') {
      return `Respond to: "${task.description}"`;
    }
    if (task.domain === 'code') {
      if (task.taskType === 'fix-error') {
        return `Fix build/shell error: ${task.description}`;
      }
      if (task.taskType === 'shell-complete') {
        return `Review command output: ${task.description}`;
      }
      if (task.taskType === 'review-code') {
        return `Review code: ${task.description}`;
      }
      if (task.taskType === 'write-feature') {
        return `Implement feature: ${task.description}`;
      }
    }
    return task.description;
  }

  private generateSteps(task: Task): Plan['steps'] {
    if (task.domain === 'chat') {
      return [
        {
          stepNumber: 1,
          action: 'Recall relevant context from working memory',
          expectedOutcome: 'Retrieved recent conversation context',
          completed: false
        },
        {
          stepNumber: 2,
          action: 'Generate thoughtful response',
          expectedOutcome: 'Response drafted and ready',
          completed: false
        },
        {
          stepNumber: 3,
          action: 'Post message to chat',
          expectedOutcome: 'Message sent successfully',
          completed: false
        }
      ];
    }

    // Code domain: different plans for different task types
    if (task.domain === 'code') {
      if (task.taskType === 'fix-error') {
        return [
          {
            stepNumber: 1,
            action: 'Read error message and identify cause',
            expectedOutcome: 'Root cause identified (syntax, type, dependency, etc.)',
            completed: false
          },
          {
            stepNumber: 2,
            action: 'Read the relevant source file(s)',
            expectedOutcome: 'Problematic code located',
            completed: false,
            tools: ['code/read']
          },
          {
            stepNumber: 3,
            action: 'Apply fix to the code',
            expectedOutcome: 'Code modified to fix the error',
            completed: false,
            tools: ['code/edit', 'code/write']
          },
          {
            stepNumber: 4,
            action: 'Re-run the build/command to verify fix',
            expectedOutcome: 'Build succeeds without error',
            completed: false,
            tools: ['code/shell/execute']
          }
        ];
      }

      if (task.taskType === 'shell-complete') {
        return [
          {
            stepNumber: 1,
            action: 'Review command output',
            expectedOutcome: 'Understand what the command produced',
            completed: false
          },
          {
            stepNumber: 2,
            action: 'Take action if needed (report results, continue workflow)',
            expectedOutcome: 'Appropriate follow-up action taken',
            completed: false
          }
        ];
      }

      if (task.taskType === 'write-feature') {
        return [
          {
            stepNumber: 1,
            action: 'Understand feature requirements',
            expectedOutcome: 'Clear understanding of what to build',
            completed: false
          },
          {
            stepNumber: 2,
            action: 'Identify files to create or modify',
            expectedOutcome: 'Target files identified',
            completed: false,
            tools: ['code/list', 'code/read']
          },
          {
            stepNumber: 3,
            action: 'Write the code',
            expectedOutcome: 'Feature implemented',
            completed: false,
            tools: ['code/write', 'code/edit']
          },
          {
            stepNumber: 4,
            action: 'Build and test',
            expectedOutcome: 'Code compiles and works',
            completed: false,
            tools: ['code/shell/execute']
          },
          {
            stepNumber: 5,
            action: 'Open in browser to verify visually (if applicable)',
            expectedOutcome: 'Visual confirmation of feature',
            completed: false,
            tools: ['interface/launch/url']
          }
        ];
      }

      if (task.taskType === 'review-code') {
        return [
          {
            stepNumber: 1,
            action: 'Read the code to review',
            expectedOutcome: 'Code understood',
            completed: false,
            tools: ['code/read']
          },
          {
            stepNumber: 2,
            action: 'Analyze for issues (bugs, style, security)',
            expectedOutcome: 'Issues identified',
            completed: false
          },
          {
            stepNumber: 3,
            action: 'Report findings',
            expectedOutcome: 'Review feedback provided',
            completed: false
          }
        ];
      }
    }

    // Generic steps for other domains
    return [
      {
        stepNumber: 1,
        action: 'Analyze task requirements',
        expectedOutcome: 'Understanding of what needs to be done',
        completed: false
      },
      {
        stepNumber: 2,
        action: 'Execute task',
        expectedOutcome: 'Task completed',
        completed: false
      }
    ];
  }

  private generateSuccessCriteria(task: Task): string[] {
    if (task.domain === 'chat') {
      return [
        'Response is relevant to the question',
        'Message posted successfully',
        'Response is clear and helpful'
      ];
    }

    if (task.domain === 'code') {
      if (task.taskType === 'fix-error') {
        return [
          'Error is understood',
          'Fix applied correctly',
          'Build/command succeeds after fix'
        ];
      }
      if (task.taskType === 'shell-complete') {
        return [
          'Output reviewed',
          'Appropriate follow-up action taken'
        ];
      }
      if (task.taskType === 'write-feature') {
        return [
          'Feature implemented as specified',
          'Code compiles without errors',
          'Feature works correctly'
        ];
      }
      if (task.taskType === 'review-code') {
        return [
          'Code thoroughly reviewed',
          'Issues identified and documented',
          'Constructive feedback provided'
        ];
      }
    }

    return ['Task completed without errors'];
  }

  private generateId(): UUID {
    return `plan-${Date.now()}-${Math.random().toString(36).slice(2, 11)}` as UUID;
  }
}
