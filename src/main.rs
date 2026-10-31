//! uvpad: texture dilating (UV padding) tool.
//!
//! A Rust port of <https://github.com/meir/uvpad>, tuned for speed and memory.
//!
//! * Default: every pixel that is not fully opaque takes the colour of its
//!   exact nearest fully opaque pixel (Euclidean distance transform, linear time).
//! * `--slower`: the GIMP uvpad plugin's look: repeated neighbour averaging,
//!   computed with a frontier instead of full-image passes.
//!
//! Memory: one RGBA8 buffer (4 bytes per pixel) plus small per-band scratch.
//! The padded rows are streamed into the PNG encoder as they are produced.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use rayon::prelude::*;

#[cfg(feature = "gui")]
mod gui;

/// Texture dilating tool
#[derive(Parser)]
#[command(name = "uvpad", version)]
struct Cli {
    /// Input image: PNG, JPEG, JPEG XL, WebP, TGA, TIFF, BMP, GIF, EXR, HDR, DDS, QOI, ICO, PNM
    /// (and AVIF when built with `--features avif`). Optional with --gui
    #[arg(required_unless_present = "gui")]
    input: Option<PathBuf>,

    /// Open the graphical interface: drop an image in, preview the result, save it
    #[arg(long)]
    gui: bool,

    /// Output PNG file [default: <input>_padded.png]
    #[arg(long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Use the GIMP UVPad look (iterated neighbour averaging) instead of nearest-pixel fill
    #[arg(long)]
    slower: bool,

    /// PNG compression of the output: fast is quickest, best is smallest
    #[arg(long, value_enum, default_value_t = Level::Fast)]
    compression: Level,
}

#[derive(Clone, Copy, ValueEnum)]
enum Level {
    Fast,
    Default,
    Best,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.gui {
        return launch_gui(cli.input);
    }
    let input = cli.input.expect("clap requires INPUT without --gui");
    let output = cli
        .output
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| default_output(&input));

    let start = Instant::now();
    run(&input, &output, cli.slower, cli.compression)?;
    println!("Execution time: {:?}", start.elapsed());
    println!("Saved padded image to {}", output.display());
    Ok(())
}

#[cfg(feature = "gui")]
fn launch_gui(input: Option<PathBuf>) -> Result<()> {
    gui::launch(input)
}

#[cfg(not(feature = "gui"))]
fn launch_gui(_input: Option<PathBuf>) -> Result<()> {
    bail!("this uvpad was built without the Qt interface; rebuild it with `--features gui`")
}

/// `dir/name.ext` -> `dir/name_padded.png`; the output is always a PNG.
fn default_output(input: &Path) -> PathBuf {
    let mut name = input.file_stem().unwrap_or_default().to_os_string();
    name.push("_padded.png");
    input.with_file_name(name)
}

/// The output is always a PNG, so refuse names that promise another format.
fn check_output(output: &Path) -> Result<()> {
    match output.extension() {
        Some(ext) if !ext.eq_ignore_ascii_case("png") => bail!(
            "the output is always a PNG, so {} needs a .png extension",
            output.display()
        ),
        _ => Ok(()),
    }
}

fn run(input: &Path, output: &Path, slower: bool, level: Level) -> Result<()> {
    check_output(output)?;
    pad(load(input)?, output, slower, level)
}

/// Pads `img` and writes it to `output` as an RGB PNG.
fn pad(mut img: Rgba, output: &Path, slower: bool, level: Level) -> Result<()> {
    let rows = band_rows(img.width);
    let (w, h) = (img.width, img.height);
    let rgb = png::ColorType::Rgb;
    let result = if slower {
        fill_gimp(&mut img)?;
        write_png(output, w, h, rows, level, rgb, |b, out| {
            rgb_band(&img, rows, b, out)
        })
    } else {
        column_nearest(&mut img, rows)?;
        write_png(output, w, h, rows, level, rgb, |b, out| {
            nearest_band(&img, rows, b, out)
        })
    };
    result.with_context(|| format!("failed to save output image {}", output.display()))
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// 8-bit RGBA, row-major. The only large allocation the tool makes.
struct Rgba {
    width: usize,
    height: usize,
    px: Vec<u8>,
}

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

/// The first bytes of a file (fewer if it is shorter).
fn magic(file: &mut File) -> io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(12);
    Read::take(&mut *file, 12).read_to_end(&mut head)?;
    file.rewind()?;
    Ok(head)
}

