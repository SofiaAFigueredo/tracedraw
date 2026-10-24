# RAM growth with large drawings: sources, measurements, fix plan

Report: "RAM skyrockets with large files" (big `.cdr` files with many
objects and bitmaps). The earlier quick check (40k curves plus 6 images,
about 340 MB) measured the document just after it was built. That number is
right: this run gets 342 MB for the same drawing. The growth comes later,
from what happens to that document: every edit, every canvas render, the
thumbnail, the auto-backup and the save.

Code base: origin/main 94271e2 (Release 0.17.0). The line numbers below are
the same in the main tree (the key files compared byte for byte).

## How it was measured

- A temporary integration test, `crates/cdr/tests/ram.rs`, ran in a
  detached worktree, with `tracedraw-render` and `serde_json` added as
  dev-dependencies there only. A copy is kept at
  `/tmp/claude-0/-home-user-tracedraw/3fbdf153-2c11-5462-94f1-9f722b56d8bd/scratchpad/diag/ram_measure_test.rs`.
  Each scenario ran in its own process. The test reads VmRSS and VmHWM from
  `/proc/self/status`, resets the peak through `/proc/self/clear_refs`, and
  calls glibc `malloc_trim(0)` before each RSS reading, so the RSS numbers
  are live memory. One reading was taken without the trim to show what the
  allocator keeps.
- Debug build of the workspace crates. Dependencies build at opt-level 2
  (workspace profile). The test machine has 4 cores, 16 GB RAM and Linux
  glibc. Times are debug times and are only useful as relative numbers.
- Pictures are 4000x3000 photo-like RGBA images: smooth light plus +-6
  levels of noise. Raw RGBA is 45.8 MB. The tiny-skia PNG encoder makes a
  14 MB PNG of one. The `.cdr` reader's own PNG of the same pixels is 40 MB.
- The "curves" are 40,000 closed paths of 12 cubic segments, each with a
  solid fill and an outline.

## Growth sources, largest first

### 1. Undo history keeps a whole deep copy of the document per step, bitmaps included

- `crates/core/src/engine.rs:149` (`run_with_label`), `:179` (`run_batch`),
  `:214` (`amend`) and `:241` (`amend_batch`) all call `self.doc.clone()`.
  The undo entry keeps that copy (`HistoryEntry.other`, `:23`).
- The bitmap bytes are a plain `Vec<u8>`, so every copy duplicates every
  picture. See `ShapeKind::Bitmap.png` at `crates/core/src/document.rs:642-650`,
  `BitmapFxStack.png` at `:673-680`, `RasterFx.png` at `:1901-1915` and
  `Pattern::Bitmap.png` at `crates/core/src/style.rs:714-720`. Fonts and ICC
  profiles already use `Arc<Vec<u8>>` (`document.rs:3671-3740`). Pictures do
  not.
- The app default is 150 undo levels (`apps/tracedraw/src/settings.rs:600-602`).
  The engine default is 200 (`engine.rs:85`). Nothing caps the history by
  bytes.

Measured (`m2_history`):

| Drawing | Document RSS | Growth per edit (moving one small curve) | Time per edit, debug |
|---|---|---|---|
| 40k curves + 6 bitmaps (244 MB reader PNG) | 342 MB | +327 MB | 455 ms |
| 6 bitmaps only | 270 MB | +244 MB (exactly the PNG bytes) | 274 ms |
| 40k curves only | 98 MB | +82 MB | 175 ms |

- After 10 moves of one small curve: 3,614 MB, against 342 MB at open. At
  150 levels the history keeps growing toward about 49 GB.
- Undo and redo do not add memory: they swap the copies.
- A live colour drag (`apply_fill_live`, `apps/tracedraw/src/app.rs:4743-4759`)
  runs one undo and one run per pointer move. RSS stays flat, but every move
  makes a full copy of the drawing: 448 ms per move in debug on the mixed
  drawing. This is lag more than growth.

### 2. The canvas renderer decodes every visible picture again, once per band thread, on every render

- `render_page` -> `render_page_in_bands` (`crates/render/src/lib.rs:247-304`)
  splits any canvas of 250k pixels or more into one band per core, up to 16
  (`:252-256`). Each band runs on a new scoped thread (`:283`).
