//! Hindi through a PDF and back (#599): text written by the PDF export
//! with its fonts, and text the way other producers write it (glyphs in
//! drawn order, ActualText, vowel signs placed apart), comes in as the
//! typed words and shapes to the same glyphs as the words themselves.

use tracedraw_core::{
    document::text_outline::TextRequest, Document, ParagraphStyle, Point, Shape, ShapeKind, Size,
    TextAlign, TextSpan,
};

const NOTO_DEVA: &[u8] =
    include_bytes!("../../../crates/text/testdata/NotoSansDevanagari-Regular.ttf");
const CORPUS: &str = include_str!("../../../crates/text/testdata/devanagari-corpus.txt");
const DEVA: &str = "Noto Sans Devanagari";

fn corpus() -> Vec<&'static str> {
    CORPUS
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect()
}

fn fonts() -> &'static tracedraw_text::FontSystem {
    tracedraw_text::install();
    let f = tracedraw_text::fonts();
    if !f.has_family(DEVA) {
        f.add_fonts(vec![NOTO_DEVA.to_vec()]);
    }
    f
}

/// The glyph ids `text` lays out to in the Devanagari font.
fn gids(text: &str) -> Vec<u16> {
    let spans = [TextSpan::new(text, DEVA, 24.0)];
    let para = ParagraphStyle::default();
    fonts()
        .glyph_layout(&TextRequest {
            spans: &spans,
            frame: None,
            align: TextAlign::Left,
            para: &para,
            on_path: None,
        })
        .glyphs
        .iter()
        .map(|g| g.glyph_id)
        .collect()
}

/// The texts of the imported file's text objects, in order.
fn imported_texts(bytes: &[u8]) -> Vec<String> {
    let mut ids = tracedraw_core::id::IdSource::default();
    let imp = tracedraw_io::pdf_import::parse(bytes, &mut ids).expect("reads");
    imp.pages
        .iter()
        .flat_map(|p| p.shapes.iter())
        .filter_map(|s| match &s.kind {
            ShapeKind::Text { spans, .. } => {
                Some(spans.iter().map(|s| s.text.as_str()).collect::<String>())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn hindi_exported_with_its_font_imports_as_the_typed_words() {
    let _ = fonts();
    let words = corpus();
    let mut doc = Document::new("t", Size::new(200.0, 20.0 + 12.0 * words.len() as f64));
    let layer = doc.pages[0].layers[0].id;
    for (i, w) in words.iter().enumerate() {
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Text {
                spans: vec![TextSpan::new(*w, DEVA, 24.0)],
                origin: Point::new(10.0, 10.0 + 12.0 * i as f64),
                frame: None,
                align: TextAlign::Left,
                para: ParagraphStyle::default(),
                on_path: None,
            },
        );
        s.fill = tracedraw_core::Fill::Solid(tracedraw_core::Color::BLACK);
        if let Ok(l) = doc.layer_mut(layer) {
            l.shapes.push(s);
        }
    }
    let (bytes, report) =
        tracedraw_io::pdf::document_to_pdf_report(&doc, &tracedraw_io::pdf::PdfOptions::default());
    assert!(report.restricted.is_empty());
    let texts = imported_texts(&bytes);
    let bad: Vec<String> = words
        .iter()
        .filter(|w| !texts.iter().any(|t| t == *w))
        .map(|w| w.to_string())
        .collect();
    assert!(bad.is_empty(), "missing {bad:?} in {texts:?}");
    for t in &texts {
        assert_eq!(gids(t), gids(t.trim()), "{t}");
    }
}

/// Hindi lines a PDF brings one object a line, joined into a paragraph:
/// typing at the start of the first line flows the words on to the next
/// lines, and no line starts inside a syllable (a vowel sign or the rest
/// of a conjunct on its own) (#599).
#[test]
fn joined_hindi_lines_flow_and_break_between_syllables() {
    let _ = fonts();
    let mut app = crate::app::App::headless();
    let lines = [
        "राष्ट्रीय विद्यालय में स्थिति स्पष्ट",
        "शुद्धि ज्ञान कृष्ण हृदय श्रीमती",
        "द्वारा उज्ज्वल संयुक्ताक्षर अर्धाक्षर",
    ];
    let mut ids = Vec::new();
    for (i, text) in lines.iter().enumerate() {
        let id = app
            .new_shape(ShapeKind::Text {
                spans: vec![TextSpan::new(*text, DEVA, 10.0)],
                origin: Point::new(20.0, 200.0 - 6.0 * i as f64),
                frame: None,
                align: TextAlign::Left,
                para: ParagraphStyle::default(),
                on_path: None,
            })
            .expect("text");
        ids.push(id);
    }
    app.select(ids);
    app.join_text_lines();
    let id = app.selection[0];
    let s = app.doc().find_shape(id).expect("paragraph").clone();
    let (before, _) =
        crate::text_editing::layout_of_text_shape(&s, kurbo::Affine::IDENTITY).expect("layout");
    assert_eq!(before.lines.len(), 3, "{:?}", before.baselines);
    let mut kind = s.kind.clone();
    if let ShapeKind::Text { spans, .. } = &mut kind {
        spans[0].text.insert_str(0, "क्षेत्र त्रिभुवन स्त्री इन्द्रिय ");
    }
    app.run(tracedraw_core::Command::SetShapeKind { shape: id, kind });
    let s = app.doc().find_shape(id).expect("paragraph").clone();
    let (after, _) =
        crate::text_editing::layout_of_text_shape(&s, kurbo::Affine::IDENTITY).expect("layout");
    assert!(after.lines.len() > 3, "{:?}", after.baselines);
    let ShapeKind::Text { spans, .. } = &s.kind else {
        panic!("text");
    };
    let chars: Vec<char> = tracedraw_core::document::spans_text(spans)
        .chars()
        .collect();
    for l in &after.lines[1..] {
        assert_eq!(
            chars.get(l.start_char.wrapping_sub(1)),
            Some(&' '),
            "line at {}",
            l.start_char
        );
    }
}
