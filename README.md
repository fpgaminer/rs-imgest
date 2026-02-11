# rs-imgest

A Rust library that wraps around the `image` crate and provides two main features:

* The versions of all image related dependencies are pinned to specific revisions that have been exhaustively tested to ensure near parity with Pillow's decoding.
* (WIP) Automatically applying image orientation and other metadata-based transformations, so that the output image data is always in a consistent format regardless of the input image's metadata. e.g. sRGB ready for display


Testing occurs on a large swath of real-world images, including many that are known to be problematic for decoders.  Each is checked against Pillow's output to ensure that the image crate is decoding correctly.  (A few bugs have been caught and upstreamed to zune-jpeg this way!)




## Supported Formats
* PNG
* JPEG
* GIF
* BMP
* WEBP
* Anything else that the `image` crate supports.


NOTE: Make sure to use the virtual env when running tests (.venv).