struct Grid {
    width: usize,
    height: usize,
    cells: Vec<bool>,
}

impl Grid {
    fn new(width: usize, height: usize) -> Self {
        let size = width * height;
        Grid {
            width,
            height,
            cells: vec![false; size],
        }
    }

    fn get_index(&self, row: usize, col: usize) -> Option<usize> {
        if row < self.height && col < self.width {
            Some(row * self.width + col)
        } else {
            None
        }
    }

    fn get_value(&self, row: usize, col: usize) -> bool {
        self.get_index(row, col).map_or(false, |index| self.cells[index])
    }

    fn set_value(&mut self, row: usize, col: usize, value: bool) {
        if let Some(index) = self.get_index(row, col) {
            self.cells[index] = value;
        }
    }
}
