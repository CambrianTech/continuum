//! Conway's Game of Life — one step on a finite grid with dead edges.
//!
//! Cell convention: `cells` is row-major, index = y * w + x (x: column 0..w,
//! y: row 0..h). Out-of-bounds neighbours are always dead; out-of-bounds
//! reads return false and out-of-bounds writes are ignored.

pub struct Life {
    pub w: usize,
    pub h: usize,
    /// Row-major cells: index = y * w + x.
    pub cells: Vec<bool>,
}

impl Life {
    /// A new grid with every cell dead.
    pub fn new(w: usize, h: usize) -> Self {
        Life { w, h, cells: vec![false; w * h] }
    }

    /// Set a cell alive or dead. Out-of-bounds coordinates are ignored.
    pub fn set(&mut self, x: usize, y: usize, alive: bool) {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x] = alive;
        }
    }

    /// Read a cell; out-of-bounds coordinates read as dead.
    pub fn get(&self, x: usize, y: usize) -> bool {
        if x < self.w && y < self.h {
            self.cells[y * self.w + x]
        } else {
            false
        }
    }

    /// Advance one generation (in place). A live cell with 2 or 3 live
    /// neighbours survives; a dead cell with exactly 3 becomes alive; all
    /// other cells die. 8-neighbourhood, edges are dead.
    pub fn step(&mut self) {
        let w = self.w;
        let h = self.h;
        // Snapshot the old generation so we never read and write the same cell.
        let mut next: Vec<bool> = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let n = count_live_neighbours(&self.cells, w, h, x, y);
                next.push(if self.cells[i] { n == 2 || n == 3 } else { n == 3 });
            }
        }
        self.cells = next;
    }

    /// Number of live cells.
    pub fn population(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }
}

