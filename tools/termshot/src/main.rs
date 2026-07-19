//! termshot — render `tmux capture-pane -e` output to PNG/GIF.
//!
//! The README media pipeline and the 1.0.6 CI screenshot grid: drive the TUI
//! in a tmux pane, capture frames *with* escape sequences, and rasterize them
//! here with a Nerd Font so the images look exactly like a terminal — no
//! vhs/ttyd/browser stack required.
//!
//! `--grid` additionally tiles every frame into one contact sheet, which is
//! how CI shows all 16 rail screens in a single artifact.
//!
//! ```text
//! termshot --bg '#111c18' --fg '#c1c497' --px 20 --out docs/assets \
//!          --gif docs/assets/tour.gif frames/01.txt:1500 frames/02.txt:900 …
//! ```
//!
//! Each positional arg is `capture.txt[:delay_ms]`; every frame becomes
//! `<out>/<stem>.png`, `--gif` assembles all frames into an animated GIF with
//! the given per-frame delays, and `--grid` tiles them into a contact sheet.
//!
//! Font paths default to the Omarchy-stock JetBrainsMono Nerd Font; override
//! with `TERMSHOT_FONT`, `TERMSHOT_FONT_BOLD` and `TERMSHOT_FONT_SYMBOLS`
//! (CI has no Nerd Font and falls back to DejaVu Sans Mono).

use std::collections::HashMap;
use std::path::PathBuf;

const FONT_REGULAR: &str = "/usr/share/fonts/TTF/JetBrainsMonoNerdFontMono-Regular.ttf";
const FONT_BOLD: &str = "/usr/share/fonts/TTF/JetBrainsMonoNerdFontMono-Bold.ttf";
/// Symbols the Nerd Font lacks (✦ ☀ …) come from here.
const FONT_SYMBOLS: &str = "/usr/share/fonts/noto/NotoSansSymbols2-Regular.ttf";

/// Standard xterm palette for the 16 named ANSI colors.
const ANSI16: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 49, 49),
    (13, 188, 121),
    (229, 229, 16),
    (36, 114, 200),
    (188, 63, 188),
    (17, 168, 205),
    (229, 229, 229),
    (102, 102, 102),
    (241, 76, 76),
    (35, 209, 139),
    (245, 245, 67),
    (59, 142, 234),
    (214, 112, 214),
    (41, 184, 219),
    (255, 255, 255),
];

#[derive(Clone, Copy, PartialEq)]
struct Style {
    fg: Option<(u8, u8, u8)>,
    bg: Option<(u8, u8, u8)>,
    bold: bool,
    dim: bool,
    reverse: bool,
}

impl Style {
    fn reset() -> Self {
        Style {
            fg: None,
            bg: None,
            bold: false,
            dim: false,
            reverse: false,
        }
    }
}

#[derive(Clone, Copy)]
struct Cell {
    ch: char,
    style: Style,
}

fn xterm256(n: u8) -> (u8, u8, u8) {
    match n {
        0..=15 => ANSI16[n as usize],
        16..=231 => {
            let n = n - 16;
            let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
            (level(n / 36), level((n / 6) % 6), level(n % 6))
        }
        _ => {
            let v = 8 + (n - 232) * 10;
            (v, v, v)
        }
    }
}

