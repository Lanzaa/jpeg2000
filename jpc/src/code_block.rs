use std::option::{Iter, IterMut};

use log::{debug, info};

use crate::coder::{Decoder, RUN_LEN, UNIFORM};

// Subband enum, TODO move somewhere sane
#[derive(Debug)]
enum SubBand {
    LL,
    HL,
    LH,
    HH,
}

#[derive(Debug)]
enum Coeff {
    // TODO i16 is probably wrong, might need generic
    Significant { value: i16, is_negative: bool },
    Insignificant(u8), // Insignificant at what bit-plane shift
}

struct CodeBlockDecodeError {}

/// decoder for codeblocks
///
/// A CodeBlockDecoder produces coefficients from compressed data.
///
struct CodeBlockDecoder {
    width: CODEBLOCKDIM,
    height: CODEBLOCKDIM,
    subband: SubBand,
    no_passes: u8, // Max 164 from table B.4
    bit_plane_shift: u8,
    coefficients: Vec<Coeff>,
}

/// Wrapper around an x, y coord
#[derive(Debug, Clone, Copy)]
struct CoeffIndex {
    y: i32,
    x: i32,
}

type CODEBLOCKDIM = i32; // TODO what is actual codeblock sizing?
impl CodeBlockDecoder {
    fn new(
        width: CODEBLOCKDIM,
        height: CODEBLOCKDIM,
        subband: SubBand,
        no_passes: u8,
        mb: u8,
    ) -> Self {
        let no_coeff: usize = (width * height) as usize;
        let mut coeffs_vec = Vec::with_capacity(no_coeff);
        coeffs_vec.resize_with(no_coeff, || Coeff::Insignificant(u8::MAX));
        Self {
            width,
            height,
            subband,
            no_passes,
            bit_plane_shift: mb - 1,
            coefficients: coeffs_vec,
        }
    }

    /// Decode coefficients from the given compressed data.
    fn decode(&mut self, coder: &mut dyn Decoder) -> Result<(), CodeBlockDecodeError> {
        info!("need to decode codeblcok...");

        // Start in CleanUp -> SignificancePropagation -> MagnitudeRefinement -> repeat ...
        // Each pass has two coding parts
        let mut state = State::CleanUp;
        let no_passes = 7; // TODO
        for _pass_number in 0..no_passes {
            info!("Beginning a pass {:?}", state);
            let next_state: State = match state {
                State::CleanUp => {
                    self.pass_cleanup(coder);
                    State::SignificancePropagation
                }
                State::SignificancePropagation => {
                    self.bit_plane_shift -= 1;
                    self.pass_significance(coder);
                    State::MagnitudeRefinement
                }
                State::MagnitudeRefinement => {
                    self.pass_refinement(coder);
                    State::CleanUp
                }
            };
            state = next_state;
            debug!("coeffs: {:?}", self.coefficients);
        }

        Ok(())
    }
    /// Return coefficients
    /// TODO return type is whak
    /// Note, return a copy, maybe need to decode more for this codeblock later and don't want to
    /// lose state
    fn coefficients(&self) -> Vec<i32> {
        self.coefficients
            .iter()
            .map(|c| match c {
                Coeff::Significant { value, is_negative } => {
                    if *is_negative {
                        -1 * value
                    } else {
                        *value
                    }
                }
                Coeff::Insignificant(_) => 0,
            } as i32)
            .collect()
    }

