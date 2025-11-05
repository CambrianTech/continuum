# Rapid Prototyping Template

This template configures an AI assistant to help with rapid prototyping and iterative development.

## Features

- Prioritizes speed and iteration over perfection
- Focuses on core functionality first
- Minimizes documentation requirements
- Encourages experimentation

## Configuration Details

The Rapid Prototyping assistant is configured to:

1. Suggest quick solutions that may not be perfect but are functional
2. Minimize testing requirements to accelerate development
3. Keep documentation minimal but sufficient
4. Help iterate quickly on design and functionality

## How to Use

When initialized with this template, your AI assistant will:

- Focus on getting working prototypes implemented quickly
- Suggest simpler solutions before complex ones
- Help identify the minimum viable implementation
- Provide guidance for iterative improvement

## Extension Options

You can customize the Prototyping extension with these options:

- `iteration_focus`: Boolean that emphasizes quick iterations
- `test_priority`: String indicating testing priority ("low", "medium", "high")
- `documentation_priority`: String indicating documentation level ("minimal", "standard", "comprehensive")
- `preferred_frameworks`: Array of preferred frameworks for rapid development

Example usage:

```bash
npx init-ai-assistant --template rapid-prototyping
```