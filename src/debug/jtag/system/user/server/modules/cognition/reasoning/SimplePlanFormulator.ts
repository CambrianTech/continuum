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

    return ['Task completed without errors'];
  }

  private generateId(): UUID {
    return `plan-${Date.now()}-${Math.random().toString(36).slice(2, 11)}` as UUID;
  }
}
