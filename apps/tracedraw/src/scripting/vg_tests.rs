//! The object model VBA macros use, driven the way compiled macros call
//! it: exact member names, 1-based items, lengths in the document unit.

use crate::app::App;
use crate::scripting::run_vgcore;
use tracedraw_core::{
    document::{Shape, ShapeKind},
    geometry::Rect,
    Color, Fill, ShapeId,
};

fn run(app: &mut App, src: &str) -> Vec<String> {
    match run_vgcore(app, src, "Macro") {
        Ok(out) => out,
        Err(e) => panic!("macro failed: {e}"),
    }
}

/// Every object on the active page, groups' insides too.
fn all(app: &App) -> Vec<Shape> {
    fn walk(s: &Shape, out: &mut Vec<Shape>) {
        out.push(s.clone());
        if let ShapeKind::Group { children } = &s.kind {
            children.iter().for_each(|c| walk(c, out));
        }
    }
    let mut out = Vec::new();
    for p in &app.doc().pages {
        for l in &p.layers {
            l.shapes.iter().for_each(|s| walk(s, &mut out));
        }
    }
    out
}

fn page_bounds(app: &App, id: ShapeId) -> Rect {
    app.doc().shape_on_page(id).expect("shape").bounds()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn lengths_follow_the_document_unit() {
    let mut app = App::headless();
    let out = run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const r = ActiveLayer.CreateRectangle(10, 30, 50, 10);
         r.Name = 'mm';
         ActiveDocument.Unit = cdrInch;
         const s = ActiveLayer.CreateRectangle2(1, 1, 2, 1);
         s.Name = 'in';
         print(s.SizeWidth, s.SizeHeight, r.SizeWidth);",
    );
    let mm = all(&app)
        .into_iter()
        .find(|s| s.name.as_deref() == Some("mm"))
        .expect("mm");
    let b = page_bounds(&app, mm.id);
    assert!(
        close(b.x0, 10.0) && close(b.y0, 10.0) && close(b.x1, 50.0) && close(b.y1, 30.0),
        "{b:?}"
    );
    let inch = all(&app)
        .into_iter()
        .find(|s| s.name.as_deref() == Some("in"))
        .expect("in");
    let b = page_bounds(&app, inch.id);
    assert!(
        close(b.x0, 25.4) && close(b.width(), 50.8) && close(b.height(), 25.4),
        "{b:?}"
    );
    let v: Vec<f64> = out[0]
        .split(' ')
        .map(|x| x.parse().expect("number"))
        .collect();
    assert!(
        close(v[0], 2.0) && close(v[1], 1.0) && close(v[2], 40.0 / 25.4),
        "{out:?}"
    );
    // One undo step for the whole run.
    assert_eq!(app.engine.undo_label(), Some("Macro"));
}

#[test]
fn the_reference_point_places_and_scales() {
    let mut app = App::headless();
    run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const r = ActiveLayer.CreateRectangle2(40, 50, 20, 10);
         r.Name = 'r';
         ActiveDocument.ReferencePoint = cdrBottomLeft;
         r.SetPosition(0, 0);
         r.SetSize(30, 0);
         ActiveDocument.ReferencePoint = cdrTopRight;
         print(r.PositionX, r.PositionY);",
    );
    let r = all(&app)
        .into_iter()
        .find(|s| s.name.as_deref() == Some("r"))
        .expect("r");
    let b = page_bounds(&app, r.id);
    // Placed at the origin, then 30 wide keeping the proportions, about
    // the lower left corner.
    assert!(close(b.x0, 0.0) && close(b.y0, 0.0), "{b:?}");
    assert!(close(b.width(), 30.0) && close(b.height(), 15.0), "{b:?}");
}

