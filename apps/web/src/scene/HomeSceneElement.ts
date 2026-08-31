/**
 * `<home-scene>` — the three.js render target for a citizen's HOME
 * (CITIZEN-HOMES-ORTHOGRAPHIC.md): a REAL 3D scene — meshes, lights,
 * shadows, an orthographic sims camera with orbit/zoom navigation — never
 * sprite/CSS stand-ins (Joel: "full sims game like… do a full 3d scene
 * render").
 *
 * It builds ONLY from [`HomeSceneModel`] — the engine-neutral contract the
 * native bevy target shares — and exposes the LiveKit dock: the office
 * SCREEN is a named mesh (`this.officeScreen`); a live call sets its
 * material map to a `VideoTexture` and the call is IN the room, on the
 * same surface, in every engine.
 *
 * Lifecycle discipline: one renderer per element, sized to the card,
 * dPR-aware, disposed on disconnect. The render loop runs ONLY while the
 * element is on screen (IntersectionObserver) and idles at a gentle
 * cadence — a profile card must never tax the transcript's paint budget.
 */

import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import type { HomeSceneModel } from './homeSceneModel';

const ACCENT = 0x35d0e0;
const WARM = 0xffd664;
const TROPHY = 0xd4a017;
const LEAF = 0x3fb950;

export class HomeSceneElement extends HTMLElement {
  private renderer?: THREE.WebGLRenderer;
  private scene?: THREE.Scene;
  private camera?: THREE.OrthographicCamera;
  private controls?: OrbitControls;
  private raf = 0;
  private visible = false;
  private io?: IntersectionObserver;
  private ro?: ResizeObserver;
  private _model?: HomeSceneModel;
  private avatar?: THREE.Group;
  private windowGlow?: THREE.PointLight;
  private clock = new THREE.Clock();

  /** The LiveKit dock — a call sets `officeScreen.material.map` to a
   *  VideoTexture; the screen is the SAME semantic surface in bevy. */
  officeScreen?: THREE.Mesh<THREE.PlaneGeometry, THREE.MeshBasicMaterial>;

  set model(m: HomeSceneModel | undefined) {
    this._model = m;
    if (m && this.renderer) this.buildScene(m);
  }
  get model(): HomeSceneModel | undefined {
    return this._model;
  }

  connectedCallback(): void {
    const canvas = document.createElement('canvas');
    canvas.style.width = '100%';
    canvas.style.height = '100%';
    canvas.style.display = 'block';
    canvas.style.borderRadius = '6px';
    this.style.display = 'block';
    this.style.height = '260px';
    this.appendChild(canvas);

    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;

    // Sims lens: orthographic, elevated three-quarter view.
    const cam = new THREE.OrthographicCamera(-6, 6, 4.5, -3, 0.1, 100);
    cam.position.set(9, 8, 9);
    cam.lookAt(0, 0.8, 0);
    this.camera = cam;

    // Navigation — real orbit/zoom (drag to turn the lot, wheel to zoom),
    // clamped so the reader can't wander under the floor.
    const controls = new OrbitControls(cam, canvas);
    controls.enablePan = false;
    controls.minZoom = 0.7;
    controls.maxZoom = 2.2;
    controls.maxPolarAngle = Math.PI / 2.4;
    controls.minPolarAngle = Math.PI / 5;
    controls.target.set(0, 0.8, 0);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;
    this.controls = controls;

    if (this._model) this.buildScene(this._model);

    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(this);
    this.io = new IntersectionObserver((entries) => {
      this.visible = entries.some((e) => e.isIntersecting);
      if (this.visible) this.loop();
    });
    this.io.observe(this);
    this.resize();
  }

  disconnectedCallback(): void {
    cancelAnimationFrame(this.raf);
    this.io?.disconnect();
    this.ro?.disconnect();
    this.controls?.dispose();
    this.scene?.traverse((o) => {
      const mesh = o as THREE.Mesh;
      if (mesh.geometry) mesh.geometry.dispose();
      const mat = mesh.material as THREE.Material | THREE.Material[] | undefined;
      if (Array.isArray(mat)) mat.forEach((m) => m.dispose());
      else mat?.dispose();
    });
    this.renderer?.dispose();
    this.renderer = undefined;
  }

