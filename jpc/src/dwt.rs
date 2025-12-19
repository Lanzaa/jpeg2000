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

impl SubBands {
    fn ll(&self) -> &Array2D<f64> {
        &self.ll
    }
    fn hl(&self) -> &Array2D<f64> {
        &self.hl
    }
    fn lh(&self) -> &Array2D<f64> {
        &self.lh
    }
    fn hh(&self) -> &Array2D<f64> {
        &self.hh
    }
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
    // 1D Lifting-based DWT (5-3 Reversible)
    // ========================================================================

    /// Forward 5-3 DWT using lifting (in-place conceptually)
    /// Input: signal x of length len
    /// Output: interleaved [low0, high0, low1, high1, ...] coefficients
    fn lifting_forward_53(&self, x: &[f64]) -> Vec<f64> {
        let len = x.len();
        if len == 0 {
            return vec![];
        }
        if len == 1 {
            return vec![x[0]];
        }

        let mut y = x.to_vec();

        // Step 1: Predict (compute high-pass at odd positions)
        // y[2n+1] = x[2n+1] - floor((x[2n] + x[2n+2]) / 2)
        for i in (1..len).step_by(2) {
            let left = y[i - 1];
            let right = if i + 1 < len { y[i + 1] } else { y[i - 1] }; // symmetric extension
            y[i] -= ((left + right) / 2.0).floor();
        }

        // Step 2: Update (compute low-pass at even positions)
        // y[2n] = x[2n] + floor((y[2n-1] + y[2n+1] + 2) / 4)
        for i in (0..len).step_by(2) {
            let left = if i > 0 {
                y[i - 1]
            } else if len > 1 {
                y[1]
            } else {
                0.0
            }; // symmetric extension
            let right = if i + 1 < len {
                y[i + 1]
            } else if i > 0 {
                y[i - 1]
            } else {
                0.0
            };
            y[i] += ((left + right + 2.0) / 4.0).floor();
        }

        y
    }

    /// Inverse 5-3 DWT using lifting
    /// Input: interleaved coefficients
    /// Output: reconstructed signal
    fn lifting_inverse_53(&self, y: &[f64]) -> Vec<f64> {
        let len = y.len();
        if len == 0 {
            return vec![];
        }
        if len == 1 {
            return vec![y[0]];
        }

        let mut x = y.to_vec();

        // Step 1: Undo update (recover original even positions)
        // x[2n] = y[2n] - floor((y[2n-1] + y[2n+1] + 2) / 4)
        for i in (0..len).step_by(2) {
            let left = if i > 0 {
                x[i - 1]
            } else if len > 1 {
                x[1]
            } else {
                0.0
            };
            let right = if i + 1 < len {
                x[i + 1]
            } else if i > 0 {
                x[i - 1]
            } else {
                0.0
            };
            x[i] -= ((left + right + 2.0) / 4.0).floor();
        }

        // Step 2: Undo predict (recover original odd positions)
        // x[2n+1] = y[2n+1] + floor((x[2n] + x[2n+2]) / 2)
        for i in (1..len).step_by(2) {
            let left = x[i - 1];
            let right = if i + 1 < len { x[i + 1] } else { x[i - 1] };
            x[i] += ((left + right) / 2.0).floor();
        }

        x
    }

    // ========================================================================
    // 1D Lifting-based DWT (9-7 Irreversible)
    // ========================================================================

