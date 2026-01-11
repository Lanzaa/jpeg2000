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
use std::io::{self, Read};

use crate::code_block::CodeBlockDecodeError;
use crate::coder::standard_decoder;
use crate::shared::{Bounds, SubBandType, I2};
use crate::tag_tree::{InclusionTagTree, ZeroPlaneTagTree};
use crate::{bit_reader::BitReader, code_block::CodeBlockDecoder};

trait PacketDecoder {}

trait PacketDecoderState {}
/// states for the PacketDecoder
///
/// Typestate pattern
pub mod states {
    use super::PacketDecoderState;
    use super::I2;
    /// Need to consume header before decoding next packet
    #[derive(Debug)]
    pub struct NeedsHeader;
    /// Ready consume a packet
    ///
    /// contains information from the relevant header
    #[derive(Debug, Default)]
    pub struct ReadyForPacket {
        // TODO is there anything from the header that is really interesting?
        pub length: usize, // TODO how big do packets get?
        pub packet_info: Vec<Vec<(I2, u8, u8)>>,
    }
    pub struct Failed;
    impl PacketDecoderState for NeedsHeader {}
    impl PacketDecoderState for ReadyForPacket {}
    impl PacketDecoderState for Failed {}
}
use log::info;
use states::{NeedsHeader, ReadyForPacket};

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
pub struct PrecinctDecoder<S: PacketDecoderState> {
    ctx: DecoderContext,
    state: S,
}

pub trait RR: io::Read {}

impl<R: io::Read> RR for R {}

pub struct D {
    pub width: u32,
    pub height: u32,
}
impl PrecinctDecoder<NeedsHeader> {
    pub fn new(xcb: u16, ycb: u16, bounds: Bounds, is_ll: bool) -> Self {
        let width = bounds.x1 - bounds.x0;
        let height = bounds.y1 - bounds.y0;
        println!("widht, height: {},{}", width, height);
        println!("xcb, yxb: {},{}", xcb, ycb);

        let num_cb_wide = width.div_ceil(xcb.into());
        let num_cb_tall = height.div_ceil(ycb.into());
        info!(
            "Createing packet decoder codeblocks wxh: {}x{}",
            num_cb_wide, num_cb_tall
        );

        //let code_block_initializer = vec![];

        let sub_bands = if is_ll {
            let mb = 9; // TODO
            let ll_width = 1;
            let ll_height = 1;
            let sb_ctx = SubBandContext {
                inclusion_tree: InclusionTagTree::new(ll_width, ll_height),
                zeros_tree: ZeroPlaneTagTree::new(ll_width, ll_height),
                cbs: vec![CodeBlockDecoder::new(1, 5, SubBandType::LL, mb)],
            };
            vec![sb_ctx]
            //todo!("Handle LL init");
        } else {
            let mb = 10; // TODO
            let sb_ctx = SubBandContext {
                inclusion_tree: InclusionTagTree::new(1, 1),
                zeros_tree: ZeroPlaneTagTree::new(1, 1),
                cbs: vec![CodeBlockDecoder::new(1, 4, SubBandType::HL, mb)],
            };
            vec![sb_ctx]
            //todo!("Handle not LL subband init");
            //vec![SubBandContext::new(Subcode_block_initializer)];
            // vec![HL, LH, HH]
        };

        PrecinctDecoder {
            ctx: DecoderContext {
                layer: 0,
                sub_bands,
            },
            state: NeedsHeader,
        }
    }

    pub fn grab_subbands(&self) -> u32 {
        // TODO combine code blocks
        let zz = self
            .ctx
            .sub_bands
            .iter()
            .map(|sbc| &sbc.cbs)
            .collect::<Vec<_>>();
        32u32
    }
    /// Consume a packet header pointed to by the reader
    pub fn consume_packet_header<R: RR>(
        self,
        reader: &mut R,
    ) -> PacketResult<PrecinctDecoder<ReadyForPacket>> {
        let Self { mut ctx, .. } = self;

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
                state: ReadyForPacket {
                    length: 0,
                    ..Default::default()
                },
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
            state: ReadyForPacket {
                length: total_to_read,
                packet_info,
            },
        })
    }
}

