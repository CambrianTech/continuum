/**
 * generate-content-types.ts — Generate content type constants from recipe JSON files
 *
 * Eliminates ALL hardcoded content type strings, switch statements, and registries.
 * The recipe JSON files on disk ARE the source of truth.
 *
 * Usage: npx tsx generator/generate-content-types.ts
 *
 * Generates:
 *   shared/generated/ContentTypes.ts — type union, icon map, path map, config
 *
 * After running this, ContentTypeRegistry.ts reads from generated code,
 * not from FALLBACK_REGISTRY.
 */

import * as fs from 'fs';
import * as path from 'path';

const RECIPES_DIR = path.resolve(__dirname, '../system/recipes');
const OUTPUT_FILE = path.resolve(__dirname, '../shared/generated/ContentTypes.ts');

interface RecipeFile {
    uniqueId: string;
    name: string;
    displayName?: string;
    description?: string;
    /** URL prefix — REQUIRED */
    view: string;
    entityType?: 'room' | 'user' | 'activity' | null;
    layout?: {
        main?: string[];
        right?: any;
        // New format: widgets array with position
        widgets?: Array<{ widget: string; position: string; order: number; config?: Record<string, unknown> }>;
    };
    team?: string[] | null;
    modes?: string[];
    inputs?: Record<string, any>;
    tags?: string[];
}

/** Extract the main/center widget from either layout format */
function getMainWidget(recipe: RecipeFile): string {
    const layout = recipe.layout;
    if (!layout) return 'chat-widget';

    // New format: widgets array with position
    if (layout.widgets && Array.isArray(layout.widgets)) {
        const center = layout.widgets.find(w => w.position === 'center');
        if (center) return center.widget;
        // Fall back to first widget if no center specified
        if (layout.widgets.length > 0) return layout.widgets[0].widget;
    }

    // Old format: main array
    if (layout.main && layout.main.length > 0) {
        return layout.main[0];
    }

    return 'chat-widget';
}

