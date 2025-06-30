/**
 * Training Resource Tracker - Track datasets, prompts, and raw training resources
 * 
 * The REAL sources of persona capabilities - not just the final LoRA layers
 */

export interface TrainingResource {
  id: string;
  type: 'dataset' | 'prompt' | 'conversation_log' | 'code_repository' | 'document_collection' | 'interaction_history';
  source_description: string;           // Natural language description of what this is
  original_prompt?: string;             // The prompt that created/guided this training
  data_summary: DataSummary;            // What's actually in this resource
  provenance: ResourceProvenance;       // Where did this come from
  influence_score: number;              // 0-1 how much this shaped the persona
  extraction_metadata: ExtractionMetadata;
  storage_location: string;             // File path or reference
  checksum: string;                     // Integrity verification
}

export interface DataSummary {
  total_size_bytes: number;
  content_type: string;                 // 'text', 'code', 'conversation', 'mixed'
  language_distribution: Record<string, number>; // Programming languages or human languages
  domain_coverage: DomainCoverage[];
  concept_density: ConceptDensity[];    // Key concepts and their frequency
  interaction_patterns: InteractionPattern[]; // For conversation data
  knowledge_depth: KnowledgeDepthMap;   // How deep the knowledge goes in each area
}

export interface ResourceProvenance {
  original_source: string;              // URL, book title, dataset name, etc.
  collection_method: string;            // How this was gathered
  collection_date: Date;
  curator: string;                      // Who/what selected this data
  selection_criteria: string;           // Why this was chosen
  preprocessing_steps: string[];        // What was done to prepare it
  quality_score: number;                // 0-1 assessed quality
}

export interface ExtractionMetadata {
  extraction_prompt: string;            // Exact prompt used during training
  extraction_context: string;           // Context around why this was needed
  target_capabilities: string[];        // What capabilities this was meant to develop
  training_strategy: string;            // How this was used in training
  success_metrics: SuccessMetric[];     // How we measured if it worked
  actual_outcomes: string[];            // What actually emerged from this training
}

export interface DomainCoverage {
  domain: string;
  coverage_percentage: number;          // 0-100
  depth_level: number;                  // 1-10
  key_topics: string[];
  expert_level_content: boolean;
}

export interface ConceptDensity {
  concept: string;
  frequency: number;                    // How often it appears
  context_diversity: number;            // How many different contexts
  complexity_level: number;             // 1-10
  connection_count: number;             // How connected to other concepts
}

export interface InteractionPattern {
  pattern_type: string;                 // 'question_answer', 'code_review', 'debugging', etc.
  frequency: number;
  quality_score: number;
  context_complexity: number;
}

export interface KnowledgeDepthMap {
  domain: string;
  surface_knowledge: number;            // 0-1
  applied_knowledge: number;            // 0-1  
  expert_knowledge: number;             // 0-1
  creative_knowledge: number;           // 0-1
  teaching_knowledge: number;           // 0-1
}

export interface SuccessMetric {
  metric_name: string;
  target_value: number;
  actual_value: number;
  measurement_method: string;
}

export interface ResourceQueryResult {
  resources: TrainingResource[];
  synthesis_potential: SynthesisPotential;
  gap_analysis: ResourceGap[];
  composition_suggestions: CompositionSuggestion[];
}

export interface SynthesisPotential {
  combinable_resources: CombinableResource[];
  novel_synthesis_opportunities: NovelSynthesis[];
  estimated_capability_gain: number;
}

export interface CombinableResource {
  resource_ids: string[];
  combination_strategy: string;
  expected_synergy: number;             // 0-1
  combination_complexity: number;       // 0-1
}

export interface ResourceGap {
  missing_domain: string;
  gap_severity: number;                 // 0-1
  potential_sources: PotentialSource[];
  acquisition_difficulty: number;       // 0-1
}

export interface PotentialSource {
  source_type: string;
  description: string;
  estimated_quality: number;
  acquisition_cost: number;
}

export interface CompositionSuggestion {
  target_capability: string;
  recommended_resources: string[];
  composition_strategy: string;
  expected_success_rate: number;
}

/**
 * Training Resource Tracker
 * 
 * This is the REAL foundation of persona capabilities. Instead of just tracking
 * final LoRA layers, we track the original resources that created those capabilities:
 * 
 * - The exact prompts used during training
 * - The datasets that provided knowledge
 * - The conversations that shaped reasoning patterns  
 * - The code repositories that built programming skills
 * - The interaction histories that developed personality
 * 
 * This enables TRUE capability synthesis by understanding the raw sources
 */
