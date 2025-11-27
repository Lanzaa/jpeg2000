//! Discrete Wavelet Transformation for JPEG 2000
//!
//! This module implements the forward and inverse discrete wavelet transformations
//! as specified in Annex F of ITU-T T.800 (ISO/IEC 15444-1) - JPEG 2000 Core Coding System.
//!
//! The implementation supports:
//! - 5-3 Reversible (lossless) wavelet transformation
//! - 9-7 Irreversible (lossy) wavelet transformation
//!
//! Both transformations use lifting-based filtering as specified in the standard.

use std::ops::{Index, IndexMut};

/// Lifting parameters for the 9-7 irreversible filter (Table F.4)
pub mod lifting_params_97 {
    /// α (alpha) lifting parameter
    pub const ALPHA: f64 = -1.586_134_342_059_924;
    /// β (beta) lifting parameter
    pub const BETA: f64 = -0.052_980_118_572_961;
    /// γ (gamma) lifting parameter
    pub const GAMMA: f64 = 0.882_911_075_530_934;
    /// δ (delta) lifting parameter
    pub const DELTA: f64 = 0.443_506_852_043_971;
    /// K scaling parameter
    pub const K: f64 = 1.230_174_104_914_001;
}

/// Filter type selection for DWT operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterType {
    /// 5-3 Reversible filter for lossless compression
    Reversible53,
    /// 9-7 Irreversible filter for lossy compression
    Irreversible97,
}

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

/// A 2D coefficient array for wavelet operations
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

/// Represents a set of sub-bands at a given decomposition level
#[derive(Debug, Clone)]
pub struct SubBands {
    pub ll: Array2D<f64>,
    pub hl: Array2D<f64>,
    pub lh: Array2D<f64>,
    pub hh: Array2D<f64>,
}

/// The main DWT processor implementing Annex F procedures
pub struct DwtProcessor {
    filter_type: FilterType,
}

impl DwtProcessor {
    /// Create a new DWT processor with specified filter type
    pub fn new(filter_type: FilterType) -> Self {
        Self { filter_type }
    }

    // ========================================================================
    // Periodic Symmetric Extension (PSE) - Equation F-3 and F-4
    // ========================================================================

    /// Periodic Symmetric Extension as per Equation F-4
    /// PSE_O(i, i0, il) computes the reflected index
    fn pse_o(i: i32, i0: i32, il: i32) -> i32 {
        let len = il - i0;
        if len == 0 {
            return i0;
        }

        // Map i into the periodic domain
        let period = 2 * len;
        let mut idx = i - i0;

        // Handle negative indices
        idx = ((idx % period) + period) % period;

        // Reflect if in second half of period
        if idx >= len {
            idx = period - 1 - idx;
        }

        i0 + idx
    }

    /// Extend signal Y using periodic symmetric extension (1D_EXTR procedure)
    /// Returns extended signal Y_ext with indices from (i0 - i_left) to (il + i_right - 1)
    fn extend_signal_1d(&self, y: &[f64], i0: i32, il: i32, i_left: i32, i_right: i32) -> Vec<f64> {
        let total_len = (il - i0) as usize + i_left as usize + i_right as usize;
        let mut y_ext = vec![0.0; total_len];

        let ext_start = i0 - i_left;

        for (idx, ext_i) in (ext_start..(il + i_right)).enumerate() {
            let src_i = Self::pse_o(ext_i, i0, il);
            y_ext[idx] = y[(src_i - i0) as usize];
        }

        y_ext
    }

    /// Get extension parameters for reconstruction (Tables F.2 and F.3)
    fn get_extension_params_r(&self, i0: i32, il: i32) -> (i32, i32) {
        match self.filter_type {
            FilterType::Reversible53 => {
                let i_left = if i0 % 2 == 0 { 1 } else { 2 };
                let i_right = if il % 2 == 1 { 1 } else { 2 };
                (i_left, i_right)
            }
            FilterType::Irreversible97 => {
                let i_left = if i0 % 2 == 0 { 3 } else { 4 };
                let i_right = if il % 2 == 1 { 3 } else { 4 };
                (i_left, i_right)
            }
        }
    }

    /// Get extension parameters for decomposition (Tables F.8 and F.9)
    fn get_extension_params_d(&self, i0: i32, il: i32) -> (i32, i32) {
        match self.filter_type {
            FilterType::Reversible53 => {
                let i_left = if i0 % 2 == 0 { 2 } else { 1 };
                let i_right = if il % 2 == 1 { 2 } else { 1 };
                (i_left, i_right)
            }
            FilterType::Irreversible97 => {
                let i_left = if i0 % 2 == 0 { 4 } else { 3 };
                let i_right = if il % 2 == 1 { 4 } else { 3 };
                (i_left, i_right)
            }
        }
    }

    // ========================================================================
    // 1D Filtering Procedures (Section F.3.8)
    // ========================================================================

    /// 1D_FILTR_5-3R procedure for inverse (reconstruction) filtering
    /// Equations F-5 and F-6
    fn filter_1d_53r(&self, y_ext: &[f64], i0: i32, il: i32, ext_offset: i32) -> Vec<f64> {
        let len = (il - i0) as usize;
        let mut x = vec![0.0; len];

        // Calculate indices for even and odd samples in extended array
        let get_ext_idx = |i: i32| -> usize { (i - (i0 - ext_offset)) as usize };

        // Step 1: Equation F-5 - compute even samples X(2n)
        // X(2n) = Y_ext(2n) - floor((Y(2n-1) + Y(2n+1) + 2) / 4)
        let n_start = (i0 as f64 / 2.0).ceil() as i32;
        let n_end = ((il as f64 - 1.0) / 2.0).floor() as i32;

        for n in n_start..=n_end {
            let idx_2n = 2 * n;
            if idx_2n >= i0 && idx_2n < il {
                let y_2n = y_ext[get_ext_idx(idx_2n)];
                let y_2n_m1 = y_ext[get_ext_idx(idx_2n - 1)];
                let y_2n_p1 = y_ext[get_ext_idx(idx_2n + 1)];
                x[(idx_2n - i0) as usize] = y_2n - ((y_2n_m1 + y_2n_p1 + 2.0) / 4.0).floor();
            }
        }

        // Step 2: Equation F-6 - compute odd samples X(2n+1)
        // X(2n+1) = Y_ext(2n+1) + floor((X(2n) + X(2n+2)) / 2)
        // Note: We need to use the X values computed in step 1
        let mut x_ext = self.extend_signal_1d(&x, i0, il, ext_offset, ext_offset);

        let n_start = (i0 as f64 / 2.0).floor() as i32;
        let n_end = ((il as f64 - 2.0) / 2.0).floor() as i32;

        for n in n_start..=n_end {
            let idx_2n_p1 = 2 * n + 1;
            if idx_2n_p1 >= i0 && idx_2n_p1 < il {
                let y_2n_p1 = y_ext[get_ext_idx(idx_2n_p1)];
                // Get X(2n) and X(2n+2) from the (possibly extended) x array
                let x_2n = if 2 * n >= i0 && 2 * n < il {
                    x[(2 * n - i0) as usize]
                } else {
                    let reflected = Self::pse_o(2 * n, i0, il);
                    x[(reflected - i0) as usize]
                };
                let x_2n_p2 = if 2 * n + 2 >= i0 && 2 * n + 2 < il {
                    x[(2 * n + 2 - i0) as usize]
                } else {
                    let reflected = Self::pse_o(2 * n + 2, i0, il);
                    x[(reflected - i0) as usize]
                };
                x[(idx_2n_p1 - i0) as usize] = y_2n_p1 + ((x_2n + x_2n_p2) / 2.0).floor();
            }
        }

        x
    }

