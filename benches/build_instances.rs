use criterion::{criterion_group, criterion_main, Criterion};
use cosmic_text::{fontdb, FontSystem};
use petruterm::config::schema::FontConfig;
use petruterm::font::TextShaper;
use petruterm::renderer::atlas::GlyphAtlas;
use petruterm::renderer::cell::{CellVertex, FLAG_COLOR_GLYPH};
use rustc_hash::FxHasher;
use std::hash::{Hash, Hasher};

const COLS: usize = 80;
const ROWS: usize = 24;
const DEFAULT_BG: [f32; 4] = [0.012, 0.012, 0.012, 1.0];
const WHITE_FG: [f32; 4] = [0.9, 0.9, 0.9, 1.0];

/// Synthetic code-like rows for the benchmark grid.
const SAMPLE_ROWS: [&str; 8] = [
    "fn main() {                                                                    ",
    "    let config = Config::load().unwrap_or_default();                           ",
    "    let event_loop = EventLoop::new()?;                                        ",
    "    event_loop.set_control_flow(ControlFlow::Poll);                            ",
    "    let mut app = App::new(config);                                            ",
    "    event_loop.run_app(&mut app)?;                                             ",
    "    Ok(())                                                                     ",
    "}                                                                              ",
];

fn create_headless_wgpu() -> (wgpu::Device, wgpu::Queue) {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::None,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .expect("headless wgpu adapter unavailable");
        adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .expect("headless wgpu device unavailable")
    })
}

fn make_shaper() -> (TextShaper, FontConfig) {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let fallback_families = ["Menlo", "Courier New", "Courier", "Monaco"];
    let mut chosen_family: Option<String> = None;
    let mut chosen_id: Option<fontdb::ID> = None;
    let mut chosen_path: Option<std::path::PathBuf> = None;

    'outer: for candidate in &fallback_families {
        for face in db.faces() {
            if face.families.iter().any(|(name, _)| name.eq_ignore_ascii_case(candidate)) {
                chosen_family = face.families.first().map(|(n, _)| n.clone());
                chosen_id = Some(face.id);
                chosen_path = match &face.source {
                    fontdb::Source::File(p) => Some(p.clone()),
                    fontdb::Source::SharedFile(p, _) => Some(p.clone()),
                    _ => None,
                };
                break 'outer;
            }
        }
    }

    let family = chosen_family.unwrap_or_else(|| "Menlo".to_string());
    let font_id = chosen_id.unwrap_or_else(fontdb::ID::dummy);
    let font_path = chosen_path.unwrap_or_else(|| std::path::PathBuf::from("/dev/null"));

    let font_config = FontConfig {
        family: family.clone(),
        size: 14.0,
        line_height: 1.2,
        lcd_antialiasing: false,
        features: vec![],
        fallbacks: vec![],
        font_path: None,
    };

    let font_system = FontSystem::new_with_locale_and_db("en-US".to_string(), db);
    let shaper = TextShaper::new(None, font_system, family, font_id, font_path, 0, &font_config, None);
    (shaper, font_config)
}

fn make_colors(n: usize) -> Vec<([f32; 4], [f32; 4])> {
    vec![(WHITE_FG, DEFAULT_BG); n]
}

#[inline]
fn pack_color(c: [f32; 4]) -> u32 {
    let r = (c[0].clamp(0.0, 1.0) * 255.0) as u32;
    let g = (c[1].clamp(0.0, 1.0) * 255.0) as u32;
    let b = (c[2].clamp(0.0, 1.0) * 255.0) as u32;
    let a = (c[3].clamp(0.0, 1.0) * 255.0) as u32;
    (r << 24) | (g << 16) | (b << 8) | a
}

#[inline]
fn colors_approx_eq(a: [f32; 4], b: [f32; 4]) -> bool {
    pack_color(a) == pack_color(b)
}

