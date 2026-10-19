# VBA macros (language)

The `tracedraw-vba` crate compiles VBA modules (`.bas`, `.cls`, `.frm`
code) to JavaScript that runs on the same engine as TraceDraw scripts,
over the same object model. This page covers the language; the object
model is documented with scripting.

## API

- `compile(&[Module]) -> Result<Program, CompileError>`: all modules of
  a project are compiled together so calls between modules resolve.
  `Module { name, kind: Standard | Class | Form | Document, source }`.
  A `CompileError` names the module and the VBA line.
- `RUNTIME_JS`: the VBA runtime (`__v`), evaluated once before the
  program.
- `entry_call(&program, module, proc)`: the JavaScript that runs one
  procedure. Run-time errors come back as
  `Run-time error 'n': description (Module.Proc, line l)`; `End` stops
  quietly.
- `list_macros(&modules)`: the public parameterless Subs of standard
  and document modules (not `Option Private Module`), the list a
  Macros dialog shows.
- `Program::vba_line(js_line)` maps a JavaScript line back to the
  module and VBA line.

## Host hooks

| Name | Use |
|---|---|
| `__host_msgbox(prompt, buttons, title)` | MsgBox; returns the button value (`vbOK` = 1 when missing) |
| `__host_inputbox(prompt, title, default)` | InputBox; returns the text |
| `__host_names` | host globals declared with `const`/`class`, so lowercase names still resolve |
| `fn.__params = ["x", ...]` | parameter names of a host method, needed for named arguments |
| `__host_form_create/show/hide/unload(form)` | forms; the host fills `form.__controls` |
| `form.__event(control, event, args)` | the host fires `Control_Event` handlers |

## Supported

Case-insensitive names everywhere, type suffixes, `&H`/`&O`, date
literals, line continuation, `#If`/`#Const` (VBA7 and Win64 true),
`Option Explicit/Base/Compare/Private Module`, Dim/Static/Const/ReDim
Preserve/Erase, arrays with any bounds and up to many dimensions, UDTs
(value copies), Enums, Sub/Function/Property Get/Let/Set, ByRef (default)
and ByVal, Optional with defaults, IsMissing, ParamArray, named
arguments, If (block and single line), Select Case (`Is`, `To`, lists),
For/Next, For Each, Do/Loop in all forms, While/Wend, With (nested),
Exit, GoTo to labels and line numbers, On Error Resume Next / GoTo
label / GoTo 0, Resume, Resume Next, Resume label, Err object (Number,
Description, Source, Raise, Clear), End, Set and object identity (`Is`,
`Nothing`), TypeOf ... Is, `!` access, default members (host `Item`,
class `VB_UserMemId = 0`), class modules with properties, Me,
Class_Initialize, forms code-behind (UserForm_Initialize, control
events, Show/Hide/Unload, default instances), Debug.Print with `;` and
`,` zones, Mid statement, Variant operators with VBA coercion, Like,
banker's rounding for integer conversions.

Built-ins: string functions (Left, Mid, InStr, InStrRev, Replace,
Split, Join, Trim, StrComp, StrConv, Format, Space, String, Chr, Asc,
Val, Hex, Oct...), math (Abs, Int, Fix, Round, Sqr, Rnd with the VBA
generator, Randomize...), conversions (CInt, CLng, CDbl, CStr, CBool,
CDate, CCur, CVar...), tests (IsNumeric, IsDate, IsEmpty, IsNull,
IsArray, IsObject, VarType, TypeName), arrays (Array, LBound, UBound,
Filter), dates (Now, Date, Time, DateSerial, DateAdd, DateDiff,
DatePart, Year...Second, Weekday, MonthName, Timer), MsgBox, InputBox,
IIf, Choose, Switch, Collection, `CreateObject("Scripting.Dictionary")`
and the vb constants.

## Not supported

These compile and fail at run time with error 445
`Not supported in TraceDraw: ...` (or 429 for objects):

- `Declare` and any Windows API call.
- File statements: Open, Close, Print #, Write #, Input #, Line Input,
  Get, Put, Kill, Name, MkDir, RmDir, ChDir, FileCopy, Dir.
- COM objects other than `Scripting.Dictionary` (FileSystemObject,
  Excel, Word, Shell...).
- GoSub/Return, On x GoTo/GoSub, AddressOf, Implements.
- RaiseEvent does nothing; WithEvents is ignored; Class_Terminate is
  not called.
- GoTo only reaches labels at the top level of a procedure.
- Named arguments to host methods need `__params`, otherwise error 448.
- Debug.Print does not add the leading space VBA prints before
  positive numbers; dates format as m/d/yyyy.

## Checks

- Given `For Each s In ActiveSelection.Shapes`, when it runs, then each
  selected shape is visited once (test `walks_the_selection...`).
- Given `On Error GoTo H` and a handler that fixes the divisor and calls
  `Resume`, then the failed statement runs again and succeeds.
- Given `Open "x" For Output As #1`, then the run stops with error 445
  naming the Open statement.
- Given `Tool.Place(label:="L", y:=2, x:=1)` with `__params`, then the
  host receives `(1, 2, "L")`.
