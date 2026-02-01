/**
 * RAG Sources - Pluggable data sources for RAG context building
 *
 * Each source is responsible for one type of context data.
 * Sources are registered with RAGComposer and loaded in parallel.
 *
 * Usage:
 * ```typescript
 * import { getRAGComposer } from '../shared/RAGComposer';
 * import { ConversationHistorySource, SemanticMemorySource, ... } from './sources';
 *
 * const composer = getRAGComposer();
 * composer.registerAll([
 *   new ConversationHistorySource(),
 *   new SemanticMemorySource(),
 *   new WidgetContextSource(),
 *   new PersonaIdentitySource()
 * ]);
 *
 * const result = await composer.compose(context);
 * ```
 */

export { ConversationHistorySource } from './ConversationHistorySource';
export { SemanticMemorySource } from './SemanticMemorySource';
export { WidgetContextSource } from './WidgetContextSource';
export { PersonaIdentitySource } from './PersonaIdentitySource';
export { GlobalAwarenessSource, registerConsciousness, unregisterConsciousness, getConsciousness } from './GlobalAwarenessSource';
export { VoiceConversationSource, registerVoiceOrchestrator, unregisterVoiceOrchestrator } from './VoiceConversationSource';
export { SocialMediaRAGSource } from './SocialMediaRAGSource';
export { CodeToolSource } from './CodeToolSource';

// Re-export types for convenience
export type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
