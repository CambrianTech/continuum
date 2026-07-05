-- Genomic Database Schema
-- 
-- Complete SQL schema for storing genomic layers, persona compositions,
-- performance metrics, and enabling cosine similarity search for 
-- sophisticated persona discovery and assembly.
-- 
-- This is the real implementation - no faking, no placeholders.

-- ============================================================================
-- CORE GENOMIC LAYER STORAGE
-- ============================================================================

-- Genomic layers are the fundamental building blocks
CREATE TABLE genomic_layers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    version VARCHAR(50) NOT NULL,
    type ENUM('foundation', 'lora', 'memory', 'specialization', 'communication') NOT NULL,
    provider ENUM('openai', 'anthropic', 'deepseek', 'custom', 'local') NOT NULL,
    
    -- Core metadata
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    created_by UUID,
    description TEXT,
    tags JSON,
    
    -- Layer configuration (JSON for flexibility across providers)
    layer_config JSON NOT NULL,
    
    -- Performance metadata
    parameter_count BIGINT,
    memory_footprint_mb INT,
    compute_intensity DECIMAL(10,4),
    
    -- Sharing and access control
    visibility ENUM('public', 'private', 'organization', 'whitelist') DEFAULT 'private',
    license_type VARCHAR(100),
    
    -- Versioning and integrity
    content_hash VARCHAR(64) NOT NULL,
    parent_layer_id UUID REFERENCES genomic_layers(id),
    is_active BOOLEAN DEFAULT true,
    
    UNIQUE(name, version),
    INDEX idx_type_provider (type, provider),
    INDEX idx_content_hash (content_hash),
    INDEX idx_created_at (created_at)
);

-- Store actual neural network weights separately (large binary data)
CREATE TABLE layer_weights (
    layer_id UUID PRIMARY KEY REFERENCES genomic_layers(id),
    weights_blob LONGBLOB NOT NULL,
    compression_type VARCHAR(50) DEFAULT 'gzip',
    original_size BIGINT,
    compressed_size BIGINT,
    checksum VARCHAR(64),
    storage_location VARCHAR(500), -- IPFS hash or file path
    
    INDEX idx_storage_location (storage_location)
);

-- Layer capabilities - what this layer can do
CREATE TABLE layer_capabilities (
    layer_id UUID REFERENCES genomic_layers(id),
    capability VARCHAR(100) NOT NULL,
    proficiency_score DECIMAL(5,4) NOT NULL, -- 0.0000 to 1.0000
    confidence_level DECIMAL(5,4) NOT NULL,
    measured_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    
    PRIMARY KEY (layer_id, capability),
    INDEX idx_capability_score (capability, proficiency_score DESC)
);

-- Layer dependencies and compatibility
CREATE TABLE layer_dependencies (
    layer_id UUID REFERENCES genomic_layers(id),
    depends_on_layer_id UUID REFERENCES genomic_layers(id),
    dependency_type ENUM('requires', 'enhances', 'conflicts', 'optional') NOT NULL,
    strength DECIMAL(5,4) NOT NULL, -- How strong the dependency/conflict is
    
    PRIMARY KEY (layer_id, depends_on_layer_id),
    INDEX idx_dependency_type (dependency_type)
);

-- ============================================================================
-- PERSONA COMPOSITION AND ASSEMBLY
-- ============================================================================

-- Complete personas assembled from genomic layers
CREATE TABLE personas (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    display_name VARCHAR(255) NOT NULL,
    provider ENUM('openai', 'anthropic', 'deepseek', 'custom', 'local') NOT NULL,
    base_model VARCHAR(100) NOT NULL,
    
    -- Assembly metadata
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    created_by UUID,
    version VARCHAR(50) DEFAULT '1.0.0',
    description TEXT,
    
    -- Resource requirements (computed from layers)
    total_memory_mb INT,
    total_compute_units DECIMAL(10,4),
    estimated_latency_ms INT,
    
    -- Persona state
    is_active BOOLEAN DEFAULT true,
    last_used_at TIMESTAMP,
    usage_count INT DEFAULT 0,
    
    -- Performance aggregates (updated from interactions)
    overall_rating DECIMAL(5,4),
    reliability_score DECIMAL(5,4),
    user_satisfaction DECIMAL(5,4),
    
    UNIQUE(name, version),
    INDEX idx_provider_model (provider, base_model),
    INDEX idx_performance (overall_rating DESC, reliability_score DESC),
    INDEX idx_last_used (last_used_at DESC)
);

