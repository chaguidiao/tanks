/// A dense row-major raster. Row 0 is the southern row.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid<T> {
    pub w: usize,
    pub h: usize,
    pub data: Vec<T>,
}

impl<T: Copy> Grid<T> {
    pub fn new(w: usize, h: usize, fill: T) -> Self {
        Self { w, h, data: vec![fill; w * h] }
    }

    pub fn from_vec(w: usize, h: usize, data: Vec<T>) -> Self {
        assert_eq!(data.len(), w * h, "grid data length mismatch");
        Self { w, h, data }
    }

    #[inline]
    pub fn idx(&self, x: usize, y: usize) -> usize {
        y * self.w + x
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> T {
        self.data[y * self.w + x]
    }

    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: T) {
        let i = y * self.w + x;
        self.data[i] = v;
    }

    /// Value at integer coordinates clamped to the grid edge.
    #[inline]
    pub fn get_clamped(&self, x: i64, y: i64) -> T {
        let x = x.clamp(0, self.w as i64 - 1) as usize;
        let y = y.clamp(0, self.h as i64 - 1) as usize;
        self.data[y * self.w + x]
    }

    #[inline]
    pub fn in_bounds(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    pub fn map<U: Copy>(&self, f: impl Fn(T) -> U) -> Grid<U> {
        Grid { w: self.w, h: self.h, data: self.data.iter().map(|&v| f(v)).collect() }
    }
}
