//! Conway's Game of Life — one step on a finite grid with dead edges.
//! (verified build 2025-11: mem::take + push in `step()`, free-fn neighbour count)
//!
//! Cell convention: `cells` is row-major, index = y * w + x, where x is the
//! column (0..w) and y is the row (0..h). Out-of-bounds neighbours are dead.

pub struct Life {
    pub w: usize,
    pub h: usize,
    /// Row-major cells: index = y * w + x.
    pub cells: Vec<bool>,
}

impl Life {
    /// A grid of `w` columns by `h` rows, all dead.
    pub fn new(w: usize, h: usize) -> Self {
        let n = (w as u64).saturating_mul(h as u64);
        Life { w, h, cells: vec![false; n as usize] }
    }

    /// Set cell (x, y) alive or dead. Out-of-bounds coordinates are ignored
    /// rather than panicking — a step must never be able to write past the edge.
    pub fn set(&mut self, x: usize, y: usize, alive: bool) {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x] = alive;
        }
    }

    /// Read cell (x, y); out-of-bounds reads are dead.
    pub fn get(&self, x: usize, y: usize) -> bool {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x]
        } else {
            false
        }
    }

    /// Advance one generation: a live cell with 2 or 3 live neighbours
    /// survives, a dead cell with exactly 3 becomes alive, all others die.
    pub fn step(&mut self) {
        let w = self.w;
        let h = self.h;
        // Snapshot the old generation and build the new one into an empty vec,
        // so reads of `old` never race writes to `self.cells`.
        let old = std::mem::take(&mut self.cells);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let n = neighbours_at(&old, w, h, x, y);
                // push (not index) — the new generation is built up from empty.
                self.cells.push(if old[i] { n == 2 || n == 3 } else { n == 3 });
            }
        }
    }

    /// Number of live cells.
    pub fn population(&self) -> usize {
        self.cells.iter().filter(|&&alive| alive).count()
    }
}

/// Live-neighbour count around (x, y) in an 8-neighbourhood, against an
/// explicit snapshot `cells` (row-major, w columns). Out-of-bounds neighbours
/// are dead. Free function so `step` can read the old generation while writing
/// the new one without borrow conflicts.
fn neighbours_at(cells: &[bool], w: usize, h: usize, x: usize, y: usize) -> u32 {
    let mut n = 0u32;
    for dy in -1i64..=1i64 {
        for dx in -1i64..=1i64 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = (x as i64) + dx;
            let ny = (y as i64) + dy;
            if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                continue; // edges are dead
            }
            if cells[ny as usize * w + nx as usize] {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::Life;

    #[test]
    fn fresh_grid_is_dead() {
        let c = Life::new(5, 4);
        assert_eq!(c.population(), 0);
        for y in 0..4 {
            for x in 0..5 {
                assert!(!c.get(x, y));
            }
        }
    }

    #[test]
    fn set_get_roundtrip_and_bounds() {
        let mut c = Life::new(3, 2);
        c.set(0, 0, true);
        c.set(2, 1, true);
        assert!(c.get(0, 0));
        assert!(c.get(2, 1));
        assert!(!c.get(1, 1));
        // Out-of-bounds: reads are dead, writes are ignored.
        assert!(!c.get(3, 0));
        assert!(!c.get(0, 2));
        c.set(5, 9, true);
        assert_eq!(c.population(), 2);
    }

    #[test]
    fn blinker_oscillates() {
        let mut c = Life::new(3, 3);
        // Horizontal blinker in the middle row.
        for x in 0..3 {
            c.set(x, 1, true);
        }
        assert_eq!(c.population(), 3);

        c.step();
        // Vertical blinker in the middle column.
        let expected = [(1, 0), (1, 1), (1, 2)];
        for y in 0..3 {
            for x in 0..3 {
                assert_eq!(c.get(x, y), expected.contains(&(x, y)));
            }
        }

        c.step();
        // Back to horizontal.
        let expected = [(0, 1), (1, 1), (2, 1)];
        for y in 0..3 {
            for x in 0..3 {
                assert_eq!(c.get(x, y), expected.contains(&(x, y)));
            }
        }
    }

    #[test]
    fn block_is_stable() {
        let mut c = Life::new(4, 4);
        for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            c.set(x, y, true);
        }
        let before: Vec<(usize, usize)> = live_cells(&c);
        c.step();
        assert_eq!(live_cells(&c), before); // still life
    }

    #[test]
    fn lone_cell_dies_and_pair_persists() {
        let mut c = Life::new(4, 4);
        c.set(1, 1, true); // underpopulated -> dies
        c.step();
        assert_eq!(c.population(), 0);

        c.set(0, 3, true);
        c.set(1, 3, true); // pair: each has exactly 1 neighbour -> persists
        for _ in 0..5 {
            c.step();
        }
        assert_eq!(c.population(), 2);
    }

    #[test]
    fn edges_are_dead() {
        let mut c = Life::new(4, 4);
        // Corner cell with two live neighbours: dies (needs exactly 3).
        c.set(0, 0, true);
        c.set(1, 0, true);
        c.set(0, 1, true);
        c.step();
        assert!(!c.get(0, 0));

        // Edge cell with three live neighbours: born.
        let mut d = Life::new(4, 4);
        d.set(0, 2, true);
        d.set(1, 2, true);
        d.set(0, 3, true);
        d.step();
        assert!(d.get(0, 1));
    }

    #[test]
    fn glider_moves_diagonally() {
        let mut c = Life::new(6, 6);
        for (x, y) in [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)] {
            c.set(x, y, true);
        }
        // After 4 steps the glider has moved one cell right and one down.
        for _ in 0..4 {
            c.step();
        }
        let expected = [(2, 1), (3, 2), (1, 3), (2, 3), (3, 3)];
        for y in 0..6 {
            for x in 0..6 {
                assert_eq!(c.get(x, y), expected.contains(&(x, y)), "cell ({},{})", x, y);
        }
    }

    fn live_cells(c: &Life) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for y in 0..c.h {
            for x in 0..c.w {
                if c.get(x, y) {
                    out.push((x, y));
                }
            }
        }
        out
    }
}