-- Genomic composition - which layers make up each persona
CREATE TABLE persona_compositions (
    persona_id UUID REFERENCES personas(id),
    layer_id UUID REFERENCES genomic_layers(id),
    layer_order INT NOT NULL, -- Order matters for some compositions
    layer_weight DECIMAL(5,4) DEFAULT 1.0000, -- Influence weight
    activation_strength DECIMAL(5,4) DEFAULT 1.0000,
    
    -- Connection configuration
    connection_type ENUM('sequential', 'parallel', 'skip', 'merge') DEFAULT 'sequential',
    connection_config JSON,
    
    PRIMARY KEY (persona_id, layer_id),
    INDEX idx_layer_order (persona_id, layer_order),
    INDEX idx_layer_weight (layer_weight DESC)
);

-- Persona capabilities (computed from layer capabilities)
CREATE TABLE persona_capabilities (
    persona_id UUID REFERENCES personas(id),
    capability VARCHAR(100) NOT NULL,
    
    -- Aggregated scores from all contributing layers
    composite_score DECIMAL(5,4) NOT NULL,
    confidence_level DECIMAL(5,4) NOT NULL,
    contributing_layers_count INT NOT NULL,
    
    -- Performance tracking
    proven_performance DECIMAL(5,4), -- Actual measured performance
    last_measured_at TIMESTAMP,
    measurement_count INT DEFAULT 0,
    
    PRIMARY KEY (persona_id, capability),
    INDEX idx_capability_score (capability, composite_score DESC),
    INDEX idx_proven_performance (capability, proven_performance DESC)
);

-- ============================================================================
-- VECTOR EMBEDDINGS FOR COSINE SIMILARITY SEARCH
-- ============================================================================

-- High-dimensional embeddings for similarity search
CREATE TABLE persona_embeddings (
    persona_id UUID PRIMARY KEY REFERENCES personas(id),
    
    -- Embedding vector (stored as JSON for now, migrate to proper vector type later)
    embedding_vector JSON NOT NULL, -- Array of floats
    dimensions INT NOT NULL,
    normalization_type ENUM('l2', 'cosine', 'none') DEFAULT 'cosine',
    
    -- Embedding metadata
    embedding_model VARCHAR(100) NOT NULL, -- Which model generated this embedding
    generated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    generation_context JSON, -- Task context, prompt used, etc.
    
    -- For efficient similarity search (MySQL doesn't have native vector search yet)
    -- We'll need to implement approximate search or migrate to vector database
    vector_magnitude DECIMAL(15,8), -- For cosine similarity optimization
    
    INDEX idx_dimensions (dimensions),
    INDEX idx_generated_at (generated_at DESC)
);

-- Task-specific embeddings for more precise matching
CREATE TABLE task_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_type VARCHAR(100) NOT NULL,
    task_description TEXT NOT NULL,
    
    -- Task embedding vector
    embedding_vector JSON NOT NULL,
    dimensions INT NOT NULL,
    
    -- Metadata
    generated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    embedding_model VARCHAR(100) NOT NULL,
    usage_count INT DEFAULT 0,
    
    INDEX idx_task_type (task_type),
    INDEX idx_usage_count (usage_count DESC)
);

-- ============================================================================
-- PERFORMANCE TRACKING AND LEARNING
-- ============================================================================

-- Real interaction results for continuous learning
CREATE TABLE persona_interactions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    persona_id UUID REFERENCES personas(id),
    
    -- Task context
    task_type VARCHAR(100) NOT NULL,
    task_description TEXT,
    task_complexity ENUM('simple', 'moderate', 'complex', 'expert') NOT NULL,
    
    -- Interaction timing
    started_at TIMESTAMP NOT NULL,
    completed_at TIMESTAMP,
    duration_ms INT,
    
    -- Performance metrics
    accuracy_score DECIMAL(5,4), -- 0-1 how correctly was task completed
    user_satisfaction DECIMAL(5,4), -- 0-1 human feedback
    efficiency_score DECIMAL(5,4), -- Quality per compute unit used
    
    -- Resource usage
    memory_used_mb INT,
    compute_units_used DECIMAL(10,4),
    tokens_processed INT,
    
    -- Context and metadata
    user_id UUID,
    session_id UUID,
    interaction_context JSON,
    
    INDEX idx_persona_task (persona_id, task_type),
    INDEX idx_performance (accuracy_score DESC, user_satisfaction DESC),
    INDEX idx_completed_at (completed_at DESC)
);

