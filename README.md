# img-fp

[![Release](https://img.shields.io/github/v/release/Danielnara24/img-fp?logo=github)](https://github.com/Danielnara24/img-fp/releases/latest)
[![crates.io](https://img.shields.io/crates/v/img-fp?logo=rust)](https://crates.io/crates/img-fp)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

**Find duplicate images from the Linux command line**, including crops, resizes
and pictures pasted into other pictures.

`img-fp` compares images by their local features rather than by a single hash of
the whole picture, so it still finds a match after an image has been
re-encoded, resized, cropped, rotated, recoloured, or placed inside a slide,
collage or screenshot. Every match is checked against the pixels before it is
reported.

## What it finds

- Exact copies under different names or in different folders
- The same image re-saved at a different size, quality or format
- Crops, including small ones
- Rotated, mirrored or colour-adjusted versions
- An image embedded in a larger one: a slide, a collage, a PDF page, a
  screenshot

The command line only reports; nothing is moved or deleted. The desktop app
shows each group and moves the images you choose to the Trash.

## Requirements

Linux on x86_64 with glibc 2.34 or newer: Ubuntu 22.04+, Debian 12+, Fedora
35+, Arch, openSUSE Tumbleweed and Leap 15.4+, RHEL 9+ and its rebuilds.
img-fp runs on any x86_64 CPU and uses AVX2 when the CPU has it (Intel Haswell
or newer, any AMD Zen).

HEIC, HEIF and AVIF files are read through the system's libheif (1.12 or
newer), when it is installed. Without it those files are reported as
unreadable and every other format works.

## Installation

### Prebuilt binary

```bash
curl -L -o img-fp \
  https://github.com/Danielnara24/img-fp/releases/latest/download/img-fp-x86_64-linux-gnu
chmod +x img-fp
sudo install -m 755 img-fp /usr/local/bin/img-fp
```

To read HEIC and AVIF files, install libheif and its decoders:

```bash
sudo apt install libheif1 libheif-plugin-libde265 libheif-plugin-dav1d  # Ubuntu 24.04+, Debian 13+
sudo apt install libheif1                                               # Ubuntu 22.04, Debian 12
sudo dnf install libheif                                                # Fedora, RHEL (EPEL)
sudo pacman -S libheif                                                  # Arch
sudo zypper install libheif1 libheif-dav1d                              # openSUSE
```

Fedora, openSUSE and RHEL leave out the HEIC decoder. Without it HEIC files are
reported as unreadable and everything else works. On Fedora it is
`libheif-freeworld` from RPM Fusion; on openSUSE, install `libheif1` and
`libheif-HEIF` from Packman.

Each release also ships a `.sha256` file if you want to verify the download:

```bash
sha256sum -c img-fp-x86_64-linux-gnu.sha256
```

### From source

Requires the Rust toolchain:

```bash
cargo install img-fp --locked
```

libheif is not needed to build; it is loaded when the first HEIC or AVIF file
is read, as for the prebuilt binary.

### Shell completions and man page

Each release ships `img-fp-<version>-extras.tar.gz` with completions for bash,
zsh and fish plus a man page:

```bash
tar -xzf img-fp-*-extras.tar.gz
sudo install -Dm644 completions/img-fp.bash /usr/share/bash-completion/completions/img-fp
sudo install -Dm644 completions/_img-fp     /usr/share/zsh/site-functions/_img-fp
sudo install -Dm644 completions/img-fp.fish /usr/share/fish/vendor_completions.d/img-fp.fish
sudo install -Dm644 man/img-fp.1            /usr/share/man/man1/img-fp.1
```

Without `sudo`, into your home directory (for zsh, use any directory on your
`fpath`):

```bash
install -Dm644 completions/img-fp.bash ~/.local/share/bash-completion/completions/img-fp
install -Dm644 completions/img-fp.fish ~/.config/fish/completions/img-fp.fish
install -Dm644 man/img-fp.1            ~/.local/share/man/man1/img-fp.1
```

A source build can generate both itself:

```bash
img-fp --completions bash | sudo tee /usr/share/bash-completion/completions/img-fp >/dev/null
img-fp --man | sudo tee /usr/share/man/man1/img-fp.1 >/dev/null
```

## Usage

```bash
# Report duplicates in a folder and all its subfolders
img-fp ~/Pictures -r

# Scan several folders, excluding one
img-fp ~/Pictures ~/Downloads -e ~/Downloads/keep -r

# Save the report as CSV or JSON
img-fp ~/Pictures -r -o dupes.csv

# Find more images embedded in slides, screenshots and collages, at some cost in speed
img-fp ~/Pictures -r --work-size 640
```

By default the scan is **not** recursive. Add `-r` to include subfolders.

Individual files can be named alongside folders. A list of paths can be read
from stdin with `-`, or from a file with `--from-file`:

```bash
fd -e jpg --changed-within 30d ~/Pictures | img-fp -
find ~/Pictures -name '*.png' -print0 | img-fp - -0
img-fp --from-file paths.txt
```

A file reached through a symlink, a hard link or two overlapping folders is
scanned once. Symlinks met inside a folder are skipped unless you pass
`--follow-symlinks`. Trash folders and thumbnail caches met inside a folder are
skipped too, since they hold copies of pictures that are elsewhere or were
deleted; name one directly to scan it. Hidden folders (names starting with a
dot) are skipped unless you pass `--hidden`, since in a home folder they hold
programs' icons and themes rather than photos; a hidden folder named directly is
scanned.

`-o`, `--dump` and `--log-file` never write over an image: naming one is
refused before anything runs.

## Desktop app

`img-fp-gui` is a window over the same scan. Pick folders and options, start
the scan, then go through the groups and choose which images to move to the
Trash.

- The groups are listed down the left side, each shown by one of its pictures
  with the number of images in it, and how many are marked once you mark some.
- A group's images fill the rest of the window. Point at an image, or select
  it with the arrow keys, to see its name, size, how it matched and what the
  scan suggests doing with it, in the bar at the bottom. The reference image,
  the one the others were matched against, is labelled, and weak matches have
  a yellow corner.
- Click the circle on an image, or press Space, to mark it for the Trash.
  Double click or Enter opens it large.
  Shift and a click marks or unmarks every image from the last one you
  marked or unmarked to the one clicked; Shift and an arrow key marks or
  unmarks each image it moves over the same way.
- The menu button at the top right, or a right click on an image, opens the
  image or its folder, marks every other image in the group, or unmarks the
  group or every group.
- *Switch to tree view*, in the same menu, lists the scanned folders down the
  left instead of the groups. Selecting a folder shows every grouped image in
  it and in its subfolders, and *Mark current folder* marks all of them.
  *Switch to group view* goes back.
- *Suggestion rule* switches between the three rules of `--suggest` without
  scanning again. Nothing is marked for you: *Mark suggested deletions* marks
  exactly the images suggested for deletion, replacing any marks already made.
- The Trash button moves the marked images to the Trash, where they can be
  restored. *Scan settings* goes back to the folders and options, keeping the
  results.
- The results of the last scan are kept in `~/.cache/img-fp/last-scan/` and
  shown again the next time the window opens, with the images you marked still
  marked and without the images already moved to the Trash. The next scan to
  finish replaces them.

Every control can be reached from the keyboard. Underlined letters are Alt
shortcuts, the menu shows its own, and F1 lists every key.

It has the same requirements as `img-fp`, plus GTK 4.10 or newer (Ubuntu
24.04+, Debian 13+, Fedora 38+, RHEL 9+, openSUSE Tumbleweed and Leap 15.6+,
Arch):

```bash
sudo apt install libgtk-4-1          # Ubuntu, Debian
sudo dnf install gtk4                # Fedora, RHEL 9+
sudo pacman -S gtk4                  # Arch
sudo zypper install libgtk-4-1       # openSUSE
curl -L -o img-fp-gui \
  https://github.com/Danielnara24/img-fp/releases/latest/download/img-fp-gui-x86_64-linux-gnu
chmod +x img-fp-gui
sudo install -m 755 img-fp-gui /usr/local/bin/img-fp-gui
```

From source, with the `gui` feature, which installs both `img-fp` and
`img-fp-gui`:

```bash
sudo apt install libgtk-4-dev pkg-config                # Ubuntu, Debian
sudo dnf install gtk4-devel pkgconf-pkg-config         # Fedora, RHEL 10
sudo pacman -S gtk4 pkgconf                            # Arch
sudo zypper install gtk4-devel pkg-config              # openSUSE
cargo install img-fp --locked --features gui
```

To add it to the applications menu, install the desktop entry from the extras
archive (or from this repository):

```bash
install -Dm644 applications/io.github.danielnara24.img-fp.desktop \
  ~/.local/share/applications/io.github.danielnara24.img-fp.desktop
```

Folders given on its command line are the ones to scan, in place of the ones
remembered from last time: `img-fp-gui ~/Pictures`. It then opens on the scan
settings, with the last scan's results one click away. With the desktop entry
installed, file managers offer it under "Open With" for a folder.

## Output

Each group has one representative, followed by every file
that matched it (don't interpret the representative as the source image):

```
group_1: 4 files
	REP,   KEEP,   2000x3000, 2.1MB, /photos/beach.jpg
	MATCH, DELETE, 2000x3000, 2.1MB, identical, /backup/beach.jpg
	MATCH, DELETE, 1000x1500, 250.8KB, 43 points, overlap 1.00, correlation 0.99, /phone/beach-crop.jpg
	MATCH, DELETE, 666x1000, 85.4KB, 172 points, overlap 1.00, correlation 1.00, /phone/beach-small.jpg
```

The second column suggests what to do with each file. img-fp never deletes
anything itself.

- `KEEP`: the best copy of the picture, a file that shows something the
  others don't, such as a collage, a slide or a meme made from it, or a
  different photo that only looks similar
- `DELETE`: everything in it, apart perhaps from a thin strip at an edge, is
  also in a file marked to stay, at about the same detail or better
- `REVIEW`: a weak match, probably a copy, such as one tinted or
  watermarked, that could not be confirmed. Look at it before deciding

A file has the same suggestion in every group it appears in. That is the
default rule, `--suggest content`. Two simpler rules read only the groups:

- `--suggest correlation`: each group's representative is `KEEP`. Another
  file is `DELETE` when it agrees with a representative closely, from halfway
  between `--min-pixel-correlation` and 1 (0.8 by default), `KEEP` when it agrees less, and `REVIEW` below
  `--min-pixel-correlation`
- `--suggest representative`: each group's representative is `KEEP` and
  every other file is `DELETE`

Both can suggest deleting a file that holds something the representative
doesn't, such as a collage or the uncropped photo.

- `points`: matching points the two images share
- `overlap`: how much of one image lies inside the other
- `correlation`: how closely the pixels of that shared area agree

A match can also be marked `mirrored` or `inverted`.

Every member was checked against the representative, not against the other
members. A file that matches two representatives appears in both groups.

`-o` picks the format from the file extension (`.txt`, `.csv` or `.json`), and
`--format` sets it explicitly. CSV has the same rows, `;`-separated, with the
suggestion in the `action` column. JSON also
lists every matched pair. The report goes to stdout; progress and the summary go
to stderr.

## Options

| Flag | Description | Default |
| --- | --- | --- |
| `<PATH>...` | Folders and/or image files to scan (required). `-` reads a list of paths from stdin | |
| `--from-file <FILE>` | Read the paths to scan from a file, one per line (`-` = stdin) | |
| `-0`, `--null` | Paths in the list are NUL-separated, for `find -print0` / `fd -0` | off |
| `-r`, `--recursive` | Include subfolders | off |
| `-e`, `--exclude <PATH>` | Leave out a folder or file; repeat for several | |
| `--follow-symlinks` | Follow symlinks met while walking a folder | off |
| `--hidden` | Also walk hidden folders, whose names start with a dot | off |
| `-x`, `--extensions <EXT>` | Extensions a folder walk treats as images, comma-separated or repeated. `-x '*'` takes every file; an entry starting with `!` is an exception, so `-x '!gif'` takes every file but GIFs. A file with no extension is taken only by those two forms | every supported format |
| `-o`, `--output <FILE>` | Save the report as `.txt`, `.csv` or `.json`. `-` writes it to stdout | stdout |
| `--format <FORMAT>` | Write the report as `txt`, `csv` or `json`, whatever `--output` is called | from the extension |
| `--work-size <PX>` | Long side, in pixels, the images are analysed at. Larger images are shrunk to it and small ones enlarged up to it. Higher finds more embedded images but is slower and uses more memory. `0` does not shrink images at all | `512` |
| `-k`, `--candidates <N>` | Candidate matches checked per image | `150` |
| `--min-aligned-points <N>` | Matching points two images must share. Higher is stricter; below 3 behaves as 3 | `10` |
| `--min-frame-overlap <F>` | How much of one image must lie inside the other, from 0 to 1. Higher is stricter | `0.85` |
| `--min-pixel-correlation <F>` | How closely the pixels of that shared area must agree, from 0 to 1. Higher is stricter | `0.6` |
| `--suggest <RULE>` | How each file's `KEEP`, `DELETE` or `REVIEW` is decided: `content`, `correlation` or `representative` (see Output) | `content` |
| `-t`, `--threads <N>` | Worker threads (`0` = all cores) | `0` |
| `-v`, `--verbose` | Print timings for each stage | off |
| `--log-file <PATH>` | Write every skipped file, problem and stage timing to this file. Truncated at the start of each run. `-` writes it to stdout | |
| `--dump <FILE>` | Write every candidate pair considered, accepted or not, to a CSV. `-` writes it to stdout | |
| `--cache <PATH>` | Use this cache file instead of the default one | `$XDG_CACHE_HOME/img-fp/analysis-IMGFPC11.bin` |
| `--no-cache` | Don't read or write the cache | off |
| `--clear-cache` | Delete the cache before running | off |
| `--prune-cache` | Drop cached entries this scan did not use: images it did not find, and analyses made at another `--work-size`. Skipped when the scan was incomplete | off |
| `--completions <SHELL>` | Print a completion script for `bash`, `zsh`, `fish`, `elvish` or `powershell` and exit | |
| `--man` | Print the man page (roff) and exit | |

Loosening the three `--min-*` options finds more but eventually groups
different photographs together. The defaults are a safe starting point.

## Formats

JPEG, PNG and APNG, GIF, WebP, BMP, TIFF, AVIF, HEIC/HEIF, JPEG XL, ICO,
PNM and PAM, TGA, QOI, OpenEXR, Radiance HDR and farbfeld. Files are identified by their content, so a wrong
extension doesn't matter, but a folder walk only picks up files whose extension
is in `-x`. Files with no extension are picked up only with `-x '*'` (or an
exception such as `-x '!gif'`).
TGA files have no signature and are recognised by the `.tga` extension.

A JPEG that is cut off, such as an interrupted download, is still analysed as
far as it goes, and is marked `DAMAGED` in the report.

## Cache

The analysis of each image is cached in `$XDG_CACHE_HOME/img-fp/analysis-IMGFPC11.bin`
(or `~/.cache/img-fp/analysis-IMGFPC11.bin`), so later runs over the same images are much
faster. One cache serves every folder you scan and every `--work-size` you use,
and entries for deleted files are dropped automatically. Use `--prune-cache` to
keep only the images in the current scan at the current `--work-size`, or
`--clear-cache` to start over.

The part of the name after `analysis-` is the cache format, which changes
between some versions. Each format has its own default file, so two versions
installed side by side keep separate caches, and after an upgrade the new
version starts a new one. The old file is left in place; delete it by hand to
reclaim the space (versions before 0.35 used `analysis.bin`).

A file named with `--cache` that was written by another version is left as it
is, and the run keeps nothing in it. Use `--clear-cache` to replace it.

Pressing Ctrl-C keeps every image analysed so far.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Ran clean |
| `1` | Fatal error; the run did not finish |
| `2` | Finished, but something failed, such as an image that would not decode. See the `Problems` summary |
| `130` | Interrupted with Ctrl-C |

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

The released binary loads the system's `libheif` (LGPL-3.0) at run time when
it is installed; it is not bundled.