    /// Handle a cleanup pass
    ///
    /// CleanUp does cleanup and sign coding
    fn pass_cleanup(&mut self, coder: &mut dyn Decoder) {
        // Iterate coefficients in strips 4 tall across full width
        for by in (0..self.height).step_by(4) {
            for x in 0..self.width {
                let mut offset_y: i32 = 0;

                // Count insignificants in this column strip
                let mut count_insig = 0;
                for y in by..(by + 4).min(self.height) {
                    count_insig += (!self.is_significant(CoeffIndex { y, x })) as i32;
                }

                let d8 = 4 == count_insig;
                if d8 {
                    // All Insignificant, determine first significant
                    let c4 = coder.decode_bit(RUN_LEN);
                    debug!("D8=true, C4 {}", c4);
                    // c4 -> d11
                    if c4 == 1 {
                        // skip all, go to next column of 4
                        debug!("Skipping column of 4");
                        continue;
                    } else {
                        // Decode how many coeffs to skip
                        // two uniform context decodes
                        let a = coder.decode_bit(UNIFORM);
                        let b = coder.decode_bit(UNIFORM);
                        let c5 = 2 * a + b;
                        assert!(c5 < 4, "Improper decode from mq coder");

                        // go forward s
                        offset_y += c5 as i32;
                        debug!("Skip {} coeffs", c5);
                    }
                    let nsi = CoeffIndex {
                        x,
                        y: by + offset_y,
                    };
                    self.make_significant(nsi);

                    // C2 decode sign bit
                    self.decode_sign_bit(nsi, coder);
                    offset_y += 1;
                }

                // remaining coefficients in this column strip
                for y in (by + offset_y)..(by + 4).min(self.height) {
                    let idx = CoeffIndex { x, y };
                    debug!("Wakka {:?} -> {:?}", idx, self.coeff_at(idx));
                    let newly_sig =
                        !self.is_significant(idx) && self.significance_decode(idx, coder);
                    if newly_sig {
                        // C2 decode sign bit
                        self.decode_sign_bit(idx, coder);
                    }
                }
            }
        }
        info!("completed cleanup pass");
    }
    /// Handle a significance propagation pass
    fn pass_significance(&mut self, coder: &mut dyn Decoder) {
        // Iterate coefficients in strips 4 tall across full width
        for by in (0..self.height).step_by(4) {
            for x in 0..self.width {
                for y in by..(by + 4).min(self.height) {
                    let idx = CoeffIndex { y, x };
                    if self.is_significant(idx) {
                        continue; // D1 yes
                    }
                    let sig_ctx = self.significance_context(idx);
                    if 0 == sig_ctx {
                        continue; // D2 yes
                    }
                    let newly_sig = self.significance_decode_ctx(sig_ctx, idx, coder);
                    if newly_sig {
                        // C2
                        self.decode_sign_bit(idx, coder);
                    } else {
                        *self.coeff_at_mut(idx) = Coeff::Insignificant(self.bit_plane_shift);
                    }
                }
            }
        }
        debug!("completed significance pass");
    }
    /// Handle a magnitude refinement pass
    fn pass_refinement(&mut self, coder: &mut dyn Decoder) {
        // Iterate coefficients in strips 4 tall across full width
        for by in (0..self.height).step_by(4) {
            for x in 0..self.width {
                for y in by..(by + 4).min(self.height) {
                    let idx = CoeffIndex { y, x };
                    if !self.is_significant(idx) {
                        continue; // D5 yes
                    }
                    // is bit set for this bit-plane
                    let is_bit_set = self.is_bit_plane_set(idx);
                    info!("Is bit set: {}, for {:?}", is_bit_set, idx);
                    if is_bit_set {
                        continue; // D6 yes
                    }
                    // C3
                    self.magnitude_decode(idx, coder);
                }
            }
        }
        debug!("completed refinement pass");
    }

    fn coeff_at(&self, idx: CoeffIndex) -> &Coeff {
        let CoeffIndex { x, y } = idx;
        let out_bounds = x < 0 || x >= self.width || y < 0 || y >= self.height;
        match out_bounds {
            true => {
                debug!("Out of bounds coeff_at {}, {}", x, y);
                &Coeff::Insignificant(u8::MAX)
            }
            false => &self.coefficients[(self.width * idx.y + idx.x) as usize],
        }
    }
    fn coeff_at_mut(&mut self, idx: CoeffIndex) -> &mut Coeff {
        &mut self.coefficients[(self.width * idx.y + idx.x) as usize]
    }

