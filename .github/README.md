# Continuum CI/CD

This directory contains GitHub-specific configuration files for Continuum's CI/CD pipelines and other GitHub integrations.

## Workflows

### Main CI Pipeline (`ci.yml`)

The main CI workflow runs on all pushes to `main` and pull requests. It:

- Builds the project
- Runs linting checks
- Executes tests
- Validates against multiple Node.js versions

### AI Config Validation (`validate-ai-config.yml`)

This workflow specifically validates AI configuration files against the schema:

- Runs when changes are made to schema, templates, or examples
- Validates all template configs against the schema
- Tests example scripts to ensure they work as expected

### Dependency Check (`dependencies.yml`)

This workflow checks package dependencies:

- Runs on changes to package.json files
- Performs npm audit for security vulnerabilities
- Checks for outdated dependencies
- Runs weekly to catch new security advisories

## Issue & PR Templates

- `ISSUE_TEMPLATE/bug_report.md`: Template for bug reports
- `ISSUE_TEMPLATE/feature_request.md`: Template for feature requests
- `PULL_REQUEST_TEMPLATE.md`: Template for pull requests

## GitHub Configuration

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [GitHub Community Contributing Guide](https://github.com/github/docs/blob/main/CONTRIBUTING.md)