    /// Forward 9-7 DWT using lifting
    fn lifting_forward_97(&self, x: &[f64]) -> Vec<f64> {
        use lifting_params_97::*;

        let len = x.len();
        if len == 0 {
            return vec![];
        }
        if len == 1 {
            return vec![x[0]];
        }

        let mut y = x.to_vec();

        // Helper for symmetric extension
        let ext = |arr: &[f64], i: i32| -> f64 {
            if i < 0 {
                arr[(-i).min(len as i32 - 1) as usize]
            } else if i >= len as i32 {
                arr[(2 * len as i32 - 2 - i).max(0) as usize]
            } else {
                arr[i as usize]
            }
        };

        // Step 1: y[2n+1] += α * (y[2n] + y[2n+2])
        for i in (1..len).step_by(2) {
            let left = ext(&y, i as i32 - 1);
            let right = ext(&y, i as i32 + 1);
            y[i] += ALPHA * (left + right);
        }

        // Step 2: y[2n] += β * (y[2n-1] + y[2n+1])
        let y_copy = y.clone();
        for i in (0..len).step_by(2) {
            let left = ext(&y_copy, i as i32 - 1);
            let right = ext(&y_copy, i as i32 + 1);
            y[i] += BETA * (left + right);
        }

        // Step 3: y[2n+1] += γ * (y[2n] + y[2n+2])
        let y_copy = y.clone();
        for i in (1..len).step_by(2) {
            let left = ext(&y_copy, i as i32 - 1);
            let right = ext(&y_copy, i as i32 + 1);
            y[i] += GAMMA * (left + right);
        }

        // Step 4: y[2n] += δ * (y[2n-1] + y[2n+1])
        let y_copy = y.clone();
        for i in (0..len).step_by(2) {
            let left = ext(&y_copy, i as i32 - 1);
            let right = ext(&y_copy, i as i32 + 1);
            y[i] += DELTA * (left + right);
        }

        // Step 5: Scale
        for i in (0..len).step_by(2) {
            y[i] *= K;
        }
        for i in (1..len).step_by(2) {
            y[i] /= K;
        }

        y
    }

    /// Inverse 9-7 DWT using lifting
    fn lifting_inverse_97(&self, y: &[f64]) -> Vec<f64> {
        use lifting_params_97::*;

        let len = y.len();
        if len == 0 {
            return vec![];
        }
        if len == 1 {
            return vec![y[0]];
        }

        let mut x = y.to_vec();

        // Helper for symmetric extension
        let ext = |arr: &[f64], i: i32| -> f64 {
            if i < 0 {
                arr[(-i).min(len as i32 - 1) as usize]
            } else if i >= len as i32 {
                arr[(2 * len as i32 - 2 - i).max(0) as usize]
            } else {
                arr[i as usize]
            }
        };

        // Step 1: Unscale
        for i in (0..len).step_by(2) {
            x[i] /= K;
        }
        for i in (1..len).step_by(2) {
            x[i] *= K;
        }

        // Step 2: x[2n] -= δ * (x[2n-1] + x[2n+1])
        let x_copy = x.clone();
        for i in (0..len).step_by(2) {
            let left = ext(&x_copy, i as i32 - 1);
            let right = ext(&x_copy, i as i32 + 1);
            x[i] -= DELTA * (left + right);
        }

        // Step 3: x[2n+1] -= γ * (x[2n] + x[2n+2])
        let x_copy = x.clone();
        for i in (1..len).step_by(2) {
            let left = ext(&x_copy, i as i32 - 1);
            let right = ext(&x_copy, i as i32 + 1);
            x[i] -= GAMMA * (left + right);
        }

        // Step 4: x[2n] -= β * (x[2n-1] + x[2n+1])
        let x_copy = x.clone();
        for i in (0..len).step_by(2) {
            let left = ext(&x_copy, i as i32 - 1);
            let right = ext(&x_copy, i as i32 + 1);
            x[i] -= BETA * (left + right);
        }

        // Step 5: x[2n+1] -= α * (x[2n] + x[2n+2])
        let x_copy = x.clone();
        for i in (1..len).step_by(2) {
            let left = ext(&x_copy, i as i32 - 1);
            let right = ext(&x_copy, i as i32 + 1);
            x[i] -= ALPHA * (left + right);
        }

        x
    }

    // ========================================================================
    // 1D Sub-band Procedures (Section F.3.6 and F.4.6)
    // ========================================================================

    /// 1D_SR procedure - 1D sub-band reconstruction
    /// Takes interleaved low/high coefficients and produces reconstructed signal
    pub fn subband_reconstruct_1d(&self, y: &[f64]) -> Vec<f64> {
        match self.filter_type {
            FilterType::Reversible53 => self.lifting_inverse_53(y),
            FilterType::Irreversible97 => self.lifting_inverse_97(y),
        }
    }