impl PrecinctDecoder<ReadyForPacket> {
    /// Consume a packet pointed to by the reader. The previous call must be to
    /// consume_packet_header to prime the handlers.
    pub fn consume_packet<R: RR>(
        self,
        reader: &mut R,
    ) -> PacketResult<PrecinctDecoder<NeedsHeader>> {
        let Self { mut ctx, state } = self;

        let ReadyForPacket { packet_info, .. } = state;

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
            state: NeedsHeader,
        })
    }
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

    use super::*;

    type E = Box<dyn error::Error>;

    #[test]
    fn test_create() {
        let dims = D {
            width: 128,
            height: 128,
        };
        let bounds = Bounds {
            x0: 0,
            x1: 128,
            y0: 0,
            y1: 128,
        };
        let r = PrecinctDecoder::new(5, 5, bounds, true);
    }

    #[test]
    fn test_packet_decode_consume_01() -> Result<(), E> {
        let ba = b"\xC7\xD4\x0C\x01\x8f\x0D\xC8\x75\x5D\x00\x00\x00";
        let mut reader = Cursor::new(ba);

        // TODO this new method sucks
        let decoder = PrecinctDecoder::new(
            5,
            5,
            Bounds {
                x0: 0,
                x1: 1,
                y0: 0,
                y1: 5,
            },
            true,
        );
        let decoder = decoder.consume_packet_header(&mut reader)?;
        assert_eq!(reader.position(), 3, "Header was 3 bytes");
        let decoder = decoder.consume_packet(&mut reader)?;
        assert_eq!(reader.position(), 9, "expected to consume 9 bytes");

        let sb = &decoder.ctx.sub_bands[0];
        let cb = &sb.cbs[0];
        let coeffs = cb.coefficients();
        assert_eq!(coeffs, vec![-26, -22, -30, -32, -19]);

        Ok(())
    }

    #[test]
    fn test_packet_decode_consume_02() -> Result<(), E> {
        let ba = b"\xC0\x7C\x21\x80\x0F\xB1\x76";
        let mut reader = Cursor::new(ba);

        // TODO this new method sucks
        let decoder = PrecinctDecoder::new(
            5,
            5,
            Bounds {
                x0: 0,
                x1: 1,
                y0: 0,
                y1: 4,
            },
            false,
        );
        let decoder = decoder.consume_packet_header(&mut reader)?;
        assert_eq!(reader.position(), 4, "Header was 4 bytes");
        let decoder = decoder.consume_packet(&mut reader)?;
        assert_eq!(reader.position(), 7, "expected to consume 7 bytes");

        let sb = &decoder.ctx.sub_bands[0];
        let cb = &sb.cbs[0];
        let coeffs = cb.coefficients();
        assert_eq!(coeffs, vec![1, 5, 1, 0]);

        Ok(())
    }

    //#[test]
    //#[ignore = "I forgot how to extend this test to work..."]
    //fn test_packet_decode_consume_8b16x16() {
    //    // From 8b16x16
    //    let ba =
    //        b"\xdf\x82\x08\x14\xbd\x9e\x08\x18\x20\xcd\x8f\x4a\x65\x75\xc6\x77\xb9\xe1\x59\xf6";
    //    // might need more data

    //    let mut ctx = SubBandPacketContext {
    //        inclusion: TagTreeDecoder::new(1, 1),
    //        zero_bits: TagTreeDecoder::new(1, 1),
    //    };
    //    let mut reader = Cursor::new(ba);

    //    let r = decode_packet(&mut ctx, &mut reader);
    //    assert!(r.is_ok());
    //    assert_eq!(reader.position(), 7, "expected to consume 7 bytes");
    //}

    #[test]
    fn test_decode_zl_packet() -> Result<(), E> {
        let ba = b"\x00";
        let mut reader = Cursor::new(ba);

        let decoder = PrecinctDecoder::new(
            5,
            5,
            Bounds {
                x0: 0,
                x1: 1,
                y0: 0,
                y1: 4,
            },
            false,
        );
        let decoder = decoder.consume_packet_header(&mut reader)?;
        let state = decoder.state;

        assert_eq!(0, state.length, "zero length packet header");
        assert_eq!(reader.position(), 1, "expected to consume 1 bytes");
        Ok(())
    }

    #[test]
    #[ignore = "TODO, need proper decoding and more data passed in"]
    fn test_packet_decode_consume_c0p0() -> Result<(), E> {
        // test case c0p0
        let ba = b"\xdf\x85\xa8\x94\x36\x0f\x77\x22\xea\xf1";
        let mut reader = Cursor::new(ba);

        todo!("not sure what parameters are needed");
        let decoder = PrecinctDecoder::new(
            5,
            5,
            Bounds {
                x0: 0,
                x1: 1,
                y0: 0,
                y1: 4,
            },
            false,
        );
        let decoder = decoder.consume_packet_header(&mut reader)?;
        assert_eq!(reader.position(), 9999999, "Header was 4 bytes");
        let decoder = decoder.consume_packet(&mut reader)?;
        assert_eq!(reader.position(), 9, "expected to consume 9 bytes");

        let sb = &decoder.ctx.sub_bands[0];
        let cb = &sb.cbs[0];
        let coeffs = cb.coefficients();
        assert_eq!(coeffs, vec![777]);

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