/// Apply one SGR parameter list to the pen.
fn apply_sgr(params: &[u16], style: &mut Style) {
    let mut i = 0;
    while i < params.len() {
        match params[i] {
            0 => *style = Style::reset(),
            1 => style.bold = true,
            2 => style.dim = true,
            7 => style.reverse = true,
            22 => {
                style.bold = false;
                style.dim = false;
            }
            27 => style.reverse = false,
            30..=37 => style.fg = Some(ANSI16[(params[i] - 30) as usize]),
            90..=97 => style.fg = Some(ANSI16[(params[i] - 90 + 8) as usize]),
            40..=47 => style.bg = Some(ANSI16[(params[i] - 40) as usize]),
            100..=107 => style.bg = Some(ANSI16[(params[i] - 100 + 8) as usize]),
            39 => style.fg = None,
            49 => style.bg = None,
            38 | 48 => {
                let is_fg = params[i] == 38;
                if params.get(i + 1) == Some(&2) && i + 4 < params.len() {
                    let c = (
                        params[i + 2] as u8,
                        params[i + 3] as u8,
                        params[i + 4] as u8,
                    );
                    if is_fg {
                        style.fg = Some(c);
                    } else {
                        style.bg = Some(c);
                    }
                    i += 4;
                } else if params.get(i + 1) == Some(&5) && i + 2 < params.len() {
                    let c = xterm256(params[i + 2] as u8);
                    if is_fg {
                        style.fg = Some(c);
                    } else {
                        style.bg = Some(c);
                    }
                    i += 2;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

/// Parse one `capture-pane -e` dump into a cols×rows cell grid.
fn parse(text: &str, cols: usize, rows: usize) -> Vec<Vec<Cell>> {
    let blank = Cell {
        ch: ' ',
        style: Style::reset(),
    };
    let mut grid = vec![vec![blank; cols]; rows];
    let mut style = Style::reset();
    for (row, line) in text.lines().take(rows).enumerate() {
        let mut col = 0usize;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                // CSI … final-byte; only SGR (`m`) changes the pen
                if chars.peek() == Some(&'[') {
                    chars.next();
                    let mut buf = String::new();
                    for c in chars.by_ref() {
                        if c.is_ascii_alphabetic() {
                            if c == 'm' {
                                let params: Vec<u16> = if buf.is_empty() {
                                    vec![0]
                                } else {
                                    buf.split([';', ':'])
                                        .map(|p| p.parse().unwrap_or(0))
                                        .collect()
                                };
                                apply_sgr(&params, &mut style);
                            }
                            break;
                        }
                        buf.push(c);
                    }
                }
                continue;
            }
            if col < cols {
                grid[row][col] = Cell { ch: c, style };
                col += 1;
            }
        }
    }
    grid
}

struct Renderer {
    regular: fontdue::Font,
    bold: fontdue::Font,
    symbols: Option<fontdue::Font>,
    px: f32,
    cell_w: u32,
    cell_h: u32,
    ascent: f32,
    default_fg: (u8, u8, u8),
    default_bg: (u8, u8, u8),
    cache: HashMap<(char, bool), (fontdue::Metrics, Vec<u8>)>,
}

impl Renderer {
    fn new(px: f32, default_fg: (u8, u8, u8), default_bg: (u8, u8, u8)) -> Self {
        let load = |path: &str| {
            let env_path;
            let path = if path == FONT_REGULAR {
                env_path = std::env::var("TERMSHOT_FONT").unwrap_or_else(|_| path.into());
                &env_path
            } else {
                env_path = std::env::var("TERMSHOT_FONT_BOLD").unwrap_or_else(|_| path.into());
                &env_path
            };
            let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("font {path}: {e}"));
            fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).expect("parse font")
        };
        let regular = load(FONT_REGULAR);
        let bold = load(FONT_BOLD);
        let symbols_path =
            std::env::var("TERMSHOT_FONT_SYMBOLS").unwrap_or_else(|_| FONT_SYMBOLS.into());
        let symbols = std::fs::read(symbols_path).ok().and_then(|bytes| {
            fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default()).ok()
        });
        let m = regular.metrics('M', px);
        let line = regular
            .horizontal_line_metrics(px)
            .expect("horizontal metrics");
        Self {
            cell_w: m.advance_width.round() as u32,
            cell_h: (line.ascent - line.descent + line.line_gap).round() as u32,
            ascent: line.ascent,
            regular,
            bold,
            symbols,
            px,
            default_fg,
            default_bg,
            cache: HashMap::new(),
        }
    }

    fn frame(&mut self, grid: &[Vec<Cell>]) -> image::RgbaImage {
        let (rows, cols) = (grid.len() as u32, grid[0].len() as u32);
        // small margin so the content doesn't kiss the image edge
        let margin = self.cell_w;
        let (w, h) = (
            cols * self.cell_w + margin * 2,
            rows * self.cell_h + margin * 2,
        );
        let bg = self.default_bg;
        let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([bg.0, bg.1, bg.2, 255]));

        for (r, row) in grid.iter().enumerate() {
            for (c, cell) in row.iter().enumerate() {
                let st = cell.style;
                let mut fg = st.fg.unwrap_or(self.default_fg);
                let mut cbg = st.bg.unwrap_or(self.default_bg);
                if st.reverse {
                    std::mem::swap(&mut fg, &mut cbg);
                }
                if st.dim {
                    fg = (
                        fg.0 / 2 + cbg.0 / 2,
                        fg.1 / 2 + cbg.1 / 2,
                        fg.2 / 2 + cbg.2 / 2,
                    );
                }
                let x0 = margin + c as u32 * self.cell_w;
                let y0 = margin + r as u32 * self.cell_h;
                if cbg != self.default_bg {
                    for y in y0..y0 + self.cell_h {
                        for x in x0..x0 + self.cell_w {
                            img.put_pixel(x, y, image::Rgba([cbg.0, cbg.1, cbg.2, 255]));
                        }
                    }
                }
                if cell.ch == ' ' {
                    continue;
                }
                let px = self.px;
                let (metrics, coverage) = self
                    .cache
                    .entry((cell.ch, st.bold))
                    .or_insert_with(|| {
                        let main = if st.bold { &self.bold } else { &self.regular };
                        let font = if main.lookup_glyph_index(cell.ch) != 0 {
                            main
                        } else {
                            self.symbols
                                .as_ref()
                                .filter(|f| f.lookup_glyph_index(cell.ch) != 0)
                                .unwrap_or(main)
                        };
                        font.rasterize(cell.ch, px)
                    })
                    .clone();
                let gx = x0 as i64 + metrics.xmin as i64;
                let gy =
                    y0 as i64 + (self.ascent as i64) - metrics.ymin as i64 - metrics.height as i64;
                for (i, cov) in coverage.iter().enumerate() {
                    if *cov == 0 {
                        continue;
                    }
                    let x = gx + (i % metrics.width) as i64;
                    let y = gy + (i / metrics.width) as i64;
                    if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                        continue;
                    }
                    let base = img.get_pixel(x as u32, y as u32).0;
                    let a = *cov as u32;
                    let blend = |f: u8, b: u8| ((f as u32 * a + b as u32 * (255 - a)) / 255) as u8;
                    img.put_pixel(
                        x as u32,
                        y as u32,
                        image::Rgba([
                            blend(fg.0, base[0]),
                            blend(fg.1, base[1]),
                            blend(fg.2, base[2]),
                            255,
                        ]),
                    );
                }
            }
        }
        img
    }
}

