# Writing an extension (a tool)

An extension is a tool: it is told what the stylus, the mouse and the keyboard do on the page, may draw over the page, and says when it is
still moving. It is **built in** (compiled into the program, no loading of code, no sandbox), so every extension is read by the owner before
it is merged. Forks add extensions and send them back; a fork adds a folder and **edits no file of the program**.

The laser pointer is the first one: read `apps/inkpdf-ext/src/laser/` (about 130 lines and its tests).

## Add one in five minutes

```bash
tools/new-extension.sh my-tool "My tool"        # makes apps/inkpdf-ext/src/my_tool/{mod.rs,icon.svg}
$EDITOR apps/inkpdf-ext/src/my_tool/mod.rs      # change what the tool does; draw the icon in icon.svg
cargo test -p inkpdf-ext -p inkpdf --lib        # the program's own tests run on your tool by themselves
cargo run --release -p inkpdf                   # its button is on the toolbar, after the Select tool
```

Nothing else is edited: `apps/inkpdf-ext/build.rs` finds every folder in `src/` that has a `mod.rs` (each must have `pub fn extension() ->
inkpdf_api::Extension`). Two forks adding two tools do not touch the same file.

## What you get (`apps/inkpdf-api`, API version 1)

| You implement (`inkpdf_api::Tool`) | When it is called |
|---|---|
| `policy(desktop)` | which devices reach the tool: stylus, mouse (as a stylus, as itself, or only scrolling), keyboard. A finger never reaches a tool: it pans and zooms. |
| `pointer(cx, event)` | a stylus or mouse message on the page: `Down`, `Move`, `Up`, `Cancel`, `Hover`, `Leave`, with the samples since the last event, the pen's buttons (eraser end, barrel) and pressure. |
| `owns`, `busy`, `end`, `cancel`, `leave` | a gesture of yours is under way; a press elsewhere ends it; a prompt or a palm cancels it; you are put away (leave nothing to surface later). |
| `key(cx, key)` | the keyboard, if the policy says so. |
| `tick(now_ms)` | every frame: return `true` while you are moving by yourself (a trail fading); an idle window asks for no frames. |
| `pages_edited(cx)` | pages were added, deleted or moved: what you keep in document space may no longer be where it was. |
| `paint(cx, painter)` | what you show over the page: lines in document space or in screen pixels (round-ended; a dot is a line from a point to itself). |

`Context` is what you may ask: `to_doc`, `doc_to_screen`, `zoom`, `dpr`, `has_document`, `request_redraw`, `theme` (the colours of the
look now, so a tool can match light and dark), `page_at` (the page under a point), and, for a tool that makes items, `add_item`, `item_at`,
`open_item`, `move_item` and `remove_item`. That is all, on purpose.

## Making items that are kept in the document (item kinds)

A tool may make **items**: boxes on a page that the program keeps with the document, so that undo, moving, copying and pasting, the lasso and the
eraser, the page edits and a search all work on them **without any code of yours**. Implement `ItemKind` (`Tool::item_kinds` gives them to the
program, once, at the start):

| You implement (`inkpdf_api::ItemKind`) | What it is for |
|---|---|
| `id()` | the kind's name, written into the file (lower case letters, digits, `-`; it never changes) |
| `paint(item, out)` | draw one item, in document space, inside its box (`ItemView`: its text, box, colour, whether it is selected) |
| `write(data)` / `read(note)` | what the item looks like in a PDF, a `Note` (a text annotation: its text, author, date, replies, whether it is done), and back; a PDF note that `read` refuses stays as the file has it |
| `text(data)` | the words a search finds in it |
| `colour()` | the colour a new one has (a person can change it like any item's) |
| `card(data)` / `act(data, action, input, who)` | what its card shows (rows of text, a field, buttons) and what a button or a sent line does: the new data, one undo step |

`data` is a text that only your kind reads and writes (the sticky notes keep a thread in it). Read `apps/inkpdf-ext/src/notes/`: it is the first
kind, about 450 lines and their tests. With the barrel button and the eraser end never needed, a tool that makes items works with the left mouse
button or the stylus tip alone: the program's lasso, eraser and bar do the rest.

## Rules (a test fails if you break one)

* An extension depends on `inkpdf-api` and nothing else. A crate you need goes in `apps/inkpdf-ext/Cargo.toml` **and** in
  `EXTENSION_DEPENDENCIES` in `apps/inkpdf/src/architecture.rs`; the owner reviews both (a supply-chain gate).
* It does not read files, open connections, run programs or use `unsafe` (the same test). What it needs of the document comes through the API.
* Its name (`id`) is lower case letters, digits and `-`, at most 24, starts with a letter, and is unique; it is what the settings file
  remembers the tool by. `label` is what a person sees. `api` must be `API_VERSION`.
* Its icon is an SVG of stroked paths in a `viewBox` (absolute `M L C Z`, like the others), and draws inside its box.
* It is fast: a pointer event costs the pen's path next to nothing (a test times it: under 150 microseconds an event of four samples).
* It survives hostile input (non-finite positions, events in any order): the program's contract suite sends the same hostile streams to
  every registered tool, yours included.
* Colours: take them from `theme()` where you can; a colour of your own is fine for something that is the same in both looks.

## Extensions with no button, and links out to the web

An extension whose `icon_svg` is empty has no button on the toolbar: it is not a tool to choose, only what its item kinds and link handlers give. The
web-link extension (`apps/inkpdf-ext/src/weblink/`) is one: `Tool::link_handlers` gives it a `LinkHandler` that is asked for every link in a PDF that goes
out to the web. If it has a `Card` for the address (rows of text and buttons, no field), the program shows that card instead of handing the address to the
system's browser; a press of one of its buttons is answered with a `LinkAct` (`Open`, `Copy`, `Close`), and the program does it: an extension touches neither
the system nor the network.

## Versioning

The API follows semantic versioning (`API_VERSION` is its major number). It changes by adding. A change that would break an extension is a new
major version, and the program refuses (and says so on the terminal) an extension written for another one. Because every extension is in
this repository, a breaking change is made together with the changes it needs.

## What an extension cannot do yet

* Make an item that is anything but a box with a text (a note, a stamp): an item is a box and the text its kind reads; ink-like items with paths
  of their own are the program's.
* Have its own **bar under the tools** or **settings** (a thickness, a colour): the program's own tools have them; an extension's will, through
  the API, when one needs them.
* Read the page's text or the selection: it will be offered read-only.

Ask for what you need by opening an issue with the tool you are writing: the API grows from real tools.