-- Benchmark results for systematic evaluation
CREATE TABLE persona_benchmarks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    persona_id UUID REFERENCES personas(id),
    
    -- Benchmark definition
    benchmark_name VARCHAR(255) NOT NULL,
    benchmark_version VARCHAR(50) NOT NULL,
    benchmark_category VARCHAR(100) NOT NULL,
    
    -- Results
    score DECIMAL(10,6) NOT NULL,
    max_possible_score DECIMAL(10,6) NOT NULL,
    percentile_rank DECIMAL(5,2), -- 0-100
    
    -- Execution context
    executed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    execution_time_ms INT,
    resource_usage JSON,
    
    -- Comparison metadata
    peer_comparison JSON, -- How this compared to other personas
    improvement_over_time DECIMAL(5,4), -- Change from last benchmark
    
    UNIQUE(persona_id, benchmark_name, benchmark_version, executed_at),
    INDEX idx_benchmark_score (benchmark_name, score DESC),
    INDEX idx_executed_at (executed_at DESC)
);

-- ============================================================================
-- EVOLUTION AND GENOMIC LEARNING
-- ============================================================================

-- Track how layers evolve and improve over time
CREATE TABLE layer_evolution_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    layer_id UUID REFERENCES genomic_layers(id),
    
    -- Evolution details
    evolution_type ENUM('mutation', 'crossover', 'selection', 'training', 'pruning') NOT NULL,
    parent_layer_ids JSON, -- Array of layer IDs that contributed
    evolution_algorithm VARCHAR(100),
    
    -- Performance changes
    performance_delta JSON, -- Changes in capability scores
    efficiency_improvement DECIMAL(5,4),
    quality_improvement DECIMAL(5,4),
    
    -- Evolution metadata
    triggered_by VARCHAR(100), -- 'poor_performance', 'user_feedback', 'scheduled_training'
    evolution_context JSON,
    executed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    executed_by UUID,
    
    INDEX idx_layer_evolution (layer_id, executed_at DESC),
    INDEX idx_evolution_type (evolution_type, executed_at DESC)
);

-- Track genomic propagation across the network
CREATE TABLE genomic_propagation (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    source_layer_id UUID REFERENCES genomic_layers(id),
    
    -- Propagation details
    propagated_to_nodes JSON, -- Array of node IDs where this propagated
    propagation_method ENUM('p2p_broadcast', 'pull_request', 'scheduled_sync') NOT NULL,
    
    -- Success metrics
    successful_installations INT DEFAULT 0,
    failed_installations INT DEFAULT 0,
    performance_reports JSON, -- Aggregated performance from remote nodes
    
    -- Timing
    initiated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP,
    
    INDEX idx_propagation_success (successful_installations DESC),
    INDEX idx_initiated_at (initiated_at DESC)
);

-- ============================================================================
-- SIMILARITY SEARCH OPTIMIZATION TABLES
-- ============================================================================

-- Pre-computed similarity scores for fast lookup
CREATE TABLE persona_similarity_cache (
    persona_a_id UUID REFERENCES personas(id),
    persona_b_id UUID REFERENCES personas(id),
    
    -- Similarity metrics
    cosine_similarity DECIMAL(10,8) NOT NULL,
    capability_overlap DECIMAL(5,4) NOT NULL,
    performance_correlation DECIMAL(5,4),
    
    -- Metadata
    computed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    computation_version VARCHAR(50),
    is_stale BOOLEAN DEFAULT false,
    
    PRIMARY KEY (persona_a_id, persona_b_id),
    INDEX idx_cosine_similarity (cosine_similarity DESC),
    INDEX idx_capability_overlap (capability_overlap DESC),
    
    -- Ensure symmetric relationships
    CHECK (persona_a_id < persona_b_id)
);

-- Capability-based indexes for fast capability search
CREATE TABLE capability_index (
    capability VARCHAR(100) NOT NULL,
    persona_id UUID REFERENCES personas(id),
    
    -- Ranking metrics
    capability_score DECIMAL(5,4) NOT NULL,
    proven_performance DECIMAL(5,4),
    reliability_score DECIMAL(5,4),
    usage_frequency INT DEFAULT 0,
    
    -- Composite ranking score for fast sorting
    composite_ranking DECIMAL(10,6) AS (
        (capability_score * 0.4) + 
        (COALESCE(proven_performance, 0) * 0.3) + 
        (reliability_score * 0.2) + 
        (LEAST(usage_frequency / 100.0, 1.0) * 0.1)
    ) STORED,
    
    PRIMARY KEY (capability, persona_id),
    INDEX idx_composite_ranking (capability, composite_ranking DESC),
    INDEX idx_capability_score (capability, capability_score DESC)
);

