use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use futures_util::StreamExt as _;
use indicatif::{ProgressBar, ProgressStyle};
use log::info;
use pyo3::{
	Py, PyErr, PyResult, Python,
	sync::PyOnceLock,
	types::{PyAnyMethods as _, PyBytes, PyBytesMethods as _, PyModule},
};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};


static PIL_IMAGE_MODULE: PyOnceLock<Py<PyModule>> = PyOnceLock::new();

// Generally speaking PNGs should always be exact matches since the format is lossless and well defined.
// But we compare using 8-bit RGBA, so if the image is 16-bit per channel originally there can be subtle differences in the 16->8 bit conversion.
// Only in those cases do we allow a small tolerance.
const PNG_AVG_DIFF_LIMIT: f64 = 0.17;
const PNG_MAX_DIFF_LIMIT: i64 = 1;


#[tokio::test]
async fn sweep_test() -> Result<()> {
	// Initialize logging
	let split = SplitWriter::new("test_loading_image.log")?;
	env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
		.default_format()
		.target(env_logger::Target::Pipe(Box::new(split)))
		.try_init()?;

	// Get Python pool ready
	//let python_pool = PythonPool::new("python3", Path::new("tests/decoder_worker.py"), 16).await
	//	.context("failed to create Python decoding pool")?;

	// Fetch all image paths from bigasp database
	let mut paths = fetch_paths().await?;

	// Truncate to 10,000 images for testing
	paths.truncate(10_000);

	let pb = add_progress_bar(paths.len() as u64, "images", "Testing image loading...");
	let pb_for_tasks = pb.clone();
	futures::stream::iter(paths)
		.for_each_concurrent(16, move |path| {
			let pb = pb_for_tasks.clone();
			async move {
				test_loading_image(path).await;
				pb.inc(1);
			}
		})
		.await;

	pb.finish_and_clear();
	info!("Completed image loading sweep test in {} seconds", pb.elapsed().as_secs_f32());

	Ok(())
}


/// Fetch all image paths from bigasp database, sorted by filehash for deterministic order
async fn fetch_paths() -> Result<Vec<PathBuf>> {
	let mut paths = Vec::new();
	let pb = add_spinner("images", "Loading paths...");
	let pool = connect_to_bigasp_db().await?;

	let mut rows = sqlx::query_scalar::<_, String>("SELECT path FROM images ORDER BY filehash").fetch(&pool);

	while let Some(path) = rows.next().await {
		let path = path.context("failed to fetch image path from database")?;
		paths.push(PathBuf::from(path));
		pb.inc(1);
	}

	pb.finish_and_clear();
	info!("Loaded {} image paths from bigasp in {} seconds", paths.len(), pb.elapsed().as_secs_f32());
	Ok(paths)
}


async fn test_loading_image(path: PathBuf) {
	// Load with Rust decoder
	let path_clone = path.clone();
	let Ok(res) = tokio::task::spawn_blocking(move || imgest::load_image(&path_clone)).await else {
		log::error!("spawn_blocking task panicked");
		return;
	};

	let img = match res {
		Ok(img) => img,
		Err(e) => {
			log::error!("IMG_FAIL: Failed to load image at path {:?}: {:?}", path, e);
			return;
		},
	};

	// Load using Pillow
	let (python_w, python_h, python_data) = match python_decode_image(path.clone()).await {
		Ok(data) => data,
		Err(e) => {
			log::error!("IMG_FAIL: Python decoding failed for image at path {:?}: {:?}", path, e);
			return;
		},
	};

	if img.width() != python_w || img.height() != python_h {
		log::error!(
			"IMG_FAIL: Image dimension mismatch at path {:?}: Rust decoder gave {}x{}, Python decoder gave {}x{}",
			path,
			img.width(),
			img.height(),
			python_w,
			python_h
		);
		return;
	}

	// Compare pixel data
	let Ok(res) = tokio::task::spawn_blocking(move || {
		// For PNGs the limits are strict except in the case of 16-bit per channel images where we allow a small tolerance for 16->8 bit conversion differences
		let is_16bit = img.color().bits_per_pixel() == 16 * (img.color().channel_count() as u16);
		let img = img.into_rgba8();
		let max_diff = if is_16bit { PNG_MAX_DIFF_LIMIT } else { 0 };
		let avg_diff_limit = if is_16bit { PNG_AVG_DIFF_LIMIT } else { 0.0 };

		let rust_data = img.into_raw();
		if rust_data.len() != python_data.len() {
			return Err(format!(
				"data length mismatch: Rust decoder gave {}, Python decoder gave {}",
				rust_data.len(),
				python_data.len()
			));
		}

		if rust_data != python_data {
			let mut diff_sum = 0;
			let mut diff_max = 0;
			for (b1, b2) in rust_data.iter().zip(python_data.iter()) {
				let diff = (*b1 as i64 - *b2 as i64).abs();
				diff_sum += diff;
				diff_max = diff_max.max(diff);
			}
			let avg_diff = diff_sum as f64 / rust_data.len() as f64;

			// Only fail if the differences exceed the limits
			if avg_diff > avg_diff_limit || diff_max > max_diff {
				return Err(format!(
					"pixel data mismatch, average difference per byte: {}, max difference: {}",
					avg_diff, diff_max
				));
			}
		}

		Ok(())
	})
	.await
	else {
		log::error!("spawn_blocking task panicked");
		return;
	};

	if let Err(msg) = res {
		log::error!("IMG_FAIL: Image data mismatch at path {:?}: {}", path, msg);
	}

	log::info!("IMG_OK: Successfully loaded and verified image at path {:?}", path);
}


