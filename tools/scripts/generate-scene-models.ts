#!/usr/bin/env npx tsx
/**
 * Generate starter room scene GLBs for persona environments.
 *
 * Each room is a proper glTF asset: walls, floor, desk, and basic furniture.
 * These are starter assets — replace with artist-made or AI-generated rooms later.
 *
 * Output: models/scenes/<name>.glb
 *
 * Runs automatically as part of `npm start` if scenes don't exist.
 * Uses @gltf-transform/core — no external tools or Python needed.
 */

import { Document, NodeIO } from '@gltf-transform/core';
import * as fs from 'fs';
import * as path from 'path';

const SCENES_DIR = path.join(__dirname, '..', 'models', 'scenes');
fs.mkdirSync(SCENES_DIR, { recursive: true });

// =============================================================================
// Geometry Helpers
// =============================================================================

/** Create a box mesh with given dimensions and color. */
function addBox(
    doc: Document,
    name: string,
    width: number,
    height: number,
    depth: number,
    color: [number, number, number],
    position: [number, number, number],
): void {
    const scene = doc.getRoot().listScenes()[0];
    const mesh = doc.createMesh(name);
    const prim = doc.createPrimitive();

    // Box vertices (24 vertices for proper normals — 4 per face)
    const hw = width / 2, hh = height / 2, hd = depth / 2;
    const [px, py, pz] = position;

    const positions: number[] = [];
    const normals: number[] = [];
    const indices: number[] = [];

    const faces: { normal: [number, number, number]; verts: [number, number, number][] }[] = [
        { normal: [0, 0, -1], verts: [[-hw, -hh, -hd], [hw, -hh, -hd], [hw, hh, -hd], [-hw, hh, -hd]] },
        { normal: [0, 0, 1], verts: [[hw, -hh, hd], [-hw, -hh, hd], [-hw, hh, hd], [hw, hh, hd]] },
        { normal: [-1, 0, 0], verts: [[-hw, -hh, hd], [-hw, -hh, -hd], [-hw, hh, -hd], [-hw, hh, hd]] },
        { normal: [1, 0, 0], verts: [[hw, -hh, -hd], [hw, -hh, hd], [hw, hh, hd], [hw, hh, -hd]] },
        { normal: [0, -1, 0], verts: [[-hw, -hh, hd], [hw, -hh, hd], [hw, -hh, -hd], [-hw, -hh, -hd]] },
        { normal: [0, 1, 0], verts: [[-hw, hh, -hd], [hw, hh, -hd], [hw, hh, hd], [-hw, hh, hd]] },
    ];

    for (const face of faces) {
        const base = positions.length / 3;
        for (const v of face.verts) {
            positions.push(v[0], v[1], v[2]);
            normals.push(face.normal[0], face.normal[1], face.normal[2]);
        }
        indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
    }

    const posAccessor = doc.createAccessor()
        .setType('VEC3')
        .setArray(new Float32Array(positions));
    const normAccessor = doc.createAccessor()
        .setType('VEC3')
        .setArray(new Float32Array(normals));
    const idxAccessor = doc.createAccessor()
        .setType('SCALAR')
        .setArray(new Uint16Array(indices));

    const material = doc.createMaterial(name + '-mat')
        .setBaseColorFactor([color[0] / 255, color[1] / 255, color[2] / 255, 1])
        .setRoughnessFactor(0.85)
        .setMetallicFactor(0.0);

    prim.setAttribute('POSITION', posAccessor)
        .setAttribute('NORMAL', normAccessor)
        .setIndices(idxAccessor)
        .setMaterial(material);

    mesh.addPrimitive(prim);

    const node = doc.createNode(name)
        .setMesh(mesh)
        .setTranslation([px, py, pz]);

    scene.addChild(node);
}

// =============================================================================
// Room Generators
// =============================================================================

function newDoc(sceneName: string): Document {
    const doc = new Document();
    doc.createBuffer();
    doc.createScene(sceneName);
    return doc;
}

