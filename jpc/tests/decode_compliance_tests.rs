//! Test cases from standard compliance suite.
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use jpc::{ImageDecoder, Profile0Decoder};

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

/// Test an 8 bit 16x16 image with 2 resolution level
#[test]
#[ignore = "lots of work needed before this is ready"]
fn test_8b16g_n2() -> Result<(), String> {
    shared::init_logger();

    let p = test_file("8b16x16.pgx")?;
    let pgx: PgxImage = load_pgx(p.as_path())?;
    assert_eq!(16 * 16, pgx.samples.length()); // basic file load test
    assert_eq!(8, pgx.bit_depth);
    let shared::PixelData::U8(pgx_data) = pgx.samples else {
        panic!("Unexpected type in test");
    };

    let j2k = test_file("8b16x16_n2.j2k")?;
    let file = File::open(j2k.as_path()).expect("Unable to load test file");
    let mut reader = BufReader::new(file);
    let mut decoder = match Profile0Decoder::new(&mut reader) {
        Ok(decoder) => decoder,
        Err(e) => {
            panic!("Error decoding file: {:?}", e);
        }
    };

    let image_info = decoder.info();
    let (width, height) = (image_info.width, image_info.height);
    assert_eq!(width, 16);
    assert_eq!(height, 16);
    assert_eq!(image_info.num_components, 1);

    // Pull out component data
    let mut buf = vec![0u8; (width * height) as usize];
    decoder.decode_component(0, &mut buf).unwrap_or_else(|e| {
        panic!("decode error: {:?}", e);
    });
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
#[ignore = "Need to handle other progression orders"]
fn test_c0p0() -> Result<(), String> {
    shared::init_logger();

    let p = test_file("c0p0_01.pgx")?;
    let pgx: PgxImage = load_pgx(p.as_path())?;
    assert_eq!(128 * 128, pgx.samples.length()); // basic file load test
    assert_eq!(8, pgx.bit_depth);
    let shared::PixelData::U8(pgx_data) = pgx.samples else {
        panic!("Unexpected type in test");
    };

    let j2k = test_file("p0_01.j2k")?;
    let file = File::open(j2k.as_path()).expect("Unable to load test file");
    let mut reader = BufReader::new(file);
    let mut decoder = match Profile0Decoder::new(&mut reader) {
        Ok(decoder) => decoder,
        Err(e) => {
            panic!("Error decoding file: {:?}", e);
        }
    };

    let image_info = decoder.info();
    let (width, height) = (image_info.width, image_info.height);
    assert_eq!(width, 128);
    assert_eq!(height, 128);
    assert_eq!(image_info.num_components, 1);

    // Pull out component data
    let mut buf = vec![0u8; (width * height) as usize];
    decoder.decode_component(0, &mut buf).unwrap_or_else(|e| {
        panic!("decode error: {:?}", e);
    });
    let fp = 40;
    assert_eq!(
        pgx_data.as_slice()[..fp],
        buf[..fp],
        "Sample data should match."
    );
    assert_eq!(pgx_data.as_slice(), buf, "Sample data should match.");

    Ok(())
    //todo!("Did we really pass !??!  YAY !!!");
}

#[test]
fn test_j10_example() -> Result<(), String> {
    shared::init_logger();
    let j2k = test_file("j10.j2k")?;
    let file = File::open(j2k.as_path()).expect("Unable to load test file");
    let mut reader = BufReader::new(file);
    let mut decoder = match Profile0Decoder::new(&mut reader) {
        Ok(decoder) => decoder,
        Err(e) => {
            panic!("Error decoding file: {:?}", e);
        }
    };

    let image_info = decoder.info();
    let (width, height) = (image_info.width, image_info.height);
    assert_eq!(width, 1);
    assert_eq!(height, 9);
    assert_eq!(image_info.num_components, 1);
    assert_eq!(image_info.components()[0].bit_depth, 8);
    assert!(!image_info.components()[0].is_signed);

    // Pull out component data
    let data_exp = [101, 103, 104, 105, 96, 97, 96, 102, 109]; // TODO remove +1
    let mut buf = vec![0u8; (width * height) as usize];
    decoder.decode_component(0, &mut buf).unwrap();
    assert_eq!(buf, data_exp, "Sample data should match.");
    Ok(())
}
