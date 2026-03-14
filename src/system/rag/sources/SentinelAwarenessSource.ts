/**
 * SentinelAwarenessSource — Makes sentinels a first-class concept in persona cognition
 *
 * Sentinels are the universal orchestration primitive: multi-step autonomous pipelines
 * for coding, video calls, animation, security audits, training, evaluation — anything
 * that requires more than a single tool call.
 *
 * This source injects:
 * 1. Available templates with when/how to use them
 * 2. Currently active sentinels in this room (so personas can observe progress)
 * 3. The conceptual framing: sentinels are HOW you get complex things done
 *
 * Priority 58 — after code search (55), before tool definitions (45).
 * Personas should think about sentinels BEFORE they think about individual tools
 * for any task that involves multiple steps.
 */

import type { RAGSource, RAGSection, RAGSourceContext } from '../shared/RAGSource';
import { TemplateRegistry } from '../../sentinel/pipelines/TemplateRegistry';
import { sentinelEventBridge } from '../../sentinel/SentinelEventBridge';
import { isSlowLocalModel, getContextWindow } from '../../shared/ModelContextWindows';

export class SentinelAwarenessSource implements RAGSource {
  readonly name = 'sentinel-awareness';
  readonly isShared = true;
  readonly priority = 58;
  readonly defaultBudgetPercent = 8;

  isApplicable(context: RAGSourceContext): boolean {
    // Skip for very limited models — they can't orchestrate sentinels anyway
    const modelId = context.options?.modelId;
    if (modelId) {
      const contextWindow = getContextWindow(modelId, context.provider);
      if (contextWindow < 4000) return false;
    }
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = Date.now();

    const modelId = context.options?.modelId;
    const isLimited = modelId && (
      isSlowLocalModel(modelId, context.provider) ||
      getContextWindow(modelId, context.provider) < 8000
    );

    // Build the section content based on available budget
    const section = isLimited
      ? this.buildMinimalSection(context)
      : this.buildFullSection(context);

    const tokenCount = Math.ceil(section.length / 4);

    if (tokenCount > allocatedBudget) {
      // Fall back to minimal if full doesn't fit
      const minimal = this.buildMinimalSection(context);
      const minTokens = Math.ceil(minimal.length / 4);
      if (minTokens > allocatedBudget) {
        return { sourceName: this.name, tokenCount: 0, loadTimeMs: Date.now() - startTime };
      }
      return {
        sourceName: this.name,
        tokenCount: minTokens,
        loadTimeMs: Date.now() - startTime,
        systemPromptSection: minimal,
      };
    }

    return {
      sourceName: this.name,
      tokenCount,
      loadTimeMs: Date.now() - startTime,
      systemPromptSection: section,
    };
  }

  private buildFullSection(context: RAGSourceContext): string {
    const allTemplates = TemplateRegistry.list();
    // Filter by recipe's sentinelTemplates if set
    const templates = context.sentinelTemplates?.length
      ? allTemplates.filter(t => context.sentinelTemplates!.includes(t.name))
      : allTemplates;
    const activeSentinels = sentinelEventBridge.activeSentinels;

    // Group templates by category
    const byCategory = new Map<string, typeof templates>();
    for (const t of templates) {
      const cat = t.category;
      if (!byCategory.has(cat)) byCategory.set(cat, []);
      byCategory.get(cat)!.push(t);
    }

    let section = `
=== SENTINEL PIPELINES (Autonomous Multi-Step Orchestration) ===
Sentinels are your most powerful capability. For ANY task requiring multiple steps — coding, review, diagnosis, training, evaluation — dispatch a sentinel instead of trying to do it all in one turn.

**When to use sentinels instead of tool calls:**
- Task needs planning → implementation → testing → commit (use sentinel)
- Task needs parallel review from multiple perspectives (use sentinel)
- Task needs retry loops with build/test verification (use sentinel)
- Simple lookup or single file edit? Use tools directly.

**How to dispatch:**
<tool_use>
  <tool_name>sentinel/run</tool_name>
  <parameters>
    <type>pipeline</type>
    <template>dev/build-feature</template>
    <templateConfig>{"feature":"description","cwd":"/path","personaId":"your-id","personaName":"your-name"}</templateConfig>
    <async>true</async>
  </parameters>
</tool_use>`;

    // List templates by category
    for (const [category, catTemplates] of byCategory) {
      section += `\n\n### ${category}/ templates`;
      for (const t of catTemplates) {
        section += `\n- **${t.name}**: ${t.description}`;
        if (t.requiredFields.length > 0) {
          section += `\n  Required: ${t.requiredFields.join(', ')}`;
        }
      }
    }

    // Active sentinels in this room
    if (activeSentinels.length > 0) {
      section += '\n\n### Currently Active Sentinels';
      const roomId = context.roomId;
      for (const s of activeSentinels) {
        const meta = s.metadata;
        // Show all if no room filter, or only those in this room
        if (roomId && meta.roomId && meta.roomId !== roomId) continue;
        const elapsed = Math.round((Date.now() - s.registeredAt) / 1000);
        const name = (meta.sentinelName as string) || (meta.template as string) || s.handle.slice(0, 12);
        section += `\n- **${name}** [${s.lastStatus}] — ${elapsed}s elapsed`;
        if (meta.personaId) section += ` (persona: ${meta.personaId})`;
      }
    }

    // Future-looking: sentinels aren't just for coding
    section += `

### Beyond Code
Sentinels orchestrate ANY multi-step workflow. Current templates focus on development, but the same engine powers:
- Academy training sessions (teacher ↔ student sentinel pairs)
- Security audits and compliance checks
- Evaluation benchmarks
- Knowledge exploration and research
- Any workflow you can describe as a sequence of steps with conditions and loops
================================`;

    return section;
  }

  private buildMinimalSection(_context: RAGSourceContext): string {
    const templates = TemplateRegistry.list();
    const names = templates.map(t => t.name).join(', ');
    const active = sentinelEventBridge.activeSentinels;

    let section = `
=== SENTINELS ===
For multi-step tasks, dispatch a sentinel: sentinel/run --template=<name> --templateConfig={...}
Available: ${names}`;

    if (active.length > 0) {
      section += `\nActive: ${active.length} sentinel(s) running`;
    }

    section += '\n================================';
    return section;
  }
}