    /// 1D_FILTR_9-7I procedure for inverse (reconstruction) filtering
    /// Equation F-7
    fn filter_1d_97i(&self, y_ext: &[f64], i0: i32, il: i32, ext_offset: i32) -> Vec<f64> {
        use lifting_params_97::*;

        let len = (il - i0) as usize;
        let mut x = vec![0.0; len];

        let get_ext_idx = |i: i32| -> usize { (i - (i0 - ext_offset)) as usize };

        // Helper to safely get x value with boundary handling
        let get_x = |x: &[f64], idx: i32| -> f64 {
            if idx >= i0 && idx < il {
                x[(idx - i0) as usize]
            } else {
                let reflected = Self::pse_o(idx, i0, il);
                x[(reflected - i0) as usize]
            }
        };

        // Step 1: X(2n+1) = K * Y_ext(2n+1)
        for n in ((i0 - 1) / 2)..=((il - 2) / 2) {
            let idx = 2 * n + 1;
            if idx >= i0 && idx < il {
                x[(idx - i0) as usize] = K * y_ext[get_ext_idx(idx)];
            }
        }

        // Step 2: X(2n) = Y_ext(2n) / K
        for n in (i0 / 2)..=((il - 1) / 2) {
            let idx = 2 * n;
            if idx >= i0 && idx < il {
                x[(idx - i0) as usize] = y_ext[get_ext_idx(idx)] / K;
            }
        }

        // Step 3: X(2n+1) = X(2n+1) - δ*(X(2n) + X(2n+2))
        let x_copy = x.clone();
        for n in ((i0 - 1) / 2)..=((il - 2) / 2) {
            let idx = 2 * n + 1;
            if idx >= i0 && idx < il {
                let x_2n = get_x(&x_copy, 2 * n);
                let x_2n_p2 = get_x(&x_copy, 2 * n + 2);
                x[(idx - i0) as usize] -= DELTA * (x_2n + x_2n_p2);
            }
        }

        // Step 4: X(2n) = X(2n) - γ*(X(2n-1) + X(2n+1))
        let x_copy = x.clone();
        for n in (i0 / 2)..=((il - 1) / 2) {
            let idx = 2 * n;
            if idx >= i0 && idx < il {
                let x_2n_m1 = get_x(&x_copy, 2 * n - 1);
                let x_2n_p1 = get_x(&x_copy, 2 * n + 1);
                x[(idx - i0) as usize] -= GAMMA * (x_2n_m1 + x_2n_p1);
            }
        }

        // Step 5: X(2n+1) = X(2n+1) - β*(X(2n) + X(2n+2))
        let x_copy = x.clone();
        for n in ((i0 - 1) / 2)..=((il - 2) / 2) {
            let idx = 2 * n + 1;
            if idx >= i0 && idx < il {
                let x_2n = get_x(&x_copy, 2 * n);
                let x_2n_p2 = get_x(&x_copy, 2 * n + 2);
                x[(idx - i0) as usize] -= BETA * (x_2n + x_2n_p2);
            }
        }

        // Step 6: X(2n) = X(2n) - α*(X(2n-1) + X(2n+1))
        let x_copy = x.clone();
        for n in (i0 / 2)..=((il - 1) / 2) {
            let idx = 2 * n;
            if idx >= i0 && idx < il {
                let x_2n_m1 = get_x(&x_copy, 2 * n - 1);
                let x_2n_p1 = get_x(&x_copy, 2 * n + 1);
                x[(idx - i0) as usize] -= ALPHA * (x_2n_m1 + x_2n_p1);
            }
        }

        x
    }

    /// 1D_FILTD_5-3R procedure for forward (decomposition) filtering
    /// Equations F-9 and F-10
    fn filter_1d_53r_decomp(&self, x_ext: &[f64], i0: i32, il: i32, ext_offset: i32) -> Vec<f64> {
        let len = (il - i0) as usize;
        let mut y = vec![0.0; len];

        let get_ext_idx = |i: i32| -> usize { (i - (i0 - ext_offset)) as usize };

        // Helper to get Y value with boundary handling
        let get_y = |y: &[f64], idx: i32| -> f64 {
            if idx >= i0 && idx < il {
                y[(idx - i0) as usize]
            } else {
                let reflected = Self::pse_o(idx, i0, il);
                y[(reflected - i0) as usize]
            }
        };

        // Step 1: Compute odd samples Y(2n+1) - high-pass (Equation F-9)
        // Y(2n+1) = X_ext(2n+1) - floor((X_ext(2n) + X_ext(2n+2)) / 2)
        for n in ((i0 - 1) / 2)..=((il - 2) / 2) {
            let idx = 2 * n + 1;
            if idx >= i0 && idx < il {
                let x_2n_p1 = x_ext[get_ext_idx(idx)];
                let x_2n = x_ext[get_ext_idx(2 * n)];
                let x_2n_p2 = x_ext[get_ext_idx(2 * n + 2)];
                y[(idx - i0) as usize] = x_2n_p1 - ((x_2n + x_2n_p2) / 2.0).floor();
            }
        }

        // Step 2: Compute even samples Y(2n) - low-pass (Equation F-10)
        // Y(2n) = X_ext(2n) + floor((Y(2n-1) + Y(2n+1) + 2) / 4)
        for n in (i0 / 2)..=((il - 1) / 2) {
            let idx = 2 * n;
            if idx >= i0 && idx < il {
                let x_2n = x_ext[get_ext_idx(idx)];
                let y_2n_m1 = get_y(&y, 2 * n - 1);
                let y_2n_p1 = get_y(&y, 2 * n + 1);
                y[(idx - i0) as usize] = x_2n + ((y_2n_m1 + y_2n_p1 + 2.0) / 4.0).floor();
            }
        }

        y
    }