#[test]
fn collections_are_one_based_named_iterable_and_reach_into_groups() {
    let mut app = App::headless();
    let out = run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const L = ActiveLayer;
         const a = L.CreateRectangle2(0, 0, 10, 10); a.Name = 'A';
         const b = L.CreateEllipse2(30, 30, 5); b.Name = 'B';
         const c = L.CreateRectangle2(50, 0, 10, 10); c.Name = 'C';
         const sr = new ShapeRange(); sr.Add(b); sr.Add(c);
         const g = sr.Group(); g.Name = 'G';
         let names = [];
         for (const s of ActivePage.Shapes) names.push(s.Name);
         print(ActivePage.Shapes.Count, ActivePage.Shapes.Item(1).Name, ActivePage.Shapes.Item('G').Type === cdrGroupShape);
         print(names.join(','));
         print(g.Shapes.Count, g.Shapes.Item(2).Name);
         print(ActivePage.Shapes.FindShapes('', cdrEllipseShape, true).Count, ActivePage.Shapes.FindShapes('', cdrEllipseShape, false).Count);
         print(ActivePage.FindShapes('', 0, true, \"@type = 'rectangle' and @name <> 'A'\").Item(1).Name);",
    );
    assert_eq!(out[0], "2 A true");
    assert_eq!(out[1], "A,G");
    assert_eq!(out[2], "2 C");
    assert_eq!(out[3], "1 0");
    assert_eq!(out[4], "C");
}

#[test]
fn colours_and_outlines_change_the_object() {
    let mut app = App::headless();
    let out = run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const r = ActiveLayer.CreateRectangle2(0, 0, 10, 10); r.Name = 'r';
         r.Fill.UniformColor.RGBAssign(255, 0, 0);
         r.Outline.Width = 0.5;
         r.Outline.Color.CMYKAssign(0, 100, 100, 0);
         const c = new Color(); c.CMYKAssign(10, 20, 30, 40);
         print(r.Fill.Type === cdrUniformFill, r.Fill.UniformColor.RGBRed, r.Outline.Width, c.CMYKMagenta, c.Type === cdrColorCMYK);
         print(r.Fill.UniformColor.HexValue);",
    );
    let r = all(&app)
        .into_iter()
        .find(|s| s.name.as_deref() == Some("r"))
        .expect("r");
    assert_eq!(r.fill, Fill::Solid(Color::rgb8(255, 0, 0)));
    let st = r.stroke.expect("outline");
    assert!(close(st.width, 0.5));
    assert!(matches!(st.color, Color::Cmyk { .. }));
    assert_eq!(out[0], "true 255 0.5 20 true");
    assert_eq!(out[1].to_lowercase(), "#ff0000");
}

#[test]
fn text_story_edits_the_text_object() {
    let mut app = App::headless();
    let out = run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const t = ActiveLayer.CreateArtisticText(10, 10, 'Hello', 0, 0, 'Arial', 18);
         t.Name = 't';
         const st = t.Text.Story;
         st.Text = 'Hello world'; st.Bold = true; st.Size = 30; st.Alignment = cdrCenterAlignment;
         print(t.Type === cdrTextShape, st.Text, st.Size, st.Bold, st.Words.Count);
         print(t.Text.Replace('world', 'there'), t.Text.Story.Text);",
    );
    assert_eq!(out[0], "true Hello world 30 true 2");
    assert_eq!(out[1], "true Hello there");
    let t = all(&app)
        .into_iter()
        .find(|s| s.name.as_deref() == Some("t"))
        .expect("t");
    let ShapeKind::Text { spans, align, .. } = &t.kind else {
        panic!("text");
    };
    assert!(spans.iter().all(|s| s.bold && s.size_pt == 30.0));
    assert_eq!(*align, tracedraw_core::TextAlign::Center);
}