    fn significance_context(&self, idx: CoeffIndex) -> usize {
        // Shorter names
        let x = idx.x;
        let y = idx.y;
        let width = self.width;
        let height = self.height;

        // mutables
        let mut h = 0; // horizontal contributions
        let mut v = 0; // vertical contributions
        let mut d = 0; // diagonal contributions

        // Count significant neighbors
        // TODO get rid of bounds checks
        if x > 0 && self.is_significant(CoeffIndex { y, x: x - 1 }) {
            h += 1;
        }
        if x < width - 1 && self.is_significant(CoeffIndex { y, x: x + 1 }) {
            h += 1;
        }
        if y > 0 && self.is_significant(CoeffIndex { y: y - 1, x }) {
            v += 1;
        }
        if y < height - 1 && self.is_significant(CoeffIndex { y: y + 1, x }) {
            v += 1;
        }

        // Diagonals (only if both adjacent orthogonal are insignificant)
        if x > 0 && y > 0 && self.is_significant(CoeffIndex { y: y - 1, x: x - 1 }) {
            d += 1;
        }
        if x < width - 1 && y > 0 && self.is_significant(CoeffIndex { y: y - 1, x: x + 1 }) {
            d += 1;
        }
        if x > 0 && y < height - 1 && self.is_significant(CoeffIndex { y: y + 1, x: x - 1 }) {
            d += 1;
        }
        if x < width - 1 && y < height - 1 && self.is_significant(CoeffIndex { y: y + 1, x: x + 1 })
        {
            d += 1;
        }

        debug!(
            "For subband {:?}, idx: {:?}, found h={}, v={}, d={}",
            self.subband, idx, h, v, d
        );

        // Compute context based on subband and neighbor counts
        // Different formulas for HL, LH, HH subbands
        match self.subband {
            SubBand::LL | SubBand::LH => match (h, v, d) {
                (0, 0, 0) => 0,
                (0, 0, 1) => 1,
                (0, 0, _) => 2,
                (0, 1, _) => 3,
                (0, 2, _) => 4,
                (1, 0, 0) => 5,
                (1, 0, _) => 6,
                (1, _, _) => 7,
                (2, _, _) => 8,
                (_, _, _) => panic!("Unknown significance context calculation"),
            },
            SubBand::HL => match (h, v, d) {
                (0, 0, 0) => 0,
                (0, 0, 1) => 1,
                (0, 0, _) => 2,
                (1, 0, _) => 3,
                (2, 0, _) => 4,
                (0, 1, 0) => 5,
                (0, 1, _) => 6,
                (_, 1, _) => 7,
                (_, 2, _) => 8,
                (_, _, _) => panic!("Unknown significance context calculation"),
            },
            SubBand::HH => todo!("HH significance context loookup"),
        }
    }

    /// Checks if the bit in this bit-plane was set
    fn is_bit_plane_set(&self, idx: CoeffIndex) -> bool {
        debug!("value for {:?}, {:?}", idx, self.coeff_at(idx));
        match self.coeff_at(idx) {
            Coeff::Insignificant(_) => {
                panic!("Attemping to check bit-plane of Insignificant coefficient")
            }
            Coeff::Significant { value, .. } => 1 == (0x1 & (value >> self.bit_plane_shift)),
        }
    }

    fn is_significant(&self, idx: CoeffIndex) -> bool {
        let CoeffIndex { x, y } = idx;
        let out_bounds = x < 0 || x >= self.width || y < 0 || y >= self.height;
        if out_bounds {
            return false;
        }
        match self.coeff_at(idx) {
            Coeff::Insignificant(_) => false,
            Coeff::Significant { .. } => true,
        }
    }

    fn make_significant(&mut self, idx: CoeffIndex) {
        debug!("Marking significant {:?}", idx);
        match self.coeff_at(idx) {
            Coeff::Insignificant(_) => {
                *self.coeff_at_mut(idx) = Coeff::Significant {
                    value: 1 << self.bit_plane_shift,
                    is_negative: false,
                };
            }
            _ => panic!("tried to make a coefficient doubly significant"),
        }
    }

