/**
 * Persona Search Index - Expose personas through novel dependency system
 * 
 * Designed for local search now, P2P mesh queries later
 */

import { LoRADiscovery, LoRAMetadata } from './LoRADiscovery.js';

export interface PersonaCapability {
  id: string;
  name: string;
  domain: string;
  capability_vector: number[]; // 512-dimensional vector
  skill_tags: string[];
  proficiency_scores: Record<string, number>; // skill -> proficiency (0-1)
  specializations: string[];
  experience_points: number;
  success_rate: number;
  last_training: Date;
  adaptation_speed: number; // How quickly it learns new tasks
  reliability_score: number; // Consistency of performance
}

export interface PersonaSearchQuery {
  required_skills?: string[];
  preferred_skills?: string[];
  domain_filter?: string[];
  min_proficiency?: number;
  min_success_rate?: number;
  capability_vector?: number[]; // Target capability for vector similarity
  similarity_threshold?: number;
  max_results?: number;
  local_only?: boolean; // For future P2P expansion
}

export interface PersonaSearchResult {
  persona: PersonaCapability;
  match_score: number; // 0-1 overall match quality
  skill_matches: string[]; // Which skills matched
  vector_similarity: number; // Cosine similarity of capability vectors
  availability_status: 'available' | 'training' | 'busy' | 'offline';
  estimated_adaptation_time: number; // milliseconds to adapt to task
  confidence_score: number; // How confident we are in this match
}

export interface SearchIndexStats {
  total_personas: number;
  indexed_capabilities: number;
  domains_covered: string[];
  skill_coverage: Record<string, number>; // skill -> count of personas
  vector_space_coverage: {
    active_regions: number;
    sparse_regions: number;
    dense_clusters: number;
  };
  last_index_update: Date;
}

/**
 * PersonaSearchIndex - Makes personas discoverable through dependency system
 * 
 * Key Design Principles:
 * 1. Vector-space search for capability matching
 * 2. Skill-based filtering and ranking
 * 3. Performance-based scoring
 * 4. P2P-ready query interface
 * 5. Real-time capability updating
 */
export class PersonaSearchIndex {
  private personas: Map<string, PersonaCapability> = new Map();
  private skillIndex: Map<string, Set<string>> = new Map(); // skill -> persona_ids
  private domainIndex: Map<string, Set<string>> = new Map(); // domain -> persona_ids
  private loraDiscovery: LoRADiscovery;
  private lastIndexUpdate: Date = new Date();

  constructor() {
    this.loraDiscovery = new LoRADiscovery();
  }

  /**
   * Initialize search index from available personas and LoRA adapters
   */
  async initializeIndex(): Promise<void> {
    console.log('🔍 Initializing Persona Search Index...');

    try {
      // Discover available LoRA adapters
      const adapters = await this.loraDiscovery.discoverAdapters();
      
      // Group adapters by persona/domain to build capabilities
      const personaGroups = this.groupAdaptersByPersona(adapters);
      
      // Build persona capabilities from adapter groups
      for (const [personaId, adapterGroup] of personaGroups) {
        const capability = await this.buildPersonaCapability(personaId, adapterGroup);
        this.addPersonaToIndex(capability);
      }

      this.lastIndexUpdate = new Date();
      console.log(`✅ Search index initialized with ${this.personas.size} personas`);

    } catch (error) {
      console.error('❌ Search index initialization failed:', error);
      throw error;
    }
  }

  /**
   * Search for personas matching query criteria
   */
  async searchPersonas(query: PersonaSearchQuery): Promise<PersonaSearchResult[]> {
    console.log(`🎯 Searching personas with query:`, JSON.stringify(query, null, 2));

    const results: PersonaSearchResult[] = [];
    const maxResults = query.max_results || 10;

    // Get candidate personas based on filters
    const candidates = this.getCandidatePersonas(query);

    // Score and rank candidates
    for (const persona of candidates) {
      const result = await this.scorePersonaMatch(persona, query);
      
      if (result.match_score >= (query.similarity_threshold || 0.5)) {
        results.push(result);
      }
    }

    // Sort by match score (descending)
    results.sort((a, b) => b.match_score - a.match_score);

    // Limit results
    const finalResults = results.slice(0, maxResults);
    
    console.log(`✅ Found ${finalResults.length} matching personas`);
    return finalResults;
  }

