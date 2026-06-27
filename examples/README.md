# cadtab examples

Sample documents for trying out the editor and the persistence / import / bundle
features. All of these are kept compiling by tests in `cadtab-core`
(`standalone_example_compiles_cleanly`, `project_example_compiles_with_its_lib`)
and `app` (`the example .ctabz parses and matches the project files`).

Run the app with `just dev` (desktop) or `just web` (browser).

## Files

### `showcase.ctab`
A syntax reference: metadata, tunings, pickup, musical repeats with 1st/2nd
endings, ties, rests, chords/pinches, techniques, and both loop forms. Open it to
see most of the language on one page.

### `cripple-creek.ctab` — single-file persistence
A complete standalone score with no imports.

- **Open / Save (`Cmd/Ctrl+O` / `Cmd/Ctrl+S`):** open it, edit a note, save. On
  desktop the second save overwrites in place (no dialog); in the browser it
  re-downloads.
- **Dirty flag:** edit a note (the name shows a `•`), then undo (`Cmd/Ctrl+Z`)
  back to the original — the `•` clears.

### `cripple-creek-project/` — multi-file import (desktop)
A project split across two files:

- `cripple-creek.ctab` — the entry score; it `import`s `licks.ctab` and also
  calls `forward_roll` from the embedded stdlib.
- `licks.ctab` — the imported library (`g_chord` and a `tag` lick).

On **desktop**, open `cripple-creek-project/cripple-creek.ctab`: `licks.ctab`
resolves as a sibling on disk, so `tag(...)` and `g_chord` work and the score
renders. (Open the entry *without* the sibling — e.g. by itself in the browser —
and you'll see `cannot resolve import "licks.ctab"` plus an unknown-name error:
that's the import system reporting a missing file.)

### `cripple-creek.ctabz` — project bundle (browser)
The same project packed into one bundle file (`{ version, entry, files }` JSON).

- **Open (browser):** `Open` accepts `.ctabz`. Opening it loads the entry into the
  editor and resolves `import "licks.ctab"` from the bundle — multi-file works in
  the browser with no filesystem.
- **Save Project (`Cmd/Ctrl+Shift+S`):** writes the whole project back out as a
  `.ctabz`. Round-trips on desktop too (the bundle's files are checked before the
  filesystem).

### `templates/`
The starter scaffolds behind the toolbar's **New…** dropdown (banjo, guitar,
blank) — minimal clean-compiling documents seeded with the right instrument /
tuning / time / default lines. The app `?raw`-imports these, and `cadtab-core`
compile-checks them (`starter_templates_compile_cleanly`).

## Exporting

With any of these open, **Export SVG** / **Export PNG** in the toolbar save the
rendered tab as a standalone image (a printable black-on-white sheet). The SVG is
self-contained; the PNG is rasterized from it. Both work on desktop and in the
browser.

> The `.ctabz` is generated from `cripple-creek-project/`. If you change those
> files, regenerate it so the two stay in sync (the app test above will fail
> otherwise):
>
> ```sh
> node -e 'const f=require("fs"),d="examples/cripple-creek-project",e="cripple-creek.ctab";f.writeFileSync("examples/cripple-creek.ctabz",JSON.stringify({version:1,entry:e,files:{[e]:f.readFileSync(d+"/"+e,"utf8"),"licks.ctab":f.readFileSync(d+"/licks.ctab","utf8")}},null,2)+"\n")'
> ```
