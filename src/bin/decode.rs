fn main() {
	// Usage: decode <path to image file> <path to output raw file>
	let args: Vec<String> = std::env::args().collect();
	if args.len() != 3 {
		eprintln!("Usage: {} <input image path> <output raw path>", args[0]);
		std::process::exit(1);
	}

	let input_path = &args[1];
	let output_path = &args[2];

	let (format, img) = match imgest::load_image(input_path) {
		Ok(v) => v,
		Err(e) => {
			eprintln!("Failed to load image {}: {:?}", input_path, e);
			std::process::exit(1);
		}
	};

	let raw_data = img.to_rgba8().into_raw();
	std::fs::write(output_path, &raw_data).expect("Failed to write output raw file");
	println!("Successfully decoded image {:?} to raw RGBA8 format, wrote {} bytes to {}", format, raw_data.len(), output_path);
}