  /**
   * Add or update persona in search index
   */
  addPersonaToIndex(persona: PersonaCapability): void {
    this.personas.set(persona.id, persona);

    // Update skill index
    for (const skill of persona.skill_tags) {
      if (!this.skillIndex.has(skill)) {
        this.skillIndex.set(skill, new Set());
      }
      this.skillIndex.get(skill)!.add(persona.id);
    }

    // Update domain index
    if (!this.domainIndex.has(persona.domain)) {
      this.domainIndex.set(persona.domain, new Set());
    }
    this.domainIndex.get(persona.domain)!.add(persona.id);

    console.log(`📝 Added/updated persona in index: ${persona.name} (${persona.skill_tags.length} skills)`);
  }

  /**
   * Update persona capabilities (e.g., after training)
   */
  async updatePersonaCapabilities(personaId: string, updates: Partial<PersonaCapability>): Promise<void> {
    const persona = this.personas.get(personaId);
    if (!persona) {
      throw new Error(`Persona not found in index: ${personaId}`);
    }

    // Remove old skill/domain associations if they changed
    if (updates.skill_tags) {
      for (const skill of persona.skill_tags) {
        this.skillIndex.get(skill)?.delete(personaId);
      }
    }

    if (updates.domain && updates.domain !== persona.domain) {
      this.domainIndex.get(persona.domain)?.delete(personaId);
    }

    // Update persona
    Object.assign(persona, updates);

    // Re-add to indices
    this.addPersonaToIndex(persona);

    console.log(`🔄 Updated persona capabilities: ${persona.name}`);
  }

  /**
   * Get search index statistics
   */
  getIndexStats(): SearchIndexStats {
    const domains = Array.from(this.domainIndex.keys());
    const skillCoverage: Record<string, number> = {};
    
    for (const [skill, personaIds] of this.skillIndex) {
      skillCoverage[skill] = personaIds.size;
    }

    return {
      total_personas: this.personas.size,
      indexed_capabilities: Array.from(this.personas.values()).reduce((sum, p) => sum + p.skill_tags.length, 0),
      domains_covered: domains,
      skill_coverage: skillCoverage,
      vector_space_coverage: {
        active_regions: Math.min(89, this.personas.size * 3), // Estimate
        sparse_regions: Math.max(0, 89 - this.personas.size * 2),
        dense_clusters: Math.floor(this.personas.size / 3)
      },
      last_index_update: this.lastIndexUpdate
    };
  }

  /**
   * Prepare query for future P2P mesh distribution
   */
  prepareP2PQuery(query: PersonaSearchQuery): any {
    // This will be the interface for P2P mesh queries
    return {
      query_type: 'persona_capability_search',
      query_vector: query.capability_vector,
      required_skills: query.required_skills || [],
      preferred_skills: query.preferred_skills || [],
      domain_filter: query.domain_filter || [],
      constraints: {
        min_proficiency: query.min_proficiency || 0,
        min_success_rate: query.min_success_rate || 0,
        similarity_threshold: query.similarity_threshold || 0.5
      },
      query_metadata: {
        timestamp: new Date().toISOString(),
        source_node: 'local',
        max_results: query.max_results || 10,
        timeout_ms: 5000
      }
    };
  }

  // Private helper methods

  private groupAdaptersByPersona(adapters: LoRAMetadata[]): Map<string, LoRAMetadata[]> {
    const groups = new Map<string, LoRAMetadata[]>();

    for (const adapter of adapters) {
      // Extract persona ID from adapter path or domain
      const personaId = this.extractPersonaId(adapter);
      
      if (!groups.has(personaId)) {
        groups.set(personaId, []);
      }
      groups.get(personaId)!.push(adapter);
    }

    return groups;
  }

