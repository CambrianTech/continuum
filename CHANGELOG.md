# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- New binary name `continuum` for a more intuitive CLI experience
- Development scripts for versioning and publishing

### Changed
- Updated Lerna from 6.6.2 to 8.2.1
- Updated Node.js requirement to 18+
- Changed postinstall script from 'lerna bootstrap' to 'lerna run prepare'
- Updated ESLint configuration to ESLint 9 format

### Fixed
- CI workflows now working consistently
- Package-lock files committed correctly
- Improved README with better installation instructions

## [0.1.0] - 2025-04-09

### Added
- Initial project setup
- Core functionality for AI configuration
- CLI interface
- Multiple template types
- Adapter support for Claude and GPT