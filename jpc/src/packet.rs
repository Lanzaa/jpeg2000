//! Packets
//!
//! ITU B.9
//!
//! All compressed image data representing a specific tile, layer, component, resolution level and
//! precinct appears in the codestream in a contiguous segment called a packet.
//!
//! This means decoding image data takes place packet by packet.
//!
//! To decode a packet requires the context of what the packet is associated to, the specific tile,
//! layer, component, resolution level, and precinct.
//!
//! Each packet decoder is for a specific tile, component, resolution level, and precinct. The
//! packet decoder consumes packets, layer by layer.
//!
//!

use core::{error, fmt};
use log::info;
use std::io::{self, Read};

use crate::code_block::CodeBlockDecodeError;
use crate::coder::standard_decoder;
use crate::shared::{Array2D, SubBandGroup, SubBandType, I2};
use crate::tag_tree::{InclusionTagTree, ZeroPlaneTagTree};
use crate::TileComponentResolutionBounds;
use crate::{bit_reader::BitReader, code_block::CodeBlockDecoder};

trait PacketDecoder {}

/// contains information from the relevant header
#[derive(Debug, Default)]
pub struct HeaderInfo {
    // TODO is there anything else from the header that is really interesting?
    pub length: usize, // TODO how big do packets get?
    pub packet_info: Vec<Vec<(I2, u8, u8)>>,
}

type PacketResult<T> = Result<T, PacketDecodeError>;

#[derive(Debug)]
pub enum PacketDecodeError {
    Stringy,
    IO(io::Error),
    CodeBlock(CodeBlockDecodeError),
}

impl From<io::Error> for PacketDecodeError {
    fn from(value: io::Error) -> Self {
        PacketDecodeError::IO(value)
    }
}

impl From<CodeBlockDecodeError> for PacketDecodeError {
    fn from(value: CodeBlockDecodeError) -> Self {
        PacketDecodeError::CodeBlock(value)
    }
}

impl error::Error for PacketDecodeError {}
impl fmt::Display for PacketDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // TODO better error messsages
        match self {
            PacketDecodeError::Stringy => todo!("Unknown"),
            PacketDecodeError::IO(error) => error.fmt(f),
            PacketDecodeError::CodeBlock(error) => error.fmt(f),
        }
        //format!("Error during packet decode: {}",)
        //todo!("Implement fmt for error")
    }
}

/// Holds the context for a packet decoder
#[derive(Debug)]
struct DecoderContext {
    sub_bands: Vec<SubBandContext>,
    layer: u8,
}

type CodeBlockIndex = u16; // TODO determine a good way to handle code block indexing
type CodeBlockRasterIndex = u16; // TODO determine a good way to handle code block indexing
#[derive(Debug)]
struct SubBandContext {
    inclusion_tree: InclusionTagTree,
    zeros_tree: ZeroPlaneTagTree,
    cbs: Vec<CodeBlockDecoder>,
}

impl SubBandContext {
    /// Determines the inclusion decision for the specific code block for the given layer by
    /// updating the tag tree with bits from the BitReader
    fn inclusion_decision<R: Read>(
        &mut self,
        dim_idx: I2,
        layer: u8,
        bit_reader: &mut BitReader<'_, R>,
    ) -> PacketResult<bool> {
        Ok(self
            .inclusion_tree
            .read_for_inclusion(dim_idx, layer as u32, bit_reader)?)
    }
    /// Determines the number of zero bit planes for a given code block based on tag tree possibly
    /// reading bits.
    ///
    /// The
    fn zero_planes<R: Read>(
        &mut self,
        dim_idx: I2,
        bit_reader: &mut BitReader<'_, R>,
    ) -> PacketResult<u8> {
        Ok(self.zeros_tree.read(dim_idx, bit_reader)?)
    }

    fn included(&self, dim_idx: I2, layer: u8) -> bool {
        self.inclusion_tree.query_inclusion(dim_idx, layer as u32)
    }
}

#[derive(Debug)]
pub struct PrecinctDecoder {
    ctx: DecoderContext,
    header: Option<HeaderInfo>,
    bounds: TileComponentResolutionBounds,
    is_ll: bool,
}

pub trait RR: io::Read {}

impl<R: io::Read> RR for R {}

pub struct D {
    pub width: u32,
    pub height: u32,
}

