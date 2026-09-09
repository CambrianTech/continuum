//! Conway's Game of Life — one step on a finite grid.
//!
//! Grid model: row-major `cells`, index = y * w + x, with dead outside edges
//! (no wrap). No external crates; verification harness lives in `main`.

pub struct Life {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<bool>,
}

impl Life {
    /// A fresh board of `w` x `h`, every cell dead.
    pub fn new(w: usize, h: usize) -> Self {
        Life {
            w,
            h,
            cells: vec![false; w * h],
        }
    }

    /// Set the cell at column `x`, row `y` to `alive`. Out-of-range is a bug in
    /// the caller — we index straight into the flat grid.
    pub fn set(&mut self, x: usize, y: usize, alive: bool) {
        self.cells[y * self.w + x] = alive;
    }

    /// Read the cell at column `x`, row `y`. Out-of-range reads are dead cells,
    /// so caller code can probe past an edge without bookkeeping.
    pub fn get(&self, x: usize, y: usize) -> bool {
        if x >= self.w || y >= self.h {
            return false;
        }
        self.cells[y * self.w + x]
    }

    /// One generation of Conway's rules over the 8-neighbourhood (edges dead):
    /// a live cell with 2 or 3 live neighbours survives, a dead cell with
    /// exactly 3 becomes live, everything else dies. Computed into a fresh grid
    /// so no cell influences its own update.
    pub fn step(&mut self) {
        let next = (0..self.h)
            .flat_map(|y| {
                (0..self.w).map(move |x| {
                    let n = self.neighbours(x, y);
                    let alive = self.cells[y * self.w + x];
                    if alive { n == 2 || n == 3 } else { n == 3 }
                })
            })
            .collect();
        self.cells = next;
    }

    /// Count of live cells.
    pub fn population(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }

