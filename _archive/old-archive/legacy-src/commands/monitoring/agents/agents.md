# Continuum Agents

## Overview

Continuum supports multiple AI agents that can be dynamically discovered and configured. Each agent has specific capabilities and can be selected based on the task at hand.

## Available Agents

### Default Agents
- **Auto** - Intelligent agent selection based on task context
- **Claude** - Anthropic's Claude AI assistant
- **GPT** - OpenAI's GPT models
- **Gemini** - Google's Gemini AI
- **Local** - Local AI models

### Remote Agents
Remote agents are discovered dynamically from connected systems and can include:
- Human collaborators
- Specialized AI agents
- Custom agent implementations
- Third-party AI services

## Agent Configuration

Agents can be configured through:
- Package.json metadata in agent modules
- Environment variables
- Runtime configuration
- Dynamic discovery protocols

## Agent Selection

The system automatically selects appropriate agents based on:
- Task requirements
- Agent capabilities
- Availability and performance
- User preferences

## Agent Communication

Agents communicate through the universal command API, enabling:
- Seamless task delegation
- Command composition
- Multi-agent coordination
- Result aggregation

## Custom Agents

Custom agents can be created by:
1. Implementing the base agent interface
2. Providing proper metadata in package.json
3. Registering with the agent discovery system
4. Following the universal command protocol