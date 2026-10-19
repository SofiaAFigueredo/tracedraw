# VBA macros

## Object model

VBA macros are compiled to JavaScript and run against the object model
VBA macros for drawing suites use (`scripting::run_vgcore`,
`apps/tracedraw/src/scripting/vg_model.js` over the `vg_*` host methods
in `scripting/vg.rs`). Member names, argument names and order, 1-based
indexes and the `cdr*` constants are the ones those macros are written
against, so the compiled code calls them unchanged. TraceDraw's own
JavaScript object model (the Scripts panel, lengths in millimetres) is
separate and unchanged.

Rules that hold everywhere:

- **Units.** Every length (positions, sizes, outline widths, radii,
  page sizes) is in `ActiveDocument.Unit`: inches unless the macro sets
  it (`cdrInch`, `cdrMillimeter`, `cdrCentimeter`, `cdrPoint`, `cdrPica`,
  `cdrPixel` = 1/96 in, `cdrFoot`, `cdrYard`, `cdrMeter`, `cdrKilometer`,
  `cdrMile`, `cdrDidots`, `cdrCicero`, `cdrAgate`, `cdrTenthMicron`).
  The unit holds for the rest of the run. Coordinates start at the
  page's lower left corner, y up; angles are degrees, counter-clockwise.
- **Reference point.** `ActiveDocument.ReferencePoint` (`cdrCenter`
  unless set; `cdrTopLeft`, `cdrBottomLeft`...) is the point
  `PositionX`/`PositionY` read, `SetPosition` places and `SetSize` and
  `Stretch` scale about.
- **One step.** Everything a run changes is one undo step;
  `BeginCommandGroup`/`EndCommandGroup` are accepted and change nothing.
  A failure keeps the changes made before it.
- **Out parameters.** Methods that hand values back through ByRef
  arguments return them as an array in parameter order: `GetPosition`
  gives `[x, y]`, `GetSize` `[w, h]`, `GetBoundingBox` `[x, y, w, h]`.
- **Named arguments.** Methods are declared with their reference
  parameter names (`Left`, `Top`, `Right`, `Bottom`, `CornerUL`...), so
  named arguments match by name.
- **Collections** are 1-based, take an index or a name in `Item`, have
  `Count`, and iterate (`For Each`) in drawing order.

### Application (also global)

`ActiveDocument`, `ActivePage`, `ActiveLayer`, `ActiveSelection` (the
selection as one object, type `cdrSelectionShape`, with `Shapes`),
`ActiveSelectionRange`, `ActiveShape` (the first selected object),
`ActiveWindow` (`Refresh`, `ActiveView`, accepted without effect),
`Documents` (the active drawing only), `Version` "27.0", `VersionMajor`
27, `VersionMinor`, `Name` "TraceDraw", `Visible`, `Optimization`,
`EventsEnabled`, `Refresh`, `CreateColor(ColorString)` ("#RRGGBB",
"RGB255,USER,r,g,b", "CMYK100,USER,c,m,y,k", "GRAY255,USER,v"),
`CreateRGBColor`, `CreateCMYKColor`, `CreateGrayColor`,
`CreateHSBColor`, `CreateShapeRange`, `CreateCurve`. `CreateDocument`
and `OpenDocument` raise an error: a macro works on the active drawing.
`Status` (`BeginProgress`, `Progress`, `EndProgress`, `SetMainText`:
the text and percentage go to the status line), `ColorManager`
(`DefaultColorContext` profile names, `ColorEngine`), `CorelScript`
(`RedrawScreen`, no effect), `UILanguage`, `CreateStructCreateOptions`.

### Document