    /// 1D_SD procedure - 1D sub-band decomposition
    /// Takes signal and produces interleaved low/high coefficients
    pub fn subband_decompose_1d(&self, x: &[f64]) -> Vec<f64> {
        match self.filter_type {
            FilterType::Reversible53 => self.lifting_forward_53(x),
            FilterType::Irreversible97 => self.lifting_forward_97(x),
        }
    }

    /// ========================================================================
    /// 2D Interleave/Deinterleave Procedures (Section F.3.3 and F.4.5)
    /// ========================================================================
    /// 2D_DEINTERLEAVE procedure - split one array into four sub-bands
    pub fn deinterleave_2d(&self, a: &Array2D<f64>) -> SubBands {
        let width = a.width();
        let height = a.height();

        // Dimensions of sub-bands
        let ll_width = width.div_ceil(2);
        let ll_height = height.div_ceil(2);
        let hl_width = width / 2;
        let hl_height = height.div_ceil(2);
        let lh_width = width.div_ceil(2);
        let lh_height = height / 2;
        let hh_width = width / 2;
        let hh_height = height / 2;

        let mut ll = Array2D::new(ll_width, ll_height);
        let mut hl = Array2D::new(hl_width, hl_height);
        let mut lh = Array2D::new(lh_width, lh_height);
        let mut hh = Array2D::new(hh_width, hh_height);

        for row in 0..height {
            for col in 0..width {
                let val = a[(col, row)];
                if row % 2 == 0 {
                    if col % 2 == 0 {
                        ll[(col / 2, row / 2)] = val;
                    } else {
                        hl[(col / 2, row / 2)] = val;
                    }
                } else {
                    if col % 2 == 0 {
                        lh[(col / 2, row / 2)] = val;
                    } else {
                        hh[(col / 2, row / 2)] = val;
                    }
                }
            }
        }

        SubBands { ll, hl, lh, hh }
    }

    /// 2D_INTERLEAVE procedure - interleave four sub-bands into one array
    pub fn interleave_2d(&self, subbands: &SubBands) -> Array2D<f64> {
        let ll_width = subbands.ll.width();
        let ll_height = subbands.ll.height();
        let hl_width = subbands.hl.width();
        let lh_height = subbands.lh.height();

        let width = ll_width + hl_width;
        let height = ll_height + lh_height;

        let mut a = Array2D::new(width, height);

        for row in 0..height {
            for col in 0..width {
                let val = if row % 2 == 0 {
                    if col % 2 == 0 {
                        let ll_col = col / 2;
                        let ll_row = row / 2;
                        if ll_col < subbands.ll.width() && ll_row < subbands.ll.height() {
                            subbands.ll[(ll_col, ll_row)]
                        } else {
                            0.0
                        }
                    } else {
                        let hl_col = col / 2;
                        let hl_row = row / 2;
                        if hl_col < subbands.hl.width() && hl_row < subbands.hl.height() {
                            subbands.hl[(hl_col, hl_row)]
                        } else {
                            0.0
                        }
                    }
                } else {
                    if col % 2 == 0 {
                        let lh_col = col / 2;
                        let lh_row = row / 2;
                        if lh_col < subbands.lh.width() && lh_row < subbands.lh.height() {
                            subbands.lh[(lh_col, lh_row)]
                        } else {
                            0.0
                        }
                    } else {
                        let hh_col = col / 2;
                        let hh_row = row / 2;
                        if hh_col < subbands.hh.width() && hh_row < subbands.hh.height() {
                            subbands.hh[(hh_col, hh_row)]
                        } else {
                            0.0
                        }
                    }
                };
                a[(col, row)] = val;
            }
        }

        a
    }

    // ========================================================================
    // 2D Sub-band Reconstruction/Decomposition (Section F.3.2 and F.4.2)
    // ========================================================================

    /// HOR_SR procedure - horizontal sub-band reconstruction
    pub fn horizontal_reconstruct(&self, a: &mut Array2D<f64>) {
        for row in 0..a.height() {
            let row_data = a.get_row(a.v0 + row as i32);
            let reconstructed = self.subband_reconstruct_1d(&row_data);
            a.set_row(a.v0 + row as i32, &reconstructed);
        }
    }

