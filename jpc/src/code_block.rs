use log::info;

//mod coder;

// Subband enum, TODO move somewhere sane
#[derive(Debug)]
enum SubBand {
    LL,
    HL,
    LH,
    HH,
}

#[derive(Debug, Default)]
enum Coeff {
    Significant,
    #[default]
    Insignificant,
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
    coefficients: Vec<Coeff>,
}

type CODEBLOCKDIM = i32; // TODO what is actual codeblock sizing?
impl CodeBlockDecoder {
    fn new(width: CODEBLOCKDIM, height: CODEBLOCKDIM, subband: SubBand) -> Self {
        Self {
            width,
            height,
            subband,
            coefficients: vec![],
        }
    }
    /// Decode coefficients from the given compressed data.
    fn decode(&mut self, _bytes: &[u8]) -> Result<(), CodeBlockDecodeError> {
        info!("need to decode codeblcok...");

        let coder = None;


        // Start in CleanUpPass -> Significance -> Refinement
        let state = State::CleanUpPass;
        let next_state: State = match state {
            State::CleanUpPass => {
                self.pass_cleanup(coder);
                State::Refinement
            },
            State::Significance => {
                self.pass_significance(coder);
                State::Refinement
            }
            State::Refinement => {
                self.pass_refinement(coder);
                State::CleanUpPass
            },
        }

        Ok(())
    }
    /// Return coefficients
    /// TODO return type is whak
    /// Note, return a copy, maybe need to decode more for this codeblock later and don't want to
    /// lose state
    fn coefficients(&self) -> Vec<i32> {
        self.coefficients.iter().map(|_c| 1i32).collect()
    }
}

// Decoder State
enum State {
    CleanUpPass,
    Refinement,
    Significance,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cb_decode_j10a() {
        // Test decoding the codeblock from J.10 for LL
        let bd = b"\x01\x8F\x0D\xC8\x75\x5D";

        // There are 16 coding passes in this example
        let mut codeblock = CodeBlockDecoder::new(1, 5, SubBand::LL);

        assert!(codeblock.decode(bd).is_ok(), "Expected decode to work");

        let coeffs = codeblock.coefficients();

        let exp_coeffs = vec![1, 5, 1, 0];
        assert_eq!(coeffs, exp_coeffs, "Coefficients didn't match");
    }

    #[test]
    fn test_cb_decode_j10b() {
        // Test decoding the codeblock from J.10 for HL
        let bd = b"\x0F\xB1\x76";

        let codeblock = CodeBlockDecoder::new(1, 4, SubBand::HL);

        todo!("work on other first");

        assert!(codeblock.decode(bd).is_ok(), "Expected decode to work");

        let coeffs = codeblock.coefficients();

        let exp_coeffs = vec![1, 5, 1, 0];
        assert_eq!(coeffs, exp_coeffs, "Coefficients didn't match");
    }
}