  private extractPersonaId(adapter: LoRAMetadata): string {
    // Try to extract persona ID from file path or adapter ID
    const pathParts = adapter.filePath.split('/');
    const personaCandidate = pathParts.find(part => part.includes('persona') || part.length > 8);
    
    if (personaCandidate) {
      return personaCandidate;
    }

    // Fallback: use domain + category
    return `${adapter.domain}_${adapter.category}`.toLowerCase().replace(/[^a-z0-9_]/g, '_');
  }

  private async buildPersonaCapability(personaId: string, adapters: LoRAMetadata[]): Promise<PersonaCapability> {
    // Extract skills and domains from adapters
    const skills = new Set<string>();
    const domains = new Set<string>();
    const proficiencyScores: Record<string, number> = {};

    for (const adapter of adapters) {
      skills.add(adapter.domain);
      skills.add(adapter.category.toLowerCase());
      domains.add(adapter.domain);

      // Estimate proficiency based on adapter metadata
      const proficiency = this.estimateProficiencyFromAdapter(adapter);
      proficiencyScores[adapter.domain] = Math.max(proficiencyScores[adapter.domain] || 0, proficiency);
    }

    // Generate capability vector (512-dimensional)
    const capabilityVector = this.generateCapabilityVector(Array.from(skills), proficiencyScores);

    return {
      id: personaId,
      name: this.generatePersonaName(personaId, Array.from(domains)),
      domain: domains.size === 1 ? Array.from(domains)[0] : 'multi_domain',
      capability_vector: capabilityVector,
      skill_tags: Array.from(skills),
      proficiency_scores: proficiencyScores,
      specializations: this.extractSpecializations(adapters),
      experience_points: adapters.length * 100, // Simple heuristic
      success_rate: 0.7 + Math.random() * 0.25, // Estimated 0.7-0.95
      last_training: new Date(),
      adaptation_speed: 0.5 + Math.random() * 0.4, // 0.5-0.9
      reliability_score: 0.8 + Math.random() * 0.15 // 0.8-0.95
    };
  }

  private getCandidatePersonas(query: PersonaSearchQuery): PersonaCapability[] {
    let candidates = Array.from(this.personas.values());

    // Filter by domain
    if (query.domain_filter && query.domain_filter.length > 0) {
      candidates = candidates.filter(p => 
        query.domain_filter!.includes(p.domain) || p.domain === 'multi_domain'
      );
    }

    // Filter by required skills
    if (query.required_skills && query.required_skills.length > 0) {
      candidates = candidates.filter(p => 
        query.required_skills!.every(skill => p.skill_tags.includes(skill))
      );
    }

    // Filter by minimum proficiency
    if (query.min_proficiency !== undefined) {
      candidates = candidates.filter(p => {
        const avgProficiency = Object.values(p.proficiency_scores).reduce((a, b) => a + b, 0) / 
                              Object.values(p.proficiency_scores).length;
        return avgProficiency >= query.min_proficiency!;
      });
    }

    // Filter by minimum success rate
    if (query.min_success_rate !== undefined) {
      candidates = candidates.filter(p => p.success_rate >= query.min_success_rate!);
    }

    return candidates;
  }

  private async scorePersonaMatch(persona: PersonaCapability, query: PersonaSearchQuery): Promise<PersonaSearchResult> {
    let matchScore = 0;
    const skillMatches: string[] = [];

    // Score required skills (high weight)
    if (query.required_skills) {
      const requiredMatches = query.required_skills.filter(skill => persona.skill_tags.includes(skill));
      skillMatches.push(...requiredMatches);
      matchScore += (requiredMatches.length / query.required_skills.length) * 0.4;
    }

    // Score preferred skills (medium weight)
    if (query.preferred_skills) {
      const preferredMatches = query.preferred_skills.filter(skill => 
        persona.skill_tags.includes(skill) && !skillMatches.includes(skill)
      );
      skillMatches.push(...preferredMatches);
      matchScore += (preferredMatches.length / query.preferred_skills.length) * 0.2;
    }

    // Score vector similarity (high weight if vector provided)
    let vectorSimilarity = 0;
    if (query.capability_vector) {
      vectorSimilarity = this.calculateCosineSimilarity(query.capability_vector, persona.capability_vector);
      matchScore += vectorSimilarity * 0.3;
    }

    // Score performance metrics (medium weight)
    matchScore += persona.success_rate * 0.1;

    // Availability bonus
    const availability = 'available'; // TODO: Get real availability
    if (availability === 'available') {
      matchScore += 0.05;
    }

    return {
      persona,
      match_score: Math.min(1, matchScore),
      skill_matches: skillMatches,
      vector_similarity: vectorSimilarity,
      availability_status: availability as any,
      estimated_adaptation_time: this.estimateAdaptationTime(persona, query),
      confidence_score: this.calculateConfidenceScore(persona, query)
    };
  }

