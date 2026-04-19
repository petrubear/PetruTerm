use cosmic_text::{fontdb, FontSystem, SwashCache, SwashContent};
use criterion::{criterion_group, criterion_main, Criterion};
use petruterm::config::schema::FontConfig;
use petruterm::font::{TextShaper, TextShaperConfig};

fn make_shaper() -> (TextShaper, FontConfig) {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let fallback_families = ["Menlo", "Courier New", "Courier", "Monaco"];
    let mut chosen_family: Option<String> = None;
    let mut chosen_id: Option<fontdb::ID> = None;
    let mut chosen_path: Option<std::path::PathBuf> = None;

    'outer: for candidate in &fallback_families {
        for face in db.faces() {
            if face
                .families
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(candidate))
            {
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
    let shaper = TextShaper::new(
        None,
        font_system,
        TextShaperConfig {
            actual_family: family,
            font_id,
            font_path,
            face_index: 0,
            font_config: &font_config,
            lcd_atlas: None,
        },
    );
    (shaper, font_config)
}

fn make_colors(n: usize) -> Vec<([f32; 4], [f32; 4])> {
    vec![([1.0f32; 4], [0.0, 0.0, 0.0, 1.0]); n]
}

/// Rasterize one glyph via swash (uncached) and convert to RGBA.
/// Returns pixel count so the optimizer can't elide the work.
fn rasterize_one(
    swash: &mut SwashCache,
    font_system: &mut FontSystem,
    key: cosmic_text::CacheKey,
) -> usize {
    let Some(image) = swash.get_image_uncached(font_system, key) else {
        return 0;
    };
    let rgba: Vec<u8> = match image.content {
        SwashContent::Mask => image.data.iter().flat_map(|&a| [a, a, a, 255u8]).collect(),
        SwashContent::Color => image.data.to_vec(),
        SwashContent::SubpixelMask => image.data.iter().flat_map(|&a| [a, a, a, 255u8]).collect(),
    };
    rgba.len()
}

/// Single ASCII glyph — measures the cost of one swash rasterization + Mask→RGBA.
fn bench_rasterize_glyph_ascii(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let colors = make_colors(1);
    let run = shaper.shape_line("A", &colors, &font_config);
    let key = match run.glyphs.first() {
        Some(g) => g.cache_key,
        None => return,
    };

    c.bench_function("rasterize_glyph_ascii", |b| {
        b.iter(|| rasterize_one(&mut shaper.swash_cache, &mut shaper.font_system, key));
    });
}

/// All glyphs from a typical code line — measures throughput across a full row.
fn bench_rasterize_line_ascii(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let text = "fn hello_world() -> &str {";
    let colors = make_colors(text.chars().count());
    let run = shaper.shape_line(text, &colors, &font_config);
    let keys: Vec<_> = run.glyphs.iter().map(|g| g.cache_key).collect();

    c.bench_function("rasterize_line_ascii", |b| {
        b.iter(|| {
            keys.iter()
                .map(|&k| rasterize_one(&mut shaper.swash_cache, &mut shaper.font_system, k))
                .sum::<usize>()
        });
    });
}

/// Ligature-heavy line — same chars as above but with HarfBuzz shaping paths.
fn bench_rasterize_line_ligatures(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let text = "let result = if x >= 0 { x } else { -x };";
    let colors = make_colors(text.chars().count());
    let run = shaper.shape_line(text, &colors, &font_config);
    let keys: Vec<_> = run.glyphs.iter().map(|g| g.cache_key).collect();

    c.bench_function("rasterize_line_ligatures", |b| {
        b.iter(|| {
            keys.iter()
                .map(|&k| rasterize_one(&mut shaper.swash_cache, &mut shaper.font_system, k))
                .sum::<usize>()
        });
    });
}

/// Unicode line — exercises swash's coverage of non-ASCII glyphs.
fn bench_rasterize_line_unicode(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let text = "// café résumé naïve fiancée";
    let colors = make_colors(text.chars().count());
    let run = shaper.shape_line(text, &colors, &font_config);
    let keys: Vec<_> = run.glyphs.iter().map(|g| g.cache_key).collect();

    c.bench_function("rasterize_line_unicode", |b| {
        b.iter(|| {
            keys.iter()
                .map(|&k| rasterize_one(&mut shaper.swash_cache, &mut shaper.font_system, k))
                .sum::<usize>()
        });
    });
}

criterion_group!(
    benches,
    bench_rasterize_glyph_ascii,
    bench_rasterize_line_ascii,
    bench_rasterize_line_ligatures,
    bench_rasterize_line_unicode,
);
criterion_main!(benches);
