use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Alive,
    Dead,
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let symbol = if *self == Cell::Alive { 'O' } else { '.' };
        write!(f, "{}", symbol)
    }
}

struct Life {
    width: usize,
    height: usize,
    cells: Vec<Vec<Cell>>,
}

impl Life {
    fn new(width: usize, height: usize) -> Self {
        let cells = vec![vec![Cell::Dead; width]; height];
        Life { width, height, cells }
    }

    fn get(&self, x: isize, y: isize) -> Cell {
        if x < 0 || x >= self.width as isize || y < 0 || y >= self.height as isize {
            Cell::Dead
        } else {
            self.cells[y as usize][x as usize]
        }
    }

    fn set(&mut self, x: usize, y: usize, cell: Cell) {
        if x < self.width && y < self.height {
            self.cells[y][x] = cell;
        }
    }

    fn step(&mut self) {
        let mut new_cells = self.cells.clone();

        for y in 0..self.height {
            for x in 0..self.width {
                let cell = self.get(x as isize, y as isize);
                let live_neighbors = self.count_live_neighbors(x, y);

                if cell == Cell::Alive && (live_neighbors < 2 || live_neighbors > 3) {
                    new_cells[y][x] = Cell::Dead;
                } else if cell == Cell::Dead && live_neighbors == 3 {
                    new_cells[y][x] = Cell::Alive;
                }
            }
        }

        self.cells = new_cells;
    }

    fn count_live_neighbors(&self, x: usize, y: usize) -> u8 {
        let mut count = 0;
        for dy in -1isize..=1 {
            for dx in -1isize..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if self.get(x as isize + dx, y as isize + dy) == Cell::Alive {
                    count += 1;
                }
            }
        }
        count
    }
}

impl fmt::Display for Life {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for row in &self.cells {
            for cell in row {
                write!(f, "{}", cell)?;
            }
            writeln!(f)?;
        }
        Ok(())
    }
}