`Unit`, `ReferencePoint`, `Pages`, `PageCount`, `ActivePage`,
`ActiveLayer`, `ActiveShape`, `Selection()`, `SelectionRange()`,
`SelectableShapes`, `ClearSelection`, `SelectAll`, `BeginCommandGroup`,
`EndCommandGroup`, `AddPages(n)`, `AddPagesEx(n, Width, Height)`,
`InsertPages(n, BeforePage, Page)`, `CreateCurve`, `Save` (to its file),
`SaveAs(FileName)` (`.cdr`, else the native format), `Export(FileName,
Filter)`, `ExportEx` and `ExportBitmap(FileName, Filter, Range,
ImageType, Width, Height, ResolutionX)` (both return a filter whose
`Finish` writes the file), `PublishToPDF`, `Name`, `Title`, `FileName`,
`FilePath`, `FullFileName`, `Dirty`, `Resolution`. Export writes the
active page: SVG, PDF (all pages), EPS, CDR, the native format, PNG, JPEG,
BMP, TIFF, GIF and WebP, chosen by the `cdr*` filter constant or by the
file's extension; bitmaps at the resolution given, or scaled to
`Width` x `Height` pixels, else 96 dpi. `Undo` and `Redo` raise an error
(the run is one step).

### Page

`Shapes` (the objects on its layers), `Layers`, `AllLayers` (with the
master layers), `ActiveLayer`, `SelectableShapes` (on visible, unlocked
layers), `Name` (read and write), `Index`, `SizeWidth`, `SizeHeight`,
`SetSize`, `GetSize`, `Orientation` (`cdrPortrait`, `cdrLandscape`;
setting it swaps the sides), `LeftX`, `RightX`, `TopY`, `BottomY`,
`CenterX`, `CenterY`, `Activate`, `Delete`, `CreateLayer`, `FindShapes`,
`FindShape`.

### Layer

`Shapes`, `Name`, `Visible`, `Printable`, `Editable`, `Master`,
`IsSpecialLayer`, `Page`, `Index`, `Activate`, `Delete`, and the
creation methods, which give new objects no fill and the default
outline:

- `CreateRectangle(Left, Top, Right, Bottom, CornerUL, CornerUR,
  CornerLR, CornerLL)`: corners as roundness, percent of half the
  shorter side.
- `CreateRectangle2(x, y, Width, Height, RadiusUL, RadiusUR, RadiusLR,
  RadiusLL)`: radii as lengths.
- `CreateEllipse(Left, Top, Right, Bottom, StartAngle, EndAngle, Pie)`,
  `CreateEllipse2(CenterX, CenterY, Radius1, Radius2, StartAngle,
  EndAngle, Pie)`.
- `CreatePolygon(Left, Top, Right, Bottom, Sides, SubPaths, Complexity,
  Star, StarComplexity, Sharpness)`, `CreatePolygon2(CenterX, CenterY,
  Radius, Sides)`.
- `CreateCurve(Curve)`: a curve built with `CreateCurve`,
  `CreateSubPath(x, y)`, `AppendLineSegment(x, y)`,
  `AppendCurveSegment2(x, y, StartingControlPointX,
  StartingControlPointY, EndingControlPointX, EndingControlPointY)` and
  `Closed`.
- `CreateLineSegment`, `CreateCurveSegment(StartX, StartY, EndX, EndY,
  StartingControlPointLength, StartingControlPointAngle,
  EndingControlPointLength, EndingControlPointAngle)`.
- `CreateArtisticText(Left, Bottom, Text, LanguageID, CharSet, Font,
  Size, Bold, Italic, Underline, Alignment)`,
  `CreateParagraphText(Left, Top, Right, Bottom, Text, ...)`.
- `Import(FileName)` (pictures).

### Shapes, ShapeRange and the selection

`Shapes` (of a layer, page or group): `Count`, `Item`, `All()`,
`Range(IndexArray)`, `AllExcluding`, `FindShapes(Name, Type, Recursive,
Query)`, `FindShape(Name, Type, StaticID, Recursive, Query)`.