  private resize(): void {
    if (!this.renderer || !this.camera) return;
    const w = this.clientWidth || 360;
    const h = this.clientHeight || 260;
    this.renderer.setSize(w, h, false);
    const aspect = w / h;
    const viewH = 4.2;
    this.camera.left = -viewH * aspect * 0.9;
    this.camera.right = viewH * aspect * 0.9;
    this.camera.top = viewH;
    this.camera.bottom = -viewH * 0.55;
    this.camera.updateProjectionMatrix();
  }

  private loop = (): void => {
    if (!this.visible || !this.renderer || !this.scene || !this.camera) return;
    const t = this.clock.getElapsedTime();
    // Idle life: her avatar breathes; the window light flickers candle-soft.
    if (this.avatar) {
      this.avatar.position.y = 0.02 * Math.sin(t * 1.6);
      if (this._model?.speaking) this.avatar.rotation.y = 0.15 * Math.sin(t * 6);
    }
    if (this.windowGlow) this.windowGlow.intensity = 1.6 + 0.12 * Math.sin(t * 2.3);
    this.controls?.update();
    this.renderer.render(this.scene, this.camera);
    this.raf = requestAnimationFrame(this.loop);
  };

  private buildScene(m: HomeSceneModel): void {
    const scene = new THREE.Scene();
    this.scene = scene;

    // ── light rig: quiet ambient + a keylight; the window is a warm point
    // light ONLY when she is home (a fact, not decor).
    scene.add(new THREE.AmbientLight(0x8899aa, 0.55));
    const key = new THREE.DirectionalLight(0xdde6ff, 1.1);
    key.position.set(6, 10, 4);
    key.castShadow = true;
    key.shadow.mapSize.set(1024, 1024);
    scene.add(key);

    const matFloor = new THREE.MeshStandardMaterial({ color: 0x1a2230, roughness: 0.9 });
    const matWall = new THREE.MeshStandardMaterial({ color: 0x223047, roughness: 0.95 });
    const matDesk = new THREE.MeshStandardMaterial({ color: 0x39485f, roughness: 0.6 });

    // ── shell
    const floor = new THREE.Mesh(new THREE.BoxGeometry(m.width, 0.15, m.depth), matFloor);
    floor.receiveShadow = true;
    scene.add(floor);
    const wallN = new THREE.Mesh(new THREE.BoxGeometry(m.width, m.wallHeight, 0.12), matWall);
    wallN.position.set(0, m.wallHeight / 2, -m.depth / 2);
    wallN.receiveShadow = true;
    scene.add(wallN);
    const wallW = new THREE.Mesh(new THREE.BoxGeometry(0.12, m.wallHeight, m.depth), matWall);
    wallW.position.set(-m.width / 2, m.wallHeight / 2, 0);
    wallW.receiveShadow = true;
    scene.add(wallW);

    // ── the window (west wall): lit = online.
    const windowPane = new THREE.Mesh(
      new THREE.PlaneGeometry(1.6, 1.1),
      new THREE.MeshBasicMaterial({
        color: m.online ? WARM : 0x2a3448,
        transparent: true,
        opacity: m.online ? 0.9 : 0.6,
      }),
    );
    windowPane.position.set(-m.width / 2 + 0.07, 1.5, -0.6);
    windowPane.rotation.y = Math.PI / 2;
    scene.add(windowPane);
    if (m.online) {
      const glow = new THREE.PointLight(WARM, 1.6, 6);
      glow.position.set(-m.width / 2 + 0.5, 1.5, -0.6);
      scene.add(glow);
      this.windowGlow = glow;
    }

    // ── OFFICE: desk + the SCREEN (the LiveKit dock) + one glowing item
    // per active run.
    const office = m.regions.find((r) => r.key === 'office');
    if (office) {
      const desk = new THREE.Mesh(new THREE.BoxGeometry(office.w, 0.1, office.d), matDesk);
      desk.position.set(office.x, 0.72, office.z);
      desk.castShadow = true;
      scene.add(desk);
      for (const [i] of Array.from({ length: 4 }, (_, i) => [i] as const)) {
        const leg = new THREE.Mesh(new THREE.BoxGeometry(0.08, 0.65, 0.08), matDesk);
        leg.position.set(
          office.x + (i % 2 === 0 ? -1 : 1) * (office.w / 2 - 0.1),
          0.36,
          office.z + (i < 2 ? -1 : 1) * (office.d / 2 - 0.1),
        );
        scene.add(leg);
      }
      const screen = new THREE.Mesh(
        new THREE.PlaneGeometry(1.1, 0.65),
        new THREE.MeshBasicMaterial({ color: 0x0c1420 }),
      );
      screen.position.set(office.x, 1.25, office.z - office.d / 2 + 0.05);
      scene.add(screen);
      this.officeScreen = screen;
      const runs = Math.min(m.activeRuns, 4);
      for (let i = 0; i < runs; i++) {
        const item = new THREE.Mesh(
          new THREE.BoxGeometry(0.18, 0.18, 0.18),
          new THREE.MeshStandardMaterial({
            color: ACCENT,
            emissive: ACCENT,
            emissiveIntensity: 0.7,
          }),
        );
        item.position.set(office.x - office.w / 2 + 0.35 + i * 0.4, 0.86, office.z + 0.25);
        item.castShadow = true;
        scene.add(item);
      }
    }

    // ── SHELF: a trophy per resolved verdict.
    const shelf = m.regions.find((r) => r.key === 'shelf');
    if (shelf) {
      const board = new THREE.Mesh(new THREE.BoxGeometry(shelf.w, 0.06, shelf.d), matDesk);
      board.position.set(shelf.x, 1.6, -m.depth / 2 + shelf.d / 2 + 0.12);
      scene.add(board);
      const trophies = Math.min(m.trophies, 8);
      for (let i = 0; i < trophies; i++) {
        const cup = new THREE.Mesh(
          new THREE.ConeGeometry(0.09, 0.24, 12),
          new THREE.MeshStandardMaterial({ color: TROPHY, metalness: 0.7, roughness: 0.3 }),
        );
        cup.position.set(shelf.x - shelf.w / 2 + 0.3 + i * 0.38, 1.76, board.position.z);
        cup.castShadow = true;
        scene.add(cup);
      }
    }

    // ── GARDEN: the genome plant — a leaf whorl per gene.
    const garden = m.regions.find((r) => r.key === 'garden');
    if (garden) {
      const pot = new THREE.Mesh(
        new THREE.CylinderGeometry(0.22, 0.16, 0.3, 14),
        new THREE.MeshStandardMaterial({ color: 0x5a3d2b, roughness: 0.9 }),
      );
      pot.position.set(garden.x, 0.23, garden.z);
      pot.castShadow = true;
      scene.add(pot);
      const height = 0.35 + Math.min(m.genes, 6) * 0.16;
      const stem = new THREE.Mesh(
        new THREE.CylinderGeometry(0.025, 0.035, height, 8),
        new THREE.MeshStandardMaterial({ color: LEAF, roughness: 0.8 }),
      );
      stem.position.set(garden.x, 0.38 + height / 2, garden.z);
      scene.add(stem);
      for (let i = 0; i < Math.min(m.genes + 1, 7); i++) {
        const leaf = new THREE.Mesh(
          new THREE.SphereGeometry(0.11, 10, 8),
          new THREE.MeshStandardMaterial({ color: LEAF, roughness: 0.7 }),
        );
        const a = i * 2.4;
        leaf.position.set(
          garden.x + 0.16 * Math.cos(a),
          0.55 + i * 0.14,
          garden.z + 0.16 * Math.sin(a),
        );
        leaf.scale.set(1.4, 0.5, 1);
        leaf.castShadow = true;
        scene.add(leaf);
      }
    }

    // ── HER: a simple capsule avatar, present exactly when she is.
    this.avatar = undefined;
    if (m.online) {
      const g = new THREE.Group();
      const body = new THREE.Mesh(
        new THREE.CapsuleGeometry(0.22, 0.5, 6, 12),
        new THREE.MeshStandardMaterial({
          color: ACCENT,
          emissive: ACCENT,
          emissiveIntensity: m.speaking ? 0.6 : 0.25,
          roughness: 0.4,
        }),
      );
      body.position.y = 0.62;
      body.castShadow = true;
      g.add(body);
      g.position.set(0.4, 0, 0.6);
      scene.add(g);
      this.avatar = g;
    }

    if (this.visible) this.loop();
    else if (this.renderer && this.camera) this.renderer.render(scene, this.camera);
  }
}

customElements.define('home-scene', HomeSceneElement);

declare global {
  interface HTMLElementTagNameMap {
    'home-scene': HomeSceneElement;
  }
}
