//! PgxImage loader helper functions

use std::io::prelude::*;
use std::io::BufReader;
use std::{fs::File, path::Path};

pub struct PgxImage {
    pub bit_depth: i64, // i16 would probably be enough
    pub width: i64,
    pub height: i64,
    pub samples: PixelData,
}

#[derive(Debug)]
pub enum PixelType {
    U8(u8),
    U16(u16),
    U32(u32),
    I8(i8),
    I16(i16),
    I32(i32),
}

#[derive(Debug)]
pub enum PixelData {
    U8(Vec<u8>),
    U16(Vec<u16>),
    U32(Vec<u32>),
    I8(Vec<i8>),
    I16(Vec<i16>),
    I32(Vec<i32>),
}

impl PixelData {
    pub fn length(&self) -> usize {
        match self {
            PixelData::U8(v) => v.len(),
            PixelData::U16(v) => v.len(),
            PixelData::U32(v) => v.len(),
            PixelData::I8(v) => v.len(),
            PixelData::I16(v) => v.len(),
            PixelData::I32(v) => v.len(),
        }
    }
}

pub fn load_pgx(p: &Path) -> Result<PgxImage, String> {
    let file = File::open(p).expect("Unable to open file");
    let mut reader = BufReader::new(file);
    let mut header = String::new();
    reader
        .read_line(&mut header)
        .expect("Unable to read header line");

    let parts: Vec<&str> = header.split_whitespace().collect();
    println!("header {}", header);

    assert!(
        parts.len() == 6 && parts[0] == "PG" && parts[1] == "ML",
        "Invalid PGX file header"
    );

    let sign = parts[2];
    let bit_depth = parts[3].parse::<i64>().unwrap();
    let width = parts[4].parse::<i64>().unwrap();
    let height = parts[5].parse::<i64>().unwrap();
    let mut raw_data = Vec::new();
    reader
        .read_to_end(&mut raw_data)
        .expect("Unable to read data");
    let samples = match (sign, bit_depth) {
        ("+", 8) => {
            let pixels: Vec<u8> = raw_data
                .chunks_exact(1)
                .map(|c| u8::from_le_bytes([c[0]]))
                .collect();
            Ok(PixelData::U8(pixels))
        }
        ("-", 8) => {
            let pixels: Vec<i8> = raw_data
                .chunks_exact(1)
                .map(|c| i8::from_le_bytes([c[0]]))
                .collect();
            Ok(PixelData::I8(pixels))
        }
        ("+", 16) => {
            let pixels: Vec<u16> = raw_data
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            Ok(PixelData::U16(pixels))
        }
        ("-", 16) => {
            let pixels: Vec<i16> = raw_data
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect();
            Ok(PixelData::I16(pixels))
        }
        ("+", 32) => {
            let pixels: Vec<u32> = raw_data
                .chunks_exact(4)
                .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            Ok(PixelData::U32(pixels))
        }
        ("-", 32) => {
            let pixels: Vec<i32> = raw_data
                .chunks_exact(4)
                .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            Ok(PixelData::I32(pixels))
        }
        _ => Err("Unknown bit_depth"),
    }
    .unwrap();
    Ok(PgxImage {
        bit_depth,
        width,
        height,
        samples,
    })
}
