# Note

This was entirely slopped out by Claude Sonnet 5.5, and later Opus 5.5. I have no fucking idea what im doing but this shit is 52x faster and far far far less ram usage than the program its based on, Claude did a very good job.

I even feel bad calling it slop since it works so well lol.

Below is whatever description Claude wanted to write. Enjoy.

# uvpad

Fast, low-memory texture dilating (UV padding) tool, written in Rust.

Padding pushes the colors of a texture's opaque areas outward into the transparent space around them, so mipmapping and texture filtering don't bleed the empty background into UV islands.

This is a Rust port and reimagining of [meir/uvpad](https://github.com/meir/uvpad), which is originally based on the [GIMP uvpad plugin](https://polycount.com/discussion/114616/uvpadder-filter-for-gimp-2-6).

## Download

Ready-to-run builds are on the [releases page](https://github.com/yayameowmeow/SlopUVPadder/releases):

| Platform | Graphical app (also works as the CLI) | CLI only, single file |
|---|---|---|
| Linux x86_64 / ARM64 | `uvpad-…-linux-x86_64.AppImage`, `…-aarch64.AppImage` | `uvpad-cli-…-linux-….tar.gz` |
| Windows x64 / ARM64 | `uvpad-…-windows-x64.zip`, `…-arm64.zip` (portable: unzip and run `uvpad.exe`) | `uvpad-cli-…-windows-….zip` |
| macOS (Apple Silicon and Intel) | `uvpad-…-macos-universal.dmg` | `uvpad-cli-…-macos-universal.tar.gz` |

- **Linux:** `chmod +x uvpad-*.AppImage` and run it; it works on any distro as new as Ubuntu 22.04. The file dialogs come from the desktop through xdg-desktop-portal (KDE's dialog on Plasma).
- **Windows:** the builds are not code signed, so SmartScreen may warn on first start ("More info" > "Run anyway").
- **macOS:** the app is not notarized, so the first time right-click it and choose **Open**. Needs macOS 12 or newer.
- AVIF input is included everywhere except Windows.

## Building

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

### AVIF input

AVIF decoding uses the dav1d library, so it is a separate feature that needs dav1d and pkg-config at build time (Arch: `sudo pacman -S --needed dav1d pkgconf`):

```
cargo install --path . --features gui,avif
```

## Usage

```
uvpad ./image.png
```

This writes `./image_padded.png` next to the input.

```
Usage: uvpad [OPTIONS] [INPUT]

Arguments:
  [INPUT]  Input image: PNG, JPEG, JPEG XL, WebP, TGA, TIFF, BMP, GIF, EXR, HDR, DDS, QOI, ICO, PNM (and AVIF when built with `--features avif`). Without one, the window opens

Options:
      --gui                      Open the graphical interface (with INPUT already loaded, if given)
      --output <FILE>            Output PNG file [default: <input>_padded.png]
      --slower                   Use the GIMP UVPad look (iterated neighbour averaging) instead of nearest-pixel fill
      --compression <COMPRESSION>  PNG compression of the output: fast is quickest, best is smallest [default: fast] [possible values: fast, default, best]
  -h, --help                     Print help
  -V, --version                  Print version
```

The output is always a PNG (RGB, since the result is fully opaque).

### Input formats

PNG, JPEG, JPEG XL, WebP, TGA, TIFF, BMP, GIF, OpenEXR, Radiance HDR, DDS, QOI, ICO, PNM/PAM and farbfeld, plus AVIF with the `avif` feature. The format is detected from the file's content; only TGA, which has no signature, relies on the `.tga` extension.

- Padding fills transparent pixels, so formats without alpha (JPEG, most BMPs) come out unchanged.
- PNG has its own, fastest decoding path. Other formats are decoded with the [`image`](https://crates.io/crates/image) crate ([`jxl-oxide`](https://crates.io/crates/jxl-oxide) for JPEG XL) and converted to 8-bit RGBA, which can briefly need a second buffer for formats that are not 8-bit RGBA already.
- 16-bit and float images (EXR, HDR) are converted to 8 bits per channel without tone mapping, so values outside 0–1 are clipped.

### GUI

```
uvpad                    # empty window (also what double-clicking the app does)
uvpad --gui ./image.png  # opens with this image already padded
```

In builds without the GUI, `uvpad` with no input prints the usage instead.

Drag a texture in any of the input formats onto the window (or press Ctrl+O). It is padded straight away and the result is shown; save it with **Save…** (Ctrl+S), which opens the system file dialog with `<input>_padded.png` filled in, or put it on the clipboard with **Copy** (Ctrl+C) to paste straight into another program.

- **Method** switches between nearest-pixel and GIMP UVPad (`--slower`) and re-pads the image. This may be used to offer a different art style effect if preferred
- **Compression** applies to the saved file.
- **Copy** puts the padded image on the clipboard as a PNG, which image editors, browsers and chat apps accept. Textures up to 4096x4096 are also copied as a plain bitmap, for programs that only take those (like Paint). On Linux the clipboard empties when uvpad closes, unless a clipboard manager (like Plasma's Klipper) keeps it.
- **Input / Output** (or Space) flips between the original and the padded image at the same zoom.
- Textures larger than 4096 px are previewed downscaled to keep memory low. Tick **Full-resolution preview** (shown only for such textures) to see them at full size, up to 16384 px. Qt decodes the whole image while loading a preview (4 bytes per pixel, for input and output), so very large textures need a few GB of free memory. Saving always writes the full-resolution result.
- Mouse wheel or pinch zooms around the pointer, drag pans, double-click or `0` fits the image again. Pixels are shown unsmoothed when zoomed in, so the padded edges can be inspected.

The preview is written to the system temp directory and deleted when the window closes. Inputs that are not PNGs get a PNG preview there as well, so the window can show every format whatever Qt image plugins are installed. The style is Qt's Fusion, which follows the desktop's light/dark palette; set `QT_QUICK_CONTROLS_STYLE` to use another one. The file dialogs are the system ones: KDE's Dolphin on Plasma, and the native dialogs on Windows and macOS. On other Linux desktops set `QT_QPA_PLATFORMTHEME=xdgdesktopportal` (or `gtk3`) to get the desktop's dialog; otherwise Qt's own is used.

## Algorithms

- **Default:** every pixel that is not fully opaque takes the color of its *exact* nearest fully opaque pixel (Euclidean distance). It runs in linear time: one pass finds the nearest opaque pixel within each column, then a per-row lower-envelope pass picks the best column for every pixel. Fastest and best output for UV Padding.
- **`--slower` (GIMP UVPad):** each pass fills every unfilled pixel that touches an opaque one with the average color of its opaque neighbors. The result is identical to the plugin's, but it is computed layer by layer with a frontier, so it visits each pixel once instead of once per pass. This may be used to offer a different art style effect if preferred.

If the image has no fully opaque pixel there is nothing to pad from, and uvpad exits with an error.

## Performance notes

- **Memory:** the decoded image is the only large allocation, at 4 bytes per pixel. The per-pixel "nearest opaque row" is kept in the unused RGB bytes of transparent pixels, so there is no second buffer.
- **Speed:** work is split into cache-sized bands of rows and spread over all cores. Padded rows are streamed into the PNG encoder while later bands are still being computed, and `--compression fast` (the default) trades some file size for encoding speed. Decoding the input PNG is single-threaded and, for very large images, is likely the largest remaining cost.
- **16-bit PNGs** are reduced to 8 bits first, so a pixel counts as opaque when its 16-bit alpha is at least `0xff00`.
- Height is limited to 16,777,214 rows.

## Releasing

Pull requests build and smoke test every package (see `.github/workflows/release.yml`). To publish a release, bump `version` in `Cargo.toml`, then tag and push:

```
git tag v0.3.0
git push origin v0.3.0
```

The workflow uploads every build, with SHA-256 checksums, to a **draft** release; check it and press **Publish** on GitHub.

## License

uvpad is free software under the [GNU General Public License, version 3 or later](LICENSE). Release builds bundle Qt (LGPLv3), Rust crates and dav1d under their own licenses, listed in `THIRD-PARTY-NOTICES.txt` inside each download.

uvpad is an independent implementation inspired by [meir/uvpad](https://github.com/meir/uvpad) and the [GIMP UVPadder plugin](https://polycount.com/discussion/114616/uvpadder-filter-for-gimp-2-6); it contains no code from either.