fn is_jxl(head: &[u8]) -> bool {
    // Bare codestream, or the ISO BMFF container.
    head.starts_with(&[0xFF, 0x0A])
        || head.starts_with(&[0, 0, 0, 0x0C, b'J', b'X', b'L', b' ', 0x0D, 0x0A, 0x87, 0x0A])
}

/// Whether `path` is a PNG, judged by its content.
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
fn is_png(path: &Path) -> bool {
    File::open(path)
        .and_then(|mut f| magic(&mut f))
        .is_ok_and(|head| head.starts_with(&PNG_SIGNATURE))
}

/// Decodes any supported format to 8-bit RGBA. The format is detected from the
/// content; the extension only matters for formats without a signature (TGA).
fn load(path: &Path) -> Result<Rgba> {
    let mut file = File::open(path)
        .with_context(|| format!("failed to open input file {}", path.display()))?;
    let head = magic(&mut file)
        .with_context(|| format!("failed to read input file {}", path.display()))?;
    let img = if head.starts_with(&PNG_SIGNATURE) {
        load_png(file)
    } else {
        load_other(file, &head, path)
    };
    img.with_context(|| format!("failed to decode input image {}", path.display()))
}

/// Everything but PNG, through the `image` crate (and `jxl-oxide` for JPEG XL).
/// 16-bit and float images are converted to 8 bits per channel.
fn load_other(file: File, head: &[u8], path: &Path) -> Result<Rgba> {
    let reader = BufReader::with_capacity(1 << 20, file);
    let img = if is_jxl(head) {
        image::DynamicImage::from_decoder(jxl_oxide::integration::JxlDecoder::new(reader)?)?
    } else {
        let mut reader = image::ImageReader::new(reader);
        if let Ok(format) = image::ImageFormat::from_path(path) {
            reader.set_format(format);
        }
        let mut reader = reader.with_guessed_format()?;
        if reader.format().is_none() {
            bail!("unsupported image format");
        }
        // The default limits refuse textures over 512 MB.
        reader.no_limits();
        reader.decode()?
    };
    // A no-op for RGBA8 images, which keeps the single-buffer memory profile.
    let rgba = img.into_rgba8();
    Ok(Rgba {
        width: rgba.width() as usize,
        height: rgba.height() as usize,
        px: rgba.into_raw(),
    })
}

/// 16-bit PNGs are reduced to 8 bits first, so alpha >= 0xff00 counts as opaque.
fn load_png(file: File) -> Result<Rgba> {
    let mut decoder = png::Decoder::new(BufReader::with_capacity(1 << 20, file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);

    let mut reader = decoder.read_info()?;
    let mut raw = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut raw)?;
    drop(reader);
    raw.truncate(info.buffer_size());

    // The common case (RGBA) is used as is, with no copy.
    let px = match info.color_type {
        png::ColorType::Rgba => raw,
        png::ColorType::Rgb => to_rgba(&raw, 3, |c| [c[0], c[1], c[2], 255]),
        png::ColorType::Grayscale => to_rgba(&raw, 1, |c| [c[0], c[0], c[0], 255]),
        png::ColorType::GrayscaleAlpha => to_rgba(&raw, 2, |c| [c[0], c[0], c[0], c[1]]),
        png::ColorType::Indexed => bail!("unexpected indexed PNG after palette expansion"),
    };
    Ok(Rgba {
        width: info.width as usize,
        height: info.height as usize,
        px,
    })
}

fn to_rgba(raw: &[u8], channels: usize, convert: impl Fn(&[u8]) -> [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() / channels * 4);
    for c in raw.chunks_exact(channels) {
        out.extend_from_slice(&convert(c));
    }
    out
}

// ---------------------------------------------------------------------------
// Nearest opaque pixel (default)
// ---------------------------------------------------------------------------
//
// Exact Euclidean nearest-seed transform in two passes, where a seed is a fully
// opaque pixel:
//
//   1. `column_nearest`: for every non-opaque pixel, the row of the nearest seed
//      in its own column. The row index is stored in the pixel's unused RGB
//      bytes, so no second buffer is needed. Rows are processed in bands.
//   2. `nearest_band`: per row, a lower envelope of parabolas over the columns
//      (Meijster et al.) picks the best column for every pixel, in O(width).
//
// The seeds themselves are never modified, so pass 2 only reads the buffer.