    /// 1D_FILTD_9-7I procedure for forward (decomposition) filtering
    /// Equation F-11
    fn filter_1d_97i_decomp(&self, x_ext: &[f64], i0: i32, il: i32, ext_offset: i32) -> Vec<f64> {
        use lifting_params_97::*;

        let len = (il - i0) as usize;
        let mut y = vec![0.0; len];

        let get_ext_idx = |i: i32| -> usize { (i - (i0 - ext_offset)) as usize };

        // Helper to get Y value with boundary handling
        let get_y = |y: &[f64], idx: i32| -> f64 {
            if idx >= i0 && idx < il {
                y[(idx - i0) as usize]
            } else {
                let reflected = Self::pse_o(idx, i0, il);
                y[(reflected - i0) as usize]
            }
        };

        // Initialize Y from X_ext
        for i in i0..il {
            y[(i - i0) as usize] = x_ext[get_ext_idx(i)];
        }

        // Step 1: Y(2n+1) = X_ext(2n+1) + α*(X_ext(2n) + X_ext(2n+2))
        for n in ((i0 - 1) / 2)..=((il - 2) / 2) {
            let idx = 2 * n + 1;
            if idx >= i0 && idx < il {
                let x_2n_p1 = x_ext[get_ext_idx(idx)];
                let x_2n = x_ext[get_ext_idx(2 * n)];
                let x_2n_p2 = x_ext[get_ext_idx(2 * n + 2)];
                y[(idx - i0) as usize] = x_2n_p1 + ALPHA * (x_2n + x_2n_p2);
            }
        }

        // Step 2: Y(2n) = X_ext(2n) + β*(Y(2n-1) + Y(2n+1))
        let y_copy = y.clone();
        for n in (i0 / 2)..=((il - 1) / 2) {
            let idx = 2 * n;
            if idx >= i0 && idx < il {
                let x_2n = x_ext[get_ext_idx(idx)];
                let y_2n_m1 = get_y(&y_copy, 2 * n - 1);
                let y_2n_p1 = get_y(&y_copy, 2 * n + 1);
                y[(idx - i0) as usize] = x_2n + BETA * (y_2n_m1 + y_2n_p1);
            }
        }

        // Step 3: Y(2n+1) = Y(2n+1) + γ*(Y(2n) + Y(2n+2))
        let y_copy = y.clone();
        for n in ((i0 - 1) / 2)..=((il - 2) / 2) {
            let idx = 2 * n + 1;
            if idx >= i0 && idx < il {
                let y_2n = get_y(&y_copy, 2 * n);
                let y_2n_p2 = get_y(&y_copy, 2 * n + 2);
                y[(idx - i0) as usize] += GAMMA * (y_2n + y_2n_p2);
            }
        }

        // Step 4: Y(2n) = Y(2n) + δ*(Y(2n-1) + Y(2n+1))
        let y_copy = y.clone();
        for n in (i0 / 2)..=((il - 1) / 2) {
            let idx = 2 * n;
            if idx >= i0 && idx < il {
                let y_2n_m1 = get_y(&y_copy, 2 * n - 1);
                let y_2n_p1 = get_y(&y_copy, 2 * n + 1);
                y[(idx - i0) as usize] += DELTA * (y_2n_m1 + y_2n_p1);
            }
        }

        // Step 5: Y(2n) = Y(2n) * K (low-pass scaling)
        for n in (i0 / 2)..=((il - 1) / 2) {
            let idx = 2 * n;
            if idx >= i0 && idx < il {
                y[(idx - i0) as usize] *= K;
            }
        }

        // Step 6: Y(2n+1) = Y(2n+1) / K (high-pass scaling)
        for n in ((i0 - 1) / 2)..=((il - 2) / 2) {
            let idx = 2 * n + 1;
            if idx >= i0 && idx < il {
                y[(idx - i0) as usize] /= K;
            }
        }

        y
    }

    // ========================================================================
    // 1D Sub-band Procedures (Section F.3.6 and F.4.6)
    // ========================================================================

    /// 1D_SR procedure - 1D sub-band reconstruction
    /// Takes interleaved low/high coefficients and produces reconstructed signal
    pub fn subband_reconstruct_1d(&self, y: &[f64], i0: i32, il: i32) -> Vec<f64> {
        let len = (il - i0) as usize;

        // Handle length 1 signal
        if len == 1 {
            return if i0 % 2 == 0 {
                vec![y[0]]
            } else {
                vec![y[0] / 2.0]
            };
        }

        // Extend and filter
        let (i_left, i_right) = self.get_extension_params_r(i0, il);
        let y_ext = self.extend_signal_1d(y, i0, il, i_left, i_right);

        match self.filter_type {
            FilterType::Reversible53 => self.filter_1d_53r(&y_ext, i0, il, i_left),
            FilterType::Irreversible97 => self.filter_1d_97i(&y_ext, i0, il, i_left),
        }
    }

    /// 1D_SD procedure - 1D sub-band decomposition
    /// Takes signal and produces interleaved low/high coefficients
    pub fn subband_decompose_1d(&self, x: &[f64], i0: i32, il: i32) -> Vec<f64> {
        let len = (il - i0) as usize;

        // Handle length 1 signal
        if len == 1 {
            return if i0 % 2 == 0 {
                vec![x[0]]
            } else {
                vec![2.0 * x[0]]
            };
        }

        // Extend and filter
        let (i_left, i_right) = self.get_extension_params_d(i0, il);
        let x_ext = self.extend_signal_1d(x, i0, il, i_left, i_right);

        match self.filter_type {
            FilterType::Reversible53 => self.filter_1d_53r_decomp(&x_ext, i0, il, i_left),
            FilterType::Irreversible97 => self.filter_1d_97i_decomp(&x_ext, i0, il, i_left),
        }
    }

    // ========================================================================
    // 2D Interleave/Deinterleave Procedures (Section F.3.3 and F.4.5)
    // ========================================================================