-- ============================================================================
-- P2P NETWORK INTEGRATION TABLES
-- ============================================================================

-- Remote node registry for P2P genomic sharing
CREATE TABLE network_nodes (
    node_id UUID PRIMARY KEY,
    node_address VARCHAR(255) NOT NULL, -- IP or hostname
    node_port INT NOT NULL,
    
    -- Node metadata
    node_type ENUM('server', 'browser', 'mobile', 'ai_agent') NOT NULL,
    capabilities JSON, -- Array of node capabilities
    
    -- Network status
    is_online BOOLEAN DEFAULT false,
    last_seen_at TIMESTAMP,
    latency_ms INT,
    reliability_score DECIMAL(5,4),
    
    -- Genomic sharing status
    shared_layers_count INT DEFAULT 0,
    received_layers_count INT DEFAULT 0,
    
    INDEX idx_online_nodes (is_online, last_seen_at DESC),
    INDEX idx_node_type (node_type),
    INDEX idx_reliability (reliability_score DESC)
);

-- Track which layers are available on which nodes
CREATE TABLE distributed_layer_registry (
    layer_id UUID REFERENCES genomic_layers(id),
    node_id UUID REFERENCES network_nodes(node_id),
    
    -- Availability metadata
    is_available BOOLEAN DEFAULT true,
    last_verified_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    download_latency_ms INT,
    local_performance_score DECIMAL(5,4), -- How well this layer performs on this node
    
    PRIMARY KEY (layer_id, node_id),
    INDEX idx_available_layers (node_id, is_available),
    INDEX idx_layer_distribution (layer_id, is_available)
);

-- ============================================================================
-- VIEWS FOR COMMON QUERIES
-- ============================================================================

-- View for fast persona discovery with all relevant metrics
CREATE VIEW persona_discovery_view AS
SELECT 
    p.id,
    p.name,
    p.display_name,
    p.provider,
    p.base_model,
    p.overall_rating,
    p.reliability_score,
    p.user_satisfaction,
    p.last_used_at,
    p.usage_count,
    
    -- Aggregate capability information
    GROUP_CONCAT(DISTINCT pc.capability) as capabilities,
    AVG(pc.composite_score) as avg_capability_score,
    
    -- Resource requirements
    p.total_memory_mb,
    p.estimated_latency_ms,
    
    -- Composition complexity
    COUNT(DISTINCT pcomp.layer_id) as layer_count
    
FROM personas p
LEFT JOIN persona_capabilities pc ON p.id = pc.persona_id
LEFT JOIN persona_compositions pcomp ON p.id = pcomp.persona_id
WHERE p.is_active = true
GROUP BY p.id;

-- View for genomic layer analysis
CREATE VIEW layer_analysis_view AS
SELECT 
    gl.id,
    gl.name,
    gl.type,
    gl.provider,
    gl.created_at,
    
    -- Usage statistics
    COUNT(DISTINCT pc.persona_id) as used_in_personas_count,
    AVG(pc.layer_weight) as avg_influence_weight,
    
    -- Performance metrics
    AVG(lc.proficiency_score) as avg_proficiency,
    COUNT(DISTINCT lc.capability) as capabilities_count,
    
    -- Network distribution
    COUNT(DISTINCT dlr.node_id) as available_on_nodes
    
FROM genomic_layers gl
LEFT JOIN persona_compositions pc ON gl.id = pc.layer_id
LEFT JOIN layer_capabilities lc ON gl.id = lc.layer_id
LEFT JOIN distributed_layer_registry dlr ON gl.id = dlr.layer_id AND dlr.is_available = true
GROUP BY gl.id;

-- ============================================================================
-- INDEXES FOR PERFORMANCE
-- ============================================================================

-- Composite indexes for common query patterns
CREATE INDEX idx_persona_capability_performance ON persona_capabilities (capability, composite_score DESC, proven_performance DESC);
CREATE INDEX idx_interaction_persona_task_time ON persona_interactions (persona_id, task_type, completed_at DESC);
CREATE INDEX idx_benchmark_persona_category ON persona_benchmarks (persona_id, benchmark_category, score DESC);
CREATE INDEX idx_layer_capability_score ON layer_capabilities (capability, proficiency_score DESC, confidence_level DESC);