const NO_SEED: &str = "the image has no fully opaque pixels to pad from";

/// "No seed in this column". Also the largest storable row index plus one.
const NONE: u32 = 0x00FF_FFFF;

/// Target size of one band of rows, so a band stays in cache.
const BAND_BYTES: usize = 2 << 20;

fn band_rows(width: usize) -> usize {
    (BAND_BYTES / (width * 4)).clamp(8, 256)
}

#[inline]
fn get24(p: &[u8]) -> u32 {
    p[0] as u32 | (p[1] as u32) << 8 | (p[2] as u32) << 16
}

#[inline]
fn set24(p: &mut [u8], v: u32) {
    p[0] = v as u8;
    p[1] = (v >> 8) as u8;
    p[2] = (v >> 16) as u8;
}

/// Pass 1. Afterwards every non-opaque pixel holds, in its RGB bytes, the row of
/// the nearest opaque pixel in its column (or `NONE`).
fn column_nearest(img: &mut Rgba, rows: usize) -> Result<()> {
    let (w, h) = (img.width, img.height);
    if h >= NONE as usize {
        bail!("image is too tall (limit is {} rows)", NONE - 1);
    }
    let stride = rows * w * 4;
    let bands = h.div_ceil(rows);

    // First and last opaque row of every column within each band (read only).
    let mut first = vec![NONE; bands * w];
    let mut last = vec![NONE; bands * w];
    img.px
        .par_chunks(stride)
        .enumerate()
        .zip(first.par_chunks_mut(w))
        .zip(last.par_chunks_mut(w))
        .for_each(|(((b, band), f), l)| {
            for (i, row) in band.chunks_exact(w * 4).enumerate() {
                let y = (b * rows + i) as u32;
                for (x, p) in row.chunks_exact(4).enumerate() {
                    if p[3] == 255 {
                        if f[x] == NONE {
                            f[x] = y;
                        }
                        l[x] = y;
                    }
                }
            }
        });
    if last.iter().all(|&v| v == NONE) {
        bail!(NO_SEED);
    }

    // Turn them into "nearest opaque row above / below the band" carries.
    let mut run = vec![NONE; w];
    for chunk in last.chunks_mut(w) {
        for (r, l) in run.iter_mut().zip(chunk) {
            let own = *l;
            *l = *r;
            if own != NONE {
                *r = own;
            }
        }
    }
    let above = last;
    let mut run = vec![NONE; w];
    for chunk in first.chunks_mut(w).rev() {
        for (r, f) in run.iter_mut().zip(chunk) {
            let own = *f;
            *f = *r;
            if own != NONE {
                *r = own;
            }
        }
    }
    let below = first;

    img.px
        .par_chunks_mut(stride)
        .enumerate()
        .for_each(|(b, band)| {
            let y0 = b * rows;
            let n = band.len() / (w * 4);

            // Downwards: nearest opaque row above.
            let mut cur = above[b * w..(b + 1) * w].to_vec();
            for i in 0..n {
                let y = (y0 + i) as u32;
                for (x, p) in band[i * w * 4..(i + 1) * w * 4]
                    .chunks_exact_mut(4)
                    .enumerate()
                {
                    if p[3] == 255 {
                        cur[x] = y;
                    } else {
                        set24(p, cur[x]);
                    }
                }
            }

            // Upwards: compare with the nearest opaque row below, keep the closer.
            let mut nxt = below[b * w..(b + 1) * w].to_vec();
            for i in (0..n).rev() {
                let y = (y0 + i) as u32;
                for (x, p) in band[i * w * 4..(i + 1) * w * 4]
                    .chunks_exact_mut(4)
                    .enumerate()
                {
                    if p[3] == 255 {
                        nxt[x] = y;
                    } else {
                        let up = get24(p);
                        let down = nxt[x];
                        let best = if up == NONE {
                            down
                        } else if down == NONE || y - up <= down - y {
                            up
                        } else {
                            down
                        };
                        set24(p, best);
                    }
                }
            }
        });
    Ok(())
}

