use std::fmt;

#[derive(Clone, PartialEq, Eq)]
struct Life {
    width: usize,
    height: usize,
    cells: Vec<bool>,
}

impl Life {
    fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        Self {
            width,
            height,
            cells: vec![false; size],
        }
    }

    fn get(&self, x: isize, y: isize) -> bool {
        if x >= 0 && x < self.width as isize && y >= 0 && y < self.height as isize {
            self.cells[(y as usize * self.width + x as usize)]
        } else {
            false
        }
    }

    fn set(&mut self, x: isize, y: isize, state: bool) {
        if x >= 0 && x < self.width as isize && y >= 0 && y < self.height as isize {
            self.cells[
                (y as usize * self.width + x as usize)
            ] = state;
        }
    }

    fn count_alive_neighbors(&self, x: usize, y: usize) -> u8 {
        let directions = [
            (-1, -1), (-1, 0), (-1, 1),
            (0, -1),         (0, 1),
            (1, -1), (1, 0), (1, 1),
        ];

        directions.iter().filter(|&&(dx, dy)| self.get(x as isize + dx, y as isize + dy)).count() as u8
    }

    fn step(&mut self) {
        let mut next = self.cells.clone();
        for y in 0..self.height {
            for x in 0..self.width {
                let alive_neighbors = self.count_alive_neighbors(x, y);
                if self.cells[y * self.width + x] {
                    // Any live cell with fewer than two live neighbours dies
                    next[y * self.width + x] = alive_neighbors == 2 || alive_neighbors == 3;
                } else {
                    // Any dead cell with exactly three live neighbours becomes a live cell
                    next[y * self.width + x] = alive_neighbors == 3;
                }
            }
        }
        self.cells = next;
    }
}