    /// Decode the significance for a specific CoeffIndex from the decoder
    fn significance_decode(&mut self, idx: CoeffIndex, decoder: &mut dyn Decoder) -> bool {
        // TODO pull context from around idx
        match self.coeff_at(idx) {
            Coeff::Insignificant(bs) => {
                // significance already coded as false
                if *bs == self.bit_plane_shift {
                    return false;
                }
            }
            _ => panic!("Should have checked if sig"),
        }
        let cx = self.significance_context(idx);
        self.significance_decode_ctx(cx, idx, decoder)
    }
    fn significance_decode_ctx(
        &mut self,
        cx: usize,
        idx: CoeffIndex,
        decoder: &mut dyn Decoder,
    ) -> bool {
        let sig = decoder.decode_bit(cx);
        debug!("Sigbit {} for {:?}", sig, idx);
        if sig == 1 {
            self.make_significant(idx);
            true
        } else {
            false
        }
    }

    /// Decode the magnitude bit for a specific CoeffIndex from the decoder
    fn magnitude_decode(&mut self, idx: CoeffIndex, decoder: &mut dyn Decoder) {
        // TODO pull context from around idx
        let cx = self.magnitude_context(idx);
        let b = decoder.decode_bit(cx);
        info!("Coef b {:?}", self.coeff_at(idx));
        *self.coeff_at_mut(idx) = match self.coeff_at(idx) {
            Coeff::Insignificant(_) => {
                panic!("Cannot set magnitude bit for an Insignificant coefficient")
            }
            Coeff::Significant { value, is_negative } => {
                let value = value | (b << self.bit_plane_shift) as i16;
                let is_negative = *is_negative;
                Coeff::Significant { value, is_negative }
            }
        };
        info!("Coef after {:?}", self.coeff_at(idx));
        debug!("Set bit {} for {:?}", b, idx);
    }

    /// Decode the sign bit for a specific CoeffIndex from the decoder
    fn decode_sign_bit(&mut self, idx: CoeffIndex, decoder: &mut dyn Decoder) {
        // TODO pull context from around idx
        let (cx, xor) = self.sign_context(idx);
        // TODO
        debug!("Decodign sign bit with ctx {} and xor {}", cx, xor);
        let sign_bit = decoder.decode_bit(cx);
        debug!("sign {} for {:?}", sign_bit, idx);
        if let Coeff::Significant { value, .. } = self.coeff_at(idx) {
            *self.coeff_at_mut(idx) = Coeff::Significant {
                value: *value,
                is_negative: (sign_bit ^ xor) != 0,
            };
        } else {
            panic!("Cannot set sign bit on coeff");
        }
    }

    fn num_zero_bit_plane(&mut self, arg: u8) {
        self.bit_plane_shift -= arg;
    }

    /// Determine the context for sign bit decoding
    fn sign_context(&self, idx: CoeffIndex) -> (usize, u8) {
        let CoeffIndex { x, y } = idx;

        let v0 = self.coeff_at(CoeffIndex { y: y - 1, x });
        let v1 = self.coeff_at(CoeffIndex { y: y + 1, x });
        let h0 = self.coeff_at(CoeffIndex { y, x: x - 1 });
        let h1 = self.coeff_at(CoeffIndex { y, x: x + 1 });

        debug!("v0 {:?} v1 {:?} h0 {:?} h1 {:?}", v0, v1, h0, h1);

        fn sp(c: &Coeff) -> i8 {
            match c {
                Coeff::Insignificant(_) => 0,
                Coeff::Significant { is_negative, .. } => 1 - 2 * (*is_negative as i8),
            }
        }
        fn c(a: &Coeff, b: &Coeff) -> i8 {
            let t = sp(a) + sp(b);
            match t {
                _ if t > 0 => 1,
                _ if t < 0 => -1,
                _ => 0,
            }
        }
        debug!("sign context vert {}, {}", sp(v0), sp(v1));
        debug!("sign context horz {}, {}", sp(h0), sp(h1));

        let vc = c(v0, v1);
        let hc = c(h0, h1);
        let (ctx, xor) = match (hc, vc) {
            (1, 1) => (13, 0),
            (1, 0) => (12, 0),
            (1, -1) => (11, 0),
            (0, 1) => (10, 0),
            (0, 0) => (9, 0),
            (0, -1) => (10, 1),
            (-1, 1) => (11, 1),
            (-1, 0) => (12, 1),
            (-1, -1) => (13, 1),
            (_, _) => panic!("Invalid context values for sign_context"),
        };
        (ctx, xor)
    }