`ShapeRange` (creatable with `New`): `Count`, `Item`, `Shapes`,
`FirstShape`, `LastShape`, `Add`, `AddRange`, `Remove`, `RemoveAll`,
`RemoveRange`, `IndexOf`, `Exists`, `Range`, `ReverseRange`, `Reverse`,
`Sort("@shape1.Left < @shape2.Left")` (Left, Right, Top, Bottom,
CenterX, CenterY, Width, Height, Name; `>` sorts down), `Group`,
`Combine`, `Ungroup`, `UngroupAll`, `BreakApart`, `Duplicate`, `Clone`,
`FindShapes`, `FindShape`, `AlignAndDistribute(MethodH, MethodV)`,
`DistributeHorizontally`, `DistributeVertically`, and every geometry and
style member below, acting on the range as one.

Queries take comparisons of `@name`, `@type` (rectangle, ellipse,
curve, polygon, bitmap, text, group, symbol) and `@fill.type` (none,
uniform, fountain, pattern, texture) with `=` or `<>`, joined by `and`
and `or`: `"@type = 'rectangle' and @name <> 'Frame'"`.

### Shape

Shared with ranges and the selection:

- **Position and size:** `PositionX`, `PositionY`, `SizeWidth`,
  `SizeHeight`, `LeftX`, `RightX`, `TopY`, `BottomY`, `CenterX`,
  `CenterY` (read and write), `GetPosition`, `GetSize`,
  `GetBoundingBox`, `SetPosition`, `SetSize` (a zero or missing side
  keeps the proportions), `SetSizeEx(CenterX, CenterY, Width, Height)`,
  `SetBoundingBox(x, y, Width, Height, KeepAspect)`.
- **Transforms:** `Move`, `Rotate` (about the rotation centre),
  `RotateEx`, `Skew`, `SkewEx`, `Stretch`, `StretchEx`, `Flip(cdrFlip...)`.
- **Style:** `Fill`, `Outline`, `Transparency`, `ApplyUniformFill`,
  `ApplyNoFill`, `SetOutlineProperties`.
- **Edit:** `Delete`, `ConvertToCurves`.
- **Selection:** `AddToSelection`, `RemoveFromSelection`,
  `CreateSelection`.
- **Order:** `OrderToFront`, `OrderToBack`, `OrderForwardOne`,
  `OrderBackOne`, `OrderFrontOf`, `OrderBackOf`.
- **Alignment:** `AlignToShape(cdrAlign..., Shape)`, `AlignToPage`,
  `AlignToPageCenter`, `AlignToPoint`.
- **State:** `Locked`, `Visible`, `OverprintFill`, `OverprintOutline`.

`Shape` alone:

- **Identity:** `Type` (`cdrRectangleShape`, `cdrEllipseShape`,
  `cdrCurveShape`, `cdrPolygonShape`, `cdrBitmapShape`, `cdrTextShape`,
  `cdrGroupShape`, `cdrSymbolShape`; a clip frame reads as its frame's
  type), `Name`, `StaticID`, `IsValid`, `IsSame`, `Selected`.
- **Rotation:** `RotationAngle`, `RotationCenterX`, `RotationCenterY`
  (kept for the run).
- **Copies:** `Duplicate(OffsetX, OffsetY)` (with new ids inside groups
  too), `Clone`.
- **Shaping:** `BreakApart`, `BreakApartEx`, `Weld`, `Trim` and
  `Intersect(TargetShape, LeaveSource, LeaveTarget)` (they return the
  new curve).
