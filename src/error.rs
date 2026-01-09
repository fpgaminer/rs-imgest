use std::error::Error as StdError;

pub enum Error {
	UnsupportedFormat,
	IoError(std::io::Error),
	Animated,
	PngDecodingError(png::DecodingError),
	TooBig, // Image exceeds hard limits
	DecodingError(image::error::DecodingError),
	Parameter(image::error::ParameterError),
	Limits(image::error::LimitError),
	Unsupported(image::error::UnsupportedError),
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Self {
		Error::IoError(err)
	}
}

impl From<png::DecodingError> for Error {
	fn from(err: png::DecodingError) -> Self {
		match err {
			png::DecodingError::IoError(io_err) => return Error::IoError(io_err),
			_ => Error::PngDecodingError(err),
		}
	}
}

impl From<image::ImageError> for Error {
	fn from(err: image::ImageError) -> Self {
		match err {
			image::ImageError::IoError(io_err) => Error::IoError(io_err),
			image::ImageError::Decoding(err) => Error::DecodingError(err),
			image::ImageError::Parameter(err) => Error::Parameter(err),
			image::ImageError::Limits(err) => Error::Limits(err),
			image::ImageError::Unsupported(err) => Error::Unsupported(err),
			image::ImageError::Encoding(err) => panic!("Encoding error: {}", err), // This shouldn't happen
		}
	}
}

impl StdError for Error {}

impl std::fmt::Display for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Error::UnsupportedFormat => write!(f, "unsupported image format"),
			Error::IoError(err) => write!(f, "I/O error: {}", err),
			Error::Animated => write!(f, "animated images are not supported"),
			Error::PngDecodingError(err) => write!(f, "PNG decoding error: {}", err),
			Error::TooBig => write!(f, "image exceeds size limits"),
			Error::DecodingError(err) => write!(f, "decoding error: {}", err),
			Error::Parameter(err) => write!(f, "parameter error: {}", err),
			Error::Limits(err) => write!(f, "limits error: {}", err),
			Error::Unsupported(err) => write!(f, "unsupported error: {}", err),
		}
	}
}

impl std::fmt::Debug for Error {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(self, f)
	}
}