#[inline]
fn cost(x: i64, col: usize, g2: &[i64]) -> i64 {
    let d = x - col as i64;
    d * d + g2[col]
}

/// Pass 2 for one band: writes the padded RGB rows `b * rows ..` into `out`.
fn nearest_band(img: &Rgba, rows: usize, b: usize, out: &mut Vec<u8>) {
    let (w, h) = (img.width, img.height);
    let (y0, y1) = (b * rows, ((b + 1) * rows).min(h));
    out.clear();
    out.resize((y1 - y0) * w * 3, 0);

    // Columns without any seed get a cost no real column can reach.
    let inf2 = ((w + h) as i64) * ((w + h) as i64);
    let mut seed_row = vec![0u32; w];
    let mut g2 = vec![0i64; w];
    let mut start = vec![0usize; w]; // first x won by each stacked parabola
    let mut col = vec![0usize; w]; // the column of each stacked parabola

    for y in y0..y1 {
        let row = &img.px[y * w * 4..(y + 1) * w * 4];
        let orow = &mut out[(y - y0) * w * 3..(y - y0 + 1) * w * 3];

        let mut all_opaque = true;
        for (x, p) in row.chunks_exact(4).enumerate() {
            let r = if p[3] == 255 {
                y as u32
            } else {
                all_opaque = false;
                get24(p)
            };
            seed_row[x] = r;
            g2[x] = if r == NONE {
                inf2
            } else {
                let d = y as i64 - r as i64;
                d * d
            };
        }
        if all_opaque {
            for (o, p) in orow.chunks_exact_mut(3).zip(row.chunks_exact(4)) {
                o.copy_from_slice(&p[..3]);
            }
            continue;
        }

        // Lower envelope of the parabolas f_i(x) = (x - i)^2 + g2[i].
        let mut n = 1;
        col[0] = 0;
        start[0] = 0;
        for u in 1..w {
            while n > 0
                && cost(start[n - 1] as i64, col[n - 1], &g2) > cost(start[n - 1] as i64, u, &g2)
            {
                n -= 1;
            }
            if n == 0 {
                n = 1;
                col[0] = u;
                start[0] = 0;
            } else {
                let (i, uu) = (col[n - 1] as i64, u as i64);
                let num = uu * uu - i * i + g2[u] - g2[i as usize];
                // First integer x where parabola u is strictly better than i.
                let wpos = 1 + num.div_euclid(2 * (uu - i));
                if (0..w as i64).contains(&wpos) {
                    col[n] = u;
                    start[n] = wpos as usize;
                    n += 1;
                }
            }
        }

        // Every pixel of a segment shares one seed, so fetch its colour once.
        let mut end = w;
        for k in (0..n).rev() {
            let xs = col[k];
            let ys = seed_row[xs] as usize;
            debug_assert!(ys != NONE as usize);
            let at = (ys * w + xs) * 4;
            let colour = &img.px[at..at + 3];
            for o in orow[start[k] * 3..end * 3].chunks_exact_mut(3) {
                o.copy_from_slice(colour);
            }
            end = start[k];
        }
    }
}

// ---------------------------------------------------------------------------
// --slower: GIMP uvpad look
// ---------------------------------------------------------------------------
//
// Same result as the plugin's full-image passes: each pass fills every unfilled
// pixel that touches (4-neighbourhood) a filled one with the average of those
// neighbours. Layer by layer that is a breadth-first search, so only the
// frontier is visited: O(pixels) instead of O(pixels x passes).
//
// The alpha byte is the state: 255 filled, 1 queued, 0 not reached yet.

#[inline]
fn neighbours(x: usize, y: usize, w: usize, h: usize) -> [Option<usize>; 4] {
    [
        (x > 0).then(|| y * w + x - 1),
        (x + 1 < w).then(|| y * w + x + 1),
        (y > 0).then(|| (y - 1) * w + x),
        (y + 1 < h).then(|| (y + 1) * w + x),
    ]
}

