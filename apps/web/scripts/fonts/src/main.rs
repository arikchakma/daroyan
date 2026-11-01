use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use hb_subset::{Blob, FontFace, SubsetInput, Tag};
use read_fonts::model::Font;
use read_fonts::{FontRef, TableProvider};
use unicode_properties::{GeneralCategory, UnicodeGeneralCategory};

mod variations;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Codepoints = BTreeSet<u32>;
type Metrics = Vec<(&'static str, f64)>;
type Fallback = (&'static Family, Metrics);

const GOOGLE_CSS: &str = "https://fonts.googleapis.com/css2?family=Inter:wght@400..700&family=Geist+Mono:wght@400..500&display=swap";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
const WIDTH_SAMPLE: &str = "The quick brown fox jumps over the lazy dog 0123456789 Daroyan servers uptime crons incidents";

const CORE_PUNCTUATION: [u32; 12] = [
    0x00A0, 0x00B7, 0x2009, 0x2013, 0x2014, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
    0x2026, 0x202F,
];
const SUBSTITUTED: [u32; 4] = [0x2010, 0x2011, 0x2028, 0x2029];

struct Family {
    name: &'static str,
    slug: &'static str,
    weight: (u16, u16),
    features: &'static [&'static [u8; 4]],
    fallback_local: &'static [&'static str],
    fallback_average_width: f64,
}

const FAMILIES: [Family; 2] = [
    Family {
        name: "Inter",
        slug: "inter",
        weight: (400, 700),
        features: &[b"ccmp", b"locl", b"calt", b"kern", b"mark", b"mkmk", b"tnum"],
        fallback_local: &["Arial", "ArialMT", "Liberation Sans", "Arimo"],
        fallback_average_width: 957.23 / 2048.0,
    },
    Family {
        name: "Geist Mono",
        slug: "geist-mono",
        weight: (400, 500),
        features: &[b"ccmp", b"locl", b"mark", b"mkmk"],
        fallback_local: &["Courier New", "CourierNewPSMT", "Liberation Mono", "Cousine"],
        fallback_average_width: 1229.0 / 2048.0,
    },
];

struct GoogleFace {
    subset: String,
    family: String,
    url: String,
    range: String,
}

struct Rule {
    family: &'static str,
    name: String,
    codepoints: Codepoints,
}

fn fetch(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url).header("User-Agent", USER_AGENT).call()?;
    Ok(response.into_body().read_to_vec()?)
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = &text[text.find(start)? + start.len()..];
    Some(&rest[..rest.find(end)?])
}

fn read_google_faces() -> Result<Vec<GoogleFace>> {
    let css = String::from_utf8(fetch(GOOGLE_CSS)?)?;
    css.split("/* ")
        .skip(1)
        .map(|block| {
            let face = || -> Option<GoogleFace> {
                Some(GoogleFace {
                    subset: block[..block.find(" */")?].to_owned(),
                    family: between(block, "font-family: '", "'")?.to_owned(),
                    url: between(block, "url(", ")")?.to_owned(),
                    range: between(block, "unicode-range: ", ";")?.to_owned(),
                })
            };
            face().ok_or_else(|| format!("unexpected Google Fonts CSS: {block}").into())
        })
        .collect()
}

fn parse_range(text: &str) -> Result<Codepoints> {
    let mut codepoints = Codepoints::new();
    for part in text.split(',') {
        let part = part.trim().trim_start_matches("U+");
        let (start, end) = part.split_once('-').unwrap_or((part, part));
        codepoints
            .extend(u32::from_str_radix(start, 16)?..=u32::from_str_radix(end, 16)?);
    }
    Ok(codepoints)
}