/// Count live neighbours (8-neighbourhood, edges dead) in a row-major snapshot.
fn count_live_neighbours(cells: &[bool], w: usize, h: usize, x: usize, y: usize) -> u32 {
    let mut n = 0u32;
    for dy in -1i64..=1i64 {
        for dx in -1i64..=1i64 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as i64 + dx;
            let ny = y as i64 + dy;
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
        for y in 0..4usize {
            for x in 0..5usize {
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
        for x in 0..3usize {
            c.set(x, 1, true);
        }
        assert_eq!(c.population(), 3);

        c.step();
        // Vertical blinker in the middle column.
        let expected = [(1, 0), (1, 1), (1, 2)];
        for y in 0..3usize {
            for x in 0..3usize {
                assert_eq!(c.get(x, y), expected.contains(&(x, y)), "cell ({},{})", x, y);
            }
        }

        c.step();
        // Back to horizontal.
        let expected = [(0, 1), (1, 1), (2, 1)];
        for y in 0..3usize {
            for x in 0..3usize {
                assert_eq!(c.get(x, y), expected.contains(&(x, y)), "cell ({},{})", x, y);
            }
        }
    }

    #[test]
    fn block_is_static() {
        let mut c = Life::new(3, 3);
        for (x, y) in [(1, 1), (2, 1), (1, 2), (2, 2)] {
            c.set(x, y, true);
        }
        c.step();
        let expected = [(1, 1), (2, 1), (1, 2), (2, 2)];
        for y in 0..3usize {
            for x in 0..3usize {
                assert_eq!(c.get(x, y), expected.contains(&(x, y)), "cell ({},{})", x, y);
            }
        }
    }

    #[test]
    fn overpopulation_and_underpopulation() {
        // A live cell with only 1 live neighbour dies (underpopulation).
        let mut c = Life::new(2, 1);
        c.set(0, 0, true);
        c.set(1, 0, true);
        c.step();
        assert_eq!(c.population(), 0);

        // A live cell with 4+ neighbours dies (overpopulation).
        let mut c = Life::new(3, 3);
        for (x, y) in [(1, 1), (0, 1), (2, 1), (1, 0), (1, 2)] {
            c.set(x, y, true);
        }
        c.step();
        assert!(!c.get(1, 1));
    }

    #[test]
    fn glider_moves() {
        // Classic glider on a 7x7 field: after one step it has moved down-right.
        let mut c = Life::new(7, 7);
        for (x, y) in [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)] {
            c.set(x, y, true);
        }
        assert_eq!(c.population(), 5);
        c.step();
        let expected = [(2, 0), (0, 1), (1, 1), (3, 1), (1, 2)];
        for y in 0..7usize {
            for x in 0..7usize {
                assert_eq!(c.get(x, y), expected.contains(&(x, y)), "cell ({},{})", x, y);
            }
        }
        // The glider is period-4: after four steps it returns to the same shape,
        // shifted by (1, 1).
        for _ in 0..3 {
            c.step();
        }
        let expected = [(2, 1), (3, 2), (1, 3), (2, 3), (3, 3)];
        for y in 0..7usize {
            for x in 0..7usize {
                assert_eq!(c.get(x, y), expected.contains(&(x, y)), "cell ({},{})", x, y);
            }
        }
    }

    #[test]
    fn edge_cells_are_dead_neighbours() {
        // A lone live cell in a corner: no neighbours survive, it dies alone.
        let mut c = Life::new(4, 4);
        c.set(0, 0, true);
        c.step();
        assert_eq!(c.population(), 0);

        // Three live cells hugging the top edge: the corner cell stays dead
        // (only 2 live neighbours), so nothing is born.
        let mut c = Life::new(3, 2);
        for x in 0..3usize {
            c.set(x, 0, true);
        }
        c.step();
        assert_eq!(c.population(), 0);
    }

    #[test]
    fn empty_grid_stays_empty() {
        let mut c = Life::new(6, 5);
        for _ in 0..3 {
            c.step();
        }
        assert_eq!(c.population(), 0);
    }
}

fn main() {
    // Standalone verification: run the same scenarios without cargo.
    let mut failures = 0usize;
    macro_rules! check {
        ($cond:expr, $msg:expr) => {
            if !$cond {
                eprintln!("FAIL: {}", $msg);
                failures += 1;
            }
        };
    }

    // Fresh grid is dead.
    let c = Life::new(5, 4);
    check!(c.population() == 0, "fresh grid must be empty");

    // Blinker oscillation.
    let mut c = Life::new(3, 3);
    for x in 0..3usize {
        c.set(x, 1, true);
    }
    check!(c.population() == 3, "blinker starts with 3 live cells");
    c.step();
    let v_ok = (0..3usize).all(|y| (0..3usize).any(|x| c.get(x, y) && x == 1));
    check!(v_ok && c.population() == 3, "blinker becomes vertical");
    c.step();
    let h_ok = (0..3usize).all(|x| {
        if c.get(x, 1) { true } else { false }
    }) && !c.get(0, 0) && !c.get(2, 2);
    check!(h_ok, "blinker returns to horizontal");

    // Block is static.
    let mut c = Life::new(3, 3);
    for (x, y) in [(1usize, 1usize), (2, 1), (1, 2), (2, 2)] {
        c.set(x, y, true);
    }
    c.step();
    check!(c.population() == 4 && c.get(1, 1) && c.get(2, 1) && c.get(1, 2) && c.get(2, 2),
           "block must be static");

    // Overpopulation and underpopulation.
    let mut c = Life::new(2, 1);
    c.set(0, 0, true);
    c.set(1, 0, true);
    c.step();
    check!(c.population() == 0, "domino dies (underpopulation)");

    // Glider after one step.
    let mut c = Life::new(7, 7);
    for (x, y) in [(1usize, 0usize), (2, 1), (0, 2), (1, 2), (2, 2)] {
        c.set(x, y, true);
    }
    c.step();
    let expected = [(2usize, 0usize), (0, 1), (1, 1), (3, 1), (1, 2)];
    let mut ok = c.population() == 5;
    for y in 0..7usize {
        for x in 0..7usize {
            if c.get(x, y) != expected.contains(&(x, y)) {
                ok = false;
            }
        }
    }
    check!(ok, "glider step-1 shape");

    // Empty grid stays empty.
    let mut c = Life::new(6, 5);
    for _ in 0..3 {
        c.step();
    }
    check!(c.population() == 0, "empty grid stays empty");

    if failures > 0 {
        eprintln!("{} conway checks FAILED", failures);
        std::process::exit(1);
    }
    println!("all conway_step checks passed");
}