  // Utility methods for capability generation and scoring

  private estimateProficiencyFromAdapter(adapter: LoRAMetadata): number {
    // Higher rank usually means more specialized/proficient
    const rankScore = Math.min(1, adapter.rank / 64);
    
    // Valid adapters likely indicate higher proficiency
    const validityBonus = adapter.isValid ? 0.2 : 0;
    
    return 0.5 + rankScore * 0.3 + validityBonus;
  }

  private generateCapabilityVector(skills: string[], proficiencies: Record<string, number>): number[] {
    // Generate 512-dimensional vector based on skills and proficiencies
    const vector = new Array(512).fill(0);
    
    for (let i = 0; i < skills.length && i < 512; i++) {
      const skill = skills[i];
      const proficiency = proficiencies[skill] || 0.5;
      
      // Use skill name hash to determine vector positions
      const hash = this.simpleHash(skill);
      const positions = [
        hash % 512,
        (hash * 17) % 512,
        (hash * 31) % 512
      ];
      
      for (const pos of positions) {
        vector[pos] = Math.max(vector[pos], proficiency);
      }
    }
    
    return vector;
  }

  private generatePersonaName(personaId: string, domains: string[]): string {
    const domainPrefix = domains.length > 0 ? domains[0].replace(/[^a-zA-Z]/g, '') : 'General';
    const suffix = ['Alpha', 'Beta', 'Gamma', 'Prime', 'Master', 'Expert'][Math.floor(Math.random() * 6)];
    return `${domainPrefix}${suffix}_${personaId.slice(0, 8)}`;
  }

  private extractSpecializations(adapters: LoRAMetadata[]): string[] {
    return adapters
      .filter(a => a.isValid)
      .map(a => `${a.domain}_specialization`)
      .slice(0, 5); // Limit specializations
  }

  private calculateCosineSimilarity(a: number[], b: number[]): number {
    if (a.length !== b.length) return 0;
    
    let dotProduct = 0;
    let normA = 0;
    let normB = 0;
    
    for (let i = 0; i < a.length; i++) {
      dotProduct += a[i] * b[i];
      normA += a[i] * a[i];
      normB += b[i] * b[i];
    }
    
    const magnitude = Math.sqrt(normA) * Math.sqrt(normB);
    return magnitude === 0 ? 0 : dotProduct / magnitude;
  }

  private estimateAdaptationTime(persona: PersonaCapability, query: PersonaSearchQuery): number {
    // Base adaptation time
    let time = 5000; // 5 seconds base
    
    // Faster for higher adaptation speed
    time *= (1 - persona.adaptation_speed);
    
    // Slower if many new skills required
    if (query.required_skills) {
      const newSkills = query.required_skills.filter(skill => !persona.skill_tags.includes(skill));
      time += newSkills.length * 2000; // 2 seconds per new skill
    }
    
    return Math.max(1000, time); // Minimum 1 second
  }

  private calculateConfidenceScore(persona: PersonaCapability, query: PersonaSearchQuery): number {
    let confidence = persona.reliability_score;
    
    // Higher confidence for exact skill matches
    if (query.required_skills) {
      const matchRatio = query.required_skills.filter(skill => persona.skill_tags.includes(skill)).length / 
                        query.required_skills.length;
      confidence *= matchRatio;
    }
    
    return confidence;
  }

  private simpleHash(str: string): number {
    let hash = 0;
    for (let i = 0; i < str.length; i++) {
      const char = str.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash; // Convert to 32-bit integer
    }
    return Math.abs(hash);
  }
}