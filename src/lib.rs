mod error;
mod jpeg_decoder;
mod png_decoder;

use std::{
	fs::File,
	io::{BufRead, BufReader, Seek},
	path::Path,
};

use image::{DynamicImage, ImageFormat, guess_format};

use crate::png_decoder::PngDecoder;


pub fn load_image_from_reader<R: BufRead + Seek>(mut reader: R) -> Result<(ImageFormat, DynamicImage), error::Error> {
	// Guess format
	let mut buf = [0; 16];
	reader.read_exact(&mut buf)?;
	reader.rewind()?;
	let Ok(format) = guess_format(&buf) else {
		return Err(error::Error::UnsupportedFormat);
	};

	match format {
		ImageFormat::Png => {
			let decoder = PngDecoder::new(reader)?;
			if decoder.is_animated() {
				return Err(error::Error::Animated);
			}
			let img = DynamicImage::from_decoder(decoder)?;
			return Ok((ImageFormat::Png, img));
		},
		ImageFormat::Jpeg => {
			let decoder = jpeg_decoder::JpegDecoder::new(reader)?;
			let img = DynamicImage::from_decoder(decoder)?;
			return Ok((ImageFormat::Jpeg, img));
		},
		_ => {
			// Use the image crate directly for other formats
			let img = image::load(reader, format)?;
			return Ok((format, img));
		},
	}
}


pub fn load_image<P: AsRef<Path>>(path: P) -> Result<(ImageFormat, DynamicImage), error::Error> {
	let file = File::open(path)?;
	let reader = BufReader::new(file);

	load_image_from_reader(reader)
}