/// Tile equal-sized frames into one contact sheet, `cols` per row.
///
/// The CI screenshot grid renders every rail screen at once, so a reviewer can
/// eyeball all of them from a single artifact instead of trusting that a
/// screen still draws.
fn grid(frames: &[image::RgbaImage], cols: u32, gap: u32, bg: (u8, u8, u8)) -> image::RgbaImage {
    let (fw, fh) = (frames[0].width(), frames[0].height());
    let cols = cols.min(frames.len() as u32).max(1);
    let rows = (frames.len() as u32).div_ceil(cols);
    let w = cols * fw + gap * (cols + 1);
    let h = rows * fh + gap * (rows + 1);
    let mut sheet = image::RgbaImage::from_pixel(w, h, image::Rgba([bg.0, bg.1, bg.2, 255]));
    for (i, frame) in frames.iter().enumerate() {
        let (c, r) = (i as u32 % cols, i as u32 / cols);
        let x = gap + c * (fw + gap);
        let y = gap + r * (fh + gap);
        image::imageops::overlay(&mut sheet, frame, x as i64, y as i64);
    }
    sheet
}

fn hex(s: &str) -> (u8, u8, u8) {
    let s = s.trim_start_matches('#');
    (
        u8::from_str_radix(&s[0..2], 16).unwrap(),
        u8::from_str_radix(&s[2..4], 16).unwrap(),
        u8::from_str_radix(&s[4..6], 16).unwrap(),
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cols = 110usize;
    let mut rows = 32usize;
    let mut px = 20.0f32;
    let mut fg = (0xc1, 0xc4, 0x97);
    let mut bg = (0x11, 0x1c, 0x18);
    let mut out_dir = PathBuf::from(".");
    let mut gif: Option<PathBuf> = None;
    let mut grid_out: Option<PathBuf> = None;
    let mut grid_cols = 4u32;
    let mut frames: Vec<(PathBuf, u32)> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--cols" => {
                i += 1;
                cols = args[i].parse().unwrap();
            }
            "--rows" => {
                i += 1;
                rows = args[i].parse().unwrap();
            }
            "--px" => {
                i += 1;
                px = args[i].parse().unwrap();
            }
            "--fg" => {
                i += 1;
                fg = hex(&args[i]);
            }
            "--bg" => {
                i += 1;
                bg = hex(&args[i]);
            }
            "--out" => {
                i += 1;
                out_dir = PathBuf::from(&args[i]);
            }
            "--gif" => {
                i += 1;
                gif = Some(PathBuf::from(&args[i]));
            }
            "--grid" => {
                i += 1;
                grid_out = Some(PathBuf::from(&args[i]));
            }
            "--grid-cols" => {
                i += 1;
                grid_cols = args[i].parse().unwrap();
            }
            frame => {
                let (path, delay) = match frame.rsplit_once(':') {
                    Some((p, d)) if d.chars().all(|c| c.is_ascii_digit()) => {
                        (p.to_string(), d.parse().unwrap())
                    }
                    _ => (frame.to_string(), 1000),
                };
                frames.push((PathBuf::from(path), delay));
            }
        }
        i += 1;
    }
    if frames.is_empty() {
        eprintln!(
            "usage: termshot [--cols N --rows N --px F --fg #hex --bg #hex --out DIR \
             --gif FILE --grid FILE --grid-cols N] capture.txt[:delay_ms] …"
        );
        std::process::exit(2);
    }

    std::fs::create_dir_all(&out_dir).unwrap();
    let mut renderer = Renderer::new(px, fg, bg);
    let mut rendered: Vec<(image::RgbaImage, u32)> = Vec::new();
    for (path, delay) in &frames {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
        let img = renderer.frame(&parse(&text, cols, rows));
        let png = out_dir.join(format!(
            "{}.png",
            path.file_stem().unwrap().to_string_lossy()
        ));
        img.save(&png).unwrap();
        println!("wrote {}", png.display());
        rendered.push((img, *delay));
    }

    if let Some(grid_path) = grid_out {
        let sheet = grid(
            &rendered
                .iter()
                .map(|(img, _)| img.clone())
                .collect::<Vec<_>>(),
            grid_cols,
            px.round() as u32,
            bg,
        );
        if let Some(parent) = grid_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        sheet.save(&grid_path).unwrap();
        println!("wrote {}", grid_path.display());
    }

    if let Some(gif_path) = gif {
        let file = std::fs::File::create(&gif_path).unwrap();
        let mut enc = image::codecs::gif::GifEncoder::new_with_speed(file, 10);
        enc.set_repeat(image::codecs::gif::Repeat::Infinite)
            .unwrap();
        for (img, delay) in rendered {
            let frame =
                image::Frame::from_parts(img, 0, 0, image::Delay::from_numer_denom_ms(delay, 1));
            enc.encode_frame(frame).unwrap();
        }
        println!("wrote {}", gif_path.display());
    }
}
