//! Temporary RAM measurements (not for commit). Run one test per process:
//! cargo test -p tracedraw-cdr --test ram -- --ignored --exact <name> --nocapture

use std::time::Instant;
use tracedraw_core::geometry::Rect;
use tracedraw_core::{
    Affine, BezPath, Color, Command, Document, Engine, Fill, Pattern, Point, Shape, ShapeId,
    ShapeKind, Size, Stroke,
};

extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

fn status() -> (u64, u64) {
    let s = std::fs::read_to_string("/proc/self/status").unwrap();
    let get = |k: &str| {
        s.lines()
            .find(|l| l.starts_with(k))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    (get("VmRSS:") / 1024, get("VmHWM:") / 1024)
}

fn reset_hwm() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

fn trim() {
    unsafe {
        malloc_trim(0);
    }
}

fn report(label: &str) -> u64 {
    let (rss, hwm) = status();
    println!("{label:<55} rss {rss:>6} MB   peak {hwm:>6} MB");
    rss
}

/// A photo-like RGBA picture: smooth light with sensor noise.
fn photo(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    let mut x32 = seed.wrapping_mul(2654435761).max(1);
    let ks = [0.0f32, 1.3, 2.1];
    // Separable terms: sin(a) cos(b), and sin(a + b) = sin a cos b + cos a sin b.
    let cols: Vec<[[f32; 3]; 3]> = (0..w)
        .map(|x| {
            let fx = x as f32 / w as f32;
            ks.map(|k| [(fx * 7.0 + k).sin(), (fx * 41.0 + k).sin(), (fx * 41.0 + k).cos()])
        })
        .collect();
    for y in 0..h {
        let fy = y as f32 / h as f32;
        let row = ks.map(|k| [(fy * 5.0 + 2.0 * k).cos(), (fy * 29.0).cos(), (fy * 29.0).sin()]);
        for col in &cols {
            x32 ^= x32 << 13;
            x32 ^= x32 >> 17;
            x32 ^= x32 << 5;
            let n = (x32 % 13) as f32 - 6.0;
            let c = |i: usize| {
                let (a, b) = (col[i], row[i]);
                (128.0 + 90.0 * a[0] * b[0] + 30.0 * (a[1] * b[1] + a[2] * b[2]) + n)
                    .clamp(0.0, 255.0) as u8
            };
            out.extend_from_slice(&[c(0), c(1), c(2), 255]);
        }
    }
    let _ = seed;
    out
}

fn png_of(rgba: Vec<u8>, w: u32, h: u32) -> Vec<u8> {
    let pm = tiny_skia::Pixmap::from_vec(rgba, tiny_skia::IntSize::from_wh(w, h).unwrap()).unwrap();
    pm.encode_png().unwrap()
}

fn curve(i: usize) -> BezPath {
    let x = (i % 200) as f64 * 1.5;
    let y = (i / 200) as f64 * 1.5;
    let mut p = BezPath::new();
    p.move_to(Point::new(x, y));
    for k in 0..12 {
        let t = k as f64 * 0.1;
        p.curve_to(
            Point::new(x + t, y + 0.3),
            Point::new(x + t + 0.05, y + 0.6),
            Point::new(x + t + 0.1, y + 0.1),
        );
    }
    p.close_path();
    p
}

/// A drawing of `curves` curves and `pngs.len()` bitmaps (each `w` x `h`
/// pixels, 200 x 150 mm), returned with the id of one small curve.
fn drawing(curves: usize, pngs: &[Vec<u8>], w: u32, h: u32) -> (Document, ShapeId) {
    let mut doc = Document::new("ram", Size::new(600.0, 600.0));
    let layer = doc.pages[0].layers[0].id;
    let mut first = None;
    let mut shapes = Vec::new();
    for (i, png) in pngs.iter().enumerate() {
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 200.0, 150.0),
                width_px: w,
                height_px: h,
                png: png.clone(),
                fx: None,
            },
        );
        s.transform = Affine::translate(((i % 3) as f64 * 200.0, (i / 3) as f64 * 150.0));
        s.fill = Fill::None;
        s.stroke = None;
        shapes.push(s);
    }
    for i in 0..curves {
        let id = doc.ids_mut().shape();
        first.get_or_insert(id);
        let mut s = Shape::new(
            id,
            ShapeKind::Path {
                path: curve(i),
                closed: true,
            },
        );
        s.fill = Fill::Solid(Color::rgb8((i % 255) as u8, 40, 90));
        s.stroke = Some(Stroke::new(Color::rgb8(0, 0, 0), 0.2));
        shapes.push(s);
    }
    doc.layer_mut(layer).unwrap().shapes = shapes;
    (doc, first.unwrap_or(ShapeId(0)))
}

