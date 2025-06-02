# Lerna 8.x Upgrade Guide

This PR updates Lerna from v6.6.2 to v8.2.1, which includes some breaking changes. To ensure a smooth transition, please follow these steps:

## Prerequisites

- **Node.js**: Version 18 or higher is required (Lerna 8 dropped support for Node 16)
- **Update `.github/workflows/*.yml`**: Make sure GitHub Actions use Node 18+

## Upgrade Steps

1. Merge this PR to update the dependency version

2. Run the update helper script:
```bash
# From the project root
./scripts/update-lerna.sh
```

3. Update your postinstall script if needed:
```json
"postinstall": "lerna run prepare"
```

4. Test that all monorepo commands work as expected:
```bash
npm run build
npm test
```

## Key Changes in Lerna 8

- Node.js v16 support has been dropped
- Configuration changes may be needed (the repair script handles most of these)
- Performance improvements for monorepo operations
- Enhanced version management

## Troubleshooting

If you encounter issues:

1. Check the [Lerna Changelog](https://github.com/lerna/lerna/blob/main/CHANGELOG.md)
2. Run `npx lerna repair` to fix configuration issues
3. Make sure your Node version is compatible (≥18.0.0)

## Related Issues

This upgrade helps modernize our build toolchain and keeps us on supported versions of the tools we depend on.