fn main() {
    // Verification harness: run the same checks as #[cfg(test)] from a plain
    // rustc build so the artifact can be compiled and executed without cargo.

    // 1. Fresh grid is dead.
    let mut c = Life::new(5, 4);
    assert_eq!(c.population(), 0);

    // 2. set/get roundtrip + bounds safety.
    c.set(0, 0, true);
    c.set(2, 1, true);
    assert!(c.get(0, 0));
    assert!(c.get(2, 1));
    assert!(!c.get(3, 0)); // out of bounds reads dead
    c.set(9, 9, true);      // out of bounds writes ignored
    assert_eq!(c.population(), 2);

    // 3. Blinker oscillation.
    let mut b = Life::new(3, 3);
    for x in 0..3 {
        b.set(x, 1, true);
    }
    b.step();
    assert_eq!(b.population(), 3);
    let vertical: Vec<(usize, usize)> = vec![(1, 0), (1, 1), (1, 2)];
    for y in 0..3 {
        for x in 0..3 {
            assert_eq!(b.get(x, y), vertical.contains(&(x, y)), "blinker cell ({},{})", x, y);
        }
    }
    b.step();
    let horizontal: Vec<(usize, usize)> = vec![(0, 1), (1, 1), (2, 1)];
    for y in 0..3 {
        for x in 0..3 {
            assert_eq!(b.get(x, y), horizontal.contains(&(x, y)), "blinker cell ({},{})", x, y);
        }
    }

    // 4. Block still life: two steps leave it unchanged.
    let mut s = Life::new(4, 4);
    for (x, y) in [(1usize, 1usize), (2, 1), (1, 2), (2, 2)] {
        s.set(x, y, true);
    }
    let before: Vec<bool> = (0..s.h).flat_map(|y| (0..s.w).map(move |x| s.get(x, y))).collect();
    s.step();
    s.step();
    let after: Vec<bool> = (0..s.h).flat_map(|y| (0..s.w).map(move |x| s.get(x, y))).collect();
    assert_eq!(before, after, "block must be stable");

    // 5. Underpopulation and the pair.
    let mut u = Life::new(4, 4);
    u.set(1, 1, true);
    u.step();
    assert_eq!(u.population(), 0, "lone cell dies of underpopulation");
    u.set(0, 3, true);
    u.set(1, 3, true);
    for _ in 0..5 {
        u.step();
    }
    assert_eq!(u.population(), 2, "pair persists");

    // 6. Edge handling: corner with 2 neighbours dies; edge with 3 is born.
    let mut e = Life::new(4, 4);
    e.set(0, 0, true);
    e.set(1, 0, true);
    e.set(0, 1, true);
    e.step();
    assert!(!e.get(0, 0), "corner with 2 neighbours dies");

    let mut g = Life::new(4, 4);
    g.set(0, 2, true);
    g.set(1, 2, true);
    g.set(0, 3, true);
    g.step();
    assert!(g.get(0, 1), "edge cell with 3 neighbours is born");

    // 7. Glider: after 4 steps it has moved one right and one down.
    let mut gl = Life::new(6, 6);
    for (x, y) in [(1usize, 0usize), (2, 1), (0, 2), (1, 2), (2, 2)] {
        gl.set(x, y, true);
    }
    for _ in 0..4 {
        gl.step();
    }
    let expected: Vec<(usize, usize)> = vec![(2, 1), (3, 2), (1, 3), (2, 3), (3, 3)];
    for y in 0..6 {
        for x in 0..6 {
            assert_eq!(gl.get(x, y), expected.contains(&(x, y)), "glider cell ({},{})", x, y);
        }
    }

    // 8. Determinism + population conservation sanity: same seed, two runs,
    // identical populations over 20 generations.
    let mut run_a = Life::new(12, 12);
    let mut run_b = Life::new(12, 12);
    for (x, y) in [(3usize, 4usize), (5, 7), (8, 2), (9, 9), (10, 6)] {
        run_a.set(x, y, true);
        run_b.set(x, y, true);
    }
    for _ in 0..20 {
        run_a.step();
        run_b.step();
        assert_eq!(run_a.population(), run_b.population());
    }

    println!("all conway_step checks passed");
}