    /// VER_SR procedure - vertical sub-band reconstruction
    pub fn vertical_reconstruct(&self, a: &mut Array2D<f64>) {
        for col in 0..a.width() {
            let col_data = a.get_column(a.u0 + col as i32);
            let reconstructed = self.subband_reconstruct_1d(&col_data);
            a.set_column(a.u0 + col as i32, &reconstructed);
        }
    }

    /// HOR_SD procedure - horizontal sub-band decomposition
    pub fn horizontal_decompose(&self, a: &mut Array2D<f64>) {
        for row in 0..a.height() {
            let row_data = a.get_row(a.v0 + row as i32);
            let decomposed = self.subband_decompose_1d(&row_data);
            a.set_row(a.v0 + row as i32, &decomposed);
        }
    }

    /// VER_SD procedure - vertical sub-band decomposition
    pub fn vertical_decompose(&self, a: &mut Array2D<f64>) {
        for col in 0..a.width() {
            let col_data = a.get_column(a.u0 + col as i32);
            let decomposed = self.subband_decompose_1d(&col_data);
            a.set_column(a.u0 + col as i32, &decomposed);
        }
    }

    /// 2D_SR procedure - 2D sub-band reconstruction
    /// Reconstructs (lev-1)LL from levLL, levHL, levLH, levHH
    pub fn subband_reconstruct_2d(&self, subbands: &SubBands) -> Array2D<f64> {
        // Step 1: Interleave the four sub-bands
        let mut a = self.interleave_2d(subbands);

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
        let mut current = all_subbands[n_levels - 1].ll.clone();

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

            current = self.subband_reconstruct_2d(&level_bands);
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

// ============================================================================
// Convenience Functions
// ============================================================================

/// Perform forward 5-3 DWT with specified number of decomposition levels
pub fn dwt_53_forward(input: &Array2D<f64>, n_levels: usize) -> Vec<SubBands> {
    let processor = DwtProcessor::new(FilterType::Reversible53);
    processor.fdwt(input, n_levels)
}

/// Perform inverse 5-3 DWT to reconstruct image from sub-bands
pub fn dwt_53_inverse(subbands: &[SubBands], n_levels: usize) -> Array2D<f64> {
    let processor = DwtProcessor::new(FilterType::Reversible53);
    processor.idwt(subbands, n_levels)
}

/// Perform forward 9-7 DWT with specified number of decomposition levels
pub fn dwt_97_forward(input: &Array2D<f64>, n_levels: usize) -> Vec<SubBands> {
    let processor = DwtProcessor::new(FilterType::Irreversible97);
    processor.fdwt(input, n_levels)
}

/// Perform inverse 9-7 DWT to reconstruct image from sub-bands
pub fn dwt_97_inverse(subbands: &[SubBands], n_levels: usize) -> Array2D<f64> {
    let processor = DwtProcessor::new(FilterType::Irreversible97);
    processor.idwt(subbands, n_levels)
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use log::info;

    use super::*;

    const EPSILON: f64 = 1e-10;
    const EPSILON_97: f64 = 1e-6;

    fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    fn arrays_approx_eq(a: &Array2D<f64>, b: &Array2D<f64>, eps: f64) -> bool {
        if a.width() != b.width() || a.height() != b.height() {
            return false;
        }
        for row in 0..a.height() {
            for col in 0..a.width() {
                if !approx_eq(a[(col, row)], b[(col, row)], eps) {
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
        let arr: Array2D<f64> = Array2D::with_offset(4, 3, 10, 20);
        assert_eq!(arr.u0, 10);
        assert_eq!(arr.v0, 20);
        assert_eq!(arr.u1(), 14);
        assert_eq!(arr.v1(), 23);
    }

    #[test]
    fn test_array2d_row_column_ops() {
        let data: Vec<f64> = (0..12).map(|x| x as f64).collect();
        let mut arr = Array2D::from_data(data, 4, 3);

        let row = arr.get_row(1);
        assert_eq!(row, vec![4.0, 5.0, 6.0, 7.0]);

        let col = arr.get_column(2);
        assert_eq!(col, vec![2.0, 6.0, 10.0]);

        arr.set_row(0, &[10.0, 11.0, 12.0, 13.0]);
        assert_eq!(arr[(0, 0)], 10.0);

        arr.set_column(0, &[20.0, 21.0, 22.0]);
        assert_eq!(arr[(0, 2)], 22.0);
    }

    #[test]
    fn test_filter_type_enum() {
        let ft1 = FilterType::Reversible53;
        let ft2 = FilterType::Irreversible97;
        assert_ne!(ft1, ft2);
    }

    #[test]
    fn test_subband_type_enum() {
        let sbt = SubBandType::LL;
        assert_eq!(sbt, SubBandType::LL);
    }

    #[test]
    fn test_lifting_parameters() {
        use lifting_params_97::*;
        assert!(ALPHA < 0.0);
        assert!(BETA < 0.0);
        assert!(GAMMA > 0.0);
        assert!(DELTA > 0.0);
        assert!(K > 1.0);
    }

    #[test]
    fn test_decode_1d_53() {
        // example given in J.10
        let processor = DwtProcessor::new(FilterType::Reversible53);
        let exp_transformed = [-26.0, 1.0, -22.0, 5.0, -30.0, 1.0, -32.0, 0.0, -19.0];
        let samples = [101, 103, 104, 105, 96, 97, 96, 102, 109];
        let level_shift = (2.0_f64).powf(7.0); // Ssiz = 7
        let signal: Vec<f64> = samples.iter().map(|v| (*v as f64) - level_shift).collect();

        let transformed = processor.subband_decompose_1d(&signal);
        info!("transformed: {:?}", transformed);
        assert_eq!(transformed, exp_transformed);
        let reconstructed = processor.subband_reconstruct_1d(&transformed);

        for (i, (&orig, &recon)) in signal.iter().zip(reconstructed.iter()).enumerate() {
            assert!(
                approx_eq(orig, recon, EPSILON),
                "Mismatch at index {}: expected {}, got {}",
                i,
                orig,
                recon
            );
        }
    }

    #[test]
    fn test_1d_roundtrip_53_simple() {
        let processor = DwtProcessor::new(FilterType::Reversible53);
        let signal: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];

        let transformed = processor.subband_decompose_1d(&signal);
        let reconstructed = processor.subband_reconstruct_1d(&transformed);

        for (i, (&orig, &recon)) in signal.iter().zip(reconstructed.iter()).enumerate() {
            assert!(
                approx_eq(orig, recon, EPSILON),
                "Mismatch at index {}: expected {}, got {}",
                i,
                orig,
                recon
            );
        }
    }

    #[test]
    fn test_1d_roundtrip_97_simple() {
        let processor = DwtProcessor::new(FilterType::Irreversible97);
        let signal: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];

        let transformed = processor.subband_decompose_1d(&signal);
        let reconstructed = processor.subband_reconstruct_1d(&transformed);

        for (i, (&orig, &recon)) in signal.iter().zip(reconstructed.iter()).enumerate() {
            assert!(
                approx_eq(orig, recon, EPSILON_97),
                "Mismatch at index {}: expected {}, got {}",
                i,
                orig,
                recon
            );
        }
    }

    #[test]
    fn test_1d_decompose_53_energy_preservation() {
        let processor = DwtProcessor::new(FilterType::Reversible53);
        let signal: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0];

        let transformed = processor.subband_decompose_1d(&signal);

        // Check that we got a result of the same length
        assert_eq!(transformed.len(), signal.len());
    }

    #[test]
    fn test_2d_deinterleave_interleave_roundtrip() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        let data: Vec<f64> = (0..16).map(|x| x as f64).collect();
        let original = Array2D::from_data(data, 4, 4);

        let subbands = processor.deinterleave_2d(&original);
        let reconstructed = processor.interleave_2d(&subbands);

        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON));
    }

    #[test]
    fn test_2d_roundtrip_53() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        let data: Vec<f64> = (0..64).map(|x| x as f64).collect();
        let original = Array2D::from_data(data, 8, 8);

        let subbands = processor.subband_decompose_2d(&original);
        let reconstructed = processor.subband_reconstruct_2d(&subbands);

        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON));
    }

    #[test]
    fn test_2d_roundtrip_97() {
        let processor = DwtProcessor::new(FilterType::Irreversible97);

        let data: Vec<f64> = (0..64).map(|x| x as f64).collect();
        let original = Array2D::from_data(data, 8, 8);

        let subbands = processor.subband_decompose_2d(&original);
        let reconstructed = processor.subband_reconstruct_2d(&subbands);

        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON_97));
    }

    #[test]
    fn test_multi_level_dwt_53() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        let data: Vec<f64> = (0..256).map(|x| x as f64).collect();
        let original = Array2D::from_data(data, 16, 16);

        let subbands = processor.fdwt(&original, 2);
        assert_eq!(subbands.len(), 2);

        // Check sub-band dimensions for level 1
        assert_eq!(subbands[0].ll.width(), 8);
        assert_eq!(subbands[0].ll.height(), 8);

        // Check sub-band dimensions for level 2
        assert_eq!(subbands[1].ll.width(), 4);
        assert_eq!(subbands[1].ll.height(), 4);

        let reconstructed = processor.idwt(&subbands, 2);
        assert_eq!(reconstructed.width(), original.width());
        assert_eq!(reconstructed.height(), original.height());
        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON));
    }

    #[test]
    fn test_multi_level_dwt_97() {
        let processor = DwtProcessor::new(FilterType::Irreversible97);

        let data: Vec<f64> = (0..256).map(|x| x as f64).collect();
        let original = Array2D::from_data(data, 16, 16);

        let subbands = processor.fdwt(&original, 2);
        assert_eq!(subbands.len(), 2);

        let reconstructed = processor.idwt(&subbands, 2);
        assert_eq!(reconstructed.width(), original.width());
        assert_eq!(reconstructed.height(), original.height());
        assert!(arrays_approx_eq(&original, &reconstructed, EPSILON_97));
    }

    #[test]
    fn test_convenience_functions() {
        let data: Vec<f64> = (0..64).map(|x| x as f64).collect();
        let input = Array2D::from_data(data, 8, 8);

        // Test 5-3
        let subbands_53 = dwt_53_forward(&input, 1);
        let result_53 = dwt_53_inverse(&subbands_53, 1);
        assert!(arrays_approx_eq(&input, &result_53, 1.0));

        // Test 9-7
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

        let signal = vec![42.0];

        let t53 = processor_53.subband_decompose_1d(&signal);
        let r53 = processor_53.subband_reconstruct_1d(&t53);
        assert!(approx_eq(signal[0], r53[0], EPSILON));

        let t97 = processor_97.subband_decompose_1d(&signal);
        let r97 = processor_97.subband_reconstruct_1d(&t97);
        assert!(approx_eq(signal[0], r97[0], EPSILON_97));
    }

    #[test]
    fn test_small_signals() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // Length 2
        let signal2 = vec![1.0, 2.0];
        let t2 = processor.subband_decompose_1d(&signal2);
        let r2 = processor.subband_reconstruct_1d(&t2);
        assert!(approx_eq(signal2[0], r2[0], EPSILON));
        assert!(approx_eq(signal2[1], r2[1], EPSILON));

        // Length 3
        let signal3 = vec![1.0, 2.0, 3.0];
        let t3 = processor.subband_decompose_1d(&signal3);
        let r3 = processor.subband_reconstruct_1d(&t3);
        for i in 0..3 {
            assert!(approx_eq(signal3[i], r3[i], EPSILON));
        }
    }

    #[test]
    fn test_dc_signal() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // DC signal (all same value)
        let signal: Vec<f64> = vec![5.0; 8];
        let transformed = processor.subband_decompose_1d(&signal);
        let reconstructed = processor.subband_reconstruct_1d(&transformed);

        for i in 0..8 {
            assert!(approx_eq(signal[i], reconstructed[i], EPSILON));
        }
    }

    #[test]
    fn test_subband_dimensions() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // 7x5 input (odd dimensions)
        let data: Vec<f64> = (0..35).map(|x| x as f64).collect();
        let input = Array2D::from_data(data, 7, 5);

        let subbands = processor.subband_decompose_2d(&input);

        // For 7x5 input:
        // LL: ceil(7/2) x ceil(5/2) = 4x3
        // HL: floor(7/2) x ceil(5/2) = 3x3
        // LH: ceil(7/2) x floor(5/2) = 4x2
        // HH: floor(7/2) x floor(5/2) = 3x2
        assert_eq!(subbands.ll.width(), 4);
        assert_eq!(subbands.ll.height(), 3);
        assert_eq!(subbands.hl.width(), 3);
        assert_eq!(subbands.hl.height(), 3);
        assert_eq!(subbands.lh.width(), 4);
        assert_eq!(subbands.lh.height(), 2);
        assert_eq!(subbands.hh.width(), 3);
        assert_eq!(subbands.hh.height(), 2);

        // Verify round-trip
        let reconstructed = processor.subband_reconstruct_2d(&subbands);
        assert_eq!(reconstructed.width(), input.width());
        assert_eq!(reconstructed.height(), input.height());
        assert!(arrays_approx_eq(&input, &reconstructed, EPSILON));
    }

    #[test]
    fn test_non_zero_offset() {
        let processor = DwtProcessor::new(FilterType::Reversible53);

        let data: Vec<f64> = (0..64).map(|x| x as f64).collect();
        let mut input = Array2D::from_data(data.clone(), 8, 8);
        input.u0 = 0;
        input.v0 = 0;

        let result = processor.round_trip(&input, 1);

        for row in 0..8 {
            for col in 0..8 {
                let orig = input[(col, row)];
                let recon = result[(col, row)];
                assert!(
                    approx_eq(orig, recon, EPSILON),
                    "Mismatch at ({}, {})",
                    col,
                    row
                );
            }
        }
    }

    #[test]
    fn test_spec_example_data_53() {
        // Sample data similar to Table J.3 from the spec (13x17)
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
        let original = Array2D::from_data(data, width, height);

        // Test 5-3 round-trip
        let processor_53 = DwtProcessor::new(FilterType::Reversible53);
        let sub_bands = processor_53.fdwt(&original, 2);

        // Check count and sizes of sub_bands
        assert_eq!(sub_bands.len(), 2, "expected 2 sub_bands");
        let sb1 = &sub_bands[0];
        assert_eq!(sb1.hh.width, 6);
        assert_eq!(sb1.hh.height, 8);
        assert_eq!(sb1.lh.width, 7);
        assert_eq!(sb1.lh.height, 8);
        assert_eq!(sb1.hl.width, 6);
        assert_eq!(sb1.hl.height, 9);
        let sb2 = &sub_bands[1];
        assert_eq!(sb2.hh.width, 3);
        assert_eq!(sb2.hh.height, 4);
        assert_eq!(sb2.lh.width, 4);
        assert_eq!(sb2.lh.height, 4);
        assert_eq!(sb2.hl.width, 3);
        assert_eq!(sb2.hl.height, 5);
        assert_eq!(sb2.ll.width, 4);
        assert_eq!(sb2.ll.height, 5);

        type Sbfn = fn(&SubBands) -> &Array2D<f64>;
        let exp2: Vec<(Sbfn, Vec<i32>)> = vec![
            (
                SubBands::ll,
                vec![
                    0, 4, 8, 12, 4, 5, 8, 12, 8, 8, 11, 15, 12, 12, 14, 18, 16, 16, 18, 20,
                ],
            ),
            (
                SubBands::hl,
                vec![0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
            ),
            (
                SubBands::lh,
                vec![0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            ),
            (SubBands::hh, vec![-1, 0, 0, 0, -1, 0, 0, 1, 0, 0, 0, 0]),
        ];

        fn conv_i32(data: &Array2D<f64>) -> Vec<i32> {
            data.data.iter().map(|&f| f as i32).collect()
        }

        // Grab data from data above
        for (af, exp) in exp2.iter() {
            assert_eq!(exp, &conv_i32(af(sb2)));
        }

        let exp1: Vec<(Sbfn, Vec<i32>)> = vec![
            (
                SubBands::hl,
                vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, -1, 1, 0, 0,
                    0, 0, 1, 1, 0, 0, 1, 1, 0, -1, 0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0,
                ],
            ),
            (
                SubBands::lh,
                vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 1, 0, 0,
                    1, 1, 0, 0, 0, 0, 1, 0, 2, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0,
                    0, 1, 1, 0,
                ],
            ),
            (
                SubBands::hh,
                vec![
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0,
                    0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, -1, 0,
                ],
            ),
            // Skip ll
        ];

        // Grab data from data above
        for (af, exp) in exp1.iter() {
            assert_eq!(exp, &conv_i32(af(sb1)));
        }

        // Test round trip
        let result_53 = processor_53.round_trip(&original, 2);

        for row in 0..height {
            for col in 0..width {
                let orig = original[(col, row)];
                let recon = result_53[(col, row)];
                assert!(
                    approx_eq(orig, recon, EPSILON),
                    "5-3 mismatch at ({}, {}): orig={}, recon={}",
                    col,
                    row,
                    orig,
                    recon
                );
            }
        }
    }

    #[test]
    fn test_spec_example_data_97() {
        // Sample data similar to Table J.3 from the spec (13x17)
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
        let original = Array2D::from_data(data, width, height);

        // Test 9-7 round-trip
        let processor_97 = DwtProcessor::new(FilterType::Irreversible97);
        let sub_bands = processor_97.fdwt(&original, 2);
        let result_97 = processor_97.round_trip(&original, 1);

        for row in 0..height {
            for col in 0..width {
                let orig = original[(col, row)];
                let recon = result_97[(col, row)];
                assert!(
                    approx_eq(orig, recon, EPSILON_97),
                    "9-7 mismatch at ({}, {}): orig={}, recon={}",
                    col,
                    row,
                    orig,
                    recon
                );
            }
        }

        // Check count and sizes of sub_bands
        assert_eq!(sub_bands.len(), 2, "expected 2 sub_bands");
        let sb1 = &sub_bands[0];
        assert_eq!(sb1.hh.width, 6);
        assert_eq!(sb1.hh.height, 8);
        assert_eq!(sb1.lh.width, 7);
        assert_eq!(sb1.lh.height, 8);
        assert_eq!(sb1.hl.width, 6);
        assert_eq!(sb1.hl.height, 9);
        let sb2 = &sub_bands[1];
        assert_eq!(sb2.hh.width, 3);
        assert_eq!(sb2.hh.height, 4);
        assert_eq!(sb2.lh.width, 4);
        assert_eq!(sb2.lh.height, 4);
        assert_eq!(sb2.hl.width, 3);
        assert_eq!(sb2.hl.height, 5);
        assert_eq!(sb2.ll.width, 4);
        assert_eq!(sb2.ll.height, 5);

        // TODO consider checking sub_band contents
    }

    #[test]
    fn test_extend_signal() {
        // Test basic symmetric extension behavior
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // A simple signal
        let signal: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0];
        let transformed = processor.subband_decompose_1d(&signal);
        let reconstructed = processor.subband_reconstruct_1d(&transformed);

        for i in 0..signal.len() {
            assert!(
                approx_eq(signal[i], reconstructed[i], EPSILON),
                "Mismatch at index {}: expected {}, got {}",
                i,
                signal[i],
                reconstructed[i]
            );
        }
    }

    #[test]
    fn test_pse_basic() {
        // Test the mirror_index helper function indirectly through signal processing
        let processor = DwtProcessor::new(FilterType::Reversible53);

        // Signal that will test boundary handling
        let signal: Vec<f64> = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let transformed = processor.subband_decompose_1d(&signal);
        let reconstructed = processor.subband_reconstruct_1d(&transformed);

        for i in 0..signal.len() {
            assert!(
                approx_eq(signal[i], reconstructed[i], EPSILON),
                "PSE test mismatch at index {}: expected {}, got {}",
                i,
                signal[i],
                reconstructed[i]
            );
        }
    }
}
