# img-fp: the Flatpak (`flatpak/`)

Read with the root `CLAUDE.md`.

`io.github.danielnara24.img-fp`: both binaries in `/app/bin`, GTK 4 from
the GNOME runtime (so Ubuntu 22.04 runs the window), libheif 1.23.6 with
libde265 built in and the runtime's dav1d in `/app/lib`, ahead of the
runtime's own libheif (AVIF only, no HEVC). `--filesystem=host`: a duplicate
finder is pointed anywhere; Flathub's linter flags it and a submission needs
the exception asked for. Built offline from `cargo-sources.json`
(flatpak-builder-tools' `flatpak-cargo-generator.py` over `Cargo.lock`;
**regenerate it whenever Cargo.lock changes**, the release job checks).
A version bump needs its `<release>` at the top of the metainfo's
`<releases>`, one plain sentence for users (`metainfo_names_this_release`
fails without it; 0.68.0-0.73.0 were tagged without one).
`release.yml`'s `flatpak` job attaches `img-fp-x86_64.flatpak` to each
release. Local build: the command at the top of the manifest
(`org.flatpak.Builder`, user install, no sudo).

Inside the sandbox `$XDG_DATA_HOME` / `$XDG_CACHE_HOME` / `$XDG_CONFIG_HOME`
are the app's own under `~/.var/app/<id>/`: the cache, the kept scan and
`gui.json` live there, apart from a host install's. What must be the
desktop's is read through `walk::desktop_dir` (`$HOST_XDG_*` when the host
set them, else the defaults under `$HOME`): the Trash and thumbnail cache a
walk sets aside, and the thumbnail cache the window reads. The Trash itself
needs nothing: `gio::File::trash` goes through the Trash portal. The
sandbox's `/tmp` is its own, so a host-side test reads outputs written under
`$HOME`.

The window as the host draws it needs two things the sandbox does not give
by itself. The desktop's GTK 4 styling is `~/.config/gtk-4.0/gtk.css`, read
from `$XDG_CONFIG_HOME`, so the sandbox drew GTK's plain default theme.
`--filesystem=xdg-config/gtk-4.0:ro` maps it in, but Flathub's linter rejects
that (`finish-args-unnecessary-xdg-config-gtk-4.0-ro-access`); with `host`
the file is readable anyway, and `main::desktop_stylesheet` loads it at
`STYLE_PROVIDER_PRIORITY_USER` when the app's own is missing. And the runtime's GTK (4.24) leaves
`gtk-font-rendering` "automatic" where the desktop does not set it (Cinnamon,
MATE, Xfce do not), which drops the hinting and subpixel antialiasing the
desktop's Xft settings ask for: soft grey text, thinner mnemonic underlines.
`main::follow_desktop_fonts` sets "manual" on GTK >= 4.16 (4.14's
behaviour); the dark variant is `gtk-interface-color-scheme` on >= 4.20,
since `gtk-application-prefer-dark-theme` warns there (`set_by_nick`: enums
newer than the `v4_10` bindings). Checked under Xvfb with the desktop's
XSETTINGS copied over: text pixel-identical to the host's GTK 4.14 build.

Verified at 0.66.0 under Xvfb: scan, worker, Cancel, Trash (lands in the
host's `~/.local/share/Trash`), restore of the kept scan, HEIC thumbnails,
large and compare views. The CLI in the sandbox is byte-identical to the
host build on a folder without HEIF; with HEIC/AVIF it differs (105 vs 106
groups on IMGS3's `WhatsApp Images`) only through libheif 1.23.6 vs the
host's 1.17.6 decoding other pixels, which moves the vocabulary and so every
pair a little: the host binary run inside the sandbox gives the Flatpak's
result exactly. Not tested here: the portal file chooser, drag and drop,
"open folder", Wayland.
