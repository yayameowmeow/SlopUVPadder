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

### With the Qt interface

The graphical interface is an optional feature, so the plain CLI never needs Qt. It needs Qt 6 (Base and Declarative/QML) and a C++ compiler at build time:

- **Arch Linux:** `sudo pacman -S --needed qt6-base qt6-declarative base-devel`
- **Debian/Ubuntu:** `sudo apt install qt6-base-dev qt6-declarative-dev qml6-module-qtquick-controls qml6-module-qtquick-dialogs qml6-module-qtquick-layouts qml6-module-qtquick-window`
- **macOS:** `brew install qt`
- **Windows:** install Qt 6 with the [Qt online installer](https://www.qt.io/download-qt-installer) (MSVC kit) and put its `bin` directory, which holds `qmake6.exe`, on `PATH`.

```
cargo install --path . --features gui
```

The build finds Qt through `qmake6`/`qmake` on `PATH`; set `QMAKE=/path/to/qmake6` to pick a specific install.

## Usage

```
uvpad ./image.png
```

This writes `./image_padded.png` next to the input.

```
Usage: uvpad [OPTIONS] <INPUT>

Arguments:
  <INPUT>  Input PNG image (optional with --gui)

Options:
      --gui                      Open the graphical interface: drop an image in, preview the result, save it
      --output <FILE>            Output image file [default: <input>_padded.<ext>]
      --slower                   Use the GIMP UVPad look (iterated neighbour averaging) instead of nearest-pixel fill
      --compression <COMPRESSION>  PNG compression of the output: fast is quickest, best is smallest [default: fast] [possible values: fast, default, best]
  -h, --help                     Print help
  -V, --version                  Print version
```

Input must be a PNG, and the output is always a PNG (RGB, since the result is fully opaque).

### GUI

```
uvpad --gui              # empty window
uvpad --gui ./image.png  # opens with this image already padded
```

Drag a PNG onto the window (or press Ctrl+O). It is padded straight away and the result is shown; save it with **Save…** (Ctrl+S), which opens the system file dialog with `<input>_padded.png` filled in.

- **Method** switches between nearest-pixel and GIMP UVPad (`--slower`) and re-pads the image.
- **Compression** applies to the saved file.
- **Input / Output** (or Space) flips between the original and the padded image at the same zoom.
- Textures larger than 4096 px are previewed downscaled to keep memory low. Tick **Full-resolution preview** (shown only for such textures) to see them at full size, up to 16384 px. Qt decodes the whole PNG while loading a preview (4 bytes per pixel, for input and output), so very large textures need a few GB of free memory. Saving always writes the full-resolution result.
- Mouse wheel or pinch zooms around the pointer, drag pans, double-click or `0` fits the image again. Pixels are shown unsmoothed when zoomed in, so the padded edges can be inspected.

The preview is written to the system temp directory and deleted when the window closes. The style is Qt's Fusion, which follows the desktop's light/dark palette; set `QT_QUICK_CONTROLS_STYLE` to use another one. On Linux the file dialogs are native when a Qt platform theme is active (Plasma, or `QT_QPA_PLATFORMTHEME=xdgdesktopportal` elsewhere); otherwise Qt's own dialog is used.

## Algorithms

- **Default:** every pixel that is not fully opaque takes the color of its *exact* nearest fully opaque pixel (Euclidean distance). It runs in linear time: one pass finds the nearest opaque pixel within each column, then a per-row lower-envelope pass picks the best column for every pixel.
- **`--slower` (GIMP UVPad):** each pass fills every unfilled pixel that touches an opaque one with the average color of its opaque neighbors. The result is identical to the plugin's, but it is computed layer by layer with a frontier, so it visits each pixel once instead of once per pass.

If the image has no fully opaque pixel there is nothing to pad from, and uvpad exits with an error.

## Performance notes

- **Memory:** the decoded image is the only large allocation, at 4 bytes per pixel (about 570 MB for 11000x13000). The per-pixel "nearest opaque row" is kept in the unused RGB bytes of transparent pixels, so there is no second buffer.
- **Speed:** work is split into cache-sized bands of rows and spread over all cores. Padded rows are streamed into the PNG encoder while later bands are still being computed, and `--compression fast` (the default) trades some file size for encoding speed. Decoding the input PNG is single-threaded and, for very large images, is likely the largest remaining cost.
- **16-bit PNGs** are reduced to 8 bits first, so a pixel counts as opaque when its 16-bit alpha is at least `0xff00`.
- Height is limited to 16,777,214 rows.
