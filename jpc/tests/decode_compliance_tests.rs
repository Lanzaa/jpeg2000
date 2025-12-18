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

#[test]
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
    let mut reader = BufReader::new(file);
    let result = decode_jpc(&mut reader);
    assert!(result.is_ok());
    let codestream = result.unwrap();

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
    info!("Hello world");

    // Pull out component data
    assert_eq!(siz.no_components(), 1);
    let mut buf: [u8; _] = [0u8; 128 * 128];
    todo!("Implement component grab");
    //codestream.components[0].read_samples(&mut buf);
    assert_eq!(buf, pgx_data.as_slice(), "Sample data should match.");

    //let tiles = codestream.tiles();
    //assert_eq!(1, tiles.len(), "Expected a single tile for this image.");
    //println!("single tile? {:?}", tiles[0]);
    //assert_eq!("tile 0 length", 7314);

    //let decoded_component = ...;

    //println!("pgx samples: {:?}", pgx.samples);
    panic!("TODO");

    // assert_eq!(pgx.samples, codestream.component[0].samples);
    Ok(())
}

#[test]
fn test_j10_example() -> Result<(), String> {
    shared::init_logger();
    let j2k = test_file("j10.j2k")?;
    let file = File::open(j2k.as_path()).expect("Unable to load test file");
    let mut reader = BufReader::new(file);
    let mut decoder = JP2Decoder::new(reader);
    let result = decoder.read_codestream();
    assert!(result.is_ok());
    let codestream = result.unwrap();

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
    //assert_eq!(siz.)

    // Pull out component data
    assert_eq!(siz.no_components(), 1);
    let data_exp = [101, 103, 104, 105, 96, 97, 96, 102, 109 + 1] as [u8; _]; // TODO remove +1
    let mut buf: [u8; _] = [0u8; 1 * 9];
    decoder.read_component(0, &mut buf);
    //codestream.components[0].read_samples(&mut buf);
    assert_eq!(buf, data_exp, "Sample data should match.");

    // let tiles = codestream.tiles();
    // assert_eq!(1, tiles.len(), "Expected a single tile for this image.");
    // println!("single tile? {:?}", tiles[0]);
    // let t0 = &tiles[0];
    // //assert_eq!(t0.header.length, 30);
    // let expected_component: Vec<u8> = vec![101, 103, 104, 105, 96, 97, 96, 102, 109];

    //let decoded_component = ...;

    //println!("pgx samples: {:?}", pgx.samples);
    panic!("TODO");

    // assert_eq!(pgx.samples, codestream.component[0].samples);
    Ok(())
}
