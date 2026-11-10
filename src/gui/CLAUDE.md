# img-fp: the window (`img-fp-gui`, feature `gui`)

Read with the root `CLAUDE.md`.

- The scan is a **child process** (`img-fp-gui --worker RESULT <argv>`, which
  saves its `Outcome` to RESULT),
  so Cancel is SIGINT. Spawned from the GTK main thread (PDEATHSIG is
  per-thread) by our own fork + `execv` of `/proc/self/exe` (`scan::spawn`;
  `std::process::Command` pulls in GLIBC_2.39 weak symbols, `current_exe()`
  breaks after a package upgrade).
- The worker's `Outcome` is written to `$XDG_RUNTIME_DIR`, read off the main
  thread (`results::prepare`); read on exit 1 too. The page keeps the
  `Outcome` and asks it for each rule's actions as files go or change.
- The Trash leaves a file whose length differs from the scan's, or whose
  ctime is from the scan's start on (`Notes::changed_after`): written,
  replaced, renamed or chmod'ed since. Suggestions resting on such a file
  turn REVIEW, as under `--last`. Files gone since are taken off the page
  (`drop_gone`); both are looked for off the GTK thread before any action
  (`Results::checked`). A file gio cannot trash is offered for permanent
  deletion (`delete_instead`).
- Rendered with cairo; textures premultiplied BGRA at drawn size
  (`thumbs::Size`); JPEG thumbnails via libjpeg scaled decode; freedesktop
  thumbnail cache used when current (`Thumb::MTime`, and `Thumb::Size` when
  written: mtime alone survives `cp -p`/`rsync -a`); HEIF/AVIF/JXL decode one
  at a time;
  texture budget by bytes (`KEEP_BYTES`, `STRIP_KEEP`), in screens: scaled
  by the largest monitor's pixels (`thumbs::keep_for`) and the strip's by
  its scale factor squared; a texture over half the budget is shown, not
  kept.
- Mosaic (`gui/mosaic.rs`) is virtual and its tiles pooled; strip is a
  `ListView`; `still.rs` keeps texture arrival from relaying out the page;
  tooltips/classes set only on change. Marks are vectors by file number
  (`Member::id`).
- UI decisions by the user: pictures take the page, details in a bottom bar
  of fixed columns; one selected image; Shift marks ranges; nothing
  pre-marked, suggestions offered (*Mark suggested deletions*, Alt+D),
  rule switchable without rescanning; Trash dialog names groups with every
  image marked; animations off; mnemonics always underlined (Cinnamon grabs
  Alt); F1 lists keys; dark theme followed from the desktop's text colour;
  tree view by folder (`gui/folders.rs`).
- The large view (`Results::preview`) is a window of its own, not modal
  (transient, so it stays above the page): it shows the page's *selected*
  image, so its arrows, the page's arrows and a click on a tile all move
  the one selection, and it looks again (`large_changed`, coalesced on idle)
  whenever `select` or `refresh_bar` runs; a new group selects its first
  image while it is open. Pictures are asked again only when their file
  changes. Info is three lines: path; size; evidence and suggestion.
  Fitted, a picture is decoded at the largest monitor's size
  (`thumbs::largest_screen`; was 1600 px whatever the screen); *Actual size*
  (1) decodes it whole (up to `ACTUAL_PIXELS`) and shows it pixel for
  pixel through `still::ActualSize` (a texture's size in the desktop's
  units: GTK reads pixels as units, 2x at scale 2), each side in a
  scroller, dragged to pan, centred when shown.
- Mnemonics live in `gui/labels.rs` with a clash test; add labels there.
- Keys live in `gui/commands.rs`: a `Section` per part of the window
  (`WINDOW`, `SCAN`, `RESULTS`, `LARGE`), each `Row` a line of the F1 list
  and the chords it describes; the results menu is `MENU` and its items'
  mnemonics. **Adding a key**: a binding in the row that says it, and an
  arm where the section's commands are done (`Results::run` for the page);
  a new command is a variant of the section's enum, and the tests fail
  until it is bound and described. Not a key controller of its own.
- The results page's two views are `results::View` (`Groups`, `Folders`):
  a question that differs between them is one `match` (`show_view`,
  `step`, `scope`, `side_list`) or a `View` method, not a flag tested where
  it is used.
- Options are one table, `gui/options.rs` (`Opt`: key in `gui.json`, clap
  id, `Kind`, label, tip, hint; `ALL`). `Settings` holds a `Value` per
  entry, `Setup` a `Control` per entry; defaults are clap's
  (`img_fp::flag`) unless the entry says otherwise (`recursive` is on in
  the window). **Adding an option**: an `Opt` in `ALL` and a place on the
  page for its widget; a flag the window does not show goes in `BY_HAND`
  or `NOT_SHOWN` (the test says which is missing). Not a field.
- Zoom (`Mosaic::zoom`, header `[-][+]`, Ctrl+Plus/Minus/0): a chosen row
  height, steps of √2 (twice/half the area); a group that fits the page at
  rows at least that tall is fitted as before. Limits are the page's: a row
  no taller than the page shown, none shorter than the tick plus a badge line
  (`Results::least_row`; an overlay measures as its main child only), no
  step down while all is on screen. `flow` ends each row where its height is
  nearest the target (stretch or shrink), not always after the overflow.
- Paths persist (`$XDG_CONFIG_HOME/img-fp/gui.json`), and options only
  where set away from their default, under `options` (`settings::Kept`), so
  an untouched one follows a moved default; clear/prune cache never kept;
  `recursive` on by default. Folders on the command line replace the
  remembered ones. Last scan and its marks (`marks.json`, keyed by scan id)
  kept in `$XDG_CACHE_HOME/img-fp/last-scan/`.
- Testing: Xvfb + `xdotool windowfocus --sync` + `import -window root`;
  on the real Cinnamon display activate with `wmctrl -i -a`. Point
  `XDG_DATA_HOME` at scratch before testing Trash. Other distros in Docker
  with `--network host` and a distinct display number per container; install
  `librsvg2-common`. Wayland: sway headless (`setcap -r`, `wtype -s 400`,
  `grim`) or headless gnome-shell driven via `org.gnome.Shell.Eval`.
