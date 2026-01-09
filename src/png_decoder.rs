use std::io::{BufRead, Seek};

use image::{
	ColorType, ExtendedColorType, ImageDecoder, ImageError, ImageFormat, ImageResult, Limits,
	error::{DecodingError, LimitError, LimitErrorKind, ParameterError, ParameterErrorKind, UnsupportedError, UnsupportedErrorKind},
};

use crate::error::Error;


const XMP_KEY: &str = "XML:com.adobe.xmp";
const IPTC_KEYS: &[&str] = &["Raw profile type iptc", "Raw profile type 8bim"];


pub struct PngDecoder<R: BufRead + Seek> {
	color_type: ColorType,
	is_16bit: bool,
	reader: png::Reader<R>,
	limits: Limits,
}


// Copied from: https://github.com/image-rs/image/blob/256dc9dd5501fa63cbf081a795a936c74c01abd9/src/codecs/png.rs
impl<R: BufRead + Seek> PngDecoder<R> {
	pub fn new(r: R) -> Result<PngDecoder<R>, Error> {
		Self::with_limits(r, Limits::no_limits())
	}

	pub fn with_limits(r: R, limits: Limits) -> Result<PngDecoder<R>, Error> {
		limits.check_support(&image::LimitSupport::default())?;

		let max_bytes = usize::try_from(limits.max_alloc.unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
		let mut decoder = png::Decoder::new_with_limits(r, png::Limits { bytes: max_bytes });
		decoder.set_ignore_text_chunk(false);

		let info = decoder.read_header_info()?;
		limits.check_dimensions(info.width, info.height)?;

		// By default the PNG decoder will scale 16 bpc to 8 bpc, so custom
		// transformations must be set. EXPAND preserves the default behavior
		// expanding bpc < 8 to 8 bpc.
		decoder.set_transformations(png::Transformations::EXPAND);
		let reader = decoder.read_info()?;
		let (color_type, bits) = reader.output_color_type();
		let color_type = match (color_type, bits) {
			(png::ColorType::Grayscale, png::BitDepth::Eight) => ColorType::L8,
			(png::ColorType::Grayscale, png::BitDepth::Sixteen) => ColorType::L16,
			(png::ColorType::GrayscaleAlpha, png::BitDepth::Eight) => ColorType::La8,
			(png::ColorType::GrayscaleAlpha, png::BitDepth::Sixteen) => ColorType::La16,
			(png::ColorType::Rgb, png::BitDepth::Eight) => ColorType::Rgb8,
			(png::ColorType::Rgb, png::BitDepth::Sixteen) => ColorType::Rgb16,
			(png::ColorType::Rgba, png::BitDepth::Eight) => ColorType::Rgba8,
			(png::ColorType::Rgba, png::BitDepth::Sixteen) => ColorType::Rgba16,

			(png::ColorType::Grayscale, png::BitDepth::One) => return Err(unsupported_color(ExtendedColorType::L1)),
			(png::ColorType::GrayscaleAlpha, png::BitDepth::One) => return Err(unsupported_color(ExtendedColorType::La1)),
			(png::ColorType::Rgb, png::BitDepth::One) => return Err(unsupported_color(ExtendedColorType::Rgb1)),
			(png::ColorType::Rgba, png::BitDepth::One) => return Err(unsupported_color(ExtendedColorType::Rgba1)),

			(png::ColorType::Grayscale, png::BitDepth::Two) => return Err(unsupported_color(ExtendedColorType::L2)),
			(png::ColorType::GrayscaleAlpha, png::BitDepth::Two) => return Err(unsupported_color(ExtendedColorType::La2)),
			(png::ColorType::Rgb, png::BitDepth::Two) => return Err(unsupported_color(ExtendedColorType::Rgb2)),
			(png::ColorType::Rgba, png::BitDepth::Two) => return Err(unsupported_color(ExtendedColorType::Rgba2)),

			(png::ColorType::Grayscale, png::BitDepth::Four) => return Err(unsupported_color(ExtendedColorType::L4)),
			(png::ColorType::GrayscaleAlpha, png::BitDepth::Four) => return Err(unsupported_color(ExtendedColorType::La4)),
			(png::ColorType::Rgb, png::BitDepth::Four) => return Err(unsupported_color(ExtendedColorType::Rgb4)),
			(png::ColorType::Rgba, png::BitDepth::Four) => return Err(unsupported_color(ExtendedColorType::Rgba4)),

			(png::ColorType::Indexed, bits) => return Err(unsupported_color(ExtendedColorType::Unknown(bits as u8))),
		};
		let is_16bit = matches!(bits, png::BitDepth::Sixteen);

		Ok(PngDecoder {
			color_type,
			reader,
			limits,
			is_16bit,
		})
	}

	/// Returns the gamma value of the image or None if no gamma value is indicated.
	///
	/// If an sRGB chunk is present this method returns a gamma value of 0.45455 and ignores the
	/// value in the gAMA chunk. This is the recommended behavior according to the PNG standard:
	///
	/// > When the sRGB chunk is present, [...] decoders that recognize the sRGB chunk but are not
	/// > capable of colour management are recommended to ignore the gAMA and cHRM chunks, and use
	/// > the values given above as if they had appeared in gAMA and cHRM chunks.
	pub fn gamma_value(&self) -> Result<Option<f64>, Error> {
		Ok(self.reader.info().source_gamma.map(|x| f64::from(x.into_scaled()) / 100_000.0))
	}

	/// Returns if the image contains an animation.
	///
	/// Note that the file itself decides if the default image is considered to be part of the
	/// animation. When it is not the common interpretation is to use it as a thumbnail.
	///
	/// If a non-animated image is converted into an `ApngDecoder` then its iterator is empty.
	pub fn is_animated(&self) -> bool {
		self.reader.info().is_animated()
	}

	/// Returns true if the image is 16 bits per channel.
	pub fn is_16bit(&self) -> bool {
		self.is_16bit
	}
}


impl<R: BufRead + Seek> ImageDecoder for PngDecoder<R> {
	fn dimensions(&self) -> (u32, u32) {
		self.reader.info().size()
	}

