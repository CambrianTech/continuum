# fluid-sim — imported-from-the-wild design-bench task (verbatim prompt)

Provenance: circulating on X 2026-08-22 via @scaling01, prompt authored by
GPT-5.6-Sol per the post. Published wild baselines at import time: Qwen3.8-27B —
40k thinking tokens, ~1 hour, black screen (fail); Opus 4.5 — one-shot,
~1 minute, working (pass); "Ox Alpha" — one-shot, 1000-line file, working.
Grading here is NOT one-shot: the citizen iterates (render → observe →
hot-edit → re-grade) and is scored on the running artifact through the
eye-node. See docs/architecture/DESIGN-BENCH-VISUAL-CRAFT.md §6b.

## Prompt (verbatim — do not edit)

Create a polished, interactive real-time 2D fluid simulation that runs entirely
inside a single self-contained `index.html` file, with all HTML, CSS,
JavaScript, shaders, controls, and visual assets embedded directly in that file
and with no build process, server, external libraries, frameworks, imports, or
network dependencies.

The simulation must behave like a continuous fluid rather than a simple
particle animation, supporting visible velocity flow, advection, diffusion or
viscosity, pressure, incompressibility, vorticity or swirling motion,
dissipation, and the transport and mixing of colored dye through the simulated
fluid field.

Allow the user to interact directly with the fluid using the mouse or pointer
by dragging through the simulation to inject momentum and dye, with the
direction and speed of the drag influencing the resulting force and with
interaction remaining smooth during rapid or continuous movement.

Provide a compact real-time control interface containing at least pause/resume,
reset, clear dye, simulation resolution, timestep or simulation speed,
viscosity, pressure strength or pressure iterations, vorticity, velocity
dissipation, dye dissipation, interaction force, interaction radius, and dye
color controls, while giving the implementation freedom to choose suitable
ranges, defaults, widgets, and presentation.

Include multiple selectable visualization modes that expose meaningful aspects
of the simulation, such as rendered dye, velocity magnitude or direction,
pressure, divergence, vorticity, or another useful diagnostic representation,
and make switching between modes possible without restarting the simulation.

The application must automatically adapt its rendering surface and simulation
to browser-window resizing, support high-DPI displays appropriately, provide
usable mouse and touch or pointer input, and remain visually coherent across
common desktop viewport sizes.

Display a small performance and simulation-status overlay containing useful
live information such as frames per second, simulation dimensions, current
visualization mode, pause state, and any other metrics the implementation
considers valuable for evaluating performance.

Design the interface and fluid rendering to look intentional and
demonstration-ready, including a full-screen or near-full-screen simulation
area, readable controls, clear interaction feedback, visually rich dye mixing,
and sensible defaults that produce interesting fluid motion immediately without
requiring configuration.

The implementation may choose WebGL, WebGL2, Canvas, CPU techniques, GPU
shaders, numerical method details, data structures, rendering style,
optimization strategies, control layout, color treatment, and additional
features freely, but every required feature must work from the delivered HTML
file alone when opened in a modern browser.

Return only the complete working HTML document, and treat correctness,
stability, fluid-like behavior, responsiveness, interaction quality, visual
quality, performance, code organization, graceful handling of unsupported
capabilities, and completeness of the required feature set as benchmark
criteria.
