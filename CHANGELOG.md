# Changelog

## 0.2.0

- The application layer was rewritten around a `Tool` trait: Pen, Shape, Text and Pointer are separate
  objects that own their gestures and paint their own overlays; the device rules are data
  (`Tool::policy`); the window's asks of the shell are a request queue. See `apps/inkpdf/ARCHITECTURE.md`.
- A layering test and a tool contract suite guard the structure.
- Long strokes cost the same per frame however long they get (the live curve is extended incrementally),
  and a frame carrying new pen samples no longer waits for tile uploads.
- Standalone portable build (`installer/build-portable.ps1`) with settings kept beside the exe.
- Behaviour is otherwise unchanged from 0.1.11.

## 0.1.x

- 0.1.11: input rules per tool (the text tool is keyboard and mouse only; the stylus does nothing to the page
  with it); line and curve tools drawn by tapping.
- 0.1.10: shapes reworked into kinds with the anchors that suit them (lines and polygons, splines,
  rectangles, ellipses, axes); straight segments stay straight.
- 0.1.8-0.1.9: text tool (typed text saved as Free Text), wrapping at the page edge.
- 0.1.5-0.1.7: shape tool and editable anchors, per-tool settings, page insertion.
- 0.1.0-0.1.4: blank page start, links, swipe-up to add a page, in-app save prompt, Explorer thumbnails,
  pen panel, pointer tool.