- The decoded-picture cache is `thread_local!`
  (`crates/render/src/image_cache.rs:60-62`, `decode` at `:83-91`). A fresh
  band thread always starts with an empty cache, and the cache is dropped
  when the thread ends. So every band that a picture crosses decodes the
  full picture again (`lib.rs:746` for bitmaps, `:1725` for bitmap patterns),
  on every render. The "decoded once and then kept" design never takes
  effect for the canvas on desktop.
- Measured (`m5_render`, 6 pictures in a 1920x1080 view, 4 cores):
  - Each canvas render peaks at +540 MB over the document, which is about
    11 full decodes of 46 MB. Each render takes about 1.5 s, almost all of
    it decoding. RSS falls back after each render because the threads end.
  - The peak scales with the core count. On a 16-core machine a picture that
    spans the view is decoded 16 times per render (about 740 MB for one
    4000x3000 photo).
  - This is also a CPU source for #616 (100% CPU on an older PC): every
    redraw after an edit or a zoom decodes every visible photo several times.

### 3. Save and auto-backup build the whole `.tdraw` as a `serde_json::Value` first

- `Document::to_json_version` (`crates/core/src/document.rs:4230-4242`) calls
  `serde_json::to_value(self)` and then `to_string_pretty`. The Value tree has
  one map per point, so it is many times larger than the document.
- Auto-backup (`apps/tracedraw/src/app.rs:5745-5790`) is on by default, every
  20 minutes (`settings.rs:835-844`). It runs on the UI thread and collects
  the JSON of every dirty drawing in `jobs` (`:5760-5772`) before writing any
  of them. `with_carried_fonts` (`crates/text/src/lib.rs:1230-1254`) clones
  the document when fonts are carried. Save `.tdraw` uses the same path
  (`app.rs:5696`).
- Measured (`m3_save_peaks`, `m6_json_direct`, 40k curves + 6 bitmaps at
  88 MB PNG, document 183 MB):

| Path | Peak | RSS after dropping the string (no trim) | RSS after `malloc_trim` |
|---|---|---|---|
| `to_json` (Value tree) | 2,407 MB | 2,012 MB | 203 MB |
| `serde_json::to_string_pretty(&doc)` | 618 MB | 224 MB | |
| `to_writer_pretty` into a `BufWriter<File>` | 204 MB | | |

- The 2 GB that stays after the save is glibc keeping the freed small
  allocations of the Value tree. Without a trim, every auto-backup leaves the
  process about 10 times the document size larger. The Windows heap behaves
  in a similar way for small blocks (to be confirmed on Windows).
- In debug, `to_json` takes 26 s. Release will be faster, but it still
  freezes the UI thread every 20 minutes on a large drawing.

### 4. Many page renders at once, each with a full copy of the document

- `apps/tracedraw/src/raster.rs:355-366`: once a page takes more than 30 ms to
  render, every later render calls `doc.clone()` (a deep copy with all
  pictures) and starts a new `std::thread`. That thread then splits into
  band threads, which decode as in source 2.
- `apps/tracedraw/src/multipage.rs:147-183` keeps one `Raster` per visible
  page (`page_rasters`, `app.rs:1166`). In Multipage view, every slow visible
  page starts its own job, with its own document copy, after every change.
- `invalidate()` (`raster.rs:220-229`) drops the receiver but leaves the
  thread running, so the next frame can start a second job alongside it.
- Measured (`m5_render`): one background job peaks at +600 MB (document copy
  plus decodes). Four page jobs at once peak at 2,669 MB, against 96 MB for
  the document.

### 5. Pictures kept as large, unfiltered PNGs; one copy per object that shows them

- `crates/cdr/src/parse.rs:4323-4345` `encode_png`: RGBA, no row filter,
  `Compression::fast()`. Measured (`m1`, `m8`) for one 4000x3000 photo:

| Encoding | Size |
|---|---|
| Raw RGBA | 45.8 MB |
| Reader PNG now (RGBA, no filter, fast) | 40 MB |
| RGBA with the Up filter, fast | 25 MB |
| RGB with the Sub filter, fast | 22 MB |
| RGB with the Sub filter, default level | 13 MB |

  So an opened `.cdr` holds about 2.9 times the PNG bytes of the same
  pictures in a `.tdraw`. Through source 1, that factor applies to every
  undo step too.
