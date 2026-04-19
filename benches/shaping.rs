use cosmic_text::{fontdb, FontSystem};
use criterion::{criterion_group, criterion_main, Criterion};
use petruterm::config::schema::FontConfig;
use petruterm::font::TextShaper;

/// Build a TextShaper with system fonts and no GPU device.
/// Uses the first monospace family found on the system.
fn make_shaper() -> (TextShaper, FontConfig) {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    // Pick the first available monospace family from system fonts.
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
        family,
        font_id,
        font_path,
        0,
        &font_config,
        None,
    );

    (shaper, font_config)
}

fn make_colors(n: usize) -> Vec<([f32; 4], [f32; 4])> {
    (0..n)
        .map(|_| ([1.0f32, 1.0, 1.0, 1.0], [0.0, 0.0, 0.0, 1.0]))
        .collect()
}

fn bench_shape_line_ascii(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let text = "fn hello_world() -> &str {";
    let colors = make_colors(text.chars().count());

    c.bench_function("shape_line_ascii", |b| {
        b.iter(|| {
            let _ = shaper.shape_line(text, &colors, &font_config);
        });
    });
}

fn bench_shape_line_ligatures(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let text = "let result = if x >= 0 { x } else { -x };";
    let colors = make_colors(text.chars().count());

    c.bench_function("shape_line_ligatures", |b| {
        b.iter(|| {
            let _ = shaper.shape_line(text, &colors, &font_config);
        });
    });
}

fn bench_shape_line_unicode(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let text = "// café résumé naïve fiancée";
    let colors = make_colors(text.chars().count());

    c.bench_function("shape_line_unicode", |b| {
        b.iter(|| {
            let _ = shaper.shape_line(text, &colors, &font_config);
        });
    });
}

/// Warm the word cache with one call, then measure the second call (word-cache hit path).
fn bench_shape_line_ascii_cached(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let text = "fn hello_world() -> &str {";
    let colors = make_colors(text.chars().count());

    // Prime the word cache (this call goes through HarfBuzz or ASCII fast path).
    let _ = shaper.shape_line(text, &colors, &font_config);

    c.bench_function("shape_line_ascii_cached", |b| {
        b.iter(|| {
            let _ = shaper.shape_line(text, &colors, &font_config);
        });
    });
}

/// Measure a line that has ligature chars but is otherwise ASCII — exercises the
/// word-cache path (per-word HarfBuzz + caching) rather than the full-line path.
fn bench_shape_line_ligatures_cached(c: &mut Criterion) {
    let (mut shaper, font_config) = make_shaper();
    let text = "let result = if x >= 0 { x } else { -x };";
    let colors = make_colors(text.chars().count());

    // Prime the word cache.
    let _ = shaper.shape_line(text, &colors, &font_config);

    c.bench_function("shape_line_ligatures_cached", |b| {
        b.iter(|| {
            let _ = shaper.shape_line(text, &colors, &font_config);
        });
    });
}

criterion_group!(
    benches,
    bench_shape_line_ascii,
    bench_shape_line_ligatures,
    bench_shape_line_unicode,
    bench_shape_line_ascii_cached,
    bench_shape_line_ligatures_cached,
);
criterion_main!(benches);