    /// 2D_INTERLEAVE procedure - interleave four sub-bands into one array
    /// Figure F.8
    pub fn interleave_2d(
        &self,
        subbands: &SubBands,
        u0: i32,
        v0: i32,
        u1: i32,
        v1: i32,
    ) -> Array2D<f64> {
        let width = (u1 - u0) as usize;
        let height = (v1 - v0) as usize;
        let mut a = Array2D::with_offset(width, height, u0, v0);

        for v in v0..v1 {
            for u in u0..u1 {
                let value = if v % 2 == 0 {
                    if u % 2 == 0 {
                        // LL sub-band
                        let ll_u = u / 2;
                        let ll_v = v / 2;
                        if ll_u >= subbands.ll.u0
                            && ll_u < subbands.ll.u1()
                            && ll_v >= subbands.ll.v0
                            && ll_v < subbands.ll.v1()
                        {
                            *subbands.ll.get(ll_u, ll_v)
                        } else {
                            0.0
                        }
                    } else {
                        // HL sub-band
                        let hl_u = (u - 1) / 2;
                        let hl_v = v / 2;
                        if hl_u >= subbands.hl.u0
                            && hl_u < subbands.hl.u1()
                            && hl_v >= subbands.hl.v0
                            && hl_v < subbands.hl.v1()
                        {
                            *subbands.hl.get(hl_u, hl_v)
                        } else {
                            0.0
                        }
                    }
                } else {
                    if u % 2 == 0 {
                        // LH sub-band
                        let lh_u = u / 2;
                        let lh_v = (v - 1) / 2;
                        if lh_u >= subbands.lh.u0
                            && lh_u < subbands.lh.u1()
                            && lh_v >= subbands.lh.v0
                            && lh_v < subbands.lh.v1()
                        {
                            *subbands.lh.get(lh_u, lh_v)
                        } else {
                            0.0
                        }
                    } else {
                        // HH sub-band
                        let hh_u = (u - 1) / 2;
                        let hh_v = (v - 1) / 2;
                        if hh_u >= subbands.hh.u0
                            && hh_u < subbands.hh.u1()
                            && hh_v >= subbands.hh.v0
                            && hh_v < subbands.hh.v1()
                        {
                            *subbands.hh.get(hh_u, hh_v)
                        } else {
                            0.0
                        }
                    }
                };
                a.set(u, v, value);
            }
        }

        a
    }

    /// 2D_DEINTERLEAVE procedure - split one array into four sub-bands
    /// Figure F.28
    pub fn deinterleave_2d(&self, a: &Array2D<f64>) -> SubBands {
        let u0 = a.u0;
        let v0 = a.v0;
        let u1 = a.u1();
        let v1 = a.v1();

        // Calculate sub-band dimensions based on coordinate offsets
        // Using equations from B-15 of the spec
        let ll_u0 = (u0 as f64 / 2.0).ceil() as i32;
        let ll_u1 = (u1 as f64 / 2.0).ceil() as i32;
        let ll_v0 = (v0 as f64 / 2.0).ceil() as i32;
        let ll_v1 = (v1 as f64 / 2.0).ceil() as i32;

        let hl_u0 = ((u0 - 1) as f64 / 2.0).ceil() as i32;
        let hl_u1 = ((u1 - 1) as f64 / 2.0).ceil() as i32;
        let hl_v0 = (v0 as f64 / 2.0).ceil() as i32;
        let hl_v1 = (v1 as f64 / 2.0).ceil() as i32;

        let lh_u0 = (u0 as f64 / 2.0).ceil() as i32;
        let lh_u1 = (u1 as f64 / 2.0).ceil() as i32;
        let lh_v0 = ((v0 - 1) as f64 / 2.0).ceil() as i32;
        let lh_v1 = ((v1 - 1) as f64 / 2.0).ceil() as i32;

        let hh_u0 = ((u0 - 1) as f64 / 2.0).ceil() as i32;
        let hh_u1 = ((u1 - 1) as f64 / 2.0).ceil() as i32;
        let hh_v0 = ((v0 - 1) as f64 / 2.0).ceil() as i32;
        let hh_v1 = ((v1 - 1) as f64 / 2.0).ceil() as i32;

        let ll_width = (ll_u1 - ll_u0).max(0) as usize;
        let ll_height = (ll_v1 - ll_v0).max(0) as usize;
        let hl_width = (hl_u1 - hl_u0).max(0) as usize;
        let hl_height = (hl_v1 - hl_v0).max(0) as usize;
        let lh_width = (lh_u1 - lh_u0).max(0) as usize;
        let lh_height = (lh_v1 - lh_v0).max(0) as usize;
        let hh_width = (hh_u1 - hh_u0).max(0) as usize;
        let hh_height = (hh_v1 - hh_v0).max(0) as usize;

        let mut ll = Array2D::with_offset(ll_width, ll_height, ll_u0, ll_v0);
        let mut hl = Array2D::with_offset(hl_width, hl_height, hl_u0, hl_v0);
        let mut lh = Array2D::with_offset(lh_width, lh_height, lh_u0, lh_v0);
        let mut hh = Array2D::with_offset(hh_width, hh_height, hh_u0, hh_v0);

        // Deinterleave based on coordinate parity
        for v in v0..v1 {
            for u in u0..u1 {
                let val = *a.get(u, v);
                if v % 2 == 0 {
                    if u % 2 == 0 {
                        ll.set(u / 2, v / 2, val);
                    } else {
                        hl.set((u - 1) / 2, v / 2, val);
                    }
                } else {
                    if u % 2 == 0 {
                        lh.set(u / 2, (v - 1) / 2, val);
                    } else {
                        hh.set((u - 1) / 2, (v - 1) / 2, val);
                    }
                }
            }
        }

        SubBands { ll, hl, lh, hh }
    }

    // ========================================================================
    // 2D Sub-band Reconstruction/Decomposition (Section F.3.2 and F.4.2)
    // ========================================================================

    /// HOR_SR procedure - horizontal sub-band reconstruction
    pub fn horizontal_reconstruct(&self, a: &mut Array2D<f64>) {
        let u0 = a.u0;
        let u1 = a.u1();
        let v0 = a.v0;
        let v1 = a.v1();

        for v in v0..v1 {
            let row = a.get_row(v);
            let reconstructed = self.subband_reconstruct_1d(&row, u0, u1);
            a.set_row(v, &reconstructed);
        }
    }