function main() {
    // 1. Read all recipe JSON files
    const recipeFiles = fs.readdirSync(RECIPES_DIR)
        .filter(f => f.endsWith('.json'))
        .sort();

    const recipes: RecipeFile[] = [];
    for (const file of recipeFiles) {
        try {
            const content = fs.readFileSync(path.join(RECIPES_DIR, file), 'utf-8');
            const recipe = JSON.parse(content) as RecipeFile;
            if (!recipe.uniqueId) {
                console.warn(`  SKIP: ${file} — missing uniqueId`);
                continue;
            }
            if (!recipe.view) {
                console.error(`  FATAL: ${file} — missing required 'view' field. Every recipe must define a URL prefix.`);
                process.exit(1);
            }
            recipes.push(recipe);
        } catch (e) {
            console.warn(`  SKIP: ${file} — ${(e as Error).message}`);
        }
    }

    console.log(`Found ${recipes.length} recipes in ${RECIPES_DIR}`);

    // 2. Extract content type IDs
    const typeIds = recipes.map(r => r.uniqueId);

    // 3. Build icon map from conventions
    // Icons are inferred from recipe metadata; override with explicit mapping if needed
    const ICON_MAP: Record<string, string> = {
        'chat': '💬', 'settings': '⚙️', 'theme': '🎨', 'help': '❓',
        'persona': '🤖', 'browser': '🌐', 'canvas': '🎯', 'diagnostics': '🔍',
        'diagnostics-log': '📋', 'live': '📹', 'profile': '👤', 'metrics': '📊',
        'grid-overview': '🖥️', 'training-dashboard': '🧪', 'inference-sample': '🔬',
        'genome-profile': '🧬', 'terminal': '💻', 'dm': '✉️',
    };

    // 4. Generate TypeScript
    const typeUnion = typeIds.map(id => `'${id}'`).join(' | ');

    const configEntries = recipes.map(r => {
        const widget = getMainWidget(r);
        const displayName = r.displayName || r.name || r.uniqueId;
        const icon = ICON_MAP[r.uniqueId] || '📄';
        // Check both old format (layout.right) and new format (layout.widgets with position: 'right')
        const hasRightPanelOld = r.layout?.right !== null && r.layout?.right !== undefined;
        const hasRightPanelNew = Array.isArray(r.layout?.widgets) && r.layout.widgets.some((w: any) => w.position === 'right');
        const hasRightPanel = hasRightPanelOld || hasRightPanelNew;
        const entityType = r.entityType || null;
        const requiresEntity = entityType !== null || !!r.inputs;

        const view = r.view;

        // Extract right panel widget and room from recipe layout
        const rightWidgets = Array.isArray(r.layout?.widgets)
            ? r.layout.widgets.filter((w: any) => w.position === 'right')
            : [];
        const rightPanelWidget = rightWidgets[0]?.widget || null;
        const rightPanelRoom = rightWidgets[0]?.config?.room || null;

        return `    '${r.uniqueId}': {
        widget: '${widget}',
        displayName: '${displayName}',
        icon: '${icon}',
        view: '${view}',
        requiresEntity: ${requiresEntity},
        entityType: ${entityType ? `'${entityType}'` : 'null'},
        hasRightPanel: ${hasRightPanel},
        rightPanelWidget: ${rightPanelWidget ? `'${rightPanelWidget}'` : 'null'},
        rightPanelRoom: ${rightPanelRoom ? `'${rightPanelRoom}'` : 'null'},
    }`;
    }).join(',\n');

    const output = `/**
 * AUTO-GENERATED by generate-content-types.ts — DO NOT EDIT
 *
 * Source of truth: system/recipes/*.json
 * Regenerate: npx tsx generator/generate-content-types.ts
 *
 * ${recipes.length} content types from ${recipeFiles.length} recipe files
 */

/**
 * All valid content type IDs — derived from recipe uniqueId fields.
 * Adding a new content type: create a recipe JSON, run the generator.
 */
export type ContentType = ${typeUnion};

/**
 * All content type IDs as a runtime array (for validation, iteration).
 */
export const CONTENT_TYPES: ContentType[] = [
${typeIds.map(id => `    '${id}'`).join(',\n')},
];

/**
 * Content type configuration — generated from recipe metadata.
 */
export type EntityType = 'room' | 'user' | 'activity' | null;

export interface ContentTypeConfig {
    widget: string;
    displayName: string;
    icon: string;
    /** URL prefix — verb/noun pattern: /chat, /live, /profile */
    view: string;
    requiresEntity: boolean;
    entityType: EntityType;
    hasRightPanel: boolean;
    /** Right panel widget tag — from recipe layout. null = no right panel. */
    rightPanelWidget: string | null;
    /** Room uniqueId for right panel chat — from recipe layout config */
    rightPanelRoom: string | null;
}

export const CONTENT_TYPE_CONFIGS: Record<ContentType, ContentTypeConfig> = {
${configEntries},
};

/**
 * Type guard: is this string a valid content type?
 */
export function isContentType(value: string): value is ContentType {
    return CONTENT_TYPES.includes(value as ContentType);
}

/**
 * Get config for a content type, or undefined if not registered.
 */
export function getContentTypeConfig(type: string): ContentTypeConfig | undefined {
    return CONTENT_TYPE_CONFIGS[type as ContentType];
}

/**
 * Get icon for a content type.
 */
export function getContentTypeIcon(type: string): string {
    return CONTENT_TYPE_CONFIGS[type as ContentType]?.icon || '📄';
}
`;

    // 5. Write output
    const outputDir = path.dirname(OUTPUT_FILE);
    if (!fs.existsSync(outputDir)) {
        fs.mkdirSync(outputDir, { recursive: true });
    }
    fs.writeFileSync(OUTPUT_FILE, output);
    console.log(`Generated: ${OUTPUT_FILE}`);
    console.log(`  ${recipes.length} content types`);
    console.log(`  Type union: ContentType`);
    console.log(`  Config map: CONTENT_TYPE_CONFIGS`);
    console.log(`  Validation: isContentType()`);
}

main();