impl PrecinctDecoder {
    fn grab_coefficients(&self) -> Vec<Array2D<i32>> {
        // TODO
        let sbs = &self.ctx.sub_bands;
        let z = &sbs[0].cbs;
        let coeff = z[0].coefficients();
        println!("grab_coefficients: {:?}", coeff);

        vec![]
    }
}
impl PrecinctDecoder {
    pub fn new(
        cbx: u8,
        cby: u8,
        exponents: &[u8],
        bounds: TileComponentResolutionBounds,
        is_res_0: bool,
    ) -> PrecinctDecoder {
        let pcbx = 2u32.pow(cbx as u32);
        let pcby = 2u32.pow(cby as u32);
        let sbs = if is_res_0 {
            vec![bounds.sub_bands_ll()]
        } else {
            vec![
                bounds.sub_bands_hl(),
                bounds.sub_bands_lh(),
                bounds.sub_bands_hh(),
            ]
        };
        let mut sub_band_ctxs = Vec::new();
        for (sub_band_bounds, mb) in sbs.iter().zip(exponents) {
            let sb_type = sub_band_bounds.sub_band_type;
            let b = sub_band_bounds.bounds;

            let width = b.x1 - b.x0;
            let height = b.y1 - b.y0;

            let num_cb_wide = width.div_ceil(pcbx) as usize;
            let num_cb_tall = height.div_ceil(pcby) as usize;
            println!("widht, height: {},{}", width, height);
            println!("xcb, yxb: {},{}", pcbx, pcby);
            info!(
                "Createing packet decoder for sub_band {:?} with codeblocks wxh: {}x{}",
                sb_type, num_cb_wide, num_cb_tall
            );

            if num_cb_tall == 0 || num_cb_wide == 0 {
                continue; // skip this sub_band
            }
            let mut cbs = Vec::new();
            for _ in 0..num_cb_tall {
                for _ in 0..num_cb_tall {
                    assert!(num_cb_tall <= 1, "only handle one codeblock");
                    assert!(num_cb_wide <= 1, "only handle one codeblock");

                    let cb_bounds = sub_band_bounds.bounds; // TODO decompose
                    cbs.push(CodeBlockDecoder::new(
                        (cb_bounds.x1 - cb_bounds.x0) as i32,
                        (cb_bounds.y1 - cb_bounds.y0) as i32,
                        sb_type,
                        *mb,
                    ));
                }
            }
            let sb_ctx = SubBandContext {
                inclusion_tree: InclusionTagTree::new(num_cb_wide, num_cb_tall),
                zeros_tree: ZeroPlaneTagTree::new(num_cb_wide, num_cb_tall),
                cbs,
            };
            sub_band_ctxs.push(sb_ctx);
        }
        let sub_bands = sub_band_ctxs;

        PrecinctDecoder {
            ctx: DecoderContext {
                layer: 0,
                sub_bands,
            },
            header: None,
            bounds,
            is_ll: is_res_0,
        }
    }

    /// Grab sub band information for this precinct
    pub fn grab_precinct_subbands(&self) -> SubBandGroup<Array2D<i32>> {
        // TODO combine code blocks
        let mut ll = None;
        let mut hl = None;
        let mut lh = None;
        let mut hh = None;

        for sb in &self.ctx.sub_bands {
            if 1 != sb.cbs.len() {
                todo!("combining code blocks not implemented ");
            }
            for code_block in &sb.cbs {
                let sbt = code_block.sub_band();
                //let to_fill = &mut match sbt {
                //    SubBandType::LL => ll,
                //    SubBandType::HL => hl,
                //    SubBandType::LH => lh,
                //    SubBandType::HH => hh,
                //};
                //let coeffs = code_block.coefficients();
                //*to_fill = Some(coeffs);
                //match sbt {
                //    SubBandType::LL => ll = code_block.coefficients(),
                //    SubBandType::HL => hl = code_block.coefficients(),
                //    SubBandType::LH => lh = code_block.coefficients(),
                //    SubBandType::HH => hh = code_block.coefficients(),
                //}
                (match sbt {
                    SubBandType::LL => &mut ll,
                    SubBandType::HL => &mut hl,
                    SubBandType::LH => &mut lh,
                    SubBandType::HH => &mut hh,
                })
                .insert(code_block.coefficients());
            }
        }

        if let Some(ll) = ll {
            SubBandGroup::LL(ll)
        } else {
            SubBandGroup::Partial {
                hl: hl.unwrap_or_else(|| Array2D::new(0, 0)),
                lh: lh.unwrap_or_else(|| Array2D::new(0, 0)),
                hh: hh.unwrap_or_else(|| Array2D::new(0, 0)),
            }
        }
    }

