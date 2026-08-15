#!/usr/bin/env npx tsx
/**
 * Widget Generator CLI Entry Point
 *
 * Generates new widgets with all necessary files:
 * - TypeScript widget class
 * - HTML template
 * - SCSS styles
 * - README documentation
 * - Recipe JSON
 *
 * Usage:
 *   npx tsx generator/generate-widget.ts --template        # Show example spec
 *   npx tsx generator/generate-widget.ts --test            # Generate test widget to /tmp
 *   npx tsx generator/generate-widget.ts <spec.json>       # Generate from spec file
 *   npx tsx generator/generate-widget.ts <spec.json> --force  # Overwrite existing
 *
 * Example spec.json:
 * {
 *   "name": "MyFeature",
 *   "description": "A widget that does something useful",
 *   "displayName": "My Feature",
 *   "pathPrefix": "/my-feature",
 *   "requiresEntity": false,
 *   "rightPanel": { "room": "help", "compact": true }
 * }
 */

// Re-export CLI from WidgetGenerator
import './WidgetGenerator';
