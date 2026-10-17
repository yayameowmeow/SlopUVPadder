# Note

This was entirely slopped out by Claude Sonnet 5.5. I have no fucking idea what im doing but this shit is 52x faster and far far far less ram usage than the program its copying.

Below is whatever description Claude wanted to make. Enjoy.

# uvpad

Fast, low-memory texture dilating (UV padding) tool, written in Rust.

Padding pushes the colors of a texture's opaque areas outward into the transparent space around them, so mipmapping and texture filtering don't bleed the empty background into UV islands.

This is a Rust port of [meir/uvpad](https://github.com/meir/uvpad), which is based on the [GIMP uvpad plugin](https://polycount.com/discussion/114616/uvpadder-filter-for-gimp-2-6).

## Installation

```
cargo install --path .
```

## Usage

```
uvpad ./image.png
```

This writes `./image_padded.png` next to the input.

```
Usage: uvpad [OPTIONS] <INPUT>

Arguments:
  <INPUT>  Input PNG image

Options:
      --output <FILE>            Output image file [default: <input>_padded.<ext>]
      --slower                   Use the GIMP UVPad look (iterated neighbour averaging) instead of nearest-pixel fill
      --compression <COMPRESSION>  PNG compression of the output: fast is quickest, best is smallest [default: fast] [possible values: fast, default, best]
  -h, --help                     Print help
  -V, --version                  Print version
```

Input must be a PNG, and the output is always a PNG (RGB, since the result is fully opaque).

## Algorithms

- **Default:** every pixel that is not fully opaque takes the color of its *exact* nearest fully opaque pixel (Euclidean distance). It runs in linear time: one pass finds the nearest opaque pixel within each column, then a per-row lower-envelope pass picks the best column for every pixel.
- **`--slower` (GIMP UVPad):** each pass fills every unfilled pixel that touches an opaque one with the average color of its opaque neighbors. The result is identical to the plugin's, but it is computed layer by layer with a frontier, so it visits each pixel once instead of once per pass.

If the image has no fully opaque pixel there is nothing to pad from, and uvpad exits with an error.

## Performance notes

- **Memory:** the decoded image is the only large allocation, at 4 bytes per pixel (about 570 MB for 11000x13000). The per-pixel "nearest opaque row" is kept in the unused RGB bytes of transparent pixels, so there is no second buffer.
- **Speed:** work is split into cache-sized bands of rows and spread over all cores. Padded rows are streamed into the PNG encoder while later bands are still being computed, and `--compression fast` (the default) trades some file size for encoding speed. Decoding the input PNG is single-threaded and, for very large images, is likely the largest remaining cost.
- **16-bit PNGs** are reduced to 8 bits first, so a pixel counts as opaque when its 16-bit alpha is at least `0xff00`.
- Height is limited to 16,777,214 rows.
