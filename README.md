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

Linux, x86_64 with AVX2 (Intel Haswell or newer, any AMD Zen), glibc 2.39 or
newer and libheif 1.17 or newer: Ubuntu 24.04+, Debian 13+, Fedora 40+, Arch,
openSUSE Tumbleweed and Leap 16, RHEL 10 and its rebuilds.

## Installation

### Prebuilt binary

First install libheif and its decoders:

```bash
sudo apt install libheif1 libheif-plugin-libde265 libheif-plugin-dav1d  # Ubuntu, Debian
sudo dnf install libheif                                                # Fedora, RHEL 10 (EPEL)
sudo pacman -S libheif                                                  # Arch
sudo zypper install libheif1 libheif-dav1d                              # openSUSE
```

Then the binary:

```bash
curl -L -o img-fp \
  https://github.com/Danielnara24/img-fp/releases/latest/download/img-fp-x86_64-linux-gnu
chmod +x img-fp
sudo install -m 755 img-fp /usr/local/bin/img-fp
```

libheif must be installed for the binary to start. The decoders are only
needed to read HEIC and AVIF files.

Fedora, openSUSE and RHEL leave out the HEIC decoder. Without it HEIC files are
reported as unreadable and everything else works. On Fedora it is
`libheif-freeworld` from RPM Fusion; on openSUSE, install `libheif1` and
`libheif-HEIF` from Packman.

Each release also ships a `.sha256` file if you want to verify the download:

```bash
sha256sum -c img-fp-x86_64-linux-gnu.sha256
```

### From source

Requires the Rust toolchain and libheif's development package:

```bash
sudo apt install libheif-dev pkg-config                  # Ubuntu, Debian
sudo dnf install libheif-devel pkgconf-pkg-config        # Fedora, RHEL 10 (EPEL)
sudo pacman -S libheif pkgconf                           # Arch
sudo zypper install libheif-devel pkg-config             # openSUSE
RUSTFLAGS="-C target-cpu=native" cargo install img-fp --locked
```

Without `target-cpu=native` the build uses slower portable code.

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
`--follow-symlinks`.

## Desktop app

`img-fp-gui` is a window over the same scan: pick folders and options, watch
the progress, then go through the groups and choose which images to move to the
Trash. Nothing is marked for you. It has the same requirements as `img-fp`,
plus GTK 4.10 or newer. With the libheif packages above installed:

```bash
sudo apt install libgtk-4-1          # Ubuntu, Debian
sudo dnf install gtk4                # Fedora, RHEL 10
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
sudo apt install libgtk-4-dev libheif-dev pkg-config                # Ubuntu, Debian
sudo dnf install gtk4-devel libheif-devel pkgconf-pkg-config        # Fedora, RHEL 10 (EPEL)
sudo pacman -S gtk4 libheif pkgconf                                 # Arch
sudo zypper install gtk4-devel libheif-devel pkg-config             # openSUSE
RUSTFLAGS="-C target-cpu=native" cargo install img-fp --locked --features gui
```

To add it to the applications menu, install the desktop entry from the extras
archive (or from this repository):

```bash
install -Dm644 applications/io.github.danielnara24.img-fp.desktop \
  ~/.local/share/applications/io.github.danielnara24.img-fp.desktop
```

Folders given on its command line are added to the list: `img-fp-gui ~/Pictures`.

Every control can be reached from the keyboard; the underlined letter of each
button and field is its Alt shortcut. F1 lists every key.

## Output

Each group has one representative, followed by every file
that matched it (don't interpret the representative as the source image):

```
group_1: 4 files
	REP,   2000x3000, 2.1MB, /photos/beach.jpg
	MATCH, 2000x3000, 2.1MB, identical, /backup/beach.jpg
	MATCH, 1000x1500, 250.8KB, 43 points, overlap 1.00, correlation 0.99, /phone/beach-crop.jpg
	MATCH, 666x1000, 85.4KB, 172 points, overlap 1.00, correlation 1.00, /phone/beach-small.jpg
```

- `points`: matching points the two images share
- `overlap`: how much of one image lies inside the other
- `correlation`: how closely the pixels of that shared area agree

A match can also be marked `mirrored` or `inverted`.

Every member was checked against the representative, not against the other
members. A file that matches two representatives appears in both groups.

`-o` picks the format from the file extension (`.txt`, `.csv` or `.json`), and
`--format` sets it explicitly. CSV has the same rows, `;`-separated. JSON also
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
| `-x`, `--extensions <EXT>` | Extensions a folder walk treats as images, comma-separated or repeated. `-x '*'` takes every file, including ones with no extension; an entry starting with `!` is an exception, so `-x '!gif'` takes every file but GIFs | every supported format |
| `-o`, `--output <FILE>` | Save the report as `.txt`, `.csv` or `.json`. `-` writes it to stdout | stdout |
| `--format <FORMAT>` | Write the report as `txt`, `csv` or `json`, whatever `--output` is called | from the extension |
| `--work-size <PX>` | Long side, in pixels, the images are analysed at. Higher finds more embedded images but is slower and uses more memory. `0` does not shrink images at all | `384` |
| `-k`, `--candidates <N>` | Candidate matches checked per image | `150` |
| `--min-aligned-points <N>` | Matching points two images must share. Higher is stricter | `10` |
| `--min-frame-overlap <F>` | How much of one image must lie inside the other, from 0 to 1. Higher is stricter | `0.85` |
| `--min-pixel-correlation <F>` | How closely the pixels of that shared area must agree, from 0 to 1. Higher is stricter | `0.6` |
| `-t`, `--threads <N>` | Worker threads (`0` = all cores) | `0` |
| `-v`, `--verbose` | Print timings for each stage | off |
| `--log-file <PATH>` | Write every skipped file, problem and stage timing to this file. Truncated at the start of each run | |
| `--dump <FILE>` | Write every candidate pair considered, accepted or not, to a CSV | |
| `--cache <PATH>` | Use this cache file instead of the default one | `$XDG_CACHE_HOME/img-fp/analysis.bin` |
| `--no-cache` | Don't read or write the cache | off |
| `--clear-cache` | Delete the cache before running | off |
| `--prune-cache` | Drop cached entries for images this scan did not find. Skipped when the scan was incomplete | off |
| `--completions <SHELL>` | Print a completion script for `bash`, `zsh`, `fish`, `elvish` or `powershell` and exit | |
| `--man` | Print the man page (roff) and exit | |

Loosening the three `--min-*` options finds more but eventually groups
different photographs together. The defaults are a safe starting point.

## Formats

JPEG, PNG, GIF, WebP, BMP, TIFF, AVIF, HEIC/HEIF, JPEG XL, ICO, PNM, TGA, DDS,
QOI, OpenEXR and farbfeld. Files are identified by their content, so a wrong
extension doesn't matter, but a folder walk only picks up files whose extension
is in `-x`.

## Cache

The analysis of each image is cached in `$XDG_CACHE_HOME/img-fp/analysis.bin`
(or `~/.cache/img-fp/analysis.bin`), so later runs over the same images are much
faster. One cache serves every folder you scan, and entries for deleted files
are dropped automatically. Use `--prune-cache` to keep only the images in the
current scan, or delete the file to start over.

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

The released binary links the system's `libheif` (LGPL-3.0) dynamically; it is
not bundled.