fn format_range(codepoints: &Codepoints) -> String {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &codepoint in codepoints {
        match runs.last_mut() {
            Some(run) if run.1 + 1 == codepoint => run.1 = codepoint,
            _ => runs.push((codepoint, codepoint)),
        }
    }
    runs.iter()
        .map(|&(start, end)| match start == end {
            true => format!("U+{start:04X}"),
            false => format!("U+{start:04X}-{end:04X}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn get_cmap(sfnt: &[u8]) -> Result<Codepoints> {
    let face = FontFace::new(Blob::from_bytes(sfnt)?)?;
    Ok(face.covered_codepoints()?.iter().map(u32::from).collect())
}

fn is_mark(codepoint: u32) -> bool {
    char::from_u32(codepoint)
        .is_some_and(|char| char.general_category() == GeneralCategory::NonspacingMark)
}

fn split_core(
    sfnt: &[u8],
    family: &Family,
    codepoints: &Codepoints,
) -> Result<(Codepoints, Codepoints)> {
    let base = Font::new(sfnt.to_vec(), 0).ok_or("unreadable font")?;
    let (low, high) = family.weight;
    let weights: BTreeSet<u16> = [low, (low + high) / 2, high].into();
    let fonts: Vec<Font> = weights
        .iter()
        .map(|&weight| {
            base.instance_builder()
                .variations([(read_fonts::types::Tag::new(b"wght"), f32::from(weight))])
                .build()
        })
        .collect();

    let shape = |font: &Font, text: &str| -> Vec<(u32, i32, i32, i32)> {
        let shaper = harfrust::ShaperFont::new(font);
        let mut buffer = harfrust::Buffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        harfrust::shape(&shaper, &mut buffer, harfrust::ShapeOptions::new())
            .expect("shapes text");
        buffer
            .glyph_infos()
            .iter()
            .zip(buffer.glyph_positions())
            .map(|(info, position)| {
                (info.glyph_id, position.x_advance, position.x_offset, position.y_offset)
            })
            .collect()
    };
    let interacts = |first: u32, second: u32| {
        let [first, second] = [first, second]
            .map(|codepoint| char::from_u32(codepoint).unwrap_or('\u{FFFD}'));
        let pair = format!("{first}{second}");
        fonts.iter().any(|font| {
            let mut apart = shape(font, &first.to_string());
            apart.extend(shape(font, &second.to_string()));
            shape(font, &pair) != apart
        })
    };

    let marks: Codepoints = codepoints
        .iter()
        .copied()
        .filter(|&codepoint| is_mark(codepoint))
        .collect();
    let mut core: Codepoints = codepoints
        .iter()
        .copied()
        .filter(|&codepoint| {
            ((0x20..0x7F).contains(&codepoint) || CORE_PUNCTUATION.contains(&codepoint))
                && !marks.contains(&codepoint)
        })
        .collect();
    let mut rest: Codepoints = codepoints - &core;
    rest = &rest - &marks;
    let mut checked = Codepoints::new();
    loop {
        let unchecked: Vec<u32> = core.difference(&checked).copied().collect();
        let moved: Codepoints = rest
            .iter()
            .copied()
            .filter(|&candidate| {
                unchecked.iter().any(|&member| {
                    interacts(candidate, member) || interacts(member, candidate)
                })
            })
            .collect();
        checked.extend(unchecked);
        if moved.is_empty() {
            return Ok((&core | &marks, &rest | &marks));
        }
        core.extend(&moved);
        rest = &rest - &moved;
    }
}

fn get_fallback_metrics(sfnt: &[u8], family: &Family) -> Result<Metrics> {
    let font = FontRef::new(sfnt)?;
    let units = f64::from(font.head()?.units_per_em());
    let cmap = font.cmap()?;
    let hmtx = font.hmtx()?;
    let mut total = 0.0;
    for char in WIDTH_SAMPLE.chars() {
        let glyph = cmap.map_codepoint(char).ok_or("sample character missing")?;
        total += f64::from(hmtx.advance(glyph).ok_or("missing advance")?);
    }
    let size = total
        / WIDTH_SAMPLE.chars().count() as f64
        / units
        / family.fallback_average_width;
    let hhea = font.hhea()?;
    Ok(vec![
        ("size-adjust", size),
        ("ascent-override", f64::from(hhea.ascender().to_i16()) / units / size),
        ("descent-override", -f64::from(hhea.descender().to_i16()) / units / size),
        ("line-gap-override", f64::from(hhea.line_gap().to_i16()) / units / size),
    ])
}

fn build_font(sfnt: &[u8], family: &Family, codepoints: &Codepoints) -> Result<Vec<u8>> {
    let mut input = SubsetInput::new()?;
    input.flags().remove_hinting();
    let mut features = input.layout_feature_tag_set();
    features.clear();
    for feature in family.features {
        features.insert(Tag::new(*feature));
    }
    drop(features);
    let mut unicodes = input.unicode_set();
    for &codepoint in codepoints {
        unicodes.insert(char::from_u32(codepoint).ok_or("invalid codepoint")?);
    }
    drop(unicodes);
    let face = FontFace::new(Blob::from_bytes(sfnt)?)?;
    let subset = input.subset_font(&face)?.underlying_blob().to_vec();
    let trimmed = variations::drop_below(&subset, b"wght", f64::from(family.weight.0))?;
    woofwoof::compress(&trimmed, Vec::new(), 11, true)
        .ok_or_else(|| "woff2 encoding failed".into())
}

fn build_faces(out_dir: &Path) -> Result<(Vec<Rule>, Vec<Fallback>)> {
    fs::create_dir_all(out_dir)?;
    for entry in fs::read_dir(out_dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "woff2") {
            fs::remove_file(path)?;
        }
    }
    let mut rules = Vec::new();
    let mut fallbacks = Vec::new();
    for face in read_google_faces()? {
        let family = FAMILIES
            .iter()
            .find(|family| family.name == face.family)
            .ok_or_else(|| format!("unknown family {}", face.family))?;
        let sfnt =
            woofwoof::decompress(&fetch(&face.url)?).ok_or("woff2 decoding failed")?;
        let cmap = get_cmap(&sfnt)?;
        let google_range = parse_range(&face.range)?;
        let wanted = &google_range & &cmap;
        let mut parts = vec![(face.subset.clone(), wanted.clone(), Codepoints::new())];
        if face.subset == "latin" {
            fallbacks.push((family, get_fallback_metrics(&sfnt, family)?));
            let (core, rest) = split_core(&sfnt, family, &wanted)?;
            let substituted = SUBSTITUTED
                .into_iter()
                .filter(|codepoint| {
                    google_range.contains(codepoint) && !cmap.contains(codepoint)
                })
                .collect();
            parts = vec![
                ("core".to_owned(), core, substituted),
                ("latin".to_owned(), rest, Codepoints::new()),
            ];
        }
        for (subset, codepoints, substituted) in parts {
            let name = format!("{}-{subset}.woff2", family.slug);
            let data = build_font(&sfnt, family, &codepoints)?;
            let contained =
                get_cmap(&woofwoof::decompress(&data).ok_or("woff2 decoding failed")?)?;
            if contained != codepoints {
                return Err(
                    format!("{name}: cmap does not match requested characters").into()
                );
            }
            fs::write(out_dir.join(&name), &data)?;
            println!("{name:32} {:7} bytes {:4} chars", data.len(), contained.len());
            rules.push(Rule {
                family: family.name,
                name,
                codepoints: &contained | &substituted,
            });
        }
    }
    Ok((rules, fallbacks))
}

fn write_css(out_dir: &Path, rules: &[Rule], fallbacks: &[Fallback]) -> Result<()> {
    let mut blocks: Vec<String> = rules
        .iter()
        .map(|rule| {
            let family = FAMILIES.iter().find(|family| family.name == rule.family).expect("known family");
            format!(
                "@font-face {{\n  font-family: '{}';\n  font-style: normal;\n  font-weight: {} {};\n  font-display: swap;\n  src: url('./{}') format('woff2');\n  unicode-range: {};\n}}\n",
                rule.family,
                family.weight.0,
                family.weight.1,
                rule.name,
                format_range(&rule.codepoints),
            )
        })
        .collect();
    for (family, metrics) in fallbacks {
        let sources = family
            .fallback_local
            .iter()
            .map(|name| format!("local('{name}')"))
            .collect::<Vec<_>>()
            .join(", ");
        let overrides: String = metrics
            .iter()
            .map(|(key, value)| format!("  {key}: {:.2}%;\n", value * 100.0))
            .collect();
        let codepoints: Codepoints = rules
            .iter()
            .filter(|rule| rule.family == family.name)
            .flat_map(|rule| rule.codepoints.iter().copied())
            .collect();
        blocks.push(format!(
            "@font-face {{\n  font-family: '{} Fallback';\n  src: {sources};\n{overrides}  unicode-range: {};\n}}\n",
            family.name,
            format_range(&codepoints),
        ));
    }
    fs::write(out_dir.join("fonts.css"), blocks.join("\n"))?;
    Ok(())
}

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../app/fonts");
    let (rules, fallbacks) = build_faces(&out_dir)?;
    write_css(&out_dir, &rules, &fallbacks)
}
