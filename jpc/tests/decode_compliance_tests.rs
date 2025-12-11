//! Test cases from standard compliance suite.
use std::{
    fs::File,
    io::{BufReader, Cursor},
    path::{Path, PathBuf},
};

use jpc::{
    decode_jpc, CodingBlockStyle, CommentRegistrationValue, MultipleComponentTransformation,
    ProgressionOrder, QuantizationStyle, TransformationFilter,
};

mod shared;
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
    let p = test_file("c0p0_01.pgx")?;
    let pgx: PgxImage = load_pgx(p.as_path())?;

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

    assert_eq!(128 * 128, pgx.samples.length());

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
    let j2k = test_file("j10.j2k")?;
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

    let tiles = codestream.tiles();
    assert_eq!(1, tiles.len(), "Expected a single tile for this image.");
    println!("single tile? {:?}", tiles[0]);
    let t0 = &tiles[0];
    //assert_eq!(t0.header.length, 30);
    let expected_component: Vec<u8> = vec![101, 103, 104, 105, 96, 97, 96, 102, 109];

    //let decoded_component = ...;

    //println!("pgx samples: {:?}", pgx.samples);
    panic!("TODO");

    // assert_eq!(pgx.samples, codestream.component[0].samples);
    Ok(())
}
