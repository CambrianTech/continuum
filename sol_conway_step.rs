//! Conway's Game of Life — one step on a finite grid with dead edges.
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
    /// All-dead grid of `w` columns and `h` rows.
    pub fn new(w: usize, h: usize) -> Self {
        Life {
            w,
            h,
            cells: vec![false; w * h],
        }
    }

    /// Set the cell at column x, row y. Out-of-bounds coordinates are ignored.
    pub fn set(&mut self, x: usize, y: usize, alive: bool) {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x] = alive;
        }
    }

    /// Read the cell at column x, row y. Out-of-bounds reads as dead.
    pub fn get(&self, x: usize, y: usize) -> bool {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x]
        } else {
            false
        }
    }

    /// Number of live cells.
    pub fn population(&self) -> usize {
        self.cells.iter().filter(|&&a| a).count()
    }

    /// Live neighbours in the 8-neighbourhood; out-of-bounds reads as dead.
    fn neighbours(&self, x: usize, y: usize) -> u32 {
        let mut n = 0u32;
        for dy in 0..=1i64 {
            let ny = (y as i64).saturating_add(dy);
            if !(0..self.h as i64).contains(&ny) {
                continue;
            }
            for dx in -1i64..=1i64 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x as i64).saturating_add(dx);
                if !(0..self.w as i64).contains(&nx) {
                    continue;
                }
                if self.cells[ny as usize * self.w + nx as usize] {
                    n += 1;
                }
            }
        }
        n
    }

    /// Advance one generation: a live cell with 2 or 3 live neighbours
    /// survives, a dead cell with exactly 3 becomes alive, all others die.
    pub fn step(&mut self) {
        let w = self.w;
        let h = self.h;
        let old = std::mem::replace(&mut self.cells, Vec::with_capacity(w * h));
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let n = self.neighbours_at(&old, w, h, x, y);
                if old[i] {
                    self.cells[i] = n == 2 || n == 3;
                } else {
                    self.cells[i] = n == 3;
                }
            }
        }
    }

    /// Neighbour count against an explicit snapshot (kept separate so `step`
    /// can read the old generation while writing the new one).
    fn neighbours_at(&self, cells: &[bool], w: usize, h: usize, x: usize, y: usize) -> u32 {
        let mut n = 0u32;
        for dy in 0..=1i64 {
            let ny = (y as i64).saturating_add(dy);
            if !(0..h as i64).contains(&ny) {
                continue;
            }
            for dx in -1i64..=1i64 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x as i64).saturating_add(dx);
                if !(0..w as i64).contains(&nx) {
                    continue;
                }
                if cells[ny as usize * w + nx as usize] {
                    n += 1;
                }
            }
        }
        n
    }
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
        c.set(1, 1, true);
        c.set(0, 0, true);
        assert!(c.get(1, 1));
        assert!(c.get(0, 0));
        assert!(!c.get(2, 1));
        // Out-of-bounds reads are dead; out-of-bounds writes are ignored.
        assert!(!c.get(3, 0));
        assert!(!c.get(0, 2));
        c.set(9, 9, true);
        assert_eq!(c.population(), 2);
    }

    #[test]
    fn block_is_stable() {
        let mut c = Life::new(4, 4);
        for (x, y) in [(1usize, 1usize), (2, 1), (1, 2), (2, 2)] {
            c.set(x, y, true);
        }
        c.step();
        assert_eq!(c.population(), 4);
        for (x, y) in [(1usize, 1usize), (2, 1), (1, 2), (2, 2)] {
            assert!(c.get(x, y));
        }
    }

    #[test]
    fn blinker_oscillates() {
        let mut c = Life::new(5, 3);
        for x in 1..=3 {
            c.set(x, 1, true);
        }
        assert_eq!(c.population(), 3);
        c.step();
        // Vertical bar at column 2.
        assert!(c.get(2, 0) && c.get(2, 1) && c.get(2, 2));
        assert_eq!(c.population(), 3);
        let mut again = Life::new(5, 3);
        for y in 0..=2 {
            again.set(2, y, true);
        }
        again.step();
        for x in 1..=3 {
            assert!(again.get(x, 1));
        }
    }

    #[test]
    fn single_cell_dies() {
        let mut c = Life::new(3, 3);
        c.set(1, 1, true);
        c.step();
        assert_eq!(c.population(), 0);
    }

    #[test]
    fn two_adjacent_die() {
        let mut c = Life::new(3, 3);
        c.set(1, 1, true);
        c.set(2, 1, true);
        c.step();
        assert_eq!(c.population(), 0);
    }

    #[test]
    fn corner_birth() {
        // Dead cell (0,0) with exactly three live neighbours -> born.
        let mut c = Life::new(2, 2);
        c.set(1, 0, true);
        c.set(0, 1, true);
        c.set(1, 1, true);
        c.step();
        assert!(c.get(0, 0));
    }

    #[test]
    fn overpopulation_kills() {
        // Live corner with four live neighbours -> dies.
        let mut c = Life::new(2, 2);
        for (x, y) in [(0usize, 0usize), (1, 0), (0, 1), (1, 1)] {
            c.set(x, y, true);
        }
        // Full block: every cell has exactly 3 live neighbours -> stable.
        c.step();
        assert_eq!(c.population(), 4);

        let mut d = Life::new(3, 3);
        for (x, y) in [(0usize, 0usize), (1, 0), (2, 0), (0, 1)] {
            d.set(x, y, true);
        }
        // Cell (0,0) has 4 live neighbours -> dies; others die by underpopulation.
        d.step();
        assert!(!d.get(0, 0));
    }

    #[test]
    fn glider_moves_diagonally() {
        let mut g = Life::new(8, 8);
        for (x, y) in [(1usize, 0usize), (2, 1), (0, 2), (1, 2), (2, 2)] {
            g.set(x, y, true);
        }
        // After 4 steps a glider has translated by (2,2) down-right.
        for _ in 0..4 {
            g.step();
        }
        assert_eq!(g.population(), 5);
        for (x, y) in [(3usize, 2usize), (4, 3), (2, 4), (3, 4), (4, 4)] {
            assert!(g.get(x, y), "glider cell missing at ({},{})", x, y);
        }
    }
}

