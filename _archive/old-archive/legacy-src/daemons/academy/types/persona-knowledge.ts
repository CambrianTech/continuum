/**
 * Persona Knowledge Architecture
 */

export interface DomainExpertise {
  domain: string;
  depth_level: number; // 0-10
  breadth_coverage: number; // 0-1
  practical_experience: PracticalExperience[];
  theoretical_knowledge: TheoreticalKnowledge[];
  intuitive_understanding: number; // 0-1
}

export interface PracticalExperience {
  experience_type: string;
  domain: string;
  proficiency_level: number;
  examples: string[];
}

export interface TheoreticalKnowledge {
  knowledge_area: string;
  depth_level: number;
  key_concepts: string[];
  understanding_confidence: number;
}

export interface SkillGraph {
  nodes: SkillNode[];
  edges: SkillConnection[];
  clusters: SkillCluster[];
  pathways: SkillPathway[];
}

export interface SkillNode {
  skill_id: string;
  skill_name: string;
  proficiency_level: number;
  related_domains: string[];
}

export interface SkillConnection {
  source_skill: string;
  target_skill: string;
  connection_strength: number;
  connection_type: string;
}

export interface SkillCluster {
  cluster_id: string;
  cluster_name: string;
  member_skills: string[];
  cluster_coherence: number;
}

export interface SkillPathway {
  pathway_id: string;
  pathway_name: string;
  skill_sequence: string[];
  learning_difficulty: number;
}

export interface ExperienceMemory {
  id: string;
  context: string;
  challenge: string;
  actions_taken: string[];
  outcome: string;
  lessons_learned: string[];
  emotional_weight: number;
  retrieval_frequency: number;
}

export interface PatternLibrary {
  recognized_patterns: Pattern[];
  pattern_hierarchies: PatternHierarchy[];
  cross_domain_patterns: CrossDomainPattern[];
}

export interface Pattern { 
  id: string; 
  name: string; 
  description: string; 
}

export interface PatternHierarchy { 
  parent: string; 
  children: string[]; 
}

export interface CrossDomainPattern { 
  pattern_id: string; 
  domains: string[]; 
}

export interface MentalModel {
  model_id: string;
  domain: string;
  conceptual_framework: ConceptualFramework;
  predictive_accuracy: number;
  confidence_level: number;
}

export interface ConceptualFramework { 
  concepts: string[]; 
  relationships: string[]; 
}

export interface KnowledgeGap {
  gap_id: string;
  domain: string;
  gap_description: string;
  severity: number;
  acquisition_difficulty: number;
}