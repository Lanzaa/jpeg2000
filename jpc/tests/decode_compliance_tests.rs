//! Test cases from standard compliance suite.
use std::{
    fs::File,
    io::{BufReader, Cursor},
    path::{Path, PathBuf},
};

use jpc::{
    decode_jpc, CodingBlockStyle, CommentRegistrationValue, Decoder, JP2Decoder,
    MultipleComponentTransformation, ProgressionOrder, QuantizationStyle, TransformationFilter,
};

mod shared;
use log::info;
use shared::{load_pgx, PgxImage};

fn test_file(filename: &str) -> Result<PathBuf, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(String::from(filename));
    if !path.exists() {
        panic!("Unable to find test file");
    }
    Ok(path)
}

/// Test an 8 bit 16x16 image with 2 resolution level
#[test]
//#[ignore = "lots of work needed before this is ready"]
fn test_8b16g_n2() -> Result<(), String> {
    shared::init_logger();

    let p = test_file("8b16x16.pgx")?;
    let pgx: PgxImage = load_pgx(p.as_path())?;
    assert_eq!(16 * 16, pgx.samples.length()); // basic file load test
    assert_eq!(8, pgx.bit_depth);
    let pgx_data = match pgx.samples {
        shared::PixelData::U8(data) => data,
        _ => panic!("Unexpected type in test"),
    };

    let j2k = test_file("8b16x16_n2.j2k")?;
    let file = File::open(j2k.as_path()).expect("Unable to load test file");
    let reader = BufReader::new(file);
    let mut decoder = JP2Decoder::new(reader);
    let codestream = match decoder.read_codestream() {
        Ok(cs) => cs,
        Err(e) => panic!("Error decoding codestream: {}", e),
    };
    let header = codestream.header();

    let siz = header.image_and_tile_size_marker_segment();
    assert_eq!(siz.reference_grid_width(), 16);
    assert_eq!(siz.reference_grid_height(), 16);
    assert_eq!(siz.image_horizontal_offset(), 0);
    assert_eq!(siz.image_vertical_offset(), 0);
    assert_eq!(siz.offset(), 4);
    //assert_eq!(siz.length(), 47);
    //assert_eq!(siz.decoder_capabilities(), 0);
    assert_eq!(siz.image_horizontal_offset(), 0);
    assert_eq!(siz.image_vertical_offset(), 0);
    assert_eq!(siz.reference_tile_width(), 16);
    assert_eq!(siz.reference_tile_height(), 16);
    assert_eq!(siz.no_components(), 1);
    assert_eq!(siz.precision(0).unwrap(), 8);
    assert_eq!(siz.values_are_signed(0).unwrap(), false);
    assert_eq!(siz.horizontal_separation(0).unwrap(), 1);
    assert_eq!(siz.vertical_separation(0).unwrap(), 1);
    let progresion_order = header.coding_style_marker_segment().progression_order();
    assert_eq!(progresion_order, ProgressionOrder::LRLCPP);

    let cod = header.coding_style_marker_segment();
    assert_eq!(cod.no_layers(), 1);
    let params = cod.coding_style_parameters();
    assert_eq!(params.no_decomposition_levels(), 1);
    assert_eq!(params.transformation(), TransformationFilter::Reversible);

    // Pull out component data
    assert_eq!(siz.no_components(), 1);

    let (width, height) = decoder.dimensions();
    assert_eq!(width, 16, "expected width to be decoded correctly");
    assert_eq!(height, 16, "expected height to be decoded correctly");
    // Pull out component data
    let mut buf = vec![0u8; (width * height) as usize];
    decoder.read_component(0, &mut buf).unwrap();
    let fp = 40;
    assert_eq!(
        pgx_data.as_slice()[..fp],
        buf[..fp],
        "Sample data should match."
    );
    assert_eq!(pgx_data.as_slice(), buf, "Sample data should match.");

    todo!("Did we really pass !??!  YAY !!!");
}