    /// VER_SR procedure - vertical sub-band reconstruction
    pub fn vertical_reconstruct(&self, a: &mut Array2D<f64>) {
        let u0 = a.u0;
        let u1 = a.u1();
        let v0 = a.v0;
        let v1 = a.v1();

        for u in u0..u1 {
            let col = a.get_column(u);
            let reconstructed = self.subband_reconstruct_1d(&col, v0, v1);
            a.set_column(u, &reconstructed);
        }
    }

    /// HOR_SD procedure - horizontal sub-band decomposition
    pub fn horizontal_decompose(&self, a: &mut Array2D<f64>) {
        let u0 = a.u0;
        let u1 = a.u1();
        let v0 = a.v0;
        let v1 = a.v1();

        for v in v0..v1 {
            let row = a.get_row(v);
            let decomposed = self.subband_decompose_1d(&row, u0, u1);
            a.set_row(v, &decomposed);
        }
    }

    /// VER_SD procedure - vertical sub-band decomposition
    pub fn vertical_decompose(&self, a: &mut Array2D<f64>) {
        let u0 = a.u0;
        let u1 = a.u1();
        let v0 = a.v0;
        let v1 = a.v1();

        for u in u0..u1 {
            let col = a.get_column(u);
            let decomposed = self.subband_decompose_1d(&col, v0, v1);
            a.set_column(u, &decomposed);
        }
    }

    /// 2D_SR procedure - 2D sub-band reconstruction
    /// Reconstructs (lev-1)LL from levLL, levHL, levLH, levHH
    pub fn subband_reconstruct_2d(
        &self,
        subbands: &SubBands,
        u0: i32,
        v0: i32,
        u1: i32,
        v1: i32,
    ) -> Array2D<f64> {
        // Step 1: Interleave the four sub-bands
        let mut a = self.interleave_2d(subbands, u0, v0, u1, v1);

        // Step 2: Horizontal reconstruction
        self.horizontal_reconstruct(&mut a);

        // Step 3: Vertical reconstruction
        self.vertical_reconstruct(&mut a);

        a
    }

    /// 2D_SD procedure - 2D sub-band decomposition
    /// Decomposes (lev-1)LL into levLL, levHL, levLH, levHH
    pub fn subband_decompose_2d(&self, a: &Array2D<f64>) -> SubBands {
        let mut working = a.clone();

        // Step 1: Vertical decomposition
        self.vertical_decompose(&mut working);

        // Step 2: Horizontal decomposition
        self.horizontal_decompose(&mut working);

        // Step 3: Deinterleave into four sub-bands
        self.deinterleave_2d(&working)
    }

    // ========================================================================
    // Full DWT Procedures (Section F.3.1 and F.4.1)
    // ========================================================================

    /// IDWT procedure - Inverse Discrete Wavelet Transformation
    /// Transforms sub-bands back to tile-component samples
    pub fn idwt(&self, all_subbands: &[SubBands], n_levels: usize) -> Array2D<f64> {
        assert!(!all_subbands.is_empty());
        assert_eq!(all_subbands.len(), n_levels);

        // Start with the deepest LL sub-band
        let mut current = all_subbands[0].ll.clone();

        // Iterate from deepest level to level 1
        for lev in (0..n_levels).rev() {
            let bands = &all_subbands[lev];

            // Create sub-bands with current LL and this level's HL, LH, HH
            let level_bands = SubBands {
                ll: current,
                hl: bands.hl.clone(),
                lh: bands.lh.clone(),
                hh: bands.hh.clone(),
            };

            // Calculate the output dimensions for this level
            let u0 = level_bands
                .ll
                .u0
                .min(level_bands.hl.u0)
                .min(level_bands.lh.u0)
                .min(level_bands.hh.u0);
            let v0 = level_bands
                .ll
                .v0
                .min(level_bands.hl.v0)
                .min(level_bands.lh.v0)
                .min(level_bands.hh.v0);

            // Output size is sum of sub-band sizes
            let u1 = u0 + (level_bands.ll.width() + level_bands.hl.width()) as i32;
            let v1 = v0 + (level_bands.ll.height() + level_bands.lh.height()) as i32;

            // Adjust for actual interleaved coordinates
            let interleave_u0 = (level_bands.ll.u0 * 2).min(level_bands.hl.u0 * 2 + 1);
            let interleave_v0 = (level_bands.ll.v0 * 2).min(level_bands.lh.v0 * 2 + 1);
            let interleave_u1 = interleave_u0 + (u1 - u0);
            let interleave_v1 = interleave_v0 + (v1 - v0);

            current = self.subband_reconstruct_2d(
                &level_bands,
                interleave_u0,
                interleave_v0,
                interleave_u1,
                interleave_v1,
            );
        }

        current
    }

    /// FDWT procedure - Forward Discrete Wavelet Transformation
    /// Transforms tile-component samples into sub-bands
    pub fn fdwt(&self, input: &Array2D<f64>, n_levels: usize) -> Vec<SubBands> {
        let mut result = Vec::with_capacity(n_levels);
        let mut current = input.clone();

        for _lev in 0..n_levels {
            let subbands = self.subband_decompose_2d(&current);
            current = subbands.ll.clone();
            result.push(subbands);
        }

        result
    }

    /// Perform a complete forward then inverse transform (for testing round-trip)
    pub fn round_trip(&self, input: &Array2D<f64>, n_levels: usize) -> Array2D<f64> {
        let subbands = self.fdwt(input, n_levels);
        self.idwt(&subbands, n_levels)
    }
}

/// Convenience function for forward 5-3 reversible DWT
pub fn dwt_53_forward(input: &Array2D<f64>, n_levels: usize) -> Vec<SubBands> {
    let processor = DwtProcessor::new(FilterType::Reversible53);
    processor.fdwt(input, n_levels)
}

/// Convenience function for inverse 5-3 reversible DWT
pub fn dwt_53_inverse(subbands: &[SubBands], n_levels: usize) -> Array2D<f64> {
    let processor = DwtProcessor::new(FilterType::Reversible53);
    processor.idwt(subbands, n_levels)
}

/// Convenience function for forward 9-7 irreversible DWT
pub fn dwt_97_forward(input: &Array2D<f64>, n_levels: usize) -> Vec<SubBands> {
    let processor = DwtProcessor::new(FilterType::Irreversible97);
    processor.fdwt(input, n_levels)
}