fn fill_gimp(img: &mut Rgba) -> Result<()> {
    let (w, h) = (img.width, img.height);
    let rows = band_rows(w);
    let bands = h.div_ceil(rows);
    let px = &mut img.px;

    px.par_chunks_mut(1 << 20).for_each(|c| {
        for p in c.chunks_exact_mut(4) {
            if p[3] != 255 {
                p[3] = 0;
            }
        }
    });

    // First frontier: unfilled pixels that touch a filled one.
    let view = &px[..];
    let mut frontier: Vec<usize> = (0..bands)
        .into_par_iter()
        .map(|b| {
            let mut found = Vec::new();
            for y in b * rows..((b + 1) * rows).min(h) {
                for x in 0..w {
                    let i = y * w + x;
                    if view[i * 4 + 3] != 255
                        && neighbours(x, y, w, h)
                            .into_iter()
                            .flatten()
                            .any(|j| view[j * 4 + 3] == 255)
                    {
                        found.push(i);
                    }
                }
            }
            found
        })
        .collect::<Vec<_>>()
        .concat();

    if frontier.is_empty() {
        if px.chunks_exact(4).any(|p| p[3] != 255) {
            bail!(NO_SEED);
        }
        return Ok(());
    }
    for &i in &frontier {
        px[i * 4 + 3] = 1;
    }

    while !frontier.is_empty() {
        // Colours depend only on the previous layer, so compute them all first.
        let view = &px[..];
        let colours: Vec<[u8; 3]> = frontier
            .par_iter()
            .map(|&i| {
                let mut sum = [0u32; 3];
                let mut count = 0u32;
                for j in neighbours(i % w, i / w, w, h).into_iter().flatten() {
                    let p = &view[j * 4..j * 4 + 4];
                    if p[3] == 255 {
                        for (s, &v) in sum.iter_mut().zip(&p[..3]) {
                            *s += v as u32 * 257; // 8-bit -> 16-bit, as the original does
                        }
                        count += 1;
                    }
                }
                let count = count.max(1);
                [
                    ((sum[0] / count) >> 8) as u8,
                    ((sum[1] / count) >> 8) as u8,
                    ((sum[2] / count) >> 8) as u8,
                ]
            })
            .collect();

        for (&i, c) in frontier.iter().zip(&colours) {
            px[i * 4..i * 4 + 3].copy_from_slice(c);
            px[i * 4 + 3] = 255;
        }
        let mut next = Vec::new();
        for &i in &frontier {
            for j in neighbours(i % w, i / w, w, h).into_iter().flatten() {
                if px[j * 4 + 3] == 0 {
                    px[j * 4 + 3] = 1;
                    next.push(j);
                }
            }
        }
        frontier = next;
    }
    Ok(())
}

/// RGB rows of an already padded (fully opaque) image.
fn rgb_band(img: &Rgba, rows: usize, b: usize, out: &mut Vec<u8>) {
    let (w, h) = (img.width, img.height);
    let (y0, y1) = (b * rows, ((b + 1) * rows).min(h));
    out.clear();
    out.reserve((y1 - y0) * w * 3);
    for p in img.px[y0 * w * 4..y1 * w * 4].chunks_exact(4) {
        out.extend_from_slice(&p[..3]);
    }
}

// ---------------------------------------------------------------------------
// PNG output
// ---------------------------------------------------------------------------

/// Calls `produce(band, buf)` for every band on the rayon pool and writes the
/// buffers to `out` in band order, so encoding overlaps with computing.
fn pipeline<W: Write>(
    out: &mut W,
    bands: usize,
    produce: impl Fn(usize, &mut Vec<u8>) + Sync,
) -> io::Result<()> {
    let stop = AtomicBool::new(false);
    std::thread::scope(|s| {
        let (tx, rx) = mpsc::sync_channel::<(usize, Vec<u8>)>(rayon::current_num_threads() * 2);
        let (produce, stop) = (&produce, &stop);
        s.spawn(move || {
            (0..bands).into_par_iter().for_each_with(tx, |tx, b| {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let mut buf = Vec::new();
                produce(b, &mut buf);
                let _ = tx.send((b, buf));
            });
        });

        let mut pending = BTreeMap::new();
        let mut next = 0;
        for (i, buf) in rx {
            pending.insert(i, buf);
            while let Some(buf) = pending.remove(&next) {
                if let Err(e) = out.write_all(&buf) {
                    stop.store(true, Ordering::Relaxed);
                    return Err(e);
                }
                next += 1;
            }
        }
        Ok(())
    })
}