fn png_bytes_in(doc: &Document) -> usize {
    fn walk(s: &Shape, n: &mut usize) {
        match &s.kind {
            ShapeKind::Bitmap { png, .. } => *n += png.len(),
            ShapeKind::Group { children } => children.iter().for_each(|c| walk(c, n)),
            ShapeKind::ClipFrame { frame, contents } => {
                walk(frame, n);
                contents.iter().for_each(|c| walk(c, n));
            }
            _ => {}
        }
        if let Fill::Pattern(Pattern::Bitmap { png, .. }) = &s.fill {
            *n += png.len();
        }
    }
    let mut n = 0;
    for p in &doc.pages {
        for l in &p.layers {
            l.shapes.iter().for_each(|s| walk(s, &mut n));
        }
    }
    n
}

const W: u32 = 4000;
const H: u32 = 3000;

fn env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// PNG sizes: the in-memory form the .cdr reader makes vs a filtered PNG.
#[test]
#[ignore]
fn m1_png_sizes() {
    let rgba = photo(W, H, 1);
    println!("raw RGBA {} MB", rgba.len() >> 20);
    let filtered = png_of(rgba, W, H);
    println!("filtered PNG (tiny-skia encoder) {} MB", filtered.len() >> 20);
    // Through a .cdr file: the reader's own PNG (no filter, fast zlib).
    let (doc, _) = drawing(0, std::slice::from_ref(&filtered), W, H);
    let cdr = tracedraw_cdr::write::document_to_cdr(&doc);
    println!(".cdr file with it {} MB", cdr.len() >> 20);
    let (back, _) = tracedraw_cdr::open_bytes(&cdr, "t").unwrap();
    println!("PNG the .cdr reader keeps {} MB", png_bytes_in(&back) >> 20);
}

/// Undo history: whole-document snapshots, bitmaps included.
#[test]
#[ignore]
fn m2_history() {
    let n_img = env("IMGS", 6);
    let curves = env("CURVES", 40_000);
    let edits = env("EDITS", 20);
    trim();
    report("start");
    let base = png_of(photo(W, H, 7), W, H);
    let (doc0, _) = drawing(0, std::slice::from_ref(&base), W, H);
    // The PNG the .cdr reader would hold.
    let cdr = tracedraw_cdr::write::document_to_cdr(&doc0);
    let (back, _) = tracedraw_cdr::open_bytes(&cdr, "t").unwrap();
    let reader_png = match &back.pages[0].layers[0].shapes[0].kind {
        ShapeKind::Bitmap { png, .. } => png.clone(),
        _ => panic!(),
    };
    drop((doc0, cdr, back, base));
    // Distinct pictures (vary one byte so each is its own allocation).
    let pngs: Vec<Vec<u8>> = (0..n_img).map(|_| reader_png.clone()).collect();
    drop(reader_png);
    let (doc, small) = drawing(curves, &pngs, W, H);
    drop(pngs);
    trim();
    let png_mb = png_bytes_in(&doc) >> 20;
    let r0 = report(&format!(
        "document: {curves} curves, {n_img} bitmaps ({png_mb} MB PNG)"
    ));
    let mut eng = Engine::new(doc);
    eng.set_max_history(150);
    trim();
    let r1 = report("engine created");
    let t = Instant::now();
    for i in 1..=edits {
        eng.run(&Command::TransformShapes {
            shapes: vec![small],
            transform: Affine::translate((0.5, 0.0)),
        })
        .unwrap();
        if i % 5 == 0 || i == 1 {
            trim();
            report(&format!("after {i} moves of one small curve"));
        }
    }
    let per_edit = t.elapsed().as_secs_f64() * 1000.0 / edits as f64;
    let r2 = status().0;
    println!(
        "growth per edit: {:.1} MB (document {} MB above start), {per_edit:.1} ms per edit (debug core)",
        (r2 - r1) as f64 / edits as f64,
        r0 - 0
    );
    for _ in 0..edits {
        eng.undo().unwrap();
    }
    trim();
    report("after undoing all");
    for _ in 0..edits {
        eng.redo().unwrap();
    }
    trim();
    report("after redoing all");
    // Live fill drag: undo + run per pointer move (apply_fill_live).
    reset_hwm();
    let t = Instant::now();
    for k in 0..10u8 {
        let _ = eng.undo();
        eng.run(&Command::SetFill {
            shapes: vec![small],
            fill: Fill::Solid(Color::rgb8(k, 0, 0)),
        })
        .unwrap();
    }
    println!(
        "live fill drag: {:.1} ms per pointer move (debug core)",
        t.elapsed().as_secs_f64() * 100.0
    );
    trim();
    report("after 10 live-fill moves");
    drop(eng);
    trim();
    report("engine dropped");
}

/// Saving: JSON (.tdraw / auto-backup) and .cdr peaks.
#[test]
#[ignore]
fn m3_save_peaks() {
    let n_img = env("IMGS", 6);
    let curves = env("CURVES", 40_000);
    let base = png_of(photo(W, H, 3), W, H);
    let pngs: Vec<Vec<u8>> = (0..n_img).map(|_| base.clone()).collect();
    drop(base);
    let (doc, _) = drawing(curves, &pngs, W, H);
    drop(pngs);
    trim();
    let png_mb = png_bytes_in(&doc) >> 20;
    report(&format!("document: {curves} curves, {n_img} bitmaps ({png_mb} MB PNG)"));
    reset_hwm();
    let t = Instant::now();
    let json = doc.to_json().unwrap();
    println!("to_json {} MB in {:.1} s", json.len() >> 20, t.elapsed().as_secs_f64());
    report("after to_json (peak = during)");
    drop(json);
    report("json dropped, before malloc_trim");
    trim();
    report("json dropped, after malloc_trim");
    reset_hwm();
    let t = Instant::now();
    let cdr = tracedraw_cdr::write::document_to_cdr(&doc);
    println!(".cdr {} MB in {:.1} s", cdr.len() >> 20, t.elapsed().as_secs_f64());
    report("after .cdr write (peak = during)");
    drop(cdr);
    trim();
    // A bitmap pattern fill: the writer keys fills by their Debug text.
    let mut d2 = Document::new("pat", Size::new(100.0, 100.0));
    let layer = d2.pages[0].layers[0].id;
    let id = d2.ids_mut().shape();
    let mut s = Shape::new(
        id,
        ShapeKind::Rect {
            rect: Rect::new(0.0, 0.0, 50.0, 50.0),
            radius: 0.0,
            corners: None,
        },
    );
    let tile = png_of(photo(2000, 1500, 5), 2000, 1500);
    let tile_mb = tile.len() >> 20;
    s.fill = Fill::Pattern(Pattern::Bitmap {
        png: tile,
        width_px: 2000,
        height_px: 1500,
        size_mm: 20.0,
    });
    d2.layer_mut(layer).unwrap().shapes.push(s);
    drop(doc);
    trim();
    report(&format!("pattern-fill drawing ({tile_mb} MB PNG tile)"));
    reset_hwm();
    let cdr = tracedraw_cdr::write::document_to_cdr(&d2);
    println!(".cdr {} MB", cdr.len() >> 20);
    report("after .cdr write of one bitmap pattern fill (peak)");
}

/// Reading a large .cdr: peak while parsing vs what the document keeps,
/// and a picture placed many times.
#[test]
#[ignore]
fn m4_cdr_read() {
    let n_img = env("IMGS", 6);
    let curves = env("CURVES", 40_000);
    let pngs: Vec<Vec<u8>> = (0..n_img)
        .map(|i| png_of(photo(W, H, 11 + i as u32), W, H))
        .collect();
    let (doc, _) = drawing(curves, &pngs, W, H);
    drop(pngs);
    let cdr = tracedraw_cdr::write::document_to_cdr(&doc);
    drop(doc);
    trim();
    report(&format!(".cdr in memory: {} MB", cdr.len() >> 20));
    reset_hwm();
    let t = Instant::now();
    let (back, rep) = tracedraw_cdr::open_bytes(&cdr, "t").unwrap();
    println!(
        "read {} shapes in {:.1} s, PNG kept {} MB",
        rep.shapes,
        t.elapsed().as_secs_f64(),
        png_bytes_in(&back) >> 20
    );
    trim();
    report("after open_bytes (peak = during the read)");
    drop(cdr);
    trim();
    report("file bytes dropped: document only");
    drop(back);
    // One picture placed many times.
    let copies = env("COPIES", 30);
    let one = png_of(photo(1600, 1200, 9), 1600, 1200);
    let one_mb = one.len() as f64 / 1048576.0;
    let mut doc = Document::new("copies", Size::new(600.0, 600.0));
    let layer = doc.pages[0].layers[0].id;
    for i in 0..copies {
        let id = doc.ids_mut().shape();
        let mut s = Shape::new(
            id,
            ShapeKind::Bitmap {
                rect: Rect::new(0.0, 0.0, 40.0, 30.0),
                width_px: 1600,
                height_px: 1200,
                png: one.clone(),
                fx: None,
            },
        );
        s.transform = Affine::translate(((i % 10) as f64 * 45.0, (i / 10) as f64 * 35.0));
        s.fill = Fill::None;
        s.stroke = None;
        doc.layer_mut(layer).unwrap().shapes.push(s);
    }
    let cdr = tracedraw_cdr::write::document_to_cdr(&doc);
    drop(doc);
    trim();
    report(&format!(
        "one {one_mb:.1} MB picture x {copies}: .cdr {} MB",
        cdr.len() >> 20
    ));
    let (back, _) = tracedraw_cdr::open_bytes(&cdr, "t").unwrap();
    drop(cdr);
    trim();
    report(&format!(
        "read back: PNG kept {} MB",
        png_bytes_in(&back) >> 20
    ));
}

/// The canvas renderer: bands on fresh threads decode every picture again.
#[test]
#[ignore]
fn m5_render() {
    use tracedraw_render::{render_page, render_page_image, RenderOptions, ViewTransform};
    let n_img = env("IMGS", 6);
    let pngs: Vec<Vec<u8>> = (0..n_img)
        .map(|i| png_of(photo(W, H, 21 + i as u32), W, H))
        .collect();
    let (doc, _) = drawing(0, &pngs, W, H);
    drop(pngs);
    let page = doc.pages[0].id;
    trim();
    report(&format!(
        "document: {n_img} bitmaps {}x{} ({} MB PNG), {} cores",
        W,
        H,
        png_bytes_in(&doc) >> 20,
        std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
    ));
    // The whole page in a 1920 x 1080 canvas.
    let zoom = 1080.0 / 600.0;
    let opts = RenderOptions {
        width: 1920,
        height: 1080,
        view: ViewTransform {
            zoom,
            origin_x: 0.0,
            origin_y: 1080.0,
        },
        draft: true,
        anti_alias: true,
        ..Default::default()
    };
    for k in 1..=3 {
        reset_hwm();
        let t = Instant::now();
        let pm = render_page(&doc, page, &opts).unwrap();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        drop(pm);
        trim();
        report(&format!("canvas render {k} (bands) {ms:.0} ms"));
    }
    // A thumbnail on this thread (store_thumbnail, navigator): kept decoded.
    reset_hwm();
    let t = Instant::now();
    let pm = render_page_image(&doc, page, 256.0 * 25.4 / 600.0).unwrap();
    println!("thumbnail {}x{}", pm.width(), pm.height());
    drop(pm);
    trim();
    report(&format!(
        "thumbnail render on main thread {:.0} ms",
        t.elapsed().as_secs_f64() * 1000.0
    ));
    // The slow-page path of raster.rs: the drawing cloned onto a thread.
    for k in 1..=2 {
        reset_hwm();
        let t = Instant::now();
        let d = doc.clone();
        let o = opts.clone();
        std::thread::spawn(move || {
            let _ = render_page(&d, page, &o);
        })
        .join()
        .unwrap();
        trim();
        report(&format!(
            "background job {k}: clone + render {:.0} ms",
            t.elapsed().as_secs_f64() * 1000.0
        ));
    }
    // Multipage view: four slow pages at once.
    reset_hwm();
    let hs: Vec<_> = (0..4)
        .map(|_| {
            let d = doc.clone();
            let o = opts.clone();
            std::thread::spawn(move || {
                let _ = render_page(&d, page, &o);
            })
        })
        .collect();
    hs.into_iter().for_each(|h| h.join().unwrap());
    trim();
    report("four page jobs at once (multipage view)");
}

/// What saving without the intermediate JSON value would cost.
#[test]
#[ignore]
fn m6_json_direct() {
    let base = png_of(photo(W, H, 3), W, H);
    let pngs: Vec<Vec<u8>> = (0..6).map(|_| base.clone()).collect();
    drop(base);
    let (doc, _) = drawing(40_000, &pngs, W, H);
    drop(pngs);
    trim();
    report("document");
    reset_hwm();
    let s = serde_json::to_string_pretty(&doc).unwrap();
    println!("direct to_string_pretty {} MB", s.len() >> 20);
    report("after direct string (peak = during)");
    drop(s);
    report("dropped, before trim");
    trim();
    reset_hwm();
    let f = std::fs::File::create("/dev/null").unwrap();
    serde_json::to_writer_pretty(std::io::BufWriter::new(f), &doc).unwrap();
    report("streamed to a file (peak = during)");
}

/// begin_preview (Edit Fill, font list hover) copies the redo stack.
#[test]
#[ignore]
fn m7_preview_redo_clone() {
    let base = png_of(photo(W, H, 3), W, H);
    let pngs: Vec<Vec<u8>> = (0..6).map(|_| base.clone()).collect();
    drop(base);
    let (doc, small) = drawing(1, &pngs, W, H);
    drop(pngs);
    let mut eng = Engine::new(doc);
    for _ in 0..5 {
        eng.run(&Command::TransformShapes {
            shapes: vec![small],
            transform: Affine::translate((0.5, 0.0)),
        })
        .unwrap();
    }
    for _ in 0..5 {
        eng.undo().unwrap();
    }
    trim();
    report("6 bitmaps (88 MB PNG), 5 steps undone (redo stack 5)");
    reset_hwm();
    eng.begin_preview();
    trim();
    report("after begin_preview");
    eng.withdraw_preview();
    trim();
    report("after withdraw_preview");
}

/// zlib stream sizes for the reader's PNG layout with and without a row filter.
#[test]
#[ignore]
fn m8_png_filters() {
    use std::io::Write;
    let rgba = photo(W, H, 1);
    let z = |data: &[u8], level| {
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), level);
        e.write_all(data).unwrap();
        e.finish().unwrap().len() >> 20
    };
    let rows = |bpp: usize, filter: u8| {
        let row = W as usize * bpp;
        let mut out = Vec::with_capacity((row + 1) * H as usize);
        let px = |y: usize, x: usize| -> &[u8] {
            let i = (y * W as usize + x) * 4;
            &rgba[i..i + bpp]
        };
        for y in 0..H as usize {
            out.push(filter);
            for x in 0..W as usize {
                for c in 0..bpp {
                    let v = px(y, x)[c];
                    let up = if y > 0 { px(y - 1, x)[c] } else { 0 };
                    let left = if x > 0 { px(y, x - 1)[c] } else { 0 };
                    out.push(match filter {
                        0 => v,
                        1 => v.wrapping_sub(left),
                        2 => v.wrapping_sub(up),
                        _ => v.wrapping_sub(((left as u16 + up as u16) / 2) as u8),
                    });
                }
            }
        }
        out
    };
    for (name, bpp, f) in [
        ("RGBA none (as now)", 4, 0u8),
        ("RGBA up", 4, 2),
        ("RGB none", 3, 0),
        ("RGB sub", 3, 1),
        ("RGB up", 3, 2),
        ("RGB average", 3, 3),
    ] {
        let data = rows(bpp, f);
        println!(
            "{name:<20} fast {:>3} MB   default {:>3} MB",
            z(&data, flate2::Compression::fast()),
            z(&data, flate2::Compression::default())
        );
    }
}