export class TrainingResourceTracker {
  private resources: Map<string, TrainingResource> = new Map();
  private resourceIndex: Map<string, Set<string>> = new Map(); // domain -> resource_ids
  private promptIndex: Map<string, Set<string>> = new Map();   // concept -> resource_ids

  /**
   * Register a training resource with full provenance tracking
   */
  async registerResource(
    resource: Omit<TrainingResource, 'id' | 'checksum'>,
    rawData?: string
  ): Promise<string> {
    const resourceId = `resource_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    
    console.log(`📚 Registering training resource: ${resource.type} - ${resource.source_description}`);

    // Calculate checksum if raw data provided
    const checksum = rawData ? this.calculateChecksum(rawData) : 'no_data';

    // Analyze the resource content if raw data provided
    let dataSummary = resource.data_summary;
    if (rawData) {
      dataSummary = await this.analyzeResourceContent(rawData, resource.type);
    }

    const fullResource: TrainingResource = {
      id: resourceId,
      checksum,
      data_summary: dataSummary,
      ...resource
    };

    // Store resource
    this.resources.set(resourceId, fullResource);

    // Update indices
    await this.updateIndices(fullResource);

    console.log(`✅ Registered resource ${resourceId}: ${resource.type} with ${dataSummary.domain_coverage.length} domains`);
    return resourceId;
  }

  /**
   * Find resources that could contribute to a capability requirement
   */
  async findResourcesForCapability(
    targetDomains: string[],
    targetCapabilities: string[],
    qualityThreshold: number = 0.5
  ): Promise<ResourceQueryResult> {
    console.log(`🎯 Finding resources for domains: ${targetDomains.join(', ')}`);
    console.log(`🎯 Target capabilities: ${targetCapabilities.join(', ')}`);

    const relevantResources: TrainingResource[] = [];

    // Find resources by domain coverage
    for (const domain of targetDomains) {
      const resourceIds = this.resourceIndex.get(domain) || new Set();
      for (const resourceId of resourceIds) {
        const resource = this.resources.get(resourceId);
        if (resource && resource.provenance.quality_score >= qualityThreshold) {
          relevantResources.push(resource);
        }
      }
    }

    // Find resources by capability keywords in prompts/descriptions
    for (const capability of targetCapabilities) {
      const resourceIds = this.promptIndex.get(capability) || new Set();
      for (const resourceId of resourceIds) {
        const resource = this.resources.get(resourceId);
        if (resource && !relevantResources.includes(resource)) {
          relevantResources.push(resource);
        }
      }
    }

    // Remove duplicates and sort by relevance
    const uniqueResources = Array.from(new Set(relevantResources))
      .sort((a, b) => this.calculateRelevance(b, targetDomains, targetCapabilities) - 
                     this.calculateRelevance(a, targetDomains, targetCapabilities));

    // Analyze synthesis potential
    const synthesisPotential = this.analyzeSynthesisPotential(uniqueResources, targetCapabilities);

    // Identify gaps
    const gaps = this.identifyResourceGaps(uniqueResources, targetDomains, targetCapabilities);

    // Generate composition suggestions
    const suggestions = this.generateCompositionSuggestions(uniqueResources, targetCapabilities);

    console.log(`✅ Found ${uniqueResources.length} relevant resources`);

    return {
      resources: uniqueResources,
      synthesis_potential: synthesisPotential,
      gap_analysis: gaps,
      composition_suggestions: suggestions
    };
  }

  /**
   * Extract capabilities from original training prompt
   */
  extractCapabilitiesFromPrompt(prompt: string): ExtractedCapabilities {
    console.log('🔍 Extracting capabilities from original training prompt...');

    // This is where we reverse-engineer what capabilities the prompt was trying to create
    const capabilities: ExtractedCapabilities = {
      explicit_capabilities: [],
      implicit_capabilities: [],
      domain_focus: [],
      personality_traits: [],
      behavioral_patterns: [],
      knowledge_requirements: [],
      skill_combinations: []
    };

    // Extract explicit capabilities (mentioned directly)
    const explicitMatches = prompt.match(/(?:good at|expert in|capable of|can|able to|skilled at)\s+([^.!?]+)/gi) || [];
    capabilities.explicit_capabilities = explicitMatches.map(match => 
      match.replace(/^(good at|expert in|capable of|can|able to|skilled at)\s+/i, '').trim()
    );

    // Extract domain focus
    const domainKeywords = ['programming', 'coding', 'software', 'debugging', 'testing', 'architecture', 
                           'biophysics', 'geology', 'chemistry', 'physics', 'mathematics', 'engineering'];
    capabilities.domain_focus = domainKeywords.filter(domain => 
      prompt.toLowerCase().includes(domain)
    );

    // Extract personality traits
    const personalityKeywords = ['helpful', 'thorough', 'creative', 'analytical', 'careful', 'innovative'];
    capabilities.personality_traits = personalityKeywords.filter(trait => 
      prompt.toLowerCase().includes(trait)
    );

    // Extract behavioral patterns
    if (prompt.includes('step by step')) capabilities.behavioral_patterns.push('systematic_approach');
    if (prompt.includes('ask questions')) capabilities.behavioral_patterns.push('clarification_seeking');
    if (prompt.includes('explain')) capabilities.behavioral_patterns.push('educational_communication');

    // Extract knowledge requirements (implicit)
    const knowledgeIndicators = prompt.match(/(?:understand|know|familiar with|experience with)\s+([^.!?]+)/gi) || [];
    capabilities.knowledge_requirements = knowledgeIndicators.map(match => 
      match.replace(/^(understand|know|familiar with|experience with)\s+/i, '').trim()
    );

    console.log(`🧠 Extracted ${capabilities.explicit_capabilities.length} explicit capabilities from prompt`);
    return capabilities;
  }

  /**
   * Analyze how datasets complement each other
   */
  analyzeDatasetSynergy(resourceIds: string[]): DatasetSynergy {
    console.log(`🔄 Analyzing synergy between ${resourceIds.length} resources...`);

    const resources = resourceIds.map(id => this.resources.get(id)).filter(Boolean) as TrainingResource[];
    
    const synergy: DatasetSynergy = {
      complementary_domains: [],
      overlapping_concepts: [],
      knowledge_depth_distribution: new Map(),
      interaction_pattern_diversity: 0,
      quality_consistency: 0,
      synthesis_complexity: 0
    };

    // Find complementary domains (different domains that work well together)
    const allDomains = new Set<string>();
    resources.forEach(r => r.data_summary.domain_coverage.forEach(d => allDomains.add(d.domain)));
    
    for (const domain1 of allDomains) {
      for (const domain2 of allDomains) {
        if (domain1 !== domain2) {
          const compatibility = this.calculateDomainCompatibility(domain1, domain2, resources);
          if (compatibility > 0.7) {
            synergy.complementary_domains.push({ domains: [domain1, domain2], compatibility });
          }
        }
      }
    }

    // Find overlapping concepts (concepts that appear in multiple resources)
    const conceptCounts = new Map<string, number>();
    resources.forEach(r => {
      r.data_summary.concept_density.forEach(cd => {
        conceptCounts.set(cd.concept, (conceptCounts.get(cd.concept) || 0) + 1);
      });
    });

    synergy.overlapping_concepts = Array.from(conceptCounts.entries())
      .filter(([_, count]) => count > 1)
      .map(([concept, count]) => ({ concept, overlap_count: count }));

    // Calculate quality consistency
    const qualityScores = resources.map(r => r.provenance.quality_score);
    const avgQuality = qualityScores.reduce((sum, q) => sum + q, 0) / qualityScores.length;
    const qualityVariance = qualityScores.reduce((sum, q) => sum + Math.pow(q - avgQuality, 2), 0) / qualityScores.length;
    synergy.quality_consistency = 1 - Math.sqrt(qualityVariance); // Lower variance = higher consistency

    console.log(`✅ Found ${synergy.complementary_domains.length} complementary domain pairs`);
    return synergy;
  }

  // Private helper methods

  private async analyzeResourceContent(rawData: string, type: string): Promise<DataSummary> {
    const summary: DataSummary = {
      total_size_bytes: Buffer.byteLength(rawData, 'utf-8'),
      content_type: this.detectContentType(rawData, type),
      language_distribution: this.analyzeLanguageDistribution(rawData, type),
      domain_coverage: this.extractDomainCoverage(rawData),
      concept_density: this.analyzeConcepts(rawData),
      interaction_patterns: this.analyzeInteractionPatterns(rawData, type),
      knowledge_depth: this.assessKnowledgeDepth(rawData)
    };

    return summary;
  }

  private detectContentType(data: string, type: string): string {
    if (type === 'code_repository') return 'code';
    if (type === 'conversation_log') return 'conversation';
    
    // Simple heuristics
    const codePatterns = /(?:function|class|import|def |var |const |let )/g;
    const codeMatches = (data.match(codePatterns) || []).length;
    
    if (codeMatches > 10) return 'code';
    if (data.includes('Human:') || data.includes('Assistant:')) return 'conversation';
    return 'text';
  }

  private analyzeLanguageDistribution(data: string, type: string): Record<string, number> {
    if (type === 'code_repository') {
      // Analyze programming language distribution
      const languages: Record<string, number> = {};
      
      if (data.includes('function') || data.includes('const ') || data.includes('=>')) {
        languages['JavaScript'] = (data.match(/(?:function|const |=>)/g) || []).length;
      }
      if (data.includes('def ') || data.includes('import ')) {
        languages['Python'] = (data.match(/(?:def |import )/g) || []).length;
      }
      if (data.includes('class ') && data.includes('public ')) {
        languages['Java'] = (data.match(/(?:class |public )/g) || []).length;
      }
      
      return languages;
    } else {
      // Analyze human language distribution (simplified)
      return { 'English': data.length }; // Assume English for now
    }
  }

  private extractDomainCoverage(data: string): DomainCoverage[] {
    const domains: DomainCoverage[] = [];
    
    // Simple domain detection based on keywords
    const domainKeywords = {
      'software_engineering': ['function', 'class', 'debug', 'test', 'code', 'programming'],
      'web_development': ['html', 'css', 'javascript', 'react', 'node', 'api'],
      'data_science': ['data', 'analysis', 'statistics', 'machine learning', 'pandas', 'numpy'],
      'biophysics': ['protein', 'molecular', 'biological', 'biophysics', 'membrane'],
      'geology': ['rock', 'mineral', 'geological', 'earth', 'sediment', 'formation'],
      'mathematics': ['equation', 'theorem', 'proof', 'calculus', 'algebra', 'geometry']
    };

    for (const [domain, keywords] of Object.entries(domainKeywords)) {
      const matches = keywords.filter(keyword => 
        data.toLowerCase().includes(keyword.toLowerCase())
      );
      
      if (matches.length > 0) {
        domains.push({
          domain,
          coverage_percentage: Math.min(100, (matches.length / keywords.length) * 100),
          depth_level: Math.min(10, matches.length),
          key_topics: matches,
          expert_level_content: matches.length > keywords.length * 0.7
        });
      }
    }

    return domains;
  }

  private analyzeConcepts(data: string): ConceptDensity[] {
    // Extract key concepts and analyze their density
    const words = data.toLowerCase().match(/\b\w{4,}\b/g) || [];
    const conceptCounts = new Map<string, number>();
    
    words.forEach(word => {
      conceptCounts.set(word, (conceptCounts.get(word) || 0) + 1);
    });

    // Convert to concept density objects
    return Array.from(conceptCounts.entries())
      .filter(([_, count]) => count > 2) // Only concepts that appear multiple times
      .sort(([_, a], [__, b]) => b - a) // Sort by frequency
      .slice(0, 50) // Top 50 concepts
      .map(([concept, frequency]) => ({
        concept,
        frequency,
        context_diversity: this.calculateContextDiversity(concept, data),
        complexity_level: Math.min(10, concept.length / 2), // Simple heuristic
        connection_count: this.calculateConceptConnections(concept, data)
      }));
  }

  private analyzeInteractionPatterns(data: string, type: string): InteractionPattern[] {
    if (type !== 'conversation_log' && !data.includes('Human:')) {
      return [];
    }

    const patterns: InteractionPattern[] = [];
    
    // Question-answer patterns
    const questionCount = (data.match(/\?/g) || []).length;
    if (questionCount > 0) {
      patterns.push({
        pattern_type: 'question_answer',
        frequency: questionCount,
        quality_score: 0.8, // Default assumption
        context_complexity: Math.min(10, data.length / 1000)
      });
    }

    // Code review patterns
    if (data.includes('code') && data.includes('review')) {
      patterns.push({
        pattern_type: 'code_review',
        frequency: (data.match(/(?:code|review)/gi) || []).length,
        quality_score: 0.7,
        context_complexity: 6
      });
    }

    return patterns;
  }

  private assessKnowledgeDepth(data: string): KnowledgeDepthMap {
    // Assess different levels of knowledge depth
    const domains = this.extractDomainCoverage(data);
    const depthMaps: KnowledgeDepthMap[] = [];

    for (const domain of domains) {
      depthMaps.push({
        domain: domain.domain,
        surface_knowledge: domain.coverage_percentage / 100,
        applied_knowledge: domain.expert_level_content ? 0.8 : 0.4,
        expert_knowledge: domain.depth_level / 10,
        creative_knowledge: Math.random() * 0.5, // Would be more sophisticated
        teaching_knowledge: data.includes('explain') ? 0.7 : 0.3
      });
    }

    // Return first domain for simplicity (in reality would return all)
    return depthMaps[0] || {
      domain: 'general',
      surface_knowledge: 0.5,
      applied_knowledge: 0.3,
      expert_knowledge: 0.2,
      creative_knowledge: 0.1,
      teaching_knowledge: 0.4
    };
  }

  private async updateIndices(resource: TrainingResource): Promise<void> {
    // Update domain index
    for (const domain of resource.data_summary.domain_coverage) {
      if (!this.resourceIndex.has(domain.domain)) {
        this.resourceIndex.set(domain.domain, new Set());
      }
      this.resourceIndex.get(domain.domain)!.add(resource.id);
    }

    // Update prompt/concept index
    if (resource.original_prompt) {
      const concepts = this.extractConceptsFromText(resource.original_prompt);
      for (const concept of concepts) {
        if (!this.promptIndex.has(concept)) {
          this.promptIndex.set(concept, new Set());
        }
        this.promptIndex.get(concept)!.add(resource.id);
      }
    }
  }

  private extractConceptsFromText(text: string): string[] {
    return (text.toLowerCase().match(/\b\w{4,}\b/g) || [])
      .filter(word => word.length > 3)
      .slice(0, 20); // Limit to top 20 concepts
  }

  private calculateRelevance(resource: TrainingResource, domains: string[], capabilities: string[]): number {
    let relevance = 0;

    // Domain relevance
    const resourceDomains = resource.data_summary.domain_coverage.map(d => d.domain);
    const domainOverlap = domains.filter(d => resourceDomains.includes(d)).length;
    relevance += (domainOverlap / domains.length) * 0.5;

    // Capability relevance (based on prompt and description)
    const resourceText = (resource.original_prompt || '') + ' ' + resource.source_description;
    const capabilityMatches = capabilities.filter(cap => 
      resourceText.toLowerCase().includes(cap.toLowerCase())
    ).length;
    relevance += (capabilityMatches / capabilities.length) * 0.3;

    // Quality boost
    relevance += resource.provenance.quality_score * 0.2;

    return Math.min(1.0, relevance);
  }

  // More complex analysis methods would be implemented here...
  private analyzeSynthesisPotential(resources: TrainingResource[], capabilities: string[]): SynthesisPotential {
    return {
      combinable_resources: [],
      novel_synthesis_opportunities: [],
      estimated_capability_gain: 0.5
    };
  }

  private identifyResourceGaps(resources: TrainingResource[], domains: string[], capabilities: string[]): ResourceGap[] {
    return [];
  }

  private generateCompositionSuggestions(resources: TrainingResource[], capabilities: string[]): CompositionSuggestion[] {
    return [];
  }

  private calculateChecksum(data: string): string {
    let hash = 0;
    for (let i = 0; i < data.length; i++) {
      const char = data.charCodeAt(i);
      hash = ((hash << 5) - hash) + char;
      hash = hash & hash;
    }
    return Math.abs(hash).toString(16);
  }

  private calculateContextDiversity(concept: string, data: string): number {
    // Count how many different contexts this concept appears in
    const sentences = data.split(/[.!?]+/);
    const contextsWithConcept = sentences.filter(sentence => 
      sentence.toLowerCase().includes(concept.toLowerCase())
    );
    return Math.min(1.0, contextsWithConcept.length / 10);
  }

  private calculateConceptConnections(concept: string, data: string): number {
    // Count how often this concept appears near other concepts
    const words = data.toLowerCase().split(/\s+/);
    let connections = 0;
    
    for (let i = 0; i < words.length; i++) {
      if (words[i].includes(concept)) {
        // Count unique words within 5 positions
        const nearby = words.slice(Math.max(0, i - 5), Math.min(words.length, i + 6));
        connections += new Set(nearby).size - 1; // -1 to exclude the concept itself
      }
    }
    
    return connections;
  }

  private calculateDomainCompatibility(domain1: string, domain2: string, resources: TrainingResource[]): number {
    // Calculate how well two domains work together based on resource analysis
    return Math.random() * 0.5 + 0.3; // Placeholder
  }
}

// Supporting interfaces
interface ExtractedCapabilities {
  explicit_capabilities: string[];
  implicit_capabilities: string[];
  domain_focus: string[];
  personality_traits: string[];
  behavioral_patterns: string[];
  knowledge_requirements: string[];
  skill_combinations: string[];
}

interface DatasetSynergy {
  complementary_domains: Array<{ domains: string[]; compatibility: number }>;
  overlapping_concepts: Array<{ concept: string; overlap_count: number }>;
  knowledge_depth_distribution: Map<string, number>;
  interaction_pattern_diversity: number;
  quality_consistency: number;
  synthesis_complexity: number;
}

interface NovelSynthesis {
  synthesis_type: string;
  resource_combination: string[];
  novelty_score: number;
  success_probability: number;
}