    /// Consume a packet header pointed to by the reader
    pub fn consume_packet_header<R: RR>(self, reader: &mut R) -> PacketResult<PrecinctDecoder> {
        let Self {
            mut ctx,
            bounds,
            is_ll,
            ..
        } = self;

        // Packets are byte aligned, so we can parse at the byte boundary
        let mut bit_r = BitReader::new(reader)?;
        let zl_mark = bit_r.next_bit()?;
        if !zl_mark {
            println!("zero length");
            // Zero length packet
            //        todo!(
            //            "What information is returned with a zero length packet? {:x?}",
            //            b
            //        );
            return Ok(PrecinctDecoder {
                ctx,
                header: Some(HeaderInfo {
                    length: 0,
                    ..Default::default()
                }),
                bounds,
                is_ll,
            });
        }

        let mut total_to_read = 0;
        let mut packet_info = vec![];
        println!("begin sub band work");
        for sub_band_ctx in ctx.sub_bands.iter_mut() {
            let mut cb_info = vec![];
            info!("Working on sub_band: {:?}", sub_band_ctx);
            // Walk code-blocks in sub band / precinct
            let code_blocks = vec![0];

            'for_code_blocks: for cb in code_blocks {
                println!("Will cb {} be included?", cb);

                // TODO index
                let cb_idx = I2 { x: 0, y: 0 };

                let to_include: bool = if sub_band_ctx.included(cb_idx, ctx.layer) {
                    // If a code-block has been previously encoded, check 1 bit for inclusion/exclusion status
                    bit_r.next_bit()?
                } else {
                    // If a code-block inclusion level has not been encoded, update tag tree until we know
                    let decision =
                        sub_band_ctx.inclusion_decision(cb_idx, ctx.layer, &mut bit_r)?;
                    println!("Checking cb inclusion {}", decision);
                    if decision {
                        // initialize zero plane information
                        let zero_planes = sub_band_ctx.zero_planes(cb_idx, &mut bit_r)?;
                        println!("Initializing code block with {zero_planes} zero planes");
                        sub_band_ctx.cbs[cb as usize].num_zero_bit_planes(zero_planes);
                    }
                    decision
                };
                println!("Survery says! {}", to_include);
                // If not including this code block, not much to do
                if !to_include {
                    continue 'for_code_blocks;
                }

                // Ok code block included and zero planes initialized
                println!("Parsing code pass count");
                let code_pass_count = parse_coding_pass(&mut bit_r)?;
                let mut to_inc = 0;
                while bit_r.next_bit()? {
                    println!("Increment lblock");
                    to_inc += 1;
                }
                println!("Incrementing lblock {}", to_inc);
                // TODO sub_band_ctx.code_block(cb).increment_lblock(to_inc);
                let lblock = 3 + to_inc;
                let count_read = lblock + code_pass_count.ilog2() as u8;

                println!("count bits to read {}", count_read);
                let coded_bytes = bit_r.take(count_read)?;
                //                println!(
                //                    "Need to read {} bytes from reader for codeblock",
                //                    coded_bytes
                //                );
                cb_info.push((cb_idx, code_pass_count, coded_bytes));
                total_to_read += coded_bytes as usize;
            }
            packet_info.push(cb_info);
        }
        Ok(PrecinctDecoder {
            ctx,
            header: Some(HeaderInfo {
                length: total_to_read,
                packet_info,
            }),
            bounds,
            is_ll,
        })
    }
    /// Consume a packet pointed to by the reader. The previous call must be to
    /// consume_packet_header to prime the handlers.
    pub fn consume_packet<R: RR>(self, reader: &mut R) -> PacketResult<PrecinctDecoder> {
        let Self {
            mut ctx,
            header,
            bounds,
            is_ll,
        } = self;

        let Some(HeaderInfo { packet_info, .. }) = header else {
            panic!("Invalid consume_packet call");
        };

        for (sb, header_info) in ctx.sub_bands.iter_mut().zip(packet_info) {
            println!("Begin subband work on ? {:?}", sb);
            // todo
            for (cb_idx, code_pass_count, coded_bytes) in header_info {
                println!("Reading {coded_bytes} for {:?}", cb_idx);
                let mut buf = vec![0u8; coded_bytes as usize];
                reader.read_exact(&mut buf)?;
                let mut coder = standard_decoder(&buf);
                // TODO handle more than one packet for a code block, handle different mq coder
                // styles
                let cb = &mut sb.cbs[0];
                cb.decode(code_pass_count, &mut coder)?;
            }
        }
        Ok(PrecinctDecoder {
            ctx,
            header: None,
            bounds,
            is_ll,
        })
    }
}

struct SubBandCoefficients<T> {
    sub_band: SubBandType,
    data: Array2D<T>,
}

// SubBandPacketContext records the tag trees used for context when decoding packet headers
#[derive(Debug)]
struct SubBandPacketContext {
    pub inclusion: InclusionTagTree,
    pub zero_bits: ZeroPlaneTagTree,
}

impl SubBandPacketContext {
    fn new(width: u8, height: u8) -> Self {
        let (width, height) = (width as usize, height as usize);
        Self {
            inclusion: InclusionTagTree::new(width, height),
            zero_bits: ZeroPlaneTagTree::new(width, height),
        }
    }
}

