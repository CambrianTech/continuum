# Academy Status Command

**Evolution Monitoring Dashboard** - Monitor AI training progress, P2P network health, and vector space evolution

## Purpose

Provides comprehensive visibility into the Academy ecosystem:
- **Training Progress**: Real-time adversarial battle statistics and evolution rates
- **P2P Network Health**: Torrent-style skill sharing efficiency and node status
- **Vector Space Evolution**: Fitness landscapes, convergence clusters, emergent behaviors
- **System Performance**: Autonomous operation scores and resource utilization

## Parameters

- `persona_id`: Specific persona to check (all if not specified)
- `detail_level`: summary | detailed | deep_metrics
- `include_p2p`: Include P2P network status (default: true)
- `include_vector_space`: Include vector space evolution metrics (default: true)
- `include_adversarial`: Include TrainerAI vs LoraAgent battle stats (default: true)

## Integration

Delegates to **AcademyDaemon** for:
- Training session tracking
- P2P network monitoring
- Vector space evolution analysis
- Performance metrics aggregation

## Example Usage

```bash
academy-status --detail_level=detailed --include_adversarial=true
```

## Sample Output

```json
{
  "academy_overview": {
    "total_personas": 3,
    "active_training_sessions": 1,
    "system_evolution_generation": 12,
    "overall_health": "excellent"
  },
  "vector_space_evolution": {
    "dimensions": 512,
    "emergent_behaviors": ["self_debugging_patterns", "adaptive_code_style_matching"]
  }
}
```

## Architecture

**Middle-Out Module**: Self-contained command with daemon delegation following universal modular architecture patterns.