function officeRoom(): Document {
    const doc = newDoc('Office');

    addBox(doc, 'back-wall', 3.0, 3.0, 0.02, [60, 55, 65], [0, 1.5, 0.4]);
    addBox(doc, 'floor', 3.0, 0.02, 2.0, [35, 32, 38], [0, -0.01, -0.2]);
    addBox(doc, 'desk-top', 1.2, 0.04, 0.6, [90, 65, 40], [0, 0.75, 0.10]);
    addBox(doc, 'desk-front', 1.2, 0.72, 0.02, [80, 58, 35], [0, 0.37, -0.19]);
    addBox(doc, 'left-wall', 0.02, 3.0, 1.5, [50, 47, 55], [-1.0, 1.5, -0.05]);
    addBox(doc, 'right-wall', 0.02, 3.0, 1.5, [50, 47, 55], [1.0, 1.5, -0.05]);
    addBox(doc, 'monitor-base', 0.25, 0.02, 0.15, [45, 45, 50], [0, 0.78, 0.12]);
    addBox(doc, 'monitor', 0.50, 0.30, 0.02, [30, 30, 35], [0, 0.95, 0.18]);
    addBox(doc, 'screen', 0.46, 0.26, 0.005, [40, 55, 70], [0, 0.95, 0.169]);
    // Bookshelf
    addBox(doc, 'shelf-back', 0.5, 1.2, 0.02, [70, 50, 35], [0.85, 1.4, 0.38]);
    for (let i = 0; i < 4; i++) {
        addBox(doc, `shelf-${i}`, 0.5, 0.02, 0.20, [75, 55, 38], [0.85, 0.85 + i * 0.3, 0.29]);
    }

    return doc;
}

function studioRoom(): Document {
    const doc = newDoc('Studio');

    addBox(doc, 'back-wall', 3.8, 3.0, 0.02, [55, 50, 58], [0, 1.5, 0.5]);
    addBox(doc, 'floor', 3.8, 0.02, 2.4, [55, 45, 35], [0, -0.01, -0.2]);
    addBox(doc, 'work-table', 1.6, 0.04, 0.7, [85, 75, 55], [0, 0.75, 0.10]);
    addBox(doc, 'trestle-left', 0.06, 0.74, 0.6, [75, 65, 45], [-0.65, 0.37, 0.10]);
    addBox(doc, 'trestle-right', 0.06, 0.74, 0.6, [75, 65, 45], [0.65, 0.37, 0.10]);
    // Art frames
    addBox(doc, 'frame-1', 0.5, 0.4, 0.02, [45, 40, 48], [-0.7, 1.6, 0.48]);
    addBox(doc, 'canvas-1', 0.44, 0.34, 0.005, [100, 75, 60], [-0.7, 1.6, 0.475]);
    addBox(doc, 'frame-2', 0.35, 0.5, 0.02, [45, 40, 48], [0.65, 1.55, 0.48]);
    addBox(doc, 'canvas-2', 0.29, 0.44, 0.005, [60, 85, 90], [0.65, 1.55, 0.475]);

    return doc;
}

function loungeRoom(): Document {
    const doc = newDoc('Lounge');

    addBox(doc, 'back-wall', 2.6, 2.6, 0.02, [65, 50, 45], [0, 1.3, 0.35]);
    addBox(doc, 'floor', 2.6, 0.02, 1.8, [40, 32, 28], [0, -0.01, -0.2]);
    addBox(doc, 'coffee-table', 0.9, 0.04, 0.5, [95, 68, 42], [0, 0.42, 0.08]);
    // Table legs
    for (const [x, z] of [[-0.38, -0.18], [0.38, -0.18], [-0.38, 0.30], [0.38, 0.30]] as [number, number][]) {
        addBox(doc, `table-leg-${x}-${z}`, 0.04, 0.40, 0.04, [85, 60, 38], [x, 0.21, z]);
    }
    addBox(doc, 'left-wall', 0.02, 2.4, 1.2, [55, 44, 40], [-0.8, 1.2, -0.05]);
    addBox(doc, 'right-wall', 0.02, 2.4, 1.2, [55, 44, 40], [0.8, 1.2, -0.05]);
    // Plant pot + foliage (simple boxes — replace with actual meshes later)
    addBox(doc, 'plant-pot', 0.16, 0.15, 0.16, [80, 55, 40], [-0.65, 0.08, 0.28]);
    addBox(doc, 'foliage', 0.25, 0.25, 0.25, [40, 75, 45], [-0.65, 0.30, 0.28]);
    addBox(doc, 'mug', 0.05, 0.07, 0.05, [120, 90, 60], [0.25, 0.47, 0.08]);

    return doc;
}