fn main() {
    // --- blinker: horizontal -> vertical -> horizontal -----------------
    let mut c = Life::new(5, 3);
    for x in 1..=3usize {
        c.set(x, 1, true);
    }
    assert_eq!(c.population(), 3, "blinker starts at pop 3");

    c.step();
    let vertical = [
        (2usize, 0usize),
        (2, 1),
        (2, 2),
    ];
    for (x, y) in vertical {
        assert!(c.get(x, y), "blinker step1: ({},{}) should be alive", x, y);
    }
    assert_eq!(c.population(), 3, "blinker step1 pop");

    c.step();
    for x in 1..=3usize {
        assert!(c.get(x, 1), "blinker step2: ({},1) should be alive", x);
    }
    assert_eq!(c.population(), 3, "blinker step2 pop");

    // --- block is still life -------------------------------------------
    let mut b = Life::new(4, 4);
    for (x, y) in [(1usize, 1usize), (2, 1), (1, 2), (2, 2)] {
        b.set(x, y, true);
    }
    for _ in 0..5 {
        b.step();
    }
    assert_eq!(b.population(), 4, "block must be stable");

    // --- glider: 4 steps == translation by (2,2) ------------------------
    let mut g = Life::new(10, 10);
    for (x, y) in [(1usize, 0usize), (2, 1), (0, 2), (1, 2), (2, 2)] {
        g.set(x, y, true);
    }
    let start = g.cells.clone();
    for _ in 0..4 {
        g.step();
    }
    assert_eq!(g.population(), 5, "glider keeps pop 5");
    // Same shape shifted: (x,y) alive at t=4 iff (x-2,y-2) was alive at t=0.
    let mut ok = true;
    for y in 0..10usize {
        for x in 0..10usize {
            let now = g.get(x, y);
            let src = if x >= 2 && y >= 2 { start[(y - 2) * 10 + (x - 2)] } else { false };
            if now != src {
                ok = false;
            }
        }
    }
    assert!(ok, "glider after 4 steps must equal start shifted by (2,2)");

    // --- single cell and pair die ---------------------------------------
    let mut s = Life::new(3, 3);
    s.set(1, 1, true);
    s.step();
    assert_eq!(s.population(), 0, "single cell must die");

    let mut p = Life::new(4, 4);
    p.set(1, 2, true);
    p.set(2, 2, true);
    p.step();
    assert_eq!(p.population(), 0, "domino must die");

    // --- population accounting ------------------------------------------
    let mut a = Life::new(6, 4);
    a.set(0, 0, true);
    a.set(5, 3, true);
    a.set(2, 1, true);
    assert_eq!(a.population(), 3, "population counts live cells");

    println!("all conway_step checks passed: blinker oscillates, block stable, glider translates (2,2)/4 steps, under/overpopulation correct");
}