    fn magnitude_context(&self, idx: CoeffIndex) -> usize {
        if let Coeff::Significant { value, .. } = self.coeff_at(idx) {
            let c = value.count_ones();
            let sv = value >> (1 + self.bit_plane_shift);
            if sv != 1 {
                debug!("First refinement for idx {:?} w/ {}, c {}", idx, value, c);
                return 16;
            }
        }
        let CoeffIndex { x, y } = idx;
        let h0 = self.is_significant(CoeffIndex { y, x: x - 1 }) as u8;
        let h1 = self.is_significant(CoeffIndex { y, x: x + 1 }) as u8;
        let v0 = self.is_significant(CoeffIndex { y: y - 1, x }) as u8;
        let v1 = self.is_significant(CoeffIndex { y: y + 1, x }) as u8;

        let c = v0 + v1 + h0 + h1;
        if c > 0 {
            // early return if we know w/o diagonals
            return 15;
        }

        let mut dc = 0u8;
        // Diagonals (only if both adjacent orthogonal are insignificant)
        dc += self.is_significant(CoeffIndex { y: y - 1, x: x - 1 }) as u8;
        dc += self.is_significant(CoeffIndex { y: y - 1, x: x + 1 }) as u8;
        dc += self.is_significant(CoeffIndex { y: y + 1, x: x - 1 }) as u8;
        dc += self.is_significant(CoeffIndex { y: y + 1, x: x + 1 }) as u8;
        if dc + c > 0 {
            15
        } else {
            14
        }
    }
}

/// ColumnIndex type to help avoid indexing mistakes
#[derive(Debug)]
struct ColumnIndex {
    pub base_y: i32,
    pub x: i32,
}

// Decoder State
#[derive(Debug, Default)]
enum State {
    SignificancePropagation,
    #[default]
    CleanUp,
    MagnitudeRefinement,
}

#[cfg(test)]
mod tests {
    use crate::coder::Decoder;

    use super::*;

    pub fn init_logger() {
        let _ = env_logger::builder()
            .is_test(true)
            .filter_level(log::LevelFilter::Debug)
            .try_init();
    }

    struct MockCoder {
        exp: Vec<(usize, u8)>,
        index: usize,
    }

    impl Decoder for MockCoder {
        fn decode_bit(&mut self, cx: usize) -> u8 {
            let (exp_cx, out) = self.exp[self.index];
            self.index += 1;
            assert_eq!(exp_cx, cx, "incorrect cx during decode");
            out
        }
    }