/// Writes `img` unchanged, alpha included (the GUI's preview of non-PNG inputs).
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
fn write_rgba(path: &Path, img: &Rgba, level: Level) -> Result<()> {
    let (w, h) = (img.width, img.height);
    let rows = band_rows(w);
    write_png(path, w, h, rows, level, png::ColorType::Rgba, |b, out| {
        out.clear();
        out.extend_from_slice(&img.px[b * rows * w * 4..((b + 1) * rows).min(h) * w * 4]);
    })
}

/// Encodes the rows `produce` makes, `rows` image rows per band, as 8-bit
/// `color` (RGB for padded images, which are always fully opaque).
fn write_png(
    path: &Path,
    width: usize,
    height: usize,
    rows: usize,
    level: Level,
    color: png::ColorType,
    produce: impl Fn(usize, &mut Vec<u8>) + Sync,
) -> Result<()> {
    let file = File::create(path).context("failed to create output file")?;
    let mut sink = BufWriter::with_capacity(1 << 20, file);
    {
        let mut encoder = png::Encoder::new(&mut sink, width as u32, height as u32);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(match level {
            Level::Fast => png::Compression::Fast,
            Level::Default => png::Compression::Default,
            Level::Best => png::Compression::Best,
        });
        encoder.set_filter(png::FilterType::Sub);
        let mut writer = encoder
            .write_header()
            .context("failed to write PNG header")?;
        let mut stream = writer
            .stream_writer()
            .context("failed to start PNG stream")?;
        pipeline(&mut stream, height.div_ceil(rows), produce).context("failed to encode PNG")?;
        stream.finish().context("failed to finish PNG")?;
    }
    sink.flush().context("failed to write output file")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// Random RGBA image with at least one opaque pixel. Non-opaque pixels carry
    /// random RGB noise, which must never leak into the output.
    fn random_image(rng: &mut Rng) -> Rgba {
        let w = 1 + rng.below(40) as usize;
        let h = 1 + rng.below(40) as usize;
        let density = 1 + rng.below(30);
        let mut px = Vec::with_capacity(w * h * 4);
        for _ in 0..w * h {
            let a = if rng.below(100) < density {
                255
            } else {
                rng.below(255) as u8
            };
            px.extend_from_slice(&[
                rng.below(256) as u8,
                rng.below(256) as u8,
                rng.below(256) as u8,
                a,
            ]);
        }
        let i = rng.below((w * h) as u64) as usize;
        px[i * 4 + 3] = 255;
        Rgba {
            width: w,
            height: h,
            px,
        }
    }

    fn run_nearest(img: &mut Rgba, rows: usize) -> Result<Vec<u8>> {
        column_nearest(img, rows)?;
        let mut out = Vec::new();
        for b in 0..img.height.div_ceil(rows) {
            let mut buf = Vec::new();
            nearest_band(img, rows, b, &mut buf);
            out.extend(buf);
        }
        Ok(out)
    }

    #[test]
    fn default_output_names() {
        assert_eq!(
            default_output(Path::new("a/b/tex.png")),
            Path::new("a/b/tex_padded.png")
        );
        assert_eq!(default_output(Path::new("tex")), Path::new("tex_padded.png"));
        assert_eq!(
            default_output(Path::new("a.b/tex")),
            Path::new("a.b/tex_padded.png")
        );
        assert_eq!(
            default_output(Path::new("photo.JPG")),
            Path::new("photo_padded.png")
        );
        assert_eq!(
            default_output(Path::new("x.tar.png")),
            Path::new("x.tar_padded.png")
        );
    }

    #[test]
    fn output_must_be_png() {
        assert!(check_output(Path::new("a/tex_padded.png")).is_ok());
        assert!(check_output(Path::new("TEX.PNG")).is_ok());
        assert!(check_output(Path::new("tex_padded")).is_ok());
        assert!(check_output(Path::new("tex.jpg")).is_err());
    }

    #[test]
    fn nearest_is_exact_on_random_images() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for _ in 0..400 {
            let mut img = random_image(&mut rng);
            let (w, h) = (img.width, img.height);
            let orig = img.px.clone();
            // Tiny bands exercise the carries between bands.
            let rows = 1 + rng.below(8) as usize;
            let out = run_nearest(&mut img, rows).unwrap();

            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    let got = [out[i * 3], out[i * 3 + 1], out[i * 3 + 2]];
                    if orig[i * 4 + 3] == 255 {
                        assert_eq!(got, [orig[i * 4], orig[i * 4 + 1], orig[i * 4 + 2]]);
                        continue;
                    }
                    // Colours of all seeds at the minimum distance (ties allowed).
                    let mut best = i64::MAX;
                    let mut colours: Vec<[u8; 3]> = Vec::new();
                    for sy in 0..h {
                        for sx in 0..w {
                            let j = sy * w + sx;
                            if orig[j * 4 + 3] != 255 {
                                continue;
                            }
                            let d = (x as i64 - sx as i64).pow(2) + (y as i64 - sy as i64).pow(2);
                            if d < best {
                                best = d;
                                colours.clear();
                            }
                            if d == best {
                                colours.push([orig[j * 4], orig[j * 4 + 1], orig[j * 4 + 2]]);
                            }
                        }
                    }
                    assert!(
                        colours.contains(&got),
                        "{w}x{h} rows={rows} pixel ({x},{y})"
                    );
                }
            }
        }
    }

    #[test]
    fn nearest_rejects_images_without_opaque_pixels() {
        let mut img = Rgba {
            width: 2,
            height: 1,
            px: vec![1, 2, 3, 0, 4, 5, 6, 128],
        };
        assert!(run_nearest(&mut img, 8).is_err());
    }

    /// The plugin's algorithm, written the slow obvious way.
    fn gimp_reference(px: &[u8], w: usize, h: usize) -> Vec<u8> {
        let mut cur = px.to_vec();
        loop {
            let mut next = cur.clone();
            let mut changed = false;
            for y in 0..h {
                for x in 0..w {
                    let i = (y * w + x) * 4;
                    if cur[i + 3] == 255 {
                        continue;
                    }
                    let mut sum = [0u32; 3];
                    let mut count = 0u32;
                    for j in neighbours(x, y, w, h).into_iter().flatten() {
                        if cur[j * 4 + 3] == 255 {
                            for c in 0..3 {
                                sum[c] += cur[j * 4 + c] as u32 * 257;
                            }
                            count += 1;
                        }
                    }
                    if count > 0 {
                        for c in 0..3 {
                            next[i + c] = ((sum[c] / count) >> 8) as u8;
                        }
                        next[i + 3] = 255;
                        changed = true;
                    }
                }
            }
            cur = next;
            if !changed {
                return cur;
            }
        }
    }

    #[test]
    fn gimp_matches_the_naive_algorithm() {
        let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
        for _ in 0..300 {
            let mut img = random_image(&mut rng);
            let (w, h) = (img.width, img.height);
            let expect = gimp_reference(&img.px, w, h);
            fill_gimp(&mut img).unwrap();
            for i in 0..w * h {
                assert_eq!(
                    img.px[i * 4..i * 4 + 3],
                    expect[i * 4..i * 4 + 3],
                    "{w}x{h} pixel {i}"
                );
                assert_eq!(img.px[i * 4 + 3], 255);
            }
        }
    }

    #[test]
    fn gimp_rejects_images_without_opaque_pixels() {
        let mut img = Rgba {
            width: 2,
            height: 1,
            px: vec![0; 8],
        };
        assert!(fill_gimp(&mut img).is_err());
    }

    #[test]
    fn gimp_leaves_opaque_images_alone() {
        let mut img = Rgba {
            width: 2,
            height: 1,
            px: vec![1, 2, 3, 255, 4, 5, 6, 255],
        };
        fill_gimp(&mut img).unwrap();
        assert_eq!(img.px, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn pipeline_writes_bands_in_order() {
        let mut out = Vec::new();
        pipeline(&mut out, 50, |b, buf| {
            buf.extend(std::iter::repeat(b as u8).take(b % 7 + 1))
        })
        .unwrap();
        let expect: Vec<u8> = (0..50)
            .flat_map(|b| std::iter::repeat(b as u8).take(b % 7 + 1))
            .collect();
        assert_eq!(out, expect);
    }
}
