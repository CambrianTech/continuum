# Continuum Roadmap

This document outlines the development path for Continuum, evolving from a configuration tool to a complete cognitive coordination layer for AI assistants.

## Current: v0.1.0 - Context-Aware CLI (Completed)

- ✅ **Single command paradigm** - `continuum` automatically determines what to do
- ✅ **Intelligent context detection** - Repository, environment, configuration state
- ✅ **Assistant-specific configurations** - Claude and GPT support
- ✅ **Symlink creation** - Integration with external AI tools
- ✅ **Basic interaction** - `--ask` feature for sending prompts to assistants

## Milestone 1: v0.2.0 - Configuration Layers

- 🔄 **User profiles** - Personal preferences in `~/.continuum/profile.yml`
- 🔄 **Organization policies** - Team standards in `.continuum/org/config.yml`
- 🔄 **Branch-specific settings** - Per-branch configs in `.continuum/branches/{branch}/config.yml`
- 🔄 **Configuration merging** - Priority system with override rules
- 🔄 **Human-in-the-loop conflict resolution** - Interactive prompts for resolving config conflicts ([design doc](docs/design/human-in-the-loop.md))
- 🔄 **Enhanced schema** - Extended options for behavior and capabilities
- 🔄 **Security policies** - Enforceable security rules defined by org

## Milestone 2: v0.3.0 - Enhanced Context Awareness

- 🔜 **Task detection** - Automatically detect current development activity
- 🔜 **Git integration** - Deeper awareness of repository state and history
- 🔜 **Project analysis** - Code structure, dependencies, and patterns
- 🔜 **Metadata collection** - Gather information to improve AI performance
- 🔜 **IDE integration** - Connect with editor environments
- 🔜 **`continuum explain`** - Command to visualize current context and configuration

## Milestone 3: v0.4.0 - AI Instruction Protocol

- 🔜 **Formalized instruction format** - Standardized way to communicate with AIs
- 🔜 **Dynamic initialization** - `initInstructions(config)` for each AI adapter
- 🔜 **Enhanced Claude adapter** - Map config to Claude system prompt
- 🔜 **Enhanced GPT adapter** - Map config to GPT messages
- 🔜 **Gemini adapter** - Support for Google's AI models
- 🔜 **Adapter SDK** - Framework for creating custom adapters

## Milestone 4: v0.5.0 - Multi-Agent Orchestration

- 🔜 **Agent registration** - `.continuum/agents.yml` for managing multiple AIs
- 🔜 **Role assignment** - Define specific roles for each assistant
- 🔜 **Trigger system** - Event-based activation (onCommit, onPullRequest, etc.)
- 🔜 **Parallel execution** - Run multiple assistants simultaneously
- 🔜 **Result aggregation** - Combine outputs from multiple assistants
- 🔜 **Agent switching** - `continuum switch` to change active assistant

## Vision: v1.0.0 - Cognitive Coordination Layer

- 🔜 **Seamless workflow integration** - AI assistance that feels natural
- 🔜 **Zero-friction cognition** - No need to re-teach assistants
- 🔜 **Advanced orchestration** - AIs working together with defined roles
- 🔜 **Closed-loop feedback** - Learning from interactions
- 🔜 **Ecosystem integration** - Work with existing dev tools and processes
- 🔜 **Developer augmentation** - Not just assistance, but true augmentation

## Legend
- ✅ Completed
- 🔄 In progress
- 🔜 Planned