async fn connect_to_bigasp_db() -> anyhow::Result<sqlx::PgPool> {
	let opts = PgConnectOptions::new()
		.username("postgres")
		.database("postgres")
		.host("/home/night/sdxl-big-asp/pg-socket");

	PgPoolOptions::new()
		.max_connections(5)
		.connect_with(opts)
		.await
		.context("failed to connect to PostgreSQL")
}


fn add_spinner(unit: &str, message: &str) -> ProgressBar {
	let pb = ProgressBar::new_spinner();
	pb.set_style(
		ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {pos} {prefix} ({per_sec} {prefix}/s) {msg}").expect("valid spinner style template"),
	);
	pb.set_prefix(unit.to_string());
	pb.set_message(message.to_string());
	pb.enable_steady_tick(std::time::Duration::from_millis(100));
	pb
}


fn add_progress_bar(len: u64, unit: &str, message: &str) -> ProgressBar {
	let pb = ProgressBar::new(len);
	pb.set_style(
		ProgressStyle::with_template(
			"[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {prefix} \
			 ({percent}%, {per_sec} {prefix}/s, ETA {eta}) {msg}",
		)
		.expect("valid progress bar template"),
	);
	pb.set_prefix(unit.to_string());
	pb.set_message(message.to_string());
	pb.enable_steady_tick(std::time::Duration::from_millis(100));
	pb
}


struct SplitWriter {
	file: std::sync::Mutex<std::io::BufWriter<std::fs::File>>,
}

impl SplitWriter {
	fn new(log_path: &str) -> anyhow::Result<Self> {
		let f = std::fs::OpenOptions::new().create(true).write(true).truncate(true).open(log_path)?;
		Ok(Self {
			file: std::sync::Mutex::new(std::io::BufWriter::new(f)),
		})
	}
}

impl std::io::Write for SplitWriter {
	fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
		let mut f = self.file.lock().unwrap();
		f.write_all(buf)?;

		let mut err = std::io::stderr();
		err.write_all(buf)?;

		Ok(buf.len())
	}

	fn flush(&mut self) -> std::io::Result<()> {
		// Flush both to be safe
		{
			let mut f = self.file.lock().unwrap();
			f.flush()?;
		}
		std::io::stderr().flush()
	}
}


fn decode_with_pillow(py: Python<'_>, path: &Path) -> PyResult<(u32, u32, Vec<u8>)> {
	let pil = PIL_IMAGE_MODULE.get_or_try_init(py, || py.import("PIL.Image").map(|m| m.unbind()))?.bind(py);

	// Open image
	let image = pil.call_method1("open", (path.to_string_lossy().as_ref(),))?;

	// Convert to RGBA8
	let image = image.call_method1("convert", ("RGBA",))?;

	// (width, height)
	let (width, height): (u32, u32) = image.getattr("size")?.extract()?;

	// Get raw bytes as Bound<PyBytes>
	let bytes = image.call_method0("tobytes")?;
	let bytes = bytes.cast::<PyBytes>()?;

	let data = bytes.as_bytes().to_vec();

	Ok((width, height, data))
}


pub async fn python_decode_image(path: PathBuf) -> Result<(u32, u32, Vec<u8>), PyErr> {
	let join_handle = tokio::task::spawn_blocking(move || {
		// Attach this blocking thread to the Python interpreter and run Pillow
		Python::attach(|py| decode_with_pillow(py, &path))
	});

	let py_result: PyResult<_> = join_handle.await.expect("blocking task panicked");
	Ok(py_result?)
}


#[tokio::test]
async fn test_png_16bit_detection() -> Result<()> {
	let img_16bit =
		imgest::load_image("/home/night/deep-raid/datasets/boorus/originals/00/09/0009cab25a5c4abc950d9c11f1476f2fde602a61fb8dd1b9d333d8f432cafb23")?;
	println!(
		"Debug: bytes_per_pixel={}, has_alpha={}, has_color={}, bits_per_pixel={}, channel_count={}",
		img_16bit.color().bytes_per_pixel(),
		img_16bit.color().has_alpha(),
		img_16bit.color().has_color(),
		img_16bit.color().bits_per_pixel(),
		img_16bit.color().channel_count()
	);
	assert_eq!(img_16bit.color().bits_per_pixel(), 16 * (img_16bit.color().channel_count() as u16));

	Ok(())
}