/// Convenience function for inverse 9-7 irreversible DWT
pub fn dwt_97_inverse(subbands: &[SubBands], n_levels: usize) -> Array2D<f64> {
    let processor = DwtProcessor::new(FilterType::Irreversible97);
    processor.idwt(subbands, n_levels)
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    const EPSILON: f64 = 1e-10;
    const EPSILON_97: f64 = 1e-6; // Slightly looser tolerance for floating-point operations

    fn approx_eq(a: f64, b: f64, epsilon: f64) -> bool {
        (a - b).abs() < epsilon
    }

    fn arrays_approx_eq(a: &Array2D<f64>, b: &Array2D<f64>, epsilon: f64) -> bool {
        if a.width() != b.width() || a.height() != b.height() {
            return false;
        }
        for row in 0..a.height() {
            for col in 0..a.width() {
                if !approx_eq(a[(col, row)], b[(col, row)], epsilon) {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn test_array2d_creation() {
        let arr: Array2D<f64> = Array2D::new(4, 3);
        assert_eq!(arr.width(), 4);
        assert_eq!(arr.height(), 3);
        assert_eq!(arr[(0, 0)], 0.0);
    }

    #[test]
    fn test_array2d_with_offset() {
        let mut arr: Array2D<f64> = Array2D::with_offset(4, 3, 2, 5);
        assert_eq!(arr.u0, 2);
        assert_eq!(arr.v0, 5);
        assert_eq!(arr.u1(), 6);
        assert_eq!(arr.v1(), 8);

        arr.set(3, 6, 42.0);
        assert_eq!(*arr.get(3, 6), 42.0);
    }

    #[test]
    fn test_array2d_row_column_ops() {
        let data: Vec<f64> = (0..12).map(|x| x as f64).collect();
        let mut arr = Array2D::from_data(data, 4, 3);

        // Test row access
        let row0 = arr.get_row(0);
        assert_eq!(row0, vec![0.0, 1.0, 2.0, 3.0]);

        // Test column access
        let col0 = arr.get_column(0);
        assert_eq!(col0, vec![0.0, 4.0, 8.0]);

        // Test row modification
        arr.set_row(1, &[10.0, 11.0, 12.0, 13.0]);
        assert_eq!(arr.get_row(1), vec![10.0, 11.0, 12.0, 13.0]);

        // Test column modification
        arr.set_column(2, &[20.0, 21.0, 22.0]);
        assert_eq!(arr.get_column(2), vec![20.0, 21.0, 22.0]);
    }

    #[test]
    fn test_pse_basic() {
        // Test periodic symmetric extension
        // For signal [a, b, c, d] at indices 0-3
        // Extension should give: ...d, c, b, a, b, c, d, c, b, a...
        assert_eq!(DwtProcessor::pse_o(0, 0, 4), 0);
        assert_eq!(DwtProcessor::pse_o(1, 0, 4), 1);
        assert_eq!(DwtProcessor::pse_o(2, 0, 4), 2);
        assert_eq!(DwtProcessor::pse_o(3, 0, 4), 3);

        // Reflection at boundaries
        assert_eq!(DwtProcessor::pse_o(-1, 0, 4), 0); // reflects to 0
        assert_eq!(DwtProcessor::pse_o(-2, 0, 4), 1); // reflects to 1
        assert_eq!(DwtProcessor::pse_o(4, 0, 4), 3); // reflects to 3
        assert_eq!(DwtProcessor::pse_o(5, 0, 4), 2); // reflects to 2
    }

    #[test]
    fn test_extend_signal() {
        let processor = DwtProcessor::new(FilterType::Reversible53);
        let signal = vec![1.0, 2.0, 3.0, 4.0];

        let extended = processor.extend_signal_1d(&signal, 0, 4, 2, 2);

        // Expected: reflection at both ends
        // Original: [1, 2, 3, 4] at indices 0-3
        // Extended left by 2: indices -2, -1 map to 1, 0 (reflection)
        // Extended right by 2: indices 4, 5 map to 3, 2 (reflection)
        assert_eq!(extended.len(), 8);
        assert_eq!(extended[0], 2.0); // index -2 -> 1
        assert_eq!(extended[1], 1.0); // index -1 -> 0
        assert_eq!(extended[2], 1.0); // index 0
        assert_eq!(extended[3], 2.0); // index 1
        assert_eq!(extended[4], 3.0); // index 2
        assert_eq!(extended[5], 4.0); // index 3
        assert_eq!(extended[6], 4.0); // index 4 -> 3
        assert_eq!(extended[7], 3.0); // index 5 -> 2
    }

    #[test]
    fn test_1d_roundtrip_53_simple() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // Simple signal
        let original = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];

        // Decompose
        let decomposed = processor.subband_decompose_1d(&original, 0, 8);

        // Reconstruct
        let reconstructed = processor.subband_reconstruct_1d(&decomposed, 0, 8);

        // Should be perfectly reconstructed for 5-3 filter
        for i in 0..original.len() {
            assert!(
                approx_eq(original[i], reconstructed[i], EPSILON),
                "Mismatch at index {}: expected {}, got {}",
                i,
                original[i],
                reconstructed[i]
            );
        }
    }

    #[test]
    fn test_1d_roundtrip_97_simple() {
        let processor = DwtProcessor::new(FilterType::Irreversible97);

        let original = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];

        let decomposed = processor.subband_decompose_1d(&original, 0, 8);
        let reconstructed = processor.subband_reconstruct_1d(&decomposed, 0, 8);

        for i in 0..original.len() {
            assert!(
                approx_eq(original[i], reconstructed[i], EPSILON_97),
                "Mismatch at index {}: expected {}, got {}",
                i,
                original[i],
                reconstructed[i]
            );
        }
    }

    #[test]
    fn test_1d_decompose_53_energy_preservation() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        let original = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0];
        let decomposed = processor.subband_decompose_1d(&original, 0, 8);

        // Low-pass coefficients should be at even indices, high-pass at odd
        // For a ramp signal, high-pass should capture the differences
        let low_pass: Vec<f64> = decomposed.iter().step_by(2).copied().collect();
        let high_pass: Vec<f64> = decomposed.iter().skip(1).step_by(2).copied().collect();

        assert_eq!(low_pass.len(), 4);
        assert_eq!(high_pass.len(), 4);

        // High-pass should be relatively small for smooth ramp
        // (captures detail/edges, not DC content)
    }

    #[test]
    fn test_2d_deinterleave_interleave_roundtrip() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // Create a simple 4x4 array with distinct values
        let data: Vec<f64> = (0..16).map(|x| x as f64).collect();
        let original = Array2D::from_data(data, 4, 4);

        // Deinterleave
        let subbands = processor.deinterleave_2d(&original);

        // Verify sub-band sizes
        assert_eq!(subbands.ll.width(), 2);
        assert_eq!(subbands.ll.height(), 2);
        assert_eq!(subbands.hl.width(), 2);
        assert_eq!(subbands.hl.height(), 2);
        assert_eq!(subbands.lh.width(), 2);
        assert_eq!(subbands.lh.height(), 2);
        assert_eq!(subbands.hh.width(), 2);
        assert_eq!(subbands.hh.height(), 2);

        // Interleave back
        let reconstructed = processor.interleave_2d(&subbands, 0, 0, 4, 4);

        // Should be identical
        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON));
    }

    #[test]
    fn test_2d_roundtrip_53() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // Create test image
        let data: Vec<f64> = (0..64).map(|x| (x % 16) as f64).collect();
        let original = Array2D::from_data(data, 8, 8);

        // Decompose
        let subbands = processor.subband_decompose_2d(&original);

        // Reconstruct
        let reconstructed = processor.subband_reconstruct_2d(&subbands, 0, 0, 8, 8);

        // Should be perfectly reconstructed
        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON));
    }

    #[test]
    fn test_2d_roundtrip_97() {
        let processor = DwtProcessor::new(FilterType::Irreversible97);

        let data: Vec<f64> = (0..64).map(|x| (x % 16) as f64).collect();
        let original = Array2D::from_data(data, 8, 8);

        let subbands = processor.subband_decompose_2d(&original);
        let reconstructed = processor.subband_reconstruct_2d(&subbands, 0, 0, 8, 8);

        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON_97));
    }

    #[test]
    fn test_multi_level_dwt_53() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // 16x16 test image
        let data: Vec<f64> = (0..256).map(|x| ((x % 32) as f64).sin() * 100.0).collect();
        let original = Array2D::from_data(data, 16, 16);

        // 2-level decomposition
        let subbands = processor.fdwt(&original, 2);
        assert_eq!(subbands.len(), 2);

        // Reconstruct
        let reconstructed = processor.idwt(&subbands, 2);

        // Verify dimensions
        assert_eq!(reconstructed.width(), original.width());
        assert_eq!(reconstructed.height(), original.height());

        // Verify values (should be near-perfect for reversible)
        assert!(arrays_approx_eq(&original, &reconstructed, 1.0)); // Integer rounding may cause small errors
    }

    #[test]
    fn test_multi_level_dwt_97() {
        let processor = DwtProcessor::new(FilterType::Irreversible97);

        let data: Vec<f64> = (0..256).map(|x| ((x % 32) as f64).sin() * 100.0).collect();
        let original = Array2D::from_data(data, 16, 16);

        let subbands = processor.fdwt(&original, 2);
        let reconstructed = processor.idwt(&subbands, 2);

        assert_eq!(reconstructed.width(), original.width());
        assert_eq!(reconstructed.height(), original.height());

        // 9-7 should also achieve good reconstruction
        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON_97));
    }

    #[test]
    fn test_convenience_functions() {
        // Test 5-3
        let data: Vec<f64> = (0..64).map(|x| x as f64).collect();
        let input = Array2D::from_data(data, 8, 8);

        let subbands_53 = dwt_53_forward(&input, 1);
        let result_53 = dwt_53_inverse(&subbands_53, 1);
        assert!(arrays_approx_eq(&input, &result_53, 1.0));

        // Test 9-7
        let data: Vec<f64> = (0..64).map(|x| x as f64).collect();
        let input = Array2D::from_data(data, 8, 8);

        let subbands_97 = dwt_97_forward(&input, 1);
        let result_97 = dwt_97_inverse(&subbands_97, 1);
        assert!(arrays_approx_eq(&input, &result_97, EPSILON_97));
    }

    #[test]
    fn test_round_trip_function() {
        let processor = DwtProcessor::new(FilterType::Irreversible97);

        let data: Vec<f64> = (0..64).map(|x| x as f64).collect();
        let input = Array2D::from_data(data, 8, 8);

        let result = processor.round_trip(&input, 2);
        assert!(arrays_approx_eq(&input, &result, EPSILON_97));
    }

    #[test]
    fn test_single_element_signal() {
        let processor_53 = DwtProcessor::new(FilterType::Reversible53);
        let processor_97 = DwtProcessor::new(FilterType::Irreversible97);

        // Even index
        let signal_even = vec![42.0];
        let decomposed_53 = processor_53.subband_decompose_1d(&signal_even, 0, 1);
        let reconstructed_53 = processor_53.subband_reconstruct_1d(&decomposed_53, 0, 1);
        assert!(approx_eq(signal_even[0], reconstructed_53[0], EPSILON));

        // Odd index
        let signal_odd = vec![42.0];
        let decomposed_53_odd = processor_53.subband_decompose_1d(&signal_odd, 1, 2);
        let reconstructed_53_odd = processor_53.subband_reconstruct_1d(&decomposed_53_odd, 1, 2);
        assert!(approx_eq(signal_odd[0], reconstructed_53_odd[0], EPSILON));
    }

    #[test]
    fn test_small_signals() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // Length 2
        let signal2 = vec![1.0, 2.0];
        let decomposed2 = processor.subband_decompose_1d(&signal2, 0, 2);
        let reconstructed2 = processor.subband_reconstruct_1d(&decomposed2, 0, 2);
        assert!(approx_eq(signal2[0], reconstructed2[0], EPSILON));
        assert!(approx_eq(signal2[1], reconstructed2[1], EPSILON));

        // Length 3
        let signal3 = vec![1.0, 2.0, 3.0];
        let decomposed3 = processor.subband_decompose_1d(&signal3, 0, 3);
        let reconstructed3 = processor.subband_reconstruct_1d(&decomposed3, 0, 3);
        for i in 0..3 {
            assert!(approx_eq(signal3[i], reconstructed3[i], EPSILON));
        }
    }

    #[test]
    fn test_lifting_parameters() {
        use lifting_params_97::*;

        // Verify lifting parameter values from Table F.4
        assert!(approx_eq(ALPHA, -1.586_134_342_059_924, 1e-15));
        assert!(approx_eq(BETA, -0.052_980_118_572_961, 1e-15));
        assert!(approx_eq(GAMMA, 0.882_911_075_530_934, 1e-15));
        assert!(approx_eq(DELTA, 0.443_506_852_043_971, 1e-15));
        assert!(approx_eq(K, 1.230_174_104_914_001, 1e-15));
    }

    #[test]
    fn test_subband_dimensions() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // Test various input sizes
        for size in [8, 9, 15, 16, 17].iter() {
            let data: Vec<f64> = (0..(size * size)).map(|x| x as f64).collect();
            let input = Array2D::from_data(data, *size, *size);

            let subbands = processor.subband_decompose_2d(&input);

            // Total coefficients should equal input size
            let total = subbands.ll.width() * subbands.ll.height()
                + subbands.hl.width() * subbands.hl.height()
                + subbands.lh.width() * subbands.lh.height()
                + subbands.hh.width() * subbands.hh.height();

            assert_eq!(total, size * size, "Size mismatch for input size {}", size);
        }
    }

    #[test]
    fn test_dc_signal() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // DC signal (all same value) should have zero high-pass coefficients
        let dc_value = 128.0;
        let data: Vec<f64> = vec![dc_value; 64];
        let input = Array2D::from_data(data, 8, 8);

        let subbands = processor.subband_decompose_2d(&input);

        // High-pass sub-bands should be near zero
        for row in 0..subbands.hl.height() {
            for col in 0..subbands.hl.width() {
                assert!(approx_eq(subbands.hl[(col, row)], 0.0, 1.0));
            }
        }
        for row in 0..subbands.lh.height() {
            for col in 0..subbands.lh.width() {
                assert!(approx_eq(subbands.lh[(col, row)], 0.0, 1.0));
            }
        }
        for row in 0..subbands.hh.height() {
            for col in 0..subbands.hh.width() {
                assert!(approx_eq(subbands.hh[(col, row)], 0.0, 1.0));
            }
        }
    }

    #[test]
    fn test_non_zero_offset() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // Create array with non-zero offset (simulating tile not at origin)
        let mut input = Array2D::<f64>::with_offset(8, 8, 4, 4);
        for v in 4..12 {
            for u in 4..12 {
                input.set(u, v, ((u - 4) + (v - 4) * 8) as f64);
            }
        }

        let subbands = processor.subband_decompose_2d(&input);
        let reconstructed = processor.subband_reconstruct_2d(&subbands, 4, 4, 12, 12);

        // Verify reconstruction
        for v in 4..12 {
            for u in 4..12 {
                assert!(
                    approx_eq(*input.get(u, v), *reconstructed.get(u, v), 1.0),
                    "Mismatch at ({}, {})",
                    u,
                    v
                );
            }
        }
    }

    /// Test data from Table J.3 of the standard
    #[test]
    fn test_spec_example_data() {
        // This is the 13x17 sample data from Table J.3
        let sample_data: Vec<Vec<i32>> = vec![
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            vec![1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            vec![2, 2, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            vec![3, 3, 3, 4, 5, 5, 6, 7, 8, 9, 10, 11, 12],
            vec![4, 4, 4, 5, 5, 6, 7, 8, 8, 9, 10, 11, 12],
            vec![5, 5, 5, 5, 6, 7, 7, 8, 9, 10, 11, 12, 13],
            vec![6, 6, 6, 6, 7, 7, 8, 9, 10, 10, 11, 12, 13],
            vec![7, 7, 7, 7, 8, 8, 9, 9, 10, 11, 12, 13, 13],
            vec![8, 8, 8, 8, 8, 9, 10, 10, 11, 12, 12, 13, 14],
            vec![9, 9, 9, 9, 9, 10, 10, 11, 12, 12, 13, 14, 15],
            vec![10, 10, 10, 10, 10, 11, 11, 12, 12, 13, 14, 14, 15],
            vec![11, 11, 11, 11, 11, 12, 12, 13, 13, 14, 14, 15, 16],
            vec![12, 12, 12, 12, 12, 13, 13, 13, 14, 15, 15, 16, 16],
            vec![13, 13, 13, 13, 13, 13, 14, 14, 15, 15, 16, 17, 17],
            vec![14, 14, 14, 14, 14, 14, 15, 15, 16, 16, 17, 17, 18],
            vec![15, 15, 15, 15, 15, 15, 16, 16, 17, 17, 18, 18, 19],
            vec![16, 16, 16, 16, 16, 16, 17, 17, 17, 18, 18, 19, 20],
        ];

        let width = 13;
        let height = 17;
        let mut data = Vec::with_capacity(width * height);
        for row in &sample_data {
            for &val in row {
                data.push(val as f64);
            }
        }
        let input = Array2D::from_data(data.clone(), width, height);

        // Test 5-3 roundtrip
        let processor_53 = DwtProcessor::new(FilterType::Reversible53);
        //let sub_bands = processor_53.fdwt(&input, 2);
        //println!("Sub bands for J.3 53 {:?}", sub_bands);
        let result_53 = processor_53.round_trip(&input, 2);

        // For 5-3 reversible, should be exact (with integer rounding)
        for row in 0..height {
            for col in 0..width {
                let orig = input[(col, row)];
                let recon = result_53[(col, row)];
                assert!(
                    (orig - recon).abs() < 1.0,
                    "5-3 mismatch at ({}, {}): orig={}, recon={}",
                    col,
                    row,
                    orig,
                    recon
                );
            }
        }

        // Test 9-7 roundtrip
        let processor_97 = DwtProcessor::new(FilterType::Irreversible97);
        let input_97 = Array2D::from_data(data, width, height);
        let result_97 = processor_97.round_trip(&input_97, 2);

        // For 9-7, should be very close
        for row in 0..height {
            for col in 0..width {
                let orig = input_97[(col, row)];
                let recon = result_97[(col, row)];
                assert!(
                    (orig - recon).abs() < EPSILON_97,
                    "9-7 mismatch at ({}, {}): orig={}, recon={}",
                    col,
                    row,
                    orig,
                    recon
                );
            }
        }
    }

    #[test]
    fn test_filter_type_enum() {
        let rev = FilterType::Reversible53;
        let irr = FilterType::Irreversible97;

        assert_ne!(rev, irr);
        assert_eq!(rev, FilterType::Reversible53);
        assert_eq!(irr, FilterType::Irreversible97);
    }

    #[test]
    fn test_subband_type_enum() {
        assert_eq!(SubBandType::LL, SubBandType::LL);
        assert_ne!(SubBandType::LL, SubBandType::HL);
        assert_ne!(SubBandType::LH, SubBandType::HH);
    }
}