fn row_hash(text: &str, colors: &[([f32; 4], [f32; 4])]) -> u64 {
    let mut h = FxHasher::default();
    text.hash(&mut h);
    for (fg, bg) in colors {
        pack_color(*fg).hash(&mut h);
        pack_color(*bg).hash(&mut h);
    }
    h.finish()
}

/// Build vertex instances for one row (cache miss path).
/// Returns the CellVertex list and a precomputed hash for cache storage.
fn build_row_vertices(
    text: &str,
    colors: &[([f32; 4], [f32; 4])],
    font: &FontConfig,
    shaper: &mut TextShaper,
    atlas: &mut GlyphAtlas,
    queue: &wgpu::Queue,
) -> (u64, Vec<CellVertex>) {
    let hash = row_hash(text, colors);
    let shaped = shaper.shape_line(text, colors, font);
    let cell_height = shaper.cell_height;
    let mut verts: Vec<CellVertex> = Vec::new();

    // BG pre-pass: emit background-only vertices for cells with non-default bg.
    for (col, (_, bg)) in colors.iter().enumerate() {
        if colors_approx_eq(*bg, DEFAULT_BG) { continue; }
        verts.push(CellVertex {
            grid_pos: [col as f32, 0.0],
            atlas_uv: [0.0; 4],
            fg: [0.0; 4],
            bg: *bg,
            glyph_offset: [0.0; 2],
            glyph_size: [0.0; 2],
            flags: 0,
            _pad: 0,
        });
    }

    // Glyph pass.
    for glyph in &shaped.glyphs {
        let key = glyph.cache_key;
        let ch = glyph.ch;
        let col = glyph.col;
        let fg = glyph.fg;
        let bg = glyph.bg;

        if ch == ' ' && colors_approx_eq(bg, DEFAULT_BG) { continue; }

        let Ok(se) = shaper.rasterize_to_atlas(key, atlas, queue) else { continue };

        let ox = se.bearing_x as f32;
        let oy = shaped.ascent - se.bearing_y as f32;
        let gw = se.width as f32;
        let gh = se.height as f32;
        let y0 = oy.max(0.0);
        let y1 = (oy + gh).min(cell_height);
        let flag = if se.is_color { FLAG_COLOR_GLYPH } else { 0 };

        let (atlas_uv, glyph_offset, glyph_size) = if y1 > y0 && gw > 0.0 && gh > 0.0 {
            let [u0, v0, u1, v1] = se.uv;
            let fy0 = (y0 - oy) / gh;
            let fy1 = (y1 - oy) / gh;
            ([u0, v0 + fy0 * (v1 - v0), u1, v0 + fy1 * (v1 - v0)], [ox, y0], [gw, y1 - y0])
        } else {
            ([0.0f32; 4], [0.0f32; 2], [0.0f32; 2])
        };

        verts.push(CellVertex {
            grid_pos: [col as f32, 0.0],
            atlas_uv,
            fg,
            bg,
            glyph_offset,
            glyph_size,
            flags: flag,
            _pad: 0,
        });
    }

    (hash, verts)
}

