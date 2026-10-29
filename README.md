# egui-agent-testing

How to test a GUI that has no DOM: a canvas app in Rust (egui) that an AI agent
can see and drive, and tests that walk the whole interface instead of clicking
one button. A small, working version of the approach behind the
[Pixtorium](https://pixtorium.com) image editor.

It is a notes app with a menu, a text field, a list and a settings panel. The
interesting part is not the app but the two things around it:

- **`src/driver.rs`** (about 300 lines): an egui plugin that reads egui's
  accessibility tree (AccessKit, the counterpart of the DOM) and feeds real
  mouse and keyboard events into egui's input, served over MCP. Four tools:
  `ui_snapshot`, `ui_click`, `ui_type`, `ui_state`.
- **`tests/walk.rs`**: tests built on `egui_kittest` that find controls by
  role and label, and *walkers* that check a property of every control.

## Try it

```
cargo run -- --driver 8799
claude mcp add --transport http notes http://127.0.0.1:8799/mcp
```

(`.mcp.json` in this repo does the same.) Then ask Claude Code to use the app
as a novice would. A snapshot looks like this:

```
- button "File" [ref=e2]
- button "Settings" [ref=e23]
- text "Ready" [ref=e6]
- text "Notes" [ref=e14]
- text "New note" [ref=e19]
- textinput "New note" [ref=e1]
- button "Add note" [ref=e20]
- text "0 notes" [ref=e7]
```

An agent needs no coordinates and no image recognition: it says "click
`Add note`" or "click e20".

## The tests

```
cargo test
```

| Test | What it shows |
| --- | --- |
| `every_control_has_a_name` | Every button, field and slider has a name. The same names serve tests, screen readers and the agent. |
| `no_two_controls_overlap_at_any_width` | A *walker*: at four window widths, no two controls overlap. |
| `the_walker_catches_an_overlap` | The walker must fail on broken code. A layout that stacks two buttons has to be reported. |
| `adding_a_note_by_label`, `an_empty_note_says_why_it_was_not_added` | Ordinary tests: find by label, click, type. |

Two rules the walkers follow:

1. **A walk that finds nothing proves nothing.** Each walker asserts it
   checked at least N controls.
2. **A test must fail on the old code.** Both walkers failed on the first
   version of this very app: the text field had no name, and at 300 px wide the
   settings panel overlapped the notes. The fixes are in `src/lib.rs`
   (`labelled_by`, and settings taking the whole window when it is narrow).

## Limits

This is a teaching version. It does not take screenshots, drag or scroll, does
not check that a widget is covered by a menu before clicking it, and does not
wait for long computations. A real driver needs those; the editor it comes
from has them.

MIT licensed.