    /// Live neighbours in the 8-neighbourhood, treating anything outside the
    /// grid as dead (no wrap-around at edges).
    fn neighbours(&self, x: usize, y: usize) -> u32 {
        let mut n = 0u32;
        for dy in 0..=1usize {
            for dx in 0..=1usize {
                if dx == 0 && dy == 0 {
                    continue;
                }
                // Mirror the opposite side too, so all 8 offsets are covered.
                let offs: [(i32, i32); 4] = [
                    (dx as i32, dy as i32),
                    (-dx as i32, dy as i32),
                    (dx as i32, -dy as i32),
                    (-dx as i32, -dy as i32),
                ];
                for &(ox, oy) in offs.iter() {
                    let nx = x as i64 + ox as i64;
                    let ny = y as i64 + oy as i64;
                    if nx < 0 || ny < 0 {
                        continue; // edge: dead
                    }
                    if (nx as usize) >= self.w || (ny as usize) >= self.h {
                        continue; // edge: dead
                    }
                    if self.cells[ny as usize * self.w + nx as usize] {
                        n += 1;
                    }
                }
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::Life;

    fn board(w: usize, h: usize, live_pts: &[(usize, usize)]) -> Life {
        let mut l = Life::new(w, h);
        for &(x, y) in live_pts {
            l.set(x, y, true);
        }
        l
    }

    fn cells_of(l: &Life) -> Vec<bool> {
        (0..l.h).flat_map(|y| (0..l.w).map(move |x| l.get(x, y))).collect()
    }

    #[test]
    fn blinker_oscillates_horizontally_then_vertically() {
        // Horizontal bar centred on a 3x3: survives one step as vertical, and
        // the second step returns it to horizontal.
        let mut l = board(3, 3, &[(0, 1), (1, 1), (2, 1)]);
        assert_eq!(l.population(), 3);
        l.step();
        assert_eq!(cells_of(&l), vec![false, true, false, false, true, false, false, true, false]);
        assert_eq!(l.population(), 3);
        l.step();
        assert_eq!(cells_of(&l), vec![false, false, false, true, true, true, false, false, false]);
    }

    #[test]
    fn block_is_still() {
        let mut l = board(2, 2, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
        l.step();
        assert_eq!(cells_of(&l), vec![true; 4]);
    }

    #[test]
    fn blinker_at_edge_does_not_wrap() {
        // Same bar in the top row: with wrap-around it would stay a bar; on a
        // finite grid it dies out (corner cells see only 2 neighbours).
        let mut l = board(3, 1, &[(0, 0), (1, 0), (2, 0)]);
        l.step();
        assert_eq!(l.population(), 0);
    }

    #[test]
    fn lone_cell_dies_and_empty_grid_stays_empty() {
        let mut l = board(3, 3, &[(1, 1)]);
        l.step();
        assert_eq!(l.population(), 0);
        let mut e = Life::new(4, 4);
        e.step();
        assert_eq!(e.population(), 0);
    }

    #[test]
    fn get_out_of_range_is_dead() {
        let l = board(2, 2, &[(1, 1)]);
        assert!(!l.get(0, 0));
        assert!(l.get(1, 1));
        assert!(!l.get(5, 1));
        assert!(!l.get(1, 7));
    }

    #[test]
    fn set_overwrites_both_ways() {
        let mut l = Life::new(2, 2);
        l.set(0, 0, true);
        assert!(l.get(0, 0));
        l.set(0, 0, false);
        assert!(!l.get(0, 0));
    }

    #[test]
    fn glider_moves_one_step_diagonally() {
        // Classic glider on a 5x5: after one step it is the same shape shifted
        // +1x/+1y.
        let mut l = board(5, 5, &[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)]);
        l.step();
        assert_eq!(cells_of(&l), vec![
            false, true, false, false, false, // y=0
            false, false, true, false, false, // y=1
            false, true, false, false, false, // y=2
            false, true, true, false, false, // y=3
            false, false, false, false, false, // y=4
        ]);
    }
}

fn main() {
    let mut failures: Vec<String> = vec![];

    // 1) Blinker: horizontal bar -> vertical bar -> horizontal again.
    let mut l = Life::new(3, 3);
    l.set(0, 1, true);
    l.set(1, 1, true);
    l.set(2, 1, true);
    assert_eq!(l.population(), 3);
    l.step();
    let want_v = [false, true, false, false, true, false, false, true, false];
    if (0..9).any(|i| l.cells[i] != want_v[i]) {
        failures.push(format!("blinker step1: {:?}", l.cells));
    }
    l.step();
    let want_h = [false; 3].to_vec().into_iter()
        .chain([true, true, true])
        .chain(std::iter::repeat(false).take(3))
        .collect::<Vec<_>>();
    if l.cells != want_h {
        failures.push(format!("blinker step2: {:?}", l.cells));
    }

    // 2) Block (still life): unchanged after a step.
    let mut b = Life::new(2, 2);
    for &(x, y) in &[(0usize, 0usize), (1, 0), (0, 1), (1, 1)] {
        b.set(x, y, true);
    }
    let before: Vec<bool> = b.cells.clone();
    b.step();
    if b.cells != before || b.population() != 4 {
        failures.push(format!("block: {:?}", b.cells));
    }

    // 3) No wrap-around: a top-row bar dies (corner cells only see 2 neighbours).
    let mut e = Life::new(3, 1);
    for x in 0..3usize {
        e.set(x, 0, true);
    }
    e.step();
    if e.population() != 0 {
        failures.push(format!("edge bar should die: {:?}", e.cells));
    }

    // 4) Lone cell dies; empty grid stays empty.
    let mut lone = Life::new(3, 3);
    lone.set(1, 1, true);
    lone.step();
    if lone.population() != 0 {
        failures.push("lone cell survived".to_string());
    }
    let mut empty = Life::new(4, 4);
    empty.step();
    if empty.population() != 0 {
        failures.push("empty grid changed".to_string());
    }

    // 5) get() out of range reads dead; set() overwrites both ways.
    let mut g = Life::new(2, 2);
    g.set(1, 1, true);
    if !g.get(1, 1) || g.get(0, 0) || g.get(5, 1) || g.get(1, 7) {
        failures.push("get() semantics".to_string());
    }
    g.set(1, 1, false);
    if g.get(1, 1) {
        failures.push("set(false) failed".to_string());
    }

    // 6) Glider: one step shifts the shape +1x/+1y.
    let mut gl = Life::new(5, 5);
    for &(x, y) in &[(1usize, 0usize), (2, 1), (0, 2), (1, 2), (2, 2)] {
        gl.set(x, y, true);
    }
    let want_gl = [
        false, true, false, false, false, // y=0
        false, false, true, false, false, // y=1
        false, true, false, false, false, // y=2
        false, true, true, false, false, // y=3
        false, false, false, false, false, // y=4
    ];
    gl.step();
    if gl.cells != want_gl.to_vec() {
        failures.push(format!("glider: {:?}", gl.cells));
    }

    // 7) Cross-check every cell's neighbour count against an independent
    //    double loop (the reference implementation for this check).
    let mut n = Life::new(4, 5);
    for &(x, y) in &[(0usize, 0usize), (3, 0), (1, 2), (2, 4)] {
        n.set(x, y, true);
    }
    for y in 0..n.h {
        for x in 0..n.w {
            let mut cnt = 0u32;
            for dy in -1i64..=1i64 {
                for dx in -1i64..=1i64 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i64 + dx;
                    let ny = y as i64 + dy;
                    if nx >= 0 && ny >= 0 && (nx as usize) < n.w && (ny as usize) < n.h {
                        if n.get(nx as usize, ny as usize) {
                            cnt += 1;
                        }
                    }
                }
            }
            if cnt != n.neighbours(x, y) {
                failures.push(format!(
                    "neighbour count mismatch at ({},{}) : {} vs {}",
                    x, y, cnt, n.neighbours(x, y)
                ));
            }
        }
    }

    // 8) Full census on a small random-ish grid: recompute the whole next state
    //    independently and compare.
    let mut c = Life::new(6, 4);
    for (i, cell) in c.cells.iter_mut().enumerate() {
        *cell = i % 7 == 2 || i % 5 == 3;
    }
    let expected: Vec<bool> = (0..c.h).flat_map(|y| (0..c.w).map(move |x| {
        let mut cnt = 0u32;
        for dy in -1i64..=1i64 {
            for dx in -1i64..=1i64 {
                if dx == 0 && dy == 0 { continue; }
                let nx = x as i64 + dx;
                let ny = y as i64 + dy;
                if nx >= 0 && ny >= 0 && (nx as usize) < c.w && (ny as usize) < c.h {
                    if c.get(nx as usize, ny as usize) { cnt += 1; }
                }
            }
        }
        let alive = c.cells[y * c.w + x];
        if alive { cnt == 2 || cnt == 3 } else { cnt == 3 }
    })).collect();
    let before_c: Vec<bool> = c.cells.clone();
    c.step();
    if c.cells != expected {
        failures.push(format!("census mismatch:\n got {:?}\nwant {:?}", before_c, expected));
    }

    if failures.is_empty() {
        println!("all conway_step checks passed");
    } else {
        for f in &failures {
            eprintln!("FAIL: {}", f);
        }
        std::process::exit(1);
    }
}