/// Apply pane offset to a slice of cached CellVertex and append to output.
fn apply_row_offset(cached: &[CellVertex], col_offset: f32, row: f32, out: &mut Vec<CellVertex>) {
    for inst in cached {
        let mut v = *inst;
        v.grid_pos[0] += col_offset;
        v.grid_pos[1] = row;
        out.push(v);
    }
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

/// Single row, row-cache miss, atlas warm (word cache warm after first iter).
/// Measures: shape_line + atlas lookup + vertex construction.
fn bench_build_row_miss(c: &mut Criterion) {
    let (device, queue) = create_headless_wgpu();
    let mut atlas = GlyphAtlas::new(&device);
    let (mut shaper, font_config) = make_shaper();
    let text = SAMPLE_ROWS[0];
    let colors = make_colors(text.chars().count());

    // Prime atlas and word cache.
    build_row_vertices(text, &colors, &font_config, &mut shaper, &mut atlas, &queue);

    c.bench_function("build_row_miss", |b| {
        b.iter(|| build_row_vertices(text, &colors, &font_config, &mut shaper, &mut atlas, &queue));
    });
}

/// Single row, row-cache hit.
/// Measures: hash check + Vec<CellVertex> copy + offset.
fn bench_build_row_hit(c: &mut Criterion) {
    let (device, queue) = create_headless_wgpu();
    let mut atlas = GlyphAtlas::new(&device);
    let (mut shaper, font_config) = make_shaper();
    let text = SAMPLE_ROWS[0];
    let colors = make_colors(text.chars().count());
    let (hash, cached) = build_row_vertices(text, &colors, &font_config, &mut shaper, &mut atlas, &queue);
    let expected_hash = row_hash(text, &colors);

    let mut out: Vec<CellVertex> = Vec::new();
    c.bench_function("build_row_hit", |b| {
        b.iter(|| {
            out.clear();
            if row_hash(text, &colors) == expected_hash {
                std::hint::black_box(hash); // suppress unused
                apply_row_offset(&cached, 0.0, 0.0, &mut out);
            }
            out.len()
        });
    });
}

/// Full 24×80 frame, all row-cache misses, atlas warm.
/// Represents continuous typing or scrolling — every row is re-shaped each frame.
fn bench_build_frame_miss(c: &mut Criterion) {
    let (device, queue) = create_headless_wgpu();
    let mut atlas = GlyphAtlas::new(&device);
    let (mut shaper, font_config) = make_shaper();

    let rows: Vec<&str> = (0..ROWS).map(|i| SAMPLE_ROWS[i % SAMPLE_ROWS.len()]).collect();
    let colors = make_colors(COLS);

    // Prime atlas and word cache.
    for &text in &rows {
        build_row_vertices(text, &colors, &font_config, &mut shaper, &mut atlas, &queue);
    }

    let mut out: Vec<CellVertex> = Vec::with_capacity(COLS * ROWS);
    c.bench_function("build_frame_miss", |b| {
        b.iter(|| {
            out.clear();
            for (row_idx, &text) in rows.iter().enumerate() {
                let (_, verts) = build_row_vertices(text, &colors, &font_config, &mut shaper, &mut atlas, &queue);
                apply_row_offset(&verts, 0.0, row_idx as f32, &mut out);
            }
            out.len()
        });
    });
}

/// Full 24×80 frame, all row-cache hits.
/// Represents a static terminal (nothing changed) — minimal work per frame.
fn bench_build_frame_hit(c: &mut Criterion) {
    let (device, queue) = create_headless_wgpu();
    let mut atlas = GlyphAtlas::new(&device);
    let (mut shaper, font_config) = make_shaper();

    let rows: Vec<&str> = (0..ROWS).map(|i| SAMPLE_ROWS[i % SAMPLE_ROWS.len()]).collect();
    let colors = make_colors(COLS);

    // Pre-build row cache.
    let row_cache: Vec<(u64, Vec<CellVertex>)> = rows
        .iter()
        .map(|&text| build_row_vertices(text, &colors, &font_config, &mut shaper, &mut atlas, &queue))
        .collect();
    let row_hashes: Vec<u64> = rows.iter().map(|&text| row_hash(text, &colors)).collect();

    let mut out: Vec<CellVertex> = Vec::with_capacity(COLS * ROWS);
    c.bench_function("build_frame_hit", |b| {
        b.iter(|| {
            out.clear();
            for (row_idx, ((hash, cached), expected)) in row_cache.iter().zip(&row_hashes).enumerate() {
                if hash == expected {
                    apply_row_offset(cached, 0.0, row_idx as f32, &mut out);
                }
            }
            out.len()
        });
    });
}

criterion_group!(
    benches,
    bench_build_row_miss,
    bench_build_row_hit,
    bench_build_frame_miss,
    bench_build_frame_hit,
);
criterion_main!(benches);