    /// Test decoding the codeblock from J.10 for LL using a mock mqcoder
    #[test]
    #[ignore]
    fn test_cb_decode_j10a_mocked() {
        init_logger();

        // Mock decoder that checks input contexts
        let mut coder = MockCoder {
            exp: vec![
                (17, 0),
                (18, 1),
                (18, 1),
                (9, 1),
                (3, 0),
                (3, 1),
                (10, 0),
                (3, 1),
                (10, 0),
                (15, 0),
                (0, 1),
                (9, 1),
                (4, 1),
                (10, 0),
                // Refinement phase
                (15, 1),
                (15, 0),
                (15, 1),
                (16, 0),
                (15, 0),
                // next bit-plane
                (16, 0),
                (16, 1),
                (16, 1),
                (16, 0),
                (16, 0),
                // next bit-plane
                (16, 1),
                (16, 1),
                (16, 1),
                (16, 0),
                (16, 1),
                // last bit-plane
                (16, 0),
                (16, 0),
                (16, 0),
                (16, 0),
                (16, 1),
            ],
            index: 0,
        };
        // There are 16 coding passes in this example
        let mut codeblock = CodeBlockDecoder::new(1, 5, SubBand::LL, 16, 9);
        // codeblock.mb(9);
        codeblock.num_zero_bit_plane(3);
        // 9 - 3 = 6 bits to set
        // 6-1 = 5 => 1+5*3 = 16 coding passes

        assert!(
            codeblock.decode(&mut coder).is_ok(),
            "Expected decode to work"
        );
        assert_eq!(
            coder.exp.len(),
            coder.index,
            "Expected all mock data to be used"
        );

        let coeffs = codeblock.coefficients();
        let exp_coeffs = vec![-26, -22, -30, -32, -19];
        assert_eq!(coeffs, exp_coeffs, "Coefficients didn't match");
    }

    /// Test decoding the codeblock from J.10 for LH using a mock mqcoder
    #[test]
    fn test_cb_decode_j10b_mocked() {
        init_logger();

        // Mock decoder that checks input contexts
        let mut coder = MockCoder {
            exp: vec![
                (17, 0),
                (18, 0),
                (18, 1),
                (9, 0),
                (3, 0),
                (0, 0),
                (3, 0),
                (3, 0),
                (14, 0),
                (0, 0),
                (3, 1),
                (10, 0),
                (3, 1),
                (10, 0),
                (3, 0),
                (16, 1),
            ],
            index: 0,
        };
        // There are 7 coding passes in this example
        let mut codeblock = CodeBlockDecoder::new(1, 4, SubBand::LH, 7, 10);
        // codeblock.mb(10);
        codeblock.num_zero_bit_plane(7);
        // 10 - 7 = 3 bits to set
        // 3 bits to set => 7 (=1cleanup+2bitplanes*3) coding passes

        assert!(
            codeblock.decode(&mut coder).is_ok(),
            "Expected decode to work"
        );
        assert_eq!(
            coder.exp.len(),
            coder.index,
            "Expected all mock data to be used"
        );

        let coeffs = codeblock.coefficients();
        let exp_coeffs = vec![1, 5, 1, 0];
        assert_eq!(coeffs, exp_coeffs, "Coefficients didn't match");
    }

    //#[test]
    //fn test_cb_decode_j10a() {
    //    init_logger();
    //    todo!("...");
    //    // Test decoding the codeblock from J.10 for LL
    //    let bd = b"\x01\x8F\x0D\xC8\x75\x5D";

    //    // There are 16 coding passes in this example
    //    let mut codeblock = CodeBlockDecoder::new(1, 5, SubBand::LL);

    //    assert!(codeblock.decode(bd).is_ok(), "Expected decode to work");

    //    let coeffs = codeblock.coefficients();

    //    let exp_coeffs = vec![1, 5, 1, 0];
    //    assert_eq!(coeffs, exp_coeffs, "Coefficients didn't match");
    //}

    //#[test]
    //fn test_cb_decode_j10b() {
    //    init_logger();
    //    todo!("...");
    //    // Test decoding the codeblock from J.10 for HL
    //    let bd = b"\x0F\xB1\x76";

    //    let codeblock = CodeBlockDecoder::new(1, 4, SubBand::HL);

    //    todo!("work on other first");

    //    assert!(codeblock.decode(bd).is_ok(), "Expected decode to work");

    //    let coeffs = codeblock.coefficients();

    //    let exp_coeffs = vec![1, 5, 1, 0];
    //    assert_eq!(coeffs, exp_coeffs, "Coefficients didn't match");
    //}
}