	fn color_type(&self) -> ColorType {
		self.color_type
	}

	fn icc_profile(&mut self) -> ImageResult<Option<Vec<u8>>> {
		Ok(self.reader.info().icc_profile.as_ref().map(|x| x.to_vec()))
	}

	fn exif_metadata(&mut self) -> ImageResult<Option<Vec<u8>>> {
		Ok(self.reader.info().exif_metadata.as_ref().map(|x| x.to_vec()))
	}

	fn xmp_metadata(&mut self) -> ImageResult<Option<Vec<u8>>> {
		if let Some(mut itx_chunk) = self.reader.info().utf8_text.iter().find(|chunk| chunk.keyword.contains(XMP_KEY)).cloned() {
			itx_chunk.decompress_text().map_err(error_from_png)?;
			return itx_chunk.get_text().map(|text| Some(text.as_bytes().to_vec())).map_err(error_from_png);
		}
		Ok(None)
	}

	fn iptc_metadata(&mut self) -> ImageResult<Option<Vec<u8>>> {
		if let Some(mut text_chunk) = self
			.reader
			.info()
			.compressed_latin1_text
			.iter()
			.find(|chunk| IPTC_KEYS.iter().any(|key| chunk.keyword.contains(key)))
			.cloned()
		{
			text_chunk.decompress_text().map_err(error_from_png)?;
			return text_chunk.get_text().map(|text| Some(text.as_bytes().to_vec())).map_err(error_from_png);
		}

		if let Some(text_chunk) = self
			.reader
			.info()
			.uncompressed_latin1_text
			.iter()
			.find(|chunk| IPTC_KEYS.iter().any(|key| chunk.keyword.contains(key)))
			.cloned()
		{
			return Ok(Some(text_chunk.text.into_bytes()));
		}
		Ok(None)
	}

	fn read_image(mut self, buf: &mut [u8]) -> ImageResult<()> {
		use byteorder_lite::{BigEndian, ByteOrder, NativeEndian};

		assert_eq!(u64::try_from(buf.len()), Ok(self.total_bytes()));
		self.reader.next_frame(buf).map_err(error_from_png)?;
		// PNG images are big endian. For 16 bit per channel and larger types,
		// the buffer may need to be reordered to native endianness per the
		// contract of `read_image`.
		// TODO: assumes equal channel bit depth.
		let bpc = self.color_type().bytes_per_pixel() / self.color_type().channel_count();

		match bpc {
			1 => (), // No reodering necessary for u8
			2 => buf.chunks_exact_mut(2).for_each(|c| {
				let v = BigEndian::read_u16(c);
				NativeEndian::write_u16(c, v);
			}),
			_ => unreachable!(),
		}
		Ok(())
	}

	fn read_image_boxed(self: Box<Self>, buf: &mut [u8]) -> ImageResult<()> {
		(*self).read_image(buf)
	}

	fn set_limits(&mut self, limits: Limits) -> ImageResult<()> {
		limits.check_support(&image::LimitSupport::default())?;
		let info = self.reader.info();
		limits.check_dimensions(info.width, info.height)?;
		self.limits = limits;
		// TODO: add `png::Reader::change_limits()` and call it here
		// to also constrain the internal buffer allocations in the PNG crate
		Ok(())
	}
}


fn unsupported_color(ect: ExtendedColorType) -> Error {
	Error::Unsupported(UnsupportedError::from_format_and_kind(
		ImageFormat::Png.into(),
		UnsupportedErrorKind::Color(ect),
	))
}


fn error_from_png(err: png::DecodingError) -> ImageError {
	match err {
		png::DecodingError::IoError(err) => ImageError::IoError(err),
		err @ png::DecodingError::Format(_) => ImageError::Decoding(DecodingError::new(ImageFormat::Png.into(), err)),
		err @ png::DecodingError::Parameter(_) => ImageError::Parameter(ParameterError::from_kind(ParameterErrorKind::Generic(err.to_string()))),
		png::DecodingError::LimitsExceeded => ImageError::Limits(LimitError::from_kind(LimitErrorKind::InsufficientMemory)),
	}
}