#[test]
fn shaping_grouping_and_new_pages_keep_ids_unique() {
    let mut app = App::headless();
    run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const a = ActiveLayer.CreateRectangle2(0, 0, 20, 20);
         const b = ActiveLayer.CreateRectangle2(10, 10, 20, 20);
         const w = b.Weld(a); w.Name = 'weld';
         const c = ActiveLayer.CreateEllipse2(80, 80, 5);
         const d = c.Duplicate(10, 0);
         const g = ActiveDocument.CreateShapeRangeFromArray(c, d).Group();
         g.Name = 'G';
         const g2 = g.Duplicate(0, 20);
         ActiveDocument.AddPages(1);
         ActivePage.Shapes.Item('G').UngroupAll();
         const p2 = ActiveDocument.Pages.Item(2);
         p2.ActiveLayer.CreateRectangle2(0, 0, 5, 5).Name = 'second page';",
    );
    let shapes = all(&app);
    let weld = shapes
        .iter()
        .find(|s| s.name.as_deref() == Some("weld"))
        .expect("weld");
    assert!(matches!(weld.kind, ShapeKind::Path { .. }));
    // The welded originals are gone: weld, group copy, two loose ellipses.
    let page1 = &app.doc().pages[0];
    assert_eq!(
        page1.layers.iter().map(|l| l.shapes.len()).sum::<usize>(),
        4
    );
    let mut ids: Vec<u64> = shapes.iter().map(|s| s.id.raw()).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n, "ids repeat");
    assert!(shapes
        .iter()
        .any(|s| s.name.as_deref() == Some("second page")));
    // The app's next id comes after every id the macro used.
    let next = app.engine.new_shape_id().raw();
    assert!(ids.iter().all(|i| *i < next));
}

#[test]
fn community_macro_resize_the_selection() {
    let mut app = App::headless();
    run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         for (let i = 0; i < 3; i++) ActiveLayer.CreateEllipse2(20 + i * 30, 50, 5 + i).AddToSelection();",
    );
    run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         ActiveDocument.ReferencePoint = cdrCenter;
         for (const s of ActiveSelectionRange) s.SetSize(20, 10);",
    );
    for id in app.selection.clone() {
        let b = page_bounds(&app, id);
        assert!(close(b.width(), 20.0) && close(b.height(), 10.0), "{b:?}");
    }
    assert_eq!(app.selection.len(), 3);
}

#[test]
fn community_macro_number_the_objects() {
    let mut app = App::headless();
    run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         for (let i = 0; i < 4; i++) ActiveLayer.CreateRectangle2(10 + i * 30, 10, 20, 20);
         const shapes = ActiveLayer.Shapes.All();
         for (let i = 1; i <= shapes.Count; i++) {
           const s = shapes.Item(i);
           const t = ActiveLayer.CreateArtisticText(0, 0, String(i));
           t.CenterX = s.CenterX; t.CenterY = s.CenterY;
         }",
    );
    let texts: Vec<Shape> = all(&app)
        .into_iter()
        .filter(|s| matches!(s.kind, ShapeKind::Text { .. }))
        .collect();
    assert_eq!(texts.len(), 4);
    for (i, t) in texts.iter().enumerate() {
        let c = page_bounds(&app, t.id).center();
        assert!((c.x - (20.0 + i as f64 * 30.0)).abs() < 1e-6, "{c:?}");
        assert!((c.y - 20.0).abs() < 1e-6, "{c:?}");
    }
}

#[test]
fn community_macro_grid_layout() {
    let mut app = App::headless();
    run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         ActiveDocument.ReferencePoint = cdrTopLeft;
         for (let i = 0; i < 9; i++) ActiveLayer.CreateEllipse2(100, 100, 4 + i % 3).AddToSelection();
         const sr = ActiveSelectionRange;
         let i = 0;
         for (const s of sr) {
           s.SetPosition(10 + (i % 3) * 25, 200 - Math.floor(i / 3) * 25);
           i++;
         }",
    );
    let mut lefts: Vec<(i64, i64)> = app
        .selection
        .iter()
        .map(|id| {
            let b = page_bounds(&app, *id);
            (b.x0.round() as i64, b.y1.round() as i64)
        })
        .collect();
    lefts.sort();
    assert_eq!(lefts.len(), 9);
    assert_eq!(lefts[0], (10, 150));
    assert_eq!(lefts[8], (60, 200));
}