- **Groups and frames:** `Ungroup`, `UngroupEx`, `UngroupAll`,
  `UngroupAllEx`, `Shapes` (a group's), `Layer`, `Page`, `ParentGroup`,
  `PowerClip` (`Shapes` inside a clip frame), `PowerClipParent`.
- **Views of the object:**
  - `Curve`: `Nodes`, `SubPaths`, `Segments`, `Closed`, `Length`.
    Nodes have `PositionX`, `PositionY`, `SetPosition`, `Move`, `Type`
    (`cdrCuspNode`, `cdrSmoothNode`, `cdrSymmetricalNode`), `IsEnding`,
    `SubPathIndex`, `Index` and `Delete`; subpaths have `Nodes`,
    `Closed`, `StartNode` and `EndNode`; segments have `StartNode` and
    `EndNode`.
  - `Text`: `Type`, `Replace(OldText, NewText, CaseSensitive)` and
    `Story`, with `Text`, `Font`, `Size` in points, `Bold`, `Italic`,
    `Underline`, `Alignment` (`cdrLeftAlignment`...), `Length`,
    `Characters.Count` and `Words.Count`.
  - `Rectangle`: `CornerUpperLeft`... as roundness, `RadiusUpperLeft`...
    as lengths, `SetRoundness`, `SetRadius`, `Width`, `Height`.
  - `Ellipse`: `StartAngle`, `EndAngle`, `Type`, `Pie`, `IsCircle`.
  - `Polygon`: `Sides`, `Type`, `Sharpness`.
  - `Bitmap`: `SizeWidth` and `SizeHeight` in pixels.
- **Data:** `Effects` (read only: `Count`, `Item(i).Type`), `ObjectData`
  and `Properties` (`Item(name).Value` read and write, `Get`, `Set`).

### Fill, Outline, Transparency

- **`Fill`:** `Type` (`cdrNoFill`, `cdrUniformFill`, `cdrFountainFill`,
  `cdrPatternFill`, `cdrTextureFill`; a mesh reads as uniform),
  `UniformColor` (a colour tied to the object: `RGBAssign` and the other
  assignments change the fill), `ApplyNoFill`, `ApplyUniformFill(Color)`,
  `ApplyFountainFill(StartColor, EndColor, Type, Angle)` (linear), and
  `CopyAssign`.
- **`Outline`:** `Type` (`cdrNoOutline`, `cdrOutline`), `Width`, `Color`
  (tied to the object like the fill's), `Style` (`DashCount`,
  `SetDashes`), `LineCaps`, `LineJoin`, `SetNoOutline`,
  `SetProperties(Width, Style, Color)`, and `CopyAssign`.
- **`Transparency`:** `Uniform`, `ApplyUniformTransparency(Value)` (0 to
  100), `ApplyNoTransparency`.

### Color

Creatable with `New`.

- **Assign:** `RGBAssign`, `CMYKAssign`, `CMYAssign`, `GrayAssign`
  (0 to 255), `BWAssign`, `HSBAssign`, `HLSAssign`, `LabAssign`,
  `RegistrationAssign`, `CopyAssign`, `StringAssign`.
- **Convert:** `ConvertToRGB`, `ConvertToCMYK`, `ConvertToGray`,
  `ConvertToHSB`, `ConvertTo(cdrColor...)`.
- **Read:** `Type` (`cdrColorRGB`, `cdrColorCMYK`, `cdrColorGray`,
  `cdrColorHSB`, `cdrColorHLS`, `cdrColorLab`, `cdrColorSpot`,
  `cdrColorRegistration`), `RGBRed`, `RGBGreen`, `RGBBlue`, `RGBValue`,
  `CMYKCyan`, `CMYKMagenta`, `CMYKYellow`, `CMYKBlack` (all read and
  write), `Gray`, `HSBHue`, `HSBSaturation`, `HSBBrightness`,
  `HexValue`, `Name`, `IsSpot`, `IsCMYK`, `IsSame`.

### Constants

The enumerations above are globals: units, reference points, shape
types, fill and fountain types, colour types, outline types, flip axes,
alignment (`cdrAlignLeft` 1, `cdrAlignRight` 2, `cdrAlignHCenter` 3,
`cdrAlignTop` 4, `cdrAlignBottom` 8, `cdrAlignVCenter` 12), text
alignment and type, font lines, node and segment types, page
orientation, export ranges, export filters, image types, `cdrTrue` and
`cdrFalse`.

Most values are the documented ones. These values are assumed and only
need to differ from each other:

- the export filters `cdrCDR`, `cdrPNG`, `cdrJPEG`, `cdrBMP`, `cdrTIFF`,
  `cdrGIF`, `cdrPDF`, `cdrSVG`, `cdrEPS` and `cdrWEBP`;
- `cdrColorMixed`;
- the shape types from `cdrHTMLFormObjectShape` on.

### Coverage from published macros

Four published macro projects (60 modules) were read only to count
which members they call; none of their code is in TraceDraw. The members
called three times or more, with what they do here (W works; N works with
no visible effect, because the feature it controls is a window mode or
does not exist; E raises an error that names the member; F belongs to
macro forms, not to this object model; R belongs to the VBA runtime or to
other libraries):

| Member | Uses | Here |
|---|---|---|
| Count, Item, Add, AddRange, RemoveAll, RemoveRange, Range, ReverseRange, All, Shapes, Layers, Pages | 195, 13, 99, 5, 66, 7, 5, 64, 53, 60, 19, 15 | W |
| Type, Name, Layer, Page, Delete, Duplicate, Activate | 173, 96, 43, 39, 7, 3, 16 | W |
| Fill, UniformColor, Fountain, Colors, ApplyUniformFill, CopyAssign | 105, 56, 22, 19, 3, 13 | W (fountain `Steps` set: E) |
| Outline, Width, Color, ScaleWithShape | 103, 31, 109, 10 | W (`MiterLimit` set: E) |
| CMYKCyan, CMYKMagenta, CMYKYellow, CMYKBlack, ConvertToRGB, ConvertToCMYK, ConvertToGray, Tint | 25 each, 10, 7, 3, 4 | W (`Tint` set on a non-spot colour: E) |
| Text, Story, Words, Range, Size, Bold, Italic, Font, LineSpacing, Overflow, IsArtisticText | 100, 12, 3, 5, 9, 9, 9, 3, 8, 4, 3 | W (`LanguageID`: E) |
| Bitmap, ResolutionX, ResolutionY, Mode, ConvertTo, Resample, ExternallyLinked, Resolution | 51, 12, 12, 7, 14, 5, 3, 4 | W |
| ConvertToBitmapEx, ConvertToCurves | 6, 4 | W |
| PowerClip, PowerClipParent | 44, 8 | W |
| EnterEditMode, LeaveEditMode | 3, 3 | N |
| Effects, DropShadow, Separate, Clear | 14, 6, 7, 14 | W (a separated shadow loses its blur) |
| MergeMode | 6 | W for normal; others E |
| Transparency | 14 | W (uniform; other kinds read their type) |
| Curve, Nodes, SubPaths, FirstNode, LastNode, Selection, Area, Length | 11, 4, 6, 22, 4, 9, 7, 3 | W |
| CanHaveFill, CanHaveOutline | 19, 21 | W |
| Visible, Printable, Editable, IsSpecialLayer, Master, MasterPage, UnlockAllShapes | 14, 14, 14, 12, 7, 4, 6 | W |
| IsGuidesLayer, IsGridLayer | 6, 6 | W (always false: guides and grid are not layers) |
| CreateGuide, Guide | 7, 3 | W |
| PositionX, PositionY, SizeWidth, SizeHeight, Height, SetSize, GetSize, SetPosition, GetPosition, Rotate | 16, 16, 9, 9, 5, 7, 6, 5, 3, 4 | W |
| Unit, ReferencePoint, Units | 18, 13, 3 | W |
| ClearSelection, CreateSelection, AddToSelection, FindShapes, OrderBackOf, CreateRectangle2, ActiveLayer | 30, 8, 7, 6, 3, 3, 3 | W |
| OverprintFill, OverprintOutline | 6, 5 | W |
| Status, BeginProgress, Progress, EndProgress | 22, 9, 14, 9 | W (shown in the status line when the run ends) |
| BeginCommandGroup, EndCommandGroup, PreserveSelection, SaveSettings, RestoreSettings | 4, 4, 6, 3, 3 | W |
| ColorContext, DefaultColorContext, ColorEngine | 6, 3, 4 | W (profile names in use) |
| ActiveWindow, ActiveView, OriginX, OriginY | 4, 13, 3, 3 | W (the window's centre); view changes N |
| CorelScript, RedrawScreen | 5, 4 | N |
| Finish | 3 | W (exports and imports) |
| Paste, FrameWork, Close | 3, 3, 3 | E |
| PageWidth, PageHeight | 3, 3 | E (the options of a new document: `CreateDocumentEx` is E) |
| AddItem, SpecialEffect, Enabled, Caption, Show, SelText, ListIndex, List, ListCount, ControlTipText, SelLength, Value | | F |
| Number, Description, Kill, Date, Case, ToString | | R |

The constants they use (128 names) all exist, except five names the
projects define themselves and six enumeration names used only to
declare variables (`cdrShapeType`, `cdrColorType`, `cdrImageType`,
`cdrUnit`, `cdrTextType`, `cdrAntiAliasingType`).

The remaining rarer members follow the same rules:

- Works: `MoveToLayer`, `OrderIsInFrontOf`, `AddToPowerClip`,
  `FindAnyOfType`, `SelectShapesAtPoint`, `ImportEx`, `DrawingOriginX`
  and `DrawingOriginY` (0: the origin is the page corner).
- Raise a named error: `GetUserClick`, `GetUserArea`, `DesktopLayer`,
  `UserAssignEx`, `PasteSpecial`, `StartShape`, `Fill.Pattern`,
  `GetPixelColor`, `Quit`, `VBE`.

### Not supported

- More than one drawing: `CreateDocument`, `OpenDocument`, and
  `Documents` beyond the active drawing.
- `Undo` and `Redo` inside a run, `Paste` and `PasteSpecial`.
- Window and interactive members: `GetUserClick`, `GetUserArea`,
  `FrameWork`, `VBE`, `Quit`, and the colour picker (`UserAssignEx`).
- Arrowheads: `StartArrow` and `EndArrow` read as nothing and setting
  them is ignored.
- The text's own language (`Story.LanguageID`), texture resolutions,
  fountain steps, a miter limit other than the renderer's, shadow merge
  modes other than normal.
- Fountain fills other than linear.
- Hatch and PostScript fills.
- Editing a curve built by a macro after it became an object
  (`AppendCurve`, `CreateNodes`).
- Segment types.
- Queries beyond `@name`, `@type` and `@fill.type`.
- Events.
- Export of the selection or of a range of pages: export writes the
  active page, and PDF writes every page.
- The view: `ActiveWindow.ActiveView` takes calls without effect.

### Checks (`scripting::vg::tests`)

- Units, both ways:
  - `cdrMillimeter` and `cdrInch` lengths land in the drawing exactly;
  - a 2 x 1 inch rectangle reads back as 2 x 1.
- Reference points: `SetPosition(0, 0)` with `cdrBottomLeft` puts the
  lower left corner on the page origin.
- Collections:
  - `Item` is 1-based and takes names;
  - `For Each` sees every object;
  - `FindShapes` with `Recursive` finds objects inside groups;
  - a group's `Shapes` lists its children.
- Colours and outlines:
  - `Fill.UniformColor.RGBAssign` changes the fill;
  - CMYK components read back;
  - the outline width follows the unit.
- Text: the story's text, size, bold and alignment change the text
  object.
- Shaping: `Weld` replaces both objects with one curve; `Group` and
  `UngroupAll` keep every id unique, also when pages are added in the
  same run.
- Community-style macros:
  - resize the selection to 20 x 10 mm;
  - number every object with a text at its centre;
  - lay out a 3 x 3 grid;
  - replace a colour across the page, inside groups too;
  - export every page as PNG.
- Members published macros use (one test per area):
  - progress in the status line;
  - bitmap resolution, mode, resampling and conversion;
  - converting objects to bitmaps;
  - clip frame contents and parents;
  - effects with drop shadows, their merge mode and separating them;
  - guides;
  - fountain colours and transparency;
  - text words, ranges, overflow and line spacing;
  - curve areas and the Shape tool's node selection;
  - document settings, master page, layers, unlocking, view origin and
    colour profiles;
  - the named errors;
  - the constants they use.
