use std::ops::{Index, IndexMut};

/// Sub-band types in the wavelet decomposition
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubBandType {
    /// Low-pass horizontal, Low-pass vertical
    LL,
    /// High-pass horizontal, Low-pass vertical
    HL,
    /// Low-pass horizontal, High-pass vertical
    LH,
    /// High-pass horizontal, High-pass vertical
    HH,
}

/// Two dimensional index
#[derive(Debug, Clone, Copy)]
pub struct I2 {
    pub x: u32,
    pub y: u32,
}

/// Convenience struct for hold image/tile/precinct/code-block bounds information
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    pub x0: u32,
    pub x1: u32,
    pub y0: u32,
    pub y1: u32,
}

/// A 2D array
#[derive(Debug, Clone)]
pub struct Array2D<T> {
    data: Vec<T>,
    pub width: usize,
    pub height: usize,
    /// Offset of the first column index (u0)
    pub u0: i32,
    /// Offset of the first row index (v0)
    pub v0: i32,
}

impl<T> Index<(usize, usize)> for Array2D<T> {
    type Output = T;

    fn index(&self, (col, row): (usize, usize)) -> &Self::Output {
        &self.data[row * self.width + col]
    }
}

impl<T> IndexMut<(usize, usize)> for Array2D<T> {
    fn index_mut(&mut self, (col, row): (usize, usize)) -> &mut Self::Output {
        &mut self.data[row * self.width + col]
    }
}

impl<T: Clone + Default> Array2D<T> {
    /// Create a new 2D array with given dimensions
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            data: vec![T::default(); width * height],
            width,
            height,
            u0: 0,
            v0: 0,
        }
    }
    /// Create from existing data
    pub fn from_data(data: Vec<T>, width: usize, height: usize) -> Self {
        assert_eq!(data.len(), width * height);
        Self {
            data,
            width,
            height,
            u0: 0,
            v0: 0,
        }
    }
}

impl<T: PartialEq> PartialEq for Array2D<T> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
            && self.width == other.width
            && self.height == other.height
            && self.u0 == other.u0
            && self.v0 == other.v0
    }
}

impl<T> Array2D<T> {
    pub fn map_elements<O, F>(&self, f: F) -> Array2D<O>
    where
        O: Default + Clone,
        F: Fn(&T) -> O,
    {
        let out_data: Vec<O> = self.data.iter().map(f).collect();
        Array2D::from_data(out_data, self.width, self.height)
    }
}

/// todo are these needed ?
impl<T: Clone + Default> Array2D<T> {
    /// Create a new 2D array with given dimensions and offset
    pub fn with_offset(width: usize, height: usize, u0: i32, v0: i32) -> Self {
        Self {
            data: vec![T::default(); width * height],
            width,
            height,
            u0,
            v0,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Get value at position (u, v) using absolute coordinates
    pub fn get(&self, u: i32, v: i32) -> &T {
        let col = (u - self.u0) as usize;
        let row = (v - self.v0) as usize;
        &self.data[row * self.width + col]
    }

    /// Get mutable value at position (u, v) using absolute coordinates
    pub fn get_mut(&mut self, u: i32, v: i32) -> &mut T {
        let col = (u - self.u0) as usize;
        let row = (v - self.v0) as usize;
        &mut self.data[row * self.width + col]
    }

    /// Set value at position (u, v) using absolute coordinates
    pub fn set(&mut self, u: i32, v: i32, value: T) {
        let col = (u - self.u0) as usize;
        let row = (v - self.v0) as usize;
        self.data[row * self.width + col] = value;
    }

    /// Get a column as a vector
    pub fn get_column(&self, u: i32) -> Vec<T> {
        let col = (u - self.u0) as usize;
        (0..self.height)
            .map(|row| self.data[row * self.width + col].clone())
            .collect()
    }

    /// Set a column from a vector
    pub fn set_column(&mut self, u: i32, values: &[T]) {
        let col = (u - self.u0) as usize;
        for (row, value) in values.iter().enumerate() {
            self.data[row * self.width + col] = value.clone();
        }
    }

    /// Get a row as a vector
    pub fn get_row(&self, v: i32) -> Vec<T> {
        let row = (v - self.v0) as usize;
        self.data[row * self.width..(row + 1) * self.width].to_vec()
    }

    /// Set a row from a vector
    pub fn set_row(&mut self, v: i32, values: &[T]) {
        let row = (v - self.v0) as usize;
        self.data[row * self.width..(row + 1) * self.width].clone_from_slice(values);
    }

    /// Get the upper bound for u coordinate (exclusive)
    pub fn u1(&self) -> i32 {
        self.u0 + self.width as i32
    }

    /// Get the upper bound for v coordinate (exclusive)
    pub fn v1(&self) -> i32 {
        self.v0 + self.height as i32
    }

    pub fn elements(&self) -> &Vec<T> {
        &self.data
    }
}

#[derive(Debug)]
pub enum SubBandGroup<T> {
    Full { ll: T, hl: T, lh: T, hh: T },
    LL(T),
    Partial { hl: T, lh: T, hh: T },
}

type ResolutionLevelSubBands = SubBandGroup<Array2D<i32>>;
