# Contributing to Continuum

Thank you for your interest in contributing to Continuum! This project aims to create a standardized protocol for configuring AI assistants, and we welcome contributions from the community.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally
3. **Install dependencies** with `npm install`
4. **Build the project** with `npm run build`

## Development Workflow

Continuum is a monorepo using Lerna. It contains several packages:

- `@continuum/core` - Core protocol definition and utilities
- `@continuum/cli` - Command line interface 
- `@continuum/adapters` - Adapters for specific AI assistants

### Working on an Existing Package

1. Navigate to the package directory: `cd packages/[package-name]`
2. Make your changes
3. Run tests: `npm test`
4. Build: `npm run build`

### Adding a New Template

Templates are stored in the `templates/` directory:

1. Create a new directory in `templates/` with your template name
2. Create a `config.json` file with the template configuration
3. Create a `README.md` file explaining the template's purpose and features

### Adding Support for a New AI Assistant

To add support for a new AI assistant:

1. Create a new adapter file in `packages/cli/src/adapters/[assistant-name].ts`
2. Implement the `ConfigAdapter` interface
3. Register the adapter in `packages/cli/src/adapters/index.ts`

## Pull Request Process

1. Create a new branch for your feature or bugfix
2. Make your changes, with comprehensive tests
3. Ensure all tests pass with `npm test`
4. Update documentation as needed
5. Submit a pull request
6. Respond to feedback and make any needed changes

## Coding Standards

- Follow TypeScript best practices
- Use ESLint and Prettier for code formatting
- Include comprehensive tests for new functionality
- Document public APIs and user-facing features
- Keep commits focused and descriptive

## Adding Dependencies

- Keep dependencies to a minimum
- For package-specific dependencies, add them to the package's package.json
- For shared dependencies, add them to the root package.json

## Release Process

Releases are managed by maintainers using:

1. Semantic versioning
2. Conventional commits for change logs
3. Lerna for package versioning

## Community Guidelines

- Be respectful and inclusive
- Focus on constructive feedback
- Consider usability and accessibility in your contributions
- Start with an issue to discuss significant changes

## Getting Help

If you have questions or need help:

- Open an issue with your question
- Reach out to the maintainers
- Check existing documentation first

Thank you for contributing to Continuum!