- `parse.rs:3312` and `:3364` (bitmap objects) and `:1205` (bitmap fills)
  copy the table's PNG into each object with `bm.png.clone()`. A picture
  placed N times is held N times.
- `crates/cdr/src/write.rs:2764-2770` (`bitmap_id`) writes one bitmap record
  per object, with no sharing, so the duplicates also reach the file.
- Measured (`m4`): one 1600x1200 picture (2.7 MB PNG) placed 30 times gives
  a 154 MB `.cdr`, and reading it back keeps 210 MB of PNG (30 x 7 MB).
- Bitmap import (`apps/tracedraw/src/ops.rs:586-610`) decodes the file to
  RGBA and keeps only a re-encoded PNG. A 3 to 5 MB JPEG photo becomes a PNG
  of 15 to 40 MB, and the original file is not kept (not measured here).

### 6. The UI-thread picture cache keeps full-size decodes made for small renders

- Renders on the UI thread fill that thread's cache, which holds up to
  1 GiB (`image_cache.rs:12-16`): the recent-file thumbnail
  (`apps/tracedraw/src/thumbnails.rs:44-56`, run on every open and save), the
  navigator (`apps/tracedraw/src/ui/mod.rs:833-848`) and the `.cdr` save
  previews (`app.rs:6950`). The canvas never reuses these decodes (it renders
  on band threads), so they only take memory.
- Measured (`m5`): after one 256x256 thumbnail render of the 6-picture page,
  RSS stays +275 MB (6 x 46 MB of full-size pixels kept for a 256 px
  picture).

### 7. `.cdr` read peak: the file, its inflated streams and two PNG copies at once

- `crates/cdr/src/lib.rs:84-100` holds, all at the same time: the file bytes,
  the RIFF stream, the external streams and the tree.
  - `container.rs:142-160` `riff_stream` copies the whole file for a RIFF
    container (`bytes.to_vec()` at `:144`) and inflates it for a ZIP one.
  - `container.rs:163-205` inflates every data member in full.
  - The bitmap table (`Ctx.bitmaps`, `parse.rs:258`) keeps every PNG while
    each object takes its own copy (source 5). `read_bitmaps`
    (`parse.rs:3378-3405`) decodes up to 4 at a time into full RGBA buffers.
- Measured (`m4`, a 195 MB `.cdr` with 40k curves and 6 photos): peak
  1,245 MB during `open_bytes`. 481 MB stays once the file bytes are
  dropped.
- `app.rs:6195-6211` (`.cdr` import into an open drawing) copies every shape
  twice (`originals`, then `reid(s.clone())`). The command batch then clones
  the document once more.

### 8. `begin_preview` deep-copies the redo stack

- `crates/core/src/engine.rs:322`: `redo: self.redo.clone()` copies every
  redo document. It is called by Edit Fill (`ui/edit_fill.rs:114`) and by the
  font list hover preview (`ui/font_list.rs:1290`).
- Measured (`m7`): with 5 steps undone on an 88 MB drawing, opening the
  preview adds +442 MB until the preview ends.

### 9. `.cdr` writer keys fills by their `Debug` text

- `crates/cdr/src/write.rs:2731-2741`: `format!("{fill:?}")` prints a bitmap
  pattern's PNG as a list of decimal numbers. That is about 5 bytes per PNG
  byte, and the key stays in `fill_ids` for the whole write.
- Measured (`m3`): a single 4 MB tile adds +62 MB of peak memory during the
  save. A 30 MB tile would add about 450 MB.
- A full `.cdr` save of the 183 MB drawing peaks at about +840 MB, because
  every bitmap is decoded and re-laid as a DIB at the same time.

### 10. Smaller per-render copies (lag more than RAM)

- `crates/core/src/document.rs:2054-2075` `drawn_as()` clones the whole shape
  and its raster PNG on every draw of an object with bitmap effects (called
  from `render/src/lib.rs:466`).
- `render/src/lib.rs:724`: a bitmap with opacity below 1 clones the shape,
  PNG included, and allocates a full-canvas layer on every draw.
- `render/src/lib.rs:632`: every clip frame copies the whole canvas pixmap on
  every draw.
