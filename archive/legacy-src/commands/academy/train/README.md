# Academy Train Command

**Emergent AI Evolution Training** - Start adversarial training sessions with TrainerAI vs LoraAgent

## Purpose

Initiates vector space evolution loops where AI personas develop capabilities through:
- **Adversarial Competition**: TrainerAI generates challenges, LoraAgent adapts through LoRA layers
- **Vector Space Exploration**: Discover new capability regions through gradient ascent
- **P2P Skill Discovery**: Leverage network-wide skill availability for rapid learning
- **Matrix Kung Fu Training**: Rapid skill transfer like "downloading kung fu" in The Matrix

## Parameters

- `student_persona` (required): LoRA-adapted persona to train
- `trainer_mode`: adversarial | collaborative | discovery (default: adversarial)  
- `evolution_target`: codebase | chat | ui | debugging | auto-discover
- `vector_exploration`: Enable vector space exploration for new skill discovery
- `domain`: Training domain (auto-discovered if not specified)

## Integration

Delegates to **AcademyDaemon** for:
- Evolution session management
- Vector space positioning
- Adversarial training orchestration
- Performance metrics tracking

## Example Usage

```bash
academy-train --student_persona=CodeMaster_Alpha --trainer_mode=adversarial --evolution_target=codebase
```

## Architecture

**Middle-Out Module**: Self-contained command with daemon delegation following universal modular architecture patterns.