#[test]
fn community_macro_replace_a_colour_across_the_page() {
    let mut app = App::headless();
    run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const a = ActiveLayer.CreateRectangle2(0, 0, 10, 10); a.Fill.UniformColor.RGBAssign(255, 0, 0);
         const b = ActiveLayer.CreateRectangle2(20, 0, 10, 10); b.Fill.UniformColor.RGBAssign(255, 0, 0);
         const c = ActiveLayer.CreateRectangle2(40, 0, 10, 10); c.Fill.UniformColor.RGBAssign(0, 255, 0);
         ActiveDocument.CreateShapeRangeFromArray(b, c).Group();",
    );
    run(
        &mut app,
        "const red = CreateRGBColor(255, 0, 0);
         const blue = CreateRGBColor(0, 0, 255);
         for (const s of ActivePage.Shapes.FindShapes('', 0, true)) {
           if (s.Fill.Type === cdrUniformFill && s.Fill.UniformColor.IsSame(red)) s.Fill.UniformColor.CopyAssign(blue);
         }",
    );
    let fills: Vec<Fill> = all(&app)
        .into_iter()
        .filter(|s| matches!(s.kind, ShapeKind::Rect { .. }))
        .map(|s| s.fill)
        .collect();
    assert_eq!(
        fills
            .iter()
            .filter(|f| **f == Fill::Solid(Color::rgb8(0, 0, 255)))
            .count(),
        2
    );
    assert_eq!(
        fills
            .iter()
            .filter(|f| **f == Fill::Solid(Color::rgb8(0, 255, 0)))
            .count(),
        1
    );
}

#[test]
fn community_macro_export_every_page_as_png() {
    let mut app = App::headless();
    let dir = std::env::temp_dir().join(format!("tracedraw-vg-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let src = format!(
        "ActiveDocument.Unit = cdrMillimeter;
         ActiveLayer.CreateRectangle2(10, 10, 50, 50).Fill.UniformColor.RGBAssign(0, 0, 255);
         ActiveDocument.AddPages(2);
         for (const p of ActiveDocument.Pages) {{
           p.Activate();
           ActiveDocument.ExportBitmap({dir:?} + '/page' + p.Index + '.png', cdrPNG, cdrCurrentPage, cdrRGBColorImage, 100, 0, 72, 72).Finish();
         }}",
        dir = dir.display().to_string()
    );
    run(&mut app, &src);
    for i in 1..=3 {
        let bytes = std::fs::read(dir.join(format!("page{i}.png"))).expect("png written");
        assert_eq!(&bytes[..4], b"\x89PNG");
        let img = image::load_from_memory(&bytes).expect("png").to_rgba8();
        assert_eq!(img.width(), 100);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn nodes_segments_and_curve_building() {
    let mut app = App::headless();
    let out = run(
        &mut app,
        "ActiveDocument.Unit = cdrMillimeter;
         const crv = ActiveDocument.CreateCurve();
         const sp = crv.CreateSubPath(0, 0);
         sp.AppendLineSegment(30, 0); sp.AppendLineSegment(30, 40); sp.Closed = true;
         const s = ActiveLayer.CreateCurve(crv);
         print(s.Type === cdrCurveShape, s.Curve.Nodes.Count, s.Curve.Segments.Count, s.Curve.Closed, Math.round(s.Curve.Length));
         s.Curve.Nodes.Item(2).SetPosition(40, 0);
         print(s.Curve.Nodes.Item(2).PositionX);",
    );
    assert_eq!(out[0], "true 3 3 true 120");
    assert_eq!(out[1], "40");
}

#[test]
fn errors_stop_the_macro_and_keep_what_ran() {
    let mut app = App::headless();
    let err = run_vgcore(
        &mut app,
        "ActiveLayer.CreateRectangle2(0, 0, 1, 1).Name = 'kept'; ActivePage.Shapes.Item(5);",
        "Macro",
    )
    .expect_err("out of range");
    assert!(err.contains("out of range"), "{err}");
    assert!(all(&app).iter().any(|s| s.name.as_deref() == Some("kept")));
}