function homeOfficeRoom(): Document {
    const doc = newDoc('HomeOffice');

    addBox(doc, 'back-wall', 3.0, 3.0, 0.02, [62, 58, 55], [0, 1.5, 0.4]);
    addBox(doc, 'window', 0.7, 0.5, 0.005, [100, 115, 130], [-0.5, 1.7, 0.389]);
    // Window frame
    addBox(doc, 'frame-top', 0.74, 0.03, 0.01, [80, 75, 70], [-0.5, 1.96, 0.388]);
    addBox(doc, 'frame-bottom', 0.74, 0.03, 0.01, [80, 75, 70], [-0.5, 1.44, 0.388]);
    addBox(doc, 'frame-left', 0.03, 0.5, 0.01, [80, 75, 70], [-0.85, 1.7, 0.388]);
    addBox(doc, 'frame-right', 0.03, 0.5, 0.01, [80, 75, 70], [-0.15, 1.7, 0.388]);
    addBox(doc, 'floor', 3.0, 0.02, 2.0, [50, 40, 32], [0, -0.01, -0.2]);
    addBox(doc, 'desk', 1.3, 0.04, 0.6, [85, 70, 48], [0, 0.75, 0.10]);
    addBox(doc, 'left-wall', 0.02, 3.0, 1.5, [55, 52, 50], [-1.0, 1.5, -0.05]);
    addBox(doc, 'laptop-base', 0.30, 0.01, 0.20, [50, 50, 55], [0, 0.78, 0.10]);
    addBox(doc, 'laptop-screen', 0.30, 0.20, 0.01, [45, 45, 50], [0, 0.89, 0.20]);
    // Plant
    addBox(doc, 'plant-pot', 0.08, 0.08, 0.08, [90, 60, 45], [0.45, 0.81, 0.12]);
    addBox(doc, 'plant', 0.14, 0.14, 0.14, [45, 80, 50], [0.45, 0.92, 0.12]);
    addBox(doc, 'shelf', 0.6, 0.02, 0.15, [80, 65, 45], [0.6, 1.3, 0.33]);

    return doc;
}

function serverRoom(): Document {
    const doc = newDoc('ServerRoom');

    addBox(doc, 'back-wall', 3.0, 3.0, 0.02, [30, 32, 40], [0, 1.5, 0.4]);
    addBox(doc, 'floor', 3.0, 0.02, 2.0, [25, 25, 30], [0, -0.01, -0.2]);
    // Server racks
    for (const [i, x] of ([-0.7, 0, 0.7] as number[]).entries()) {
        addBox(doc, `rack-${i}`, 0.5, 2.0, 0.15, [35, 35, 42], [x, 1.0, 0.32]);
        for (let j = 0; j < 5; j++) {
            addBox(doc, `led-${i}-${j}`, 0.42, 0.008, 0.005, [40, 100, 180], [x, 0.3 + j * 0.4, 0.24]);
        }
    }
    addBox(doc, 'desk', 1.4, 0.04, 0.6, [40, 40, 48], [0, 0.75, 0.05]);
    // Dual monitors
    for (const [i, x] of ([-0.22, 0.22] as number[]).entries()) {
        addBox(doc, `monitor-${i}`, 0.38, 0.24, 0.02, [25, 25, 30], [x, 0.93, 0.15]);
        addBox(doc, `screen-${i}`, 0.34, 0.20, 0.005, [35, 50, 65], [x, 0.93, 0.139]);
    }

    return doc;
}

// =============================================================================
// Main
// =============================================================================

const ROOMS: Record<string, { name: string; gen: () => Document }> = {
    'office': { name: 'Standard Office', gen: officeRoom },
    'studio': { name: 'Creative Studio', gen: studioRoom },
    'lounge': { name: 'Cozy Lounge', gen: loungeRoom },
    'home-office': { name: 'Home Office', gen: homeOfficeRoom },
    'server-room': { name: 'Server Room', gen: serverRoom },
};

async function main() {
    const io = new NodeIO();
    let generated = 0;
    let existing = 0;

    for (const [id, { name, gen }] of Object.entries(ROOMS)) {
        const filepath = path.join(SCENES_DIR, `${id}.glb`);

        // Skip if already exists and non-empty
        if (fs.existsSync(filepath) && fs.statSync(filepath).size > 100) {
            existing++;
            continue;
        }

        const doc = gen();
        await io.write(filepath, doc);
        const size = (fs.statSync(filepath).size / 1024).toFixed(1);
        console.log(`  ${id}.glb${' '.repeat(20 - id.length)}${size} KB  (${name})`);
        generated++;
    }

    if (generated > 0) {
        console.log(`\n  ${generated} scene(s) generated, ${existing} already existed`);
    } else {
        console.log(`  All ${existing} scene models already exist`);
    }
}

main().catch(err => {
    console.error('Scene generation failed:', err);
    process.exit(1);
});