- Also: `symmetry_edit.rs:241-249` caches one extra document copy while a
  symmetry is edited, `color_policy.rs:330` makes a transient copy at open,
  and `files.rs:32` (web build only) keeps every uploaded file for the whole
  session.

### Checked and not a source

- The visual, overflow and selected caches (`app.rs:1027-1046`) are cleared
  on each revision.
- The procedural fill cache is bounded (`render/src/lib.rs:2172-2207`: 12
  entries, 12 Mpx).
- The page textures are one per tab or page, at canvas size, with no mip
  levels.
- Document tabs swap their fields, they are not copied (`documents.rs`).
- Undo and redo themselves copy nothing.

## Fix plan, ranked by expected savings

Every item keeps the per-frame cost the same or lower (AGENTS.md rule 9). Each
item names the measurement in `ram_measure_test.rs` to run again before and
after, and the frame benchmarks (`cargo test --release -- --ignored
frame_times`) must not get slower.

1. **Share picture bytes: `Arc` instead of `Vec<u8>`** (core: `ShapeKind::Bitmap`,
   `BitmapFxStack`, `RasterFx`, `Pattern::Bitmap`). Add a `SharedBytes`
   newtype over `Arc<[u8]>` that keeps the base64 serde format (the existing
   `shared_bytes` module does this), with `Deref<Target=[u8]>` and a
   `PartialEq` that checks `Arc::ptr_eq` before comparing bytes. Code that
   makes new pixels replaces the `Arc`. About 200 match sites, mostly
   mechanical.
   - Savings: the bitmap share of every undo step (244 of 327 MB per edit in
     `m2`, all of the growth in a bitmap-only drawing). The reader's
     per-object copies (`m4`: 210 MB down to 7 MB). The background-job copy
     (`m5`). The `drawn_as` and opacity copies in the renderer.
   - Per frame: one pointer dereference, otherwise unchanged. The image cache
     can then key by pointer and length instead of sampling bytes.
2. **One process-wide decoded-picture cache, shared by all threads**
   (`image_cache.rs`). Replace `thread_local!` with a `Mutex<Cache>`. Each
   entry is an `Arc<OnceLock<Option<Arc<Pixmap>>>>`, so a picture is decoded
   once while other bands wait for that result instead of decoding it again.
   Lower the budget to 512 MB on desktop.
   - Savings: the per-render peak drops from bands x decoded size to one
     decode the first time, then nothing (`m5`: +540 MB and 1.5 s per render
     down to a one-time decode). The thumbnail and navigator decodes become
     the canvas's cache instead of extra memory.
   - Per frame: much faster, since decodes go and one short lock per drawn
     picture comes in. Large CPU win for #616 as well.
3. **Decoded pictures at the needed resolution (mip levels)**. Keep levels at
   1/2, 1/4 and so on in the cache. Pick the smallest level that is at least
   the on-screen scale times 1.5, and build it once from the full decode on a
   band or job thread.
   - Savings: zoomed-out pages and thumbnails keep about 1/16 to 1/64 of the
     pixels (`m5` thumbnail: 275 MB retained down to about 5 MB).
   - Per frame: the same or faster, since sampling smaller textures is
     cheaper. Edges look smoother when zoomed out.
4. **History bounded by memory as well as by step count** (engine). Track an
   estimate of each snapshot's unshared bytes and drop the oldest steps above
   a budget (for example 1 GB, or 25% of RAM, in Options > General next to
   Undo levels). This is cheap and protects against every other source while
   items 5 and 6 land.
   - Per frame: none.
5. **Copy-on-write shapes in the history** (core). Make top-level shapes
   `Vec<Arc<Shape>>`, or `Arc`-shared chunks of a layer, so a snapshot shares
   every shape the command did not touch. `Arc::make_mut` goes in the
   `find_shape_mut` and `layer_mut` paths.
   - Savings: the vector part of each step (`m2`: 82 MB per edit for 40k
     curves, down to about 0.3 MB for the pointer vector plus the changed
     shapes). Edits and live drags become proportional to what they change.
     Fixes the copy per pointer move of `apply_fill_live` and lag from
     cloning large drawings.
   - This is the largest code change (about 750 `.shapes` uses, mostly reads
     that auto-deref). The engine's own comment already plans a delta history
     behind the same API.
   - Per frame: one pointer dereference per shape while drawing, which is
     negligible.
