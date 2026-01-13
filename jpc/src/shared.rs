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
    width: usize,
    height: usize,
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