#[test]
#[ignore = "lots of work needed before this is ready"]
fn test_c0p0() -> Result<(), String> {
    shared::init_logger();

    let p = test_file("c0p0_01.pgx")?;
    let pgx: PgxImage = load_pgx(p.as_path())?;
    assert_eq!(128 * 128, pgx.samples.length()); // basic file load test
    assert_eq!(8, pgx.bit_depth);
    let pgx_data = match pgx.samples {
        shared::PixelData::U8(data) => data,
        _ => panic!("Unexpected type in test"),
    };

    let j2k = test_file("p0_01.j2k")?;
    let file = File::open(j2k.as_path()).expect("Unable to load test file");
    let reader = BufReader::new(file);
    let mut decoder = JP2Decoder::new(reader);
    let codestream = match decoder.read_codestream() {
        Ok(cs) => cs,
        Err(e) => panic!("Error decoding codestream: {}", e),
    };
    let header = codestream.header();

    let siz = header.image_and_tile_size_marker_segment();
    assert_eq!(siz.reference_grid_width(), 128);
    assert_eq!(siz.reference_grid_height(), 128);
    assert_eq!(siz.image_horizontal_offset(), 0);
    assert_eq!(siz.image_vertical_offset(), 0);
    assert_eq!(siz.offset(), 4);
    //assert_eq!(siz.length(), 47);
    //assert_eq!(siz.decoder_capabilities(), 0);
    assert_eq!(siz.image_horizontal_offset(), 0);
    assert_eq!(siz.image_vertical_offset(), 0);
    assert_eq!(siz.reference_tile_width(), 128);
    assert_eq!(siz.reference_tile_height(), 128);
    assert_eq!(siz.no_components(), 1);
    assert_eq!(siz.precision(0).unwrap(), 8);
    assert_eq!(siz.values_are_signed(0).unwrap(), false);
    assert_eq!(siz.horizontal_separation(0).unwrap(), 1);
    assert_eq!(siz.vertical_separation(0).unwrap(), 1);
    let progresion_order = header.coding_style_marker_segment().progression_order();
    assert_eq!(progresion_order, ProgressionOrder::RLLCPP);

    // assert_eq!(codestream.tiles.len(), 1, "Only one tile");

    info!("Hello world");

    let precision = siz.precision(0).unwrap();
    assert_eq!(precision, 8);

    // Pull out component data
    assert_eq!(siz.no_components(), 1);

    let (width, height) = decoder.dimensions();
    assert_eq!(width, 128, "expected width to be decoded correctly");
    assert_eq!(height, 128, "expected height to be decoded correctly");
    // Pull out component data
    let mut buf = vec![0u8; (width * height) as usize];
    decoder.read_component(0, &mut buf).unwrap();
    let fp = 40;
    assert_eq!(
        pgx_data.as_slice()[..fp],
        buf[..fp],
        "Sample data should match."
    );
    assert_eq!(pgx_data.as_slice(), buf, "Sample data should match.");

    todo!("Did we really pass !??!  YAY !!!");
}

#[test]
#[ignore = "lots of work needed before this is ready"]
fn test_j10_example() -> Result<(), String> {
    shared::init_logger();
    let j2k = test_file("j10.j2k")?;
    let file = File::open(j2k.as_path()).expect("Unable to load test file");
    let reader = BufReader::new(file);
    let mut decoder = JP2Decoder::new(reader);
    let codestream = match decoder.read_codestream() {
        Ok(cs) => cs,
        Err(e) => panic!("Error decoding codestream: {}", e),
    };
    let header = codestream.header();

    let siz = header.image_and_tile_size_marker_segment();
    assert_eq!(siz.reference_grid_width(), 1);
    assert_eq!(siz.reference_grid_height(), 9);
    assert_eq!(siz.image_horizontal_offset(), 0);
    assert_eq!(siz.image_vertical_offset(), 0);
    assert_eq!(siz.offset(), 4);
    //assert_eq!(siz.length(), 47);
    //assert_eq!(siz.decoder_capabilities(), 0);
    assert_eq!(siz.image_horizontal_offset(), 0);
    assert_eq!(siz.image_vertical_offset(), 0);
    assert_eq!(siz.reference_tile_width(), 1);
    assert_eq!(siz.reference_tile_height(), 9);
    assert_eq!(siz.no_components(), 1);
    assert_eq!(siz.precision(0).unwrap(), 8);
    assert!(!siz.values_are_signed(0).unwrap());
    assert_eq!(siz.horizontal_separation(0).unwrap(), 1);
    assert_eq!(siz.vertical_separation(0).unwrap(), 1);
    let precision = siz.precision(0).unwrap();
    assert_eq!(precision, 8);

    assert_eq!(decoder.no_components(), 1);

    let (width, height) = decoder.dimensions();
    assert_eq!(width, 1, "expected width to be decoded correctly");
    assert_eq!(height, 9, "expected height to be decoded correctly");

    // Pull out component data
    let data_exp = [101, 103, 104, 105, 96, 97, 96, 102, 109 + 1] as [u8; _]; // TODO remove +1
    let mut buf = vec![0u8; (width * height) as usize];
    decoder.read_component(0, &mut buf).unwrap();
    assert_eq!(buf, data_exp, "Sample data should match.");
    todo!("Did we really pass !??!  YAY !!!");
}