6. **Save and auto-backup straight to the file** (`document.rs:4230-4242`,
   `app.rs:5745-5790`). For the current format version, use
   `serde_json::to_writer_pretty` (or compact) into a `BufWriter`. Build the
   Value only when writing an older version that needs `DOWN_STEPS`. Write
   each auto-backup as it is made instead of collecting `jobs`. After item 5,
   run it on a thread with a cheap snapshot.
   - Savings (`m3`/`m6`): peak 2.4 GB down to 0.2 GB; 2.0 GB kept afterwards
     down to about 0. Also removes a 20-minute UI freeze.
   - Per frame: none (fewer freezes).
7. **One background page job at a time** (`raster.rs:355-366`,
   `multipage.rs:147-183`). Queue the page jobs, keep only the newest request
   per page, and stop a job's result when `invalidate` makes it stale. After
   items 1 and 5 the `doc.clone()` is cheap. Until then, an
   `Arc<Document>` snapshot taken once per revision could be shared by every
   job.
   - Savings (`m5`): 4 jobs at once, peak 2.7 GB, down to one job.
   - Per frame: none. The current page goes first.
8. **Smaller PNGs from the `.cdr` reader** (`parse.rs:4323-4345`). Write RGB
   when the bitmap has no mask (alpha is always 255 otherwise), with a Sub or
   Up filter, keeping `Compression::fast()`. Measure open time before and
   after on the #323 file.
   - Savings (`m8`): 40 MB down to 22 to 25 MB per 4000x3000 photo in the
     document. Undo steps shrink too until item 1 lands.
   - Also: share the table entry instead of `bm.png.clone()` (`parse.rs:1205`,
     `:3312`, `:3364`); comes with item 1. Use a `Cow` instead of
     `bytes.to_vec()` for RIFF containers (`container.rs:144`), which saves
     one file size at the open peak.
   - Per frame: none.
9. **`begin_preview` without copying** (`engine.rs:322`). Use
   `std::mem::take(&mut self.redo)`. Put the stack back in `withdraw_preview`,
   and in `keep_preview` when no step was recorded.
   - Savings (`m7`): +442 MB down to 0.
   - Per frame: none.
10. **`.cdr` writer** (`write.rs:2731-2741`, `:2764-2770`). Key fills by a
    hash, or by the shared bytes' pointer, instead of the `Debug` text. Write
    one bitmap record per distinct picture.
    - Savings: the `m3` pattern-fill peak, and a 30-placement file going from
      154 MB to about 5 MB.
11. **Bitmap import keeps the original JPEG or PNG bytes** (`ops.rs:586-610`).
    Store the source encoding alongside, or instead of, the re-encoded PNG.
    The renderer needs a JPEG decoder for this, which is a new dependency, so
    it comes later.
12. **Linux: return freed heap memory** after large transient operations
    (save, auto-backup, closing a drawing, open). Call `malloc_trim(0)` behind
    `cfg(all(target_os = "linux", target_env = "gnu"))`. Optional:
    evaluate mimalloc for the app binary.

Suggested order: 1, 2, 9 and 6 are small or mechanical and remove the
largest measured numbers. Then 4 (safety net), 7, 8, 3, 10, and 5 as its own
project.

## Data still needed from the reporter

- OS, TraceDraw version, physical RAM, and CPU core count. The core count
  sets the decode multiplier in source 2 (up to 16 bands).
- Whether Multipage view is on, and how many drawings or tabs are open.
- Options > General Undo levels, and whether auto-backup is on and how
  often.
- The file, or its numbers: page count, object count, bitmap count with
  pixel sizes, whether the same picture is placed many times, and which
  `.cdr` version.
- Memory at each moment: right after open (before any edit), after the first
  fit-page render, after 5 and 20 edits, after a save, after closing the
  drawing, and while idle for 20+ minutes (auto-backup). On Windows, Task
  Manager's "Memory (private working set)" and "Commit size" columns.
- Whether memory goes back down after closing the drawing. If not, look for
  a retained cache or allocator behaviour rather than the history.
- `tracedraw.log` from that session.
