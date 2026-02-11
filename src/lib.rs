mod error;
mod jpeg_decoder;
mod png_decoder;

use std::{
	fs::File,
	io::{BufReader, Read as _, Seek},
	path::Path,
};

use image::{DynamicImage, ImageFormat, guess_format};

use crate::png_decoder::PngDecoder;


pub fn load_image<P: AsRef<Path>>(path: P) -> Result<(ImageFormat, DynamicImage), error::Error> {
	let file = File::open(path)?;
	let mut reader = BufReader::new(file);

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