fn decode_packet<R: RR>(ctx: &mut SubBandPacketContext, reader: &mut R) -> ! {
    todo!("decode packet");
}

fn parse_coding_pass<R: Read>(br: &mut BitReader<'_, R>) -> PacketResult<u8> {
    if !br.next_bit()? {
        // 0b0
        return Ok(1);
    }
    if !br.next_bit()? {
        // 0b10
        return Ok(2);
    }
    // 0b11 ?
    let r = br.take(2)?;
    if r != 0b11 {
        // 0b 11 xx
        return Ok(3 + r);
    }
    // 0b 1111 ?
    let r = br.take(5)?;
    if r != 0b11111 {
        return Ok(6 + r);
    }
    // 0b 1111 11111 ?
    let r = br.take(7)?;
    Ok(37 + r)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Seek};

    use crate::shared::Bounds;

    use super::*;

    type E = Box<dyn error::Error>;

    #[test]
    fn test_create() {
        let bounds = Bounds {
            x0: 0,
            x1: 32,
            y0: 0,
            y1: 32,
        };
        let tcr = TileComponentResolutionBounds(bounds);
        let r = PrecinctDecoder::new(5, 5, &[9], tcr, true);
    }

    #[test]
    fn test_packet_decode_consume_01() -> Result<(), E> {
        let ba = b"\xC7\xD4\x0C\x01\x8f\x0D\xC8\x75\x5D\x00\x00\x00";
        let mut reader = Cursor::new(ba);

        // TODO this new method sucks
        let tcr = TileComponentResolutionBounds(Bounds {
            x0: 0,
            x1: 1,
            y0: 0,
            y1: 5,
        });
        let decoder = PrecinctDecoder::new(5, 5, &[9], tcr, true);
        let decoder = decoder.consume_packet_header(&mut reader)?;
        assert_eq!(reader.position(), 3, "Header was 3 bytes");
        let decoder = decoder.consume_packet(&mut reader)?;
        assert_eq!(reader.position(), 9, "expected to consume 9 bytes");

        let sb = &decoder.ctx.sub_bands[0];
        let cb = &sb.cbs[0];
        let coeffs = cb.coefficients();
        let exp = Array2D::from_data(vec![-26, -22, -30, -32, -19], 1, 5);
        assert_eq!(coeffs, exp);

        Ok(())
    }

    #[test]
    fn test_packet_decode_consume_02() -> Result<(), E> {
        let ba = b"\xC0\x7C\x21\x80\x0F\xB1\x76";
        let mut reader = Cursor::new(ba);

        // TODO this new method sucks
        let tcr = TileComponentResolutionBounds(Bounds {
            x0: 0,
            x1: 1,
            y0: 0,
            y1: 9,
        });
        let decoder = PrecinctDecoder::new(5, 5, &[10, 10, 10], tcr, false);
        let decoder = decoder.consume_packet_header(&mut reader)?;
        assert_eq!(reader.position(), 4, "Header was 4 bytes");
        let decoder = decoder.consume_packet(&mut reader)?;
        assert_eq!(reader.position(), 7, "expected to consume 7 bytes");

        let sb = decoder.ctx.sub_bands.last().expect("Expected to grab LH");
        let cb = &sb.cbs[0];
        let coeffs = cb.coefficients();
        assert_eq!(coeffs, Array2D::from_data(vec![1, 5, 1, 0], 1, 4));

        Ok(())
    }

    #[test]
    fn test_decode_zl_packet() -> Result<(), E> {
        let ba = b"\x00";
        let mut reader = Cursor::new(ba);

        let tcr = TileComponentResolutionBounds(Bounds {
            x0: 0,
            x1: 1,
            y0: 0,
            y1: 9,
        });
        let decoder = PrecinctDecoder::new(5, 5, &[10, 10, 10], tcr, false);
        let decoder = decoder.consume_packet_header(&mut reader)?;
        let state = decoder.header;

        assert_eq!(
            0,
            state.expect("expected header").length,
            "zero length packet header"
        );
        assert_eq!(reader.position(), 1, "expected to consume 1 bytes");
        Ok(())
    }

    /// from B.10 packet header
    #[test]
    fn test_parse_pass_count() {
        let vals: Vec<(u8, &[u8])> = vec![
            (1, b"\x00"),
            (2, b"\x80"),
            (4, b"\xD0"),
            (6, b"\xF0\x00"),
            (37, b"\xFF\x80"),
            (37 + 4, b"\xFF\x84"),
            (37 + 112, b"\xFF\xF0"),
        ];
        for (exp, bs) in vals {
            let mut cursor = Cursor::new(bs);
            let mut br = BitReader::new(&mut cursor).expect("unable to create reader");
            assert_eq!(exp, parse_coding_pass(&mut br).expect("didn't expect fail"));
        }
    }
}
