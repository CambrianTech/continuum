# Release Process for Continuum

This document describes the process for releasing new versions of Continuum.

## Pre-Release Versioning (0.x.x)

During pre-release development, Continuum follows semantic versioning with the following principles:

- **Patch (0.0.x)**: Bug fixes and minor changes
- **Minor (0.x.0)**: New features that don't break compatibility 
- **Major (1.0.0)**: The first stable release

## Release Checklist

1. **Ensure tests pass**
   ```bash
   npm test
   ```

2. **Ensure linting passes**
   ```bash
   npm run lint
   ```

3. **Update version**
   
   Choose one of:
   ```bash
   # For bug fixes
   npm run version:patch
   
   # For new features
   npm run version:minor
   
   # For major changes
   npm run version:major
   ```

4. **Update changelog**
   
   Add new version details to `CHANGELOG.md`

5. **Commit version changes**
   ```bash
   git add .
   git commit -m "chore: release v0.x.x"
   ```

6. **Tag release**
   ```bash
   git tag v0.x.x
   ```

7. **Push changes**
   ```bash
   git push
   git push --tags
   ```

8. **Publish to npm**
   
   For stable releases:
   ```bash
   npm login
   lerna publish from-git
   ```
   
   For development releases:
   ```bash
   npm run publish:dev
   ```

## Post-Release

1. Verify package is available on npm
2. Test global installation:
   ```bash
   npm install -g @continuum/cli
   continuum --version
   ```
3. Close relevant GitHub issues and milestones
4. Announce release in appropriate channels

## Hotfix Process

For urgent fixes to a release:

1. Create a branch from the release tag
2. Apply fix
3. Follow steps 1-8 from Release Checklist
4. Use patch version increment