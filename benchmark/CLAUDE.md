# img-fp benchmark: competitors' runners

Read with the root `CLAUDE.md` (*Benchmarking discipline*, *Corpora*).

## Gotchas already paid for

- czkawka exits 11 on finding duplicates (`-W`), skips files < 16 KB unless
  `-m 1`; use `-z Lanczos3 --geometric-invariance mirror-flip-rotate90`.
- findimagedupes: space-separated output; needs a hard-link farm under
  `~/.cache` (it resolves symlinks).
- imagededup: no CLI, not recursive, keys by basename; runner builds a flat
  symlink farm.
- SSCD: unreadable files need a placeholder tensor of the batch's shape.
- difPy: run `.../site-packages/difPy/dif.py` directly; `-s 200` already
  merges everything.
- dupeGuru: runner supplies a PIL-backed `Photo` subclass.
- A tool that cannot decode a format looks like one that failed to match.