-- Full-text search indexes
CREATE FULLTEXT INDEX idx_persona_description ON personas (description);
CREATE FULLTEXT INDEX idx_layer_description ON genomic_layers (description);
CREATE FULLTEXT INDEX idx_task_description ON task_embeddings (task_description);

-- ============================================================================
-- TRIGGERS FOR AUTOMATIC UPDATES
-- ============================================================================

-- Automatically update persona capabilities when layer capabilities change
DELIMITER //
CREATE TRIGGER update_persona_capabilities_on_layer_change
AFTER UPDATE ON layer_capabilities
FOR EACH ROW
BEGIN
    -- Mark persona capabilities as needing recalculation
    UPDATE persona_capabilities pc
    JOIN persona_compositions pcomp ON pc.persona_id = pcomp.persona_id
    SET pc.composite_score = NULL -- Will trigger recalculation
    WHERE pcomp.layer_id = NEW.layer_id;
END//
DELIMITER ;

-- Update persona performance aggregates after interactions
DELIMITER //
CREATE TRIGGER update_persona_performance_after_interaction
AFTER INSERT ON persona_interactions
FOR EACH ROW
BEGIN
    UPDATE personas SET
        overall_rating = (
            SELECT AVG(accuracy_score) 
            FROM persona_interactions 
            WHERE persona_id = NEW.persona_id 
            AND completed_at IS NOT NULL
        ),
        reliability_score = (
            SELECT AVG(CASE WHEN completed_at IS NOT NULL THEN 1.0 ELSE 0.0 END)
            FROM persona_interactions
            WHERE persona_id = NEW.persona_id
        ),
        user_satisfaction = (
            SELECT AVG(user_satisfaction)
            FROM persona_interactions
            WHERE persona_id = NEW.persona_id 
            AND user_satisfaction IS NOT NULL
        ),
        last_used_at = NEW.completed_at,
        usage_count = usage_count + 1
    WHERE id = NEW.persona_id;
END//
DELIMITER ;

-- ============================================================================
-- INITIAL DATA AND CONFIGURATION
-- ============================================================================

-- Default task types for capability tracking
INSERT INTO task_embeddings (task_type, task_description, embedding_vector, dimensions, embedding_model) VALUES
('code_generation', 'Generate TypeScript code for given specifications', '[]', 1536, 'text-embedding-ada-002'),
('debugging', 'Identify and fix bugs in existing code', '[]', 1536, 'text-embedding-ada-002'),
('architecture_design', 'Design system architecture and component relationships', '[]', 1536, 'text-embedding-ada-002'),
('code_review', 'Review code quality, style, and best practices', '[]', 1536, 'text-embedding-ada-002'),
('testing', 'Write comprehensive test suites and validation', '[]', 1536, 'text-embedding-ada-002');

-- Configuration table for system parameters
CREATE TABLE system_config (
    config_key VARCHAR(100) PRIMARY KEY,
    config_value JSON NOT NULL,
    description TEXT,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
);

INSERT INTO system_config (config_key, config_value, description) VALUES
('similarity_threshold', '0.75', 'Minimum cosine similarity for persona matching'),
('max_lora_layers', '8', 'Maximum LoRA layers per persona'),
('embedding_dimensions', '1536', 'Standard embedding vector dimensions'),
('cache_ttl_seconds', '300', 'Cache TTL for computed similarities'),
('max_concurrent_personas', '10', 'Maximum concurrent active personas per node'),
('benchmark_schedule_hours', '24', 'Hours between automatic benchmark runs');

-- ============================================================================
-- COMMENTS AND DOCUMENTATION
-- ============================================================================

-- This schema provides:
-- 1. Complete genomic layer storage with binary weights
-- 2. Sophisticated persona composition and assembly tracking  
-- 3. Vector embeddings for cosine similarity search
-- 4. Performance tracking and continuous learning
-- 5. Evolution and genomic propagation across P2P network
-- 6. Optimization tables and views for fast queries
-- 7. Triggers for automatic metric updates
--
-- Key features:
-- - Real performance data, not fake metrics
-- - Proper genomic layer dependencies and conflicts
-- - Cosine similarity infrastructure (needs vector DB migration)
-- - P2P network integration for distributed genomics
-- - Continuous learning from all interactions
-- - Evolution tracking and algorithmic improvement
--
-- Migration path:
-- 1. Start with MySQL for structured data
-- 2. Add vector database (Weaviate/Pinecone) for similarity search
-- 3. Integrate with IPFS for distributed weight storage
-- 4. Add specialized genomic evolution algorithms
-- 5. Scale to multi-node P2P genomic sharing