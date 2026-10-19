//! One scan, from the paths asked for to the report: `run` and its stages.

use super::*;

/// What every stage of a run reads: what it was asked, and where it says
/// what it is doing.
struct Ctx<'a> {
    args: &'a Args,
    log: &'a Log,
    progress: progress::Progress,
    t: Instant,
}

impl Ctx<'_> {
    /// A line about the run's progress: on the console under `-v`, and in
    /// the log file whatever the console asked for. `-v` is about the
    /// terminal and `--log-file` is about the file, and one flag must not
    /// quietly decide the other. (That exact confusion is written up in
    /// `vid-fp`'s `verbosity`, where a `-q --log-file` run wrote an empty
    /// log.) Made only when something reads it.
    fn stage(&self, line: impl FnOnce() -> String) {
        if self.args.verbose || self.log.active() {
            let line = format!("[{:6.1}s] {}", self.t.elapsed().as_secs_f64(), line());
            if self.args.verbose {
                self.progress.println(&line);
            }
            self.log.line(&line);
        }
    }

    /// Said on the console whatever the flags, and kept in the log with it.
    fn say(&self, line: &str) {
        self.progress.println(line);
        self.log.line(line);
    }
}

macro_rules! stage {
    ($cx:expr, $($arg:tt)*) => {
        $cx.stage(|| format!($($arg)*))
    };
}

macro_rules! say {
    ($cx:expr, $($arg:tt)*) => {
        $cx.say(&format!($($arg)*))
    };
}

/// What the run was asked about: which files ask the index first, which
/// pairs the report speaks of, and which files the keep rule may not delete.
/// A plain scan is the scope in which every file is the scan's.
///
/// `--against` is the scope with a library: the scanned files ask, then the
/// library files they reach, and the library a scan never touches asks
/// nothing (see `retrieve_and_verify` and `against.rs`). Nothing is said
/// about two of its files, and none of them is deleted.
struct Scope {
    /// Whether the run was asked against a library, even one that walked to
    /// no file: the report is the library's kind either way.
    against: bool,
    /// Whether each file is the library's; all false in a plain scan.
    library: Vec<bool>,
}

impl Scope {
    /// Whether the file asks in the first round of retrieval.
    fn asks_first(&self, i: usize) -> bool {
        !self.library[i]
    }

    /// Whether the run states the pair: against a library, nothing is said
    /// about two of its files, since the run was not asked and in the
    /// default mode did not look.
    fn speaks_of(&self, a: usize, b: usize) -> bool {
        !(self.library[a] && self.library[b])
    }

    /// The files the keep rule keeps whatever the rest say.
    fn pinned(&self) -> &[bool] {
        &self.library
    }

    /// A query's best `k` candidates. Against a library not every file asks,
    /// so each candidate is read both ways round (`both_ways`), by the idf
    /// mass of the query's list and of every file's (`mass`, kept only then).
    fn rank(&self, scored: &mut Vec<(u32, f32)>, query_mass: impl FnOnce() -> f32, mass: &[f32], k: usize) {
        if self.against {
            both_ways(scored, query_mass(), mass);
        }
        rank_best(scored, k);
    }
}

/// One scan, from the paths asked for to the report: each stage below hands
/// the next what it found, and lets go of what nothing after it reads.
pub(crate) fn run(args: &Args, log: &Log, problems: &mut Problems, gui: Option<&Path>) -> Result<()> {
    // Before anything is read: a file changed since is not one this run can
    // vouch for (`last::Notes::changed_after`).
    let started = last::now_nanos();
    // Before anything else: a `-x` that can match no file is a mistake to
    // stop on, not a walk to take.
    let (wanted, extensions_note) = extensions::normalize(&args.extensions)?;
    // So is a path list that cannot be read, and it is read before anything
    // has a side effect: `--clear-cache` with a mistyped `--from-file` must
    // not delete the cache and then stop.
    let (roots, lists) = walk::requested_roots(
        &walk::Sources {
            named: &args.roots,
            from_file: args.from_file.as_deref(),
            null_separated: args.null,
        },
    )?;
    prof::start();
    let t_start = Instant::now();
    few_arenas();
    if args.threads > 0 {
        rayon::ThreadPoolBuilder::new().num_threads(args.threads).build_global()?;
    }
    let cx = Ctx { args, log, progress: progress::Progress::new(), t: t_start };

    // Where the analysis is kept. Resolved even when the file does not exist
    // yet, because that is the first run and it is the run that creates it —
    // and even under `--no-cache`, if the run has been asked to delete it.
    let cache_path = if args.no_cache && !args.clear_cache {
        None
    } else {
        cache::resolve_path(args.cache.as_deref(), problems)
    };
    // The run's header, in `vid-fp`'s words: what it was asked to do, before
    // it does any of it, so a run's log says which settings produced it.
    say!(
        cx,
        "Settings -> Work size: {}, Candidates: {}, Min aligned points: {}, Min frame overlap: {}, \
         Min pixel correlation: {}, Mirrored and inverted: {}, Threads: {}, Recursive: {}, Follow symlinks: {}, \
         Hidden folders: {}",
        args.work_size,
        args.candidates,
        args.min_aligned_points,
        args.min_frame_overlap,
        args.min_pixel_correlation,
        if args.skip_mirrored { "skipped" } else { "searched" },
        rayon::current_num_threads(),
        args.recursive,
        args.follow_symlinks,
        args.hidden
    );
    if let Some(note) = &extensions_note {
        say!(cx, "{note}");
    }
    if args.min_aligned_points < MIN_ALIGNED_POINTS {
        say!(
            cx,
            "Note: --min-aligned-points {} behaves the same as {MIN_ALIGNED_POINTS}, the fewest points a match is fitted through.",
            args.min_aligned_points
        );
    }
    if args.clear_cache {
        if let Some(p) = &cache_path {
            say!(cx, "Clearing all cache at {}...", p.display());
            match std::fs::remove_file(p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                // Asked for and not done, and the next run will read the cache
                // this one meant to be rid of.
                Err(e) => problems.cache(format!("could not delete {}: {e}", p.display())),
            }
        }
    }
    let cache_path = cache_path.filter(|_| !args.no_cache);
    match &cache_path {
        Some(p) => say!(cx, "Cache: {}", p.display()),
        None if args.no_cache => say!(cx, "Cache: off (--no-cache)"),
        None => say!(cx, "Cache: off"),
    }
    // The analysis goes to a scratch file beside the cache as it is made, and
    // is read back through a mapping; see `spill`. Where that cannot be, it
    // stays in the heap, as it always did, and the run is the same run.
    let scratch = cache::locate(args.cache.as_deref()).parent().filter(|d| !d.as_os_str().is_empty()).map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    match std::fs::create_dir_all(&scratch).and_then(|()| spill::begin(&scratch)) {
        Ok(()) => stage!(cx, "scratch file in {}", scratch.display()),
        Err(e) => stage!(cx, "no scratch file in {} ({e}): the analysis is held in memory", scratch.display()),
    }
    let named: Vec<&PathBuf> = args.roots.iter().filter(|p| p.as_os_str() != "-").collect();
    if !named.is_empty() || lists.is_empty() {
        say!(cx, "Scanning: {:?}", named);
    }
    for (from, n) in &lists {
        say!(cx, "Scanning: {n} path(s) read from {from}");
    }
    if !args.against.is_empty() {
        say!(cx, "Against: {:?}", args.against);
    }
    if !args.exclude.is_empty() {
        say!(cx, "Excluding: {:?}", args.exclude);
    }
    say!(cx, "{}", wanted.describe());
    let walk::Listing { files, canonical, stats, library } = walk::walk(
        &walk::Request {
            roots: &roots,
            library: &args.against,
            exclude: &args.exclude,
            wanted: &wanted,
            recursive: args.recursive,
            follow_symlinks: args.follow_symlinks,
            hidden: args.hidden,
        },
        problems,
    );
    stage!(cx, "{} files", files.len());
    let scope = Scope { against: !args.against.is_empty(), library };
    if scope.against {
        let lib = scope.library.iter().filter(|&&l| l).count();
        stage!(cx, "against: {} scanned, {lib} in the library", files.len() - lib);
    }
    if files.is_empty() {
        // Still a finished run, and its report says so. Returning without one
        // left whatever the last run wrote to `-o` in place, and a script
        // reading it read the last run's groups as this one's.
        return finish(&cx, &scope, gui, &files, Found::default(), problems, started);
    }
    let mut analysed = analyse_all(&cx, &files, canonical, &stats, &wanted, &cache_path, problems);
    let (items, same) = (&mut analysed.items, &analysed.same);
    let n = files.len();

    // Everything from here to the assembly works on distinct contents: each
    // set of byte-identical files is its original, and a copy is given its
    // original's pairs at the end (`Same::expand`). A copy used to be indexed,
    // queried and verified like any other file — against its own original,
    // among others, to find the identity the exact pass had already found —
    // and it took a place in every candidate list its original was in.
    //
    // The same goes for files that are not byte-identical and decode to the
    // same picture: a photograph saved again as PNG, TIFF or lossless WebP.
    // They were matched as separate files until 0.30, and that brought back
    // the bug byte-identical copies had had — each counted the others as
    // matches, so in a library holding every picture in three such formats
    // no file was ever re-asked mirrored or inverted: eight photographs and
    // their mirror images, each saved three ways, found 9 of the 72 pairs
    // between a photograph and its mirror image, and 15 groups for 8.
    let matched: Vec<bool> = (0..n).map(|i| items[i].ok && same.answers_for(i) == i).collect();
    let n_same = (0..n).filter(|&i| items[i].ok && !same.is_copy(i) && same.answers_for(i) != i).count();
    if n_same > 0 {
        stage!(cx, "same pixels: {n_same} files decode to a picture another file already holds");
    }

    say!(cx, "Analysis complete. Matching {}...", count(analysed.n_ok, "image", "images"));
    let policy = verify::Policy::new(aligned_points(args), args.min_frame_overlap, args.min_pixel_correlation);
    // What a verdict must already have before its pixels are worth reading:
    // the weakest bar any tier applies, or everything a dump would record.
    //
    // A dump widens the gate and never narrows it: the bars the run's own
    // tiers use still decide, so a run with `--dump` finds the pairs the same
    // run without it finds. (It used to *set* the gate to (3, 0.2), which is
    // tighter than a `--min-frame-overlap` below 0.2 and so lost pairs.)
    let mut dump = args.dump.is_some().then(Dump::default);
    let tier_gate = (policy.corroborated.min_aligned_points, policy.corroborated.min_frame_overlap);
    let gate = if dump.is_some() { (tier_gate.0.min(3), tier_gate.1.min(0.2)) } else { tier_gate };
    let pass = Pass { items, matched: &matched, policy: &policy, gate, dumping: dump.is_some() };
    let (index, all_direct, verified) = retrieve_and_verify(&cx, &scope, &pass);
    let variant_edges = second_look(&cx, &scope, &pass, index, &all_direct, dump.as_mut());

    // Nothing after this point matches a descriptor against another. What is
    // left — propagation, corroboration, assembly — works from thumbnails,
    // frame sizes and verdicts already taken, so every descriptor in the
    // corpus is dead weight from here, as the index the second look let go
    // was. Together they are the largest thing the run holds, and they were
    // being held to the end.
    for it in items.iter_mut() {
        // The frame size stays: propagation and corroboration still ask how
        // large each picture was. Only the keypoints and descriptors go.
        it.feats = std::sync::Arc::new(Features { w: it.feats.w, h: it.feats.h, ..Default::default() });
    }
    timed!(37, release_memory());
    stage!(cx, "released the index and the descriptors");

    let claims = cluster(&cx, items, &policy, &all_direct, &variant_edges, verified, dump.as_mut());
    drop(variant_edges);
    if let (Some(path), Some(dump)) = (&args.dump, dump) {
        write_dump(&cx, path, &files, items, &all_direct, dump)?;
    }
    // Every verdict the matching made, and what the dump alone reads: nothing
    // below does. Held to the end, they were 65 MB on 27,000 pictures through
    // the assembly, which is where such a run peaks.
    drop(all_direct);

    let found = assemble(&cx, &scope, gui, &files, &stats, analysed, claims);
    finish(&cx, &scope, gui, &files, found, problems, started)
}

/// What the analysis hands on: every file's analysis, which files are one
/// file, and the cache, still open for the colours the suggestion measures.
struct Analysed {
    items: Vec<Item>,
    same: Same,
    store: Option<cache::Store>,
    /// Each file's name in the cache, and where its record sits there.
    names: Vec<PathBuf>,
    span_of: Vec<Option<cache::Span>>,
    /// Files the run was not asked about after all; see `analyse_all`.
    not_asked: Vec<bool>,
    /// Each file's keypoints, for the grey twins' stated pairs once the
    /// descriptors are let go.
    n_kps: Vec<u32>,
    n_ok: usize,
}

/// Find the byte-identical copies, then describe every other file or read it
/// from the cache, keep what was made, and say what would not open.
fn analyse_all(
    cx: &Ctx,
    files: &[PathBuf],
    canonical: Vec<Option<PathBuf>>,
    stats: &[walk::Stat],
    wanted: &extensions::Wanted,
    cache_path: &Option<PathBuf>,
    problems: &mut Problems,
) -> Analysed {
    let (args, progress) = (cx.args, &cx.progress);
    // The bar's first estimate of the whole run, from nothing but the file
    // count; every stage below refines it as soon as it knows better.
    progress.forecast(|f| {
        f.files = files.len();
        f.cache_bytes = cache_path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
        f.cache_write = cache_path.is_some();
        f.candidates_per_query = args.candidates;
        if args.skip_mirrored {
            f.variants = Some(0);
        }
    });

    // Byte-identical copies, before anything is decoded.
    progress.begin(Stage::Identical);
    // Under a wildcard, a file is compared byte for byte only if its head may
    // be a picture or its name says it is one: anything else is dropped from
    // the groups below once it fails to decode, and reading two copies of a
    // video whole to find that out was gigabytes for nothing. A list walk
    // took only image names, so it asks nothing.
    let guessed = wanted.is_a_guess_at_images();
    let comparable = |i: usize| guessed || extensions::names_an_image(&files[i]) || decode::may_be_image(&files[i]);
    let exact = timed!(33, exact_groups(files, stats, comparable));
    stage!(cx, "exact duplicates: {} groups", exact.len());
    let mut same = Same::of_bytes(files.len(), exact);

    // Decode and describe.
    let sp = sift::Params {
        max_features: FEATURES,
        ..Default::default()
    };
    let done = AtomicUsize::new(0);
    let n = files.len();
    let settings = cache::Settings {
        work_size: args.work_size as u32,
        features: FEATURES as u32,
        thumb: THUMB_LONG as u32,
    };
    // The cache is keyed on the paths themselves, not on their printable
    // form: two names that are not UTF-8 can print the same and be two files.
    // And on each file's canonical path, not the one this run spells; see
    // `cache_names`.
    let names: Vec<PathBuf> = if cache_path.is_some() { cache_names(files, canonical) } else { Vec::new() };
    let (mut cached, mut store) = match &cache_path {
        Some(p) => {
            progress.begin(Stage::CacheRead);
            // Only this run's paths are unpacked; the rest of the machine's
            // cache is read for where it is, not for what it says. See
            // `cache::open`.
            let walked: std::collections::HashSet<&Path> = names.iter().map(|p| p.as_path()).collect();
            let (cached, store) = cache::open(p, settings, &|path| walked.contains(path), problems);
            (cached, Some(store))
        }
        None => Default::default(),
    };
    // A cache another version of img-fp wrote is that version's, and this run
    // leaves it as it is; see `cache::MAGIC`.
    if let (Some(p), Some(v)) = (&cache_path, store.as_ref().and_then(|s| s.other_version())) {
        say!(
            cx,
            "Note: {} was written by another version of img-fp ({v}; this one reads {}), so it is left as it is \
             and this run keeps nothing in it. Give this version a file of its own with --cache, or replace that \
             one with --clear-cache.",
            p.display(),
            cache::FORMAT
        );
    }
    // Records in the file that the map does not hold, and that are not this
    // file's records at other settings: each superseded by a later one for
    // the same path. Something to compact away, if nothing else is.
    let superseded = store.as_ref().map_or(0, |s| s.records() - cached.len() - s.other_settings().len());
    if !cached.is_empty() {
        stage!(cx, "cache: {} usable records", cached.len());
    }
    // Every walked file's record comes *out* of the map rather than being
    // copied from it. The analysis is the largest thing the run holds, and
    // cloning it here made it exist twice over — once in the map and once in
    // `items` — for the length of the phase that already sets the peak, and
    // then charged a second bill to free the originals. On the found corpus
    // that was 1.9 s of dropping a nine-thousand-record map, about as much
    // again cloning into it, and 164 MB of peak.
    //
    // What is left in the map afterwards is exactly the records for files this
    // run did not walk, which is what `carry_over` wants; the two counts taken
    // here are what tells the save below whether it has anything to write.
    let (mut in_cache, mut same_key) = (0usize, 0usize);
    let mut mine: Vec<Option<(cache::Record, cache::Span)>> = files
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let got = cached.remove(names.get(i)?)?;
            in_cache += 1;
            let k = stats[i].key?;
            if got.0 != k {
                return None;
            }
            same_key += 1;
            // A walked path's record is always unpacked.
            Some((got.1?, got.2))
        })
        .collect();
    // Where each file's record sits in the cache file, for the ones that have
    // one. A cached record's is known now; a new one's, when its worker has
    // appended it.
    let mut span_of: Vec<Option<cache::Span>> = mine.iter().map(|m| m.as_ref().map(|r| r.1)).collect();
    // The exact pass has already grouped the files whose bytes hash the same,
    // and the analysis depends on nothing but those bytes. Describing the
    // second copy of a file is not a cheaper way to reach the same answer, it
    // is the same work done twice: each group's original (`Same::of_bytes`)
    // is described and the rest are copied from it. Those groups are already
    // claimed as duplicates in the output, so sharing one analysis between
    // them asserts nothing the run does not assert anyway.
    //
    // The bytes are the same, so the analysis is: a copy's record serves its
    // original, and stays under the copy's name in the file, where the next
    // run finds it again. Any other copy keeps no record of its own: it is
    // answered by its original on every run, cached or not, so a record under
    // its name is the same analysis stored twice and never read. One written
    // by an earlier build is left out of the file at the next compaction.
    for g in same.byte_groups() {
        let holder = g.iter().copied().find(|&i| mine[i].is_some());
        if let Some(k) = holder.filter(|&k| k != g[0]) {
            mine[g[0]] = mine[k].take();
        }
        for &i in &g[1..] {
            if holder != Some(i) {
                span_of[i] = None;
            }
        }
    }
    // What each file still to be analysed is expected to cost, for the bar
    // and nothing else. Read from headers, in parallel, and only for files the
    // cache and the exact pass have not already answered, so a cached run
    // opens nothing here.
    let todo = (0..n).filter(|&i| !same.is_copy(i) && mine[i].is_none()).count();
    // Whether the save below will have anything to write, as far as it can be
    // told yet: something new to describe, or a record that no longer fitted.
    let rewrite = cache_path.is_some()
        && (superseded > 0 || same_key != in_cache || args.prune_cache || (0..n).any(|i| same.is_copy(i) && mine[i].is_some()));
    progress.forecast(|f| {
        f.to_describe = Some(todo);
        f.cache_write = rewrite;
    });
    let headers = progress.begin_counted(Stage::Headers, n as u64, n, "files");
    // The headers are kept as well: the decode claims its share of the
    // decode budget from them before it reads the file.
    let (weight, header): (Vec<u64>, Vec<Option<decode::Probe>>) = (0..n)
        .into_par_iter()
        .map(|i| {
            headers.tick();
            if same.is_copy(i) || mine[i].is_some() {
                return (0, None);
            }
            let probe = decode::probe(&files[i]);
            (progress::analysis_cost(probe.as_ref(), stats[i].len, args.work_size, sp.upsample_below), probe)
        })
        .unzip();
    stage!(cx, "{todo} files to analyse");
    // A wildcard walk has not guessed at anything, so what it found is files;
    // calling them images is how a home directory reads as a photo library.
    let found = if wanted.is_a_guess_at_images() { ("image", "images") } else { ("file", "files") };
    // The three add up to `n`: a copy is answered by its original whether or
    // not the cache also knew it, so it is counted as a copy and only there.
    let copies = (0..n).filter(|&i| same.is_copy(i)).count();
    let from_cache = n - copies - todo;
    if from_cache > 0 || copies > 0 {
        say!(
            cx,
            "Found {}; {from_cache} already cached, {}, {todo} to analyse.",
            count(n, found.0, found.1),
            count(copies, "identical copy", "identical copies")
        );
    } else {
        say!(cx, "Found {}. Analysing...", count(n, found.0, found.1));
    }
    let total_weight: u64 = weight.iter().sum();
    progress.forecast(|f| f.describe = Some(total_weight));
    let bar = progress.begin_counted(Stage::Describe, total_weight, todo, "images");
    // Each record goes into the cache file as soon as it is made, which is
    // what lets an interrupt keep the analysis without writing anything; see
    // `cache::Store`.
    let keep = |i: usize, key: Option<cache::Key>, it: &Item| -> Option<cache::Span> {
        let (s, key) = (store.as_ref()?, key?);
        // A damaged file is analysed again on every run rather than kept, so
        // that every run says it is damaged: the record has nowhere to say it.
        if !it.ok || it.damaged.is_some() {
            return None;
        }
        s.append(&names[i], key, it.dims, &it.traits, &it.feats, &it.thumb)
    };
    let planes = Planes::default();
    let (mut items, appended): (Vec<Item>, Vec<Option<cache::Span>>) = mine
        .into_par_iter()
        .enumerate()
        .map(|(i, rec)| {
            if same.is_copy(i) {
                return (Item::default(), None);
            }
            if let Some((rec, _)) = rec {
                let it = Item {
                    feats: rec.feats.into(),
                    thumb: rec.thumb.into(),
                    dims: rec.dims,
                    ok: true,
                    err: None,
                    damaged: None,
                    traits: rec.traits,
                };
                return (it, None);
            }
            let f = &files[i];
            // Keyed before it is read — by the walk, before anything was —
            // so that a file changing under the analysis is described again
            // next time rather than trusted.
            let key = store.as_ref().and_then(|_| stats[i].key);
            let it = analyse(f, args.work_size, &sp, header[i].as_ref(), &planes);
            let span = keep(i, key, &it);
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            bar.add(weight[i]);
            if d % 250 == 0 {
                stage!(cx, "  described {d}");
            }
            (it, span)
        })
        .unzip();
    drop(weight);
    drop(header);
    drop(planes);
    for (s, a) in span_of.iter_mut().zip(appended) {
        if a.is_some() {
            *s = a;
        }
    }
    // A copy of a file that would not decode fails as its original did, and
    // is reported under its own path by the loop over `items` below. It used
    // to be decoded again, here, on one thread, one copy after another — the
    // same bytes through the same decoder, for the same error. (Both were read
    // whole by the exact pass, so an original that could not be *opened* is
    // never in a group to begin with.)
    for i in (0..n).filter(|&i| same.is_copy(i)) {
        items[i] = items[same.original(i)].clone();
    }
    // Which files share an analysis, and which of those a colour too: from
    // here to the assembly, each such set is matched as its first file.
    timed!(37, same.analysed(files, &items));
    let n_kps: Vec<u32> = items.iter().map(|it| it.feats.len() as u32).collect();
    // `--prune-cache` keeps only what this scan found, and gives that up when
    // the scan is not a complete account of what is out there: a root that
    // would not resolve or a directory that would not open leaves files
    // unseen, and dropping their records would cost a re-analysis of images
    // nothing is wrong with. The run says so and exits 2 rather than pruning
    // anyway or pruning silently.
    let prune = args.prune_cache
        && (problems.walk_was_complete() || {
            say!(cx, "not pruning: the walk could not read everything it was pointed at");
            problems.cache("--prune-cache skipped: the walk was incomplete".into());
            false
        });
    progress.forecast(|f| {
        // What the matching stages will be handed: originals, not copies.
        f.images = Some((0..n).filter(|&i| items[i].ok && same.answers_for(i) == i).count());
        f.descriptors = Some((0..n).filter(|&i| same.answers_for(i) == i).map(|i| items[i].feats.len()).sum());
    });
    if let (Some(p), Some(store)) = (cache_path, store.as_mut()) {
        save_cache(cx, p, store, &items, &names, &span_of, &cached, prune, problems);
    }
    // The store stays open until the suggestion, which keeps in it the colour
    // of each JPEG it has to decode to learn it (`cache::Store::keep_colour`).
    // Nothing more is analysed or read from the cache. What was written stays
    // readable for as long as it is held.
    spill::end();
    // What is left in the map is the analyses of files this run did not walk,
    // which the save above has just finished borrowing. Nothing reads them
    // again and they are megabytes apiece.
    drop(cached);
    stage!(
        cx,
        "cpu: decode {:.0}s, features {:.0}s",
        T_DECODE.load(Ordering::Relaxed) as f64 / 1e6,
        T_SIFT.load(Ordering::Relaxed) as f64 / 1e6
    );
    // Nothing after this point extracts features, so the workers' blur scratch
    // is dead weight from here on.
    rayon::broadcast(|_| sift::release_scratch());
    sift::release_scratch();
    // The analysis phase churns through buffers far larger than anything that
    // follows — a full-resolution decode, then a scale space per worker — and
    // the allocator keeps those pages against a demand that never comes,
    // because it has watched this process ask for them over and over. Handing
    // them back is worth several hundred megabytes for the rest of the run.
    // (This was measured once before and judged worthless, and it was: the
    // peak then stood in the middle of this very phase. With the decode
    // budget holding that down, what is left is the plateau this releases.)
    timed!(37, release_memory());
    let not_asked = tell_problems(files, &items, wanted, problems);
    let n_ok = items.iter().filter(|i| i.ok).count();
    let n_desc: usize = items.iter().map(|i| i.feats.len()).sum();
    stage!(cx, "described {n_ok}/{n} images, {n_desc} descriptors");
    Analysed { items, same, store, names, span_of, not_asked, n_kps, n_ok }
}

/// Keep in the cache file what this run made and what it should carry over,
/// and compact it when what else it holds is worth a copy of the rest.
#[allow(clippy::too_many_arguments)]
fn save_cache(
    cx: &Ctx,
    p: &Path,
    store: &mut cache::Store,
    items: &[Item],
    names: &[PathBuf],
    span_of: &[Option<cache::Span>],
    cached: &HashMap<PathBuf, cache::Entry>,
    prune: bool,
    problems: &mut Problems,
) {
    let n = items.len();
    cx.progress.begin(Stage::CacheWrite);
    if let Some(e) = store.failure() {
        // Not fatal — the pairs this run reports are the same pairs — but
        // the next run will pay for this one's analysis all over again,
        // which is exactly what a problem is here.
        problems.cache(format!("could not write {}: {e}", p.display()));
    }
    let mut entries: Vec<(&Path, cache::Span)> =
        (0..n).filter(|&i| items[i].ok).filter_map(|i| Some((names[i].as_path(), span_of[i]?))).collect();
    // What the cache already knew about images this run never walked —
    // which, every walked file's record having been taken out of the map
    // above, is everything still in it. The cache is one file for the
    // machine rather than one per corpus, so scanning a second directory
    // must not cost the first one its analysis; see `cache::carry_over`.
    let kept = if prune { Vec::new() } else { cache::carry_over(&cached) };
    if !kept.is_empty() {
        stage!(cx, "cache: {} records kept from other scans", kept.len());
    }
    entries.extend_from_slice(&kept);
    // And the analyses made at other settings, which this run did not
    // read: kept while their file is there, and dropped by `--prune-cache`,
    // which keeps only what this scan used. They used to be kept under it
    // too, for the files this scan found, so one run at another
    // `--work-size` doubled the cache for good: nothing short of
    // `--clear-cache`, which takes every record with it, could give the
    // copy back.
    let others: Vec<(PathBuf, cache::Span)> = if prune {
        Vec::new()
    } else {
        let gone = cache::gone(store.other_settings().iter().map(|(p, _)| p.as_path()));
        store.other_settings().iter().filter(|(p, _)| !gone.contains(p.as_path())).cloned().collect()
    };
    if !others.is_empty() {
        stage!(cx, "cache: {} records kept from other working sizes", others.len());
    }
    let others_dropped = store.other_settings().len() - others.len();
    entries.extend(others.iter().map(|(p, s)| (p.as_path(), *s)));
    let dropped = cached.len() - kept.len() + others_dropped;
    if prune && dropped > 0 {
        say!(cx, "pruned {dropped} cached record(s) this scan did not use");
    }
    // Every record worth keeping is already in the file, since each went
    // in as it was made. The file is rewritten only when what else it
    // holds — records superseded, for files that have gone, or pruned —
    // is worth a copy of the rest; see `cache::Store::worth_compacting`.
    // A threshold sweep, a dozen runs over one unchanged corpus, finds the
    // file already says what it would write and writes nothing.
    if !store.worth_compacting(&entries, prune) {
        let dead = store.records().saturating_sub(entries.len());
        stage!(cx, "cache: {} records, {dead} of them unused, not compacted", store.records());
    } else {
        match store.compact(&mut entries) {
            Ok(true) => {}
            Ok(false) => stage!(cx, "cache: not compacted, another run has it open"),
            Err(e) => problems.cache(format!("could not write {}: {e}", p.display())),
        }
    }
}

/// Say what would not open, and which files the run was not asked about
/// after all: a file a wildcard walk reached that is no picture, and a macOS
/// metadata file, whatever it is called (it is named for the picture it was
/// copied beside, and holds none of it).
///
/// Counted at the end of the analysis rather than at the JSON `failures`,
/// which is built from the same `items`: this is the last point every file
/// has been through, and a fatal error further down should not lose the
/// account of what would not open.
fn tell_problems(files: &[PathBuf], items: &[Item], wanted: &extensions::Wanted, problems: &mut Problems) -> Vec<bool> {
    let n = files.len();
    let not_asked: Vec<bool> = (0..n)
        .map(|i| {
            let err = items[i].err.as_deref();
            err == Some(decode::MAC_METADATA)
                || (!wanted.is_a_guess_at_images() && !extensions::names_an_image(&files[i]) && err == Some(decode::NOT_AN_IMAGE))
        })
        .collect();
    for (i, it) in items.iter().enumerate() {
        match &it.err {
            Some(e) if e == decode::MAC_METADATA => problems.mac_metadata(&files[i].display().to_string()),
            // A raw file is a picture, not read: a skip, and still in its
            // exact group, since a copy of its bytes is a copy of it.
            Some(e) if e == decode::CAMERA_RAW => problems.camera_raw(&files[i].display().to_string()),
            // A file a wildcard walk reached that is no picture at all never
            // claimed to be one; see `extensions.rs`.
            Some(_) if not_asked[i] => problems.not_image_content(&files[i].display().to_string()),
            Some(e) => problems.unreadable(&files[i].display().to_string(), e),
            // Decoded, described, and described to nothing. Nothing failed —
            // a blank picture really has no local features — but the file is
            // in the corpus and the matcher has nothing to say about it, and
            // that is worth a line for the same reason a refusal is: the run
            // did not answer the question it was asked about this file.
            None if it.feats.len() == 0 => problems.featureless(&files[i].display().to_string()),
            None => {}
        }
        if let Some(why) = &it.damaged {
            problems.damaged(&files[i].display().to_string(), why);
        }
    }
    not_asked
}

/// What both verification passes read: the files, which of them are
/// matched (`run`), the acceptance tiers, and the gate a verdict must clear
/// before its pixels are read.
struct Pass<'a> {
    items: &'a [Item],
    matched: &'a [bool],
    policy: &'a verify::Policy,
    gate: (u32, f32),
    /// Whether `--dump` records every verdict with three aligned points.
    dumping: bool,
}

/// One worker's scratch for `Pass::verify`: the shared words, the matches
/// and the verifier's own.
type Work = (Vec<(u32, u32)>, Vec<(u32, u32)>, verify::Scratch);

impl Pass<'_> {
    /// Candidate `j` put to the geometry and the pixels against file `i`,
    /// whose features under `variant` are `fi` and their words `wi` (its own
    /// for the direct pass). The edge, from the lower index to the higher,
    /// when `tier` accepts it or `--dump` records it, and whether `tier` did.
    #[allow(clippy::too_many_arguments)]
    fn verify(&self, lists: &[WordList], i: usize, fi: &Features, wi: &WordList, j: usize, variant: Variant, tier: &verify::Rules, (cands, matches, scratch): &mut Work) -> Option<(Edge, bool)> {
        timed!(15, index::shared(wi, &lists[j], cands, 60_000));
        if cands.len() < 3 {
            return None;
        }
        let items = self.items;
        let p = verify::Pair {
            fa: fi,
            fb: &items[j].feats,
            ta: &items[i].thumb,
            tb: &items[j].thumb,
        };
        let v = verify::verify(&p, cands, variant, self.gate, matches, scratch);
        let accepted = v.accepted(tier);
        if !accepted && !(self.dumping && v.n_in >= 3) {
            return None;
        }
        // Every edge runs from the lower index to the higher, and its verdict
        // with it: stored the other way round, the report's `scale` for the
        // pair was the reciprocal of the scale between the files it names.
        let e = if i < j { Edge::new(i, j, v) } else { Edge::new(j, i, v.reversed()?) };
        Some((e, accepted))
    }
}

/// Anchors only: a pair believed on its own evidence. These are what decide
/// which files end up in one cluster. They are read out of the direct
/// verdicts where they lie rather than copied: they are nearly all of them,
/// and a copy was another hundred-odd megabytes held through the second look,
/// which is where a large corpus's run peaks.
fn anchors<'a>(direct: &'a [Edge], policy: &'a verify::Policy) -> impl Iterator<Item = Edge> + 'a {
    direct.iter().filter(|e| e.v.accepted(&policy.anchor)).copied()
}

/// The vocabulary and every matched file's words, which the second look asks
/// again with, and each list's idf mass against a library (`both_ways`).
struct Index<'a> {
    vocab: Vocabulary<'a>,
    lists: Vec<WordList>,
    mass: Vec<f32>,
}

/// Build the vocabulary from the corpus, quantise every matched file, ask the
/// inverted file for each one's candidates and verify them: the direct
/// verdicts, and how many candidates were verified, which is what a round of
/// propagation may spend.
fn retrieve_and_verify<'a>(cx: &Ctx, scope: &Scope, pass: &Pass<'a>) -> (Index<'a>, Vec<Edge>, usize) {
    let (args, progress) = (cx.args, &cx.progress);
    let (items, matched) = (pass.items, pass.matched);
    let n = items.len();
    let n_match = matched.iter().filter(|&&m| m).count();
    let n_desc_match: usize = (0..n).filter(|&i| matched[i]).map(|i| items[i].feats.len()).sum();

    // Vocabulary from the corpus itself, at a depth the corpus chooses. A
    // copy's descriptors are its original's, so counting them would weigh
    // that picture twice in the sample and size the tree for words that are
    // not there.
    let vp = index::VocabParams::for_corpus(n_desc_match);
    progress.forecast(|f| f.vocab_sample = Some(vp.sample.min(n_desc_match)));
    progress.begin(Stage::Vocabulary);
    let mut pool: Vec<&[u8; DESC_LEN]> = Vec::with_capacity(vp.sample.min(n_desc_match));
    timed!(34, {
        // Even sampling across images, so one feature-rich image cannot own
        // the vocabulary.
        let per = (vp.sample / n_match.max(1)).max(8);
        for it in (0..n).filter(|&i| matched[i]).map(|i| &items[i]) {
            let take = it.feats.len().min(per);
            let step = (it.feats.len() / take.max(1)).max(1);
            for i in (0..it.feats.len()).step_by(step).take(take) {
                pool.push(it.feats.d(i).try_into().unwrap());
            }
        }
    });
    let vocab = timed!(12, Vocabulary::build(pool, &vp));
    stage!(cx, "vocabulary: {} live words of {} from {} samples", vocab.n_live_words(), vocab.n_words(), vp.sample.min(n_desc_match));

    // Quantise. The word lists are built straight into the vector the inverted
    // file and every later stage read from: holding a second copy per image
    // costs as much again as the lists themselves.
    progress.forecast(|f| f.live_words = Some(vocab.n_live_words()));
    let bar = progress.begin_counted(Stage::Quantise, n_desc_match as u64, n_match, "images");
    let lists: Vec<WordList> = (0..n)
        .into_par_iter()
        .map(|i| {
            if !matched[i] {
                return WordList::default();
            }
            let it = &items[i];
            let wl = timed!(11, quantise(&vocab, &it.feats));
            bar.add(it.feats.len() as u64);
            wl
        })
        .collect();
    stage!(cx, "quantised");

    // Inverted file, of every word; see `InvertedFile::build`.
    progress.begin(Stage::InvertedFile);
    let inv = timed!(13, InvertedFile::build(&lists, vocab.n_live_words()));
    stage!(cx, "inverted file");

    // Against a library, not every file asks, so each list's idf mass is kept
    // for reading its answers both ways round; see `both_ways`.
    let mass: Vec<f32> = if scope.against { (0..n).map(|i| inv.mass(&lists[i])).collect() } else { Vec::new() };

    // ---- candidates, then verification
    //
    // Retrieval and verification are separate passes so that a pair proposed
    // from both sides is verified once. Containment scoring is asymmetric —
    // the crop finds the photograph much more readily than the reverse — so
    // both directions genuinely have to be asked.
    let bar = progress.begin_counted(Stage::Candidates, n_desc_match as u64, n_match, "images");
    // Against a library, the scanned files ask first, and then every library
    // file a file that has asked anchored asks in turn, round after round
    // until none is new: the library's own pairs are what a chain from a
    // scanned file runs through (`propagate`), and they are wanted only where
    // the library touches the scan. A library of unique photographs is
    // nearly all unreached and asks nothing, where the union of both sides
    // had every file of it ask and be re-asked mirrored.
    let mut asked = vec![false; n];
    let mut ask: Vec<usize> = (0..n).filter(|&i| matched[i] && scope.asks_first(i)).collect();
    let mut seen: std::collections::HashSet<(u32, u32)> = Default::default();
    let mut all_direct: Vec<Edge> = Vec::new();
    let mut verified = 0usize;
    let mut round = 0usize;
    loop {
    for &i in ask.iter() {
        asked[i] = true;
    }
    let first = round == 0;
    let mut cand_pairs: Vec<(u32, u32)> = ask
        .par_iter()
        .copied()
        .map_init(
            || (vec![0f32; n], Vec::new()),
            |(acc, scored), i| {
                timed!(14, inv.query(&lists[i], i as u32, acc, scored));
                if first {
                    bar.add(items[i].feats.len() as u64);
                }
                timed!(41, {
                    // Every image the query touched, including one that scores
                    // nothing: that happens only when all it shares are words
                    // in every image, which only a small folder or one that is
                    // all one picture has (see `query_touched`) — and on a
                    // folder of two it is every word a duplicate shares. Dropping it
                    // there found no pair at all; the second look never did.
                    scored.retain(|&(j, _)| matched[j as usize]);
                    scope.rank(scored, || mass[i], &mass, args.candidates);
                    scored
                        .iter()
                        .map(|&(j, _)| if (j as usize) < i { (j, i as u32) } else { (i as u32, j) })
                        .collect::<Vec<_>>()
                })
            },
        )
        .flatten()
        .collect();
    timed!(35, {
        cand_pairs.par_sort_unstable();
        cand_pairs.dedup();
    });
    // Only a library's files ask after the first round, and only they can
    // propose a pair an earlier round verified.
    if scope.against {
        cand_pairs.retain(|p| seen.insert(*p));
    }
    if first {
        stage!(cx, "candidates: {} pairs", cand_pairs.len());
    } else {
        stage!(cx, "  library round {round}: {} library files asked, {} more candidates", ask.len(), cand_pairs.len());
    }

    if first {
        progress.forecast(|f| f.candidate_pairs = Some(cand_pairs.len()));
    }
    let vbar = first.then(|| progress.begin_counted(Stage::Verify, cand_pairs.len() as u64, cand_pairs.len(), "pairs"));
    // Verified a slice of the candidates at a time, each slice's verdicts
    // appended to the one list as it finishes. A parallel `collect` of a
    // filtered stream gathers every worker's results into vectors of their
    // own, grown by doubling, and only then copies them into one: on a corpus
    // of 27,000 pictures that was some 300 MB alive for a moment at the end of
    // verification, which was the run's peak, and the holes it left in the
    // heap were dirtied again by the second look. The slices are taken in
    // order and each is collected in order, so the list is the one a single
    // collect gave, verdict for verdict.
    let from = all_direct.len();
    for slice in cand_pairs.chunks(VERIFY_SLICE) {
        let part: Vec<Edge> = slice
            .par_iter()
            .map_init(Work::default, |work, &(i, j)| timed!(42, {
                if let Some(b) = &vbar {
                    b.tick();
                }
                let (i, j) = (i as usize, j as usize);
                let tier = &pass.policy.corroborated;
                pass.verify(&lists, i, &items[i].feats, &lists[i], j, Variant::default(), tier, work).map(|(e, _)| e)
            }))
            .flatten()
            .collect();
        all_direct.extend(part);
    }
    verified += cand_pairs.len();
    drop(cand_pairs);
    // The library files this round's anchors reached that have not asked:
    // none in a plain scan, whose files all asked in the first round.
    let mut next = vec![false; n];
    for e in &all_direct[from..] {
        let (i, j) = e.ends();
        if e.v.accepted(&pass.policy.anchor) {
            for (x, y) in [(i, j), (j, i)] {
                if asked[x] && scope.library[y] && !asked[y] {
                    next[y] = true;
                }
            }
        }
    }
    ask = (0..n).filter(|&i| next[i]).collect();
    if ask.is_empty() {
        break;
    }
    round += 1;
    }
    // Verification reads the word lists and never the postings, and the
    // second look, which does, is asked of a few per cent of the files after
    // verification is done. So the postings are let go here and built again
    // from the same lists if the second look has anything to ask: on a corpus
    // of 27,000 pictures that is 94 MB off the run's peak, which is the end of
    // verification, for a rebuild of under two seconds on one thread. The
    // build is deterministic, so the second index is the first.
    drop(inv);
    let n_edges = anchors(&all_direct, pass.policy).count();
    stage!(cx, "anchors: {} of {} verified pairs", n_edges, all_direct.len());
    (Index { vocab, lists, mass }, all_direct, verified)
}

/// The second look at images nothing matched: each asked again mirrored and
/// inverted. The anchors it finds; the index goes with it.
fn second_look(cx: &Ctx, scope: &Scope, pass: &Pass, index: Index, direct: &[Edge], dump: Option<&mut Dump>) -> Vec<Edge> {
    // A mirrored or inverted copy shares no visual words with its original —
    // the descriptor bins are permuted — so retrieval cannot find it, and the
    // query has to be re-asked with the permutation applied. Doing that for
    // every image would roughly double the retrieval cost for a handful of
    // pairs, so it is asked only where the first pass came up short: a file
    // with no matches, or with so few that it may be hanging off the edge of
    // its real cluster.
    let (args, progress) = (cx.args, &cx.progress);
    let (items, matched, policy) = (pass.items, pass.matched, pass.policy);
    let Index { vocab, lists, mass } = index;
    let n = items.len();
    let n_edges = anchors(direct, policy).count();
    let mut degree = vec![0u32; n];
    for e in anchors(direct, policy) {
        degree[e.a()] += 1;
        degree[e.b()] += 1;
    }
    // A byte-identical copy is not counted. It is the same picture, so it says
    // nothing about whether the picture's other versions were found — and
    // counted, it kept every file that has a copy from ever being asked
    // again: in a library where each photograph is there twice over, no file
    // was, and every mirrored and inverted version in it went unfound.
    // Ask again, mirrored and inverted, for images that no *pair* of matches
    // has anchored yet. A file with one match is not safely found: that match
    // may be the wrong one, and a mirrored query is cheap next to being wrong.
    // Two is not a fitted threshold, it is "more than one" — the same minimal
    // constant the bridge test uses for "more than one file", and the only
    // place a count appears in either rule. Trying the tighter reading, "no
    // matches at all", costs a seed apiece on three of the held-out mirror
    // transforms.
    //
    // Against a library, a library file is asked as any file is once the scan
    // has reached it: one with no match at all is no part of an answer, and
    // asking each of those is what made matching both sides as one four times
    // the cost against a library of unique photographs. That leaves the case
    // those files covered: a mirror of a scanned picture that nothing else in
    // the library matches, which matched as one scan is re-asked itself, for
    // having no match, and here is found from the scan's side or not at all.
    // Counting a scanned file's matches cannot tell when to look — on
    // IMGS-AGAINST the files it caught had six — and re-asking every scanned
    // file found them at a third more CPU. A mirror of one file of a picture's
    // family is a mirror of the family, so each clean-anchor component that
    // holds a scanned file is re-asked once, through its best-connected
    // member, and propagation carries what that finds to the rest: 2,635
    // files re-asked there against 11,024, the same finds. Clean anchors,
    // because weak ones join families into a few large components, and one
    // member of each found ten fewer.
    //
    // `--skip-mirrored` asks it of nobody, and everything below then does
    // nothing: no word lists made, no postings rebuilt, no verdicts.
    let lonely: Vec<usize> = if args.skip_mirrored {
        Vec::new()
    } else {
        // Against a library, each clean-anchor component that holds a scanned
        // file is re-asked through its best-connected member, whatever its
        // matches; see above.
        let mut rep = vec![false; n];
        if scope.against {
            let mut d = Dsu::new(n);
            for e in anchors(direct, policy) {
                if e.v.accepted(&policy.clean) {
                    d.union(e.a(), e.b());
                }
            }
            // Per component: whether a scanned file is in it, and its best-
            // connected member, ties to the lowest index.
            let mut best: HashMap<usize, (bool, usize)> = HashMap::new();
            for i in (0..n).filter(|&i| degree[i] > 0) {
                let e = best.entry(d.find(i)).or_insert((false, i));
                e.0 |= !scope.library[i];
                if degree[i] > degree[e.1] {
                    e.1 = i;
                }
            }
            for (scanned, i) in best.into_values() {
                rep[i] |= scanned;
            }
        }
        (0..n)
            .filter(|&i| matched[i] && (rep[i] || (degree[i] < 2 && !(scope.library[i] && degree[i] == 0))))
            .collect()
    };
    let variants = [
        Variant { mirror: true, invert: false },
        Variant { mirror: false, invert: true },
        Variant { mirror: true, invert: true },
    ];
    // Each variant is asked the same retrieval question it always was, and
    // then the three answers are spent as one: a candidate is verified once,
    // under the variant it scored highest for, and only the best `-k` of the
    // merged list are verified at all.
    //
    // It used to be the best `-k` of *each* variant, three verdicts' worth of
    // candidates per file, and a verdict is what the pass spends most of its
    // time rejecting — 3.94 M of them on a found corpus for 459 pairs. Almost
    // every file the pass is asked about has no mirrored or inverted twin, so
    // two of its three lists are nothing but the retrieval's best guesses at
    // a question with no answer, and a third list of those says nothing the
    // first two did not. What a true pair looks like is a candidate that
    // scores far above the rest under the one variant that relates the two
    // files; it keeps its place when the lists are merged, and the pairs
    // measure it: 459 of 459 anchors on the found corpus come from their
    // highest-scoring variant, 447 of them inside the merged best 150, and
    // the benchmark corpus's output is 217,376 pairs against 217,380 with
    // not one false pair more.
    // What each re-asked file will cost the bar: three descents and three
    // queries, which scale with its descriptors, and up to `-k` verdicts.
    let lonely_weight: Vec<u64> = {
        let fc = progress.forecast_now();
        lonely.iter().map(|&i| fc.variant_cost(items[i].feats.len())).collect()
    };
    let total_weight: u64 = lonely_weight.iter().sum();
    progress.forecast(|f| {
        f.variants = Some(total_weight);
        f.edges = Some(n_edges);
    });
    let bar = progress.begin_counted(Stage::Variants, total_weight, lonely.len(), "images");
    // The vocabulary is wanted here only to quantise the re-asked files'
    // mirrored and inverted descriptors, and the postings only to query them.
    // Where those files are few — a benchmark corpus, whose files nearly all
    // found their family — their word lists are quantised first and the
    // vocabulary let go before the postings are built again (see the drop
    // after the candidates), so the two are never held together: on 27,000
    // pictures that is 1,336 files' lists, some 20 MB, against a 181 MB
    // vocabulary. Where they are most of the corpus — a found one, where
    // nearly every file is re-asked — their lists would outweigh the
    // vocabulary several times over, and each file's are made and spent in
    // turn as before. The words are the same words either way.
    let n_live = vocab.n_live_words();
    let mut vocab = Some(vocab);
    let ahead_bytes = lonely.iter().map(|&i| lists[i].len()).sum::<usize>() * 3 * WordList::BYTES_PER_ENTRY;
    let ahead: Option<Vec<[WordList; 3]>> = (ahead_bytes < vocab.as_ref().unwrap().heap_bytes()).then(|| {
        let vocab = vocab.as_ref().unwrap();
        lonely
            .par_iter()
            .map(|&i| {
                std::array::from_fn(|v| {
                    let vf = timed!(40, variant_features(&items[i].feats, variants[v]));
                    timed!(39, quantise(vocab, &vf))
                })
            })
            .collect()
    });
    if ahead.is_some() {
        vocab = None;
    }
    let inv = (!lonely.is_empty()).then(|| timed!(13, InvertedFile::build(&lists, n_live)));
    let variant_all: Vec<(Edge, bool)> = lonely
        .par_iter()
        .enumerate()
        .zip(lonely_weight.par_iter())
        .map_init(
            || (vec![0f32; n], Vec::new(), Work::default(), Vec::new()),
            |(acc, scored, work, merged), ((k, &i), &w)| timed!(38, {
                let mut out: Vec<(Edge, bool)> = Vec::new();
                let vf: [Features; 3] = timed!(40, std::array::from_fn(|v| variant_features(&items[i].feats, variants[v])));
                let made: [WordList; 3];
                let wl: &[WordList; 3] = match &ahead {
                    Some(a) => &a[k],
                    None => {
                        let vocab = vocab.as_ref().unwrap();
                        made = std::array::from_fn(|v| timed!(39, quantise(vocab, &vf[v])));
                        &made
                    }
                };
                merged.clear();
                for v in 0..3 {
                    let inv = inv.as_ref().unwrap();
                    timed!(14, inv.query(&wl[v], i as u32, acc, scored));
                    timed!(41, scope.rank(scored, || inv.mass(&wl[v]), &mass, args.candidates));
                    merged.extend(scored.iter().map(|&(j, s)| (j, s, v as u8)));
                }
                timed!(41, merge_variants(merged, args.candidates));
                for &(j, _, v) in merged.iter() {
                    let (j, v) = (j as usize, v as usize);
                    // An anchor, or — for `--dump`, which records every
                    // verdict the geometry could fit, as the direct pass's
                    // does — anything with three aligned points.
                    out.extend(pass.verify(&lists, i, &vf[v], &wl[v], j, variants[v], &policy.anchor, work));
                }
                bar.add(w);
                out
            }),
        )
        .flatten()
        .collect();
    let variant_edges: Vec<Edge> = variant_all.iter().filter(|e| e.1).map(|e| e.0).collect();
    if let Some(dump) = dump {
        dump.variant = variant_all.into_iter().map(|e| e.0).collect();
    }
    if args.skip_mirrored {
        stage!(cx, "variants: skipped");
    } else {
        stage!(cx, "variants: {} more pairs from {} unmatched images", variant_edges.len(), lonely.len());
    }
    // Nothing after this matches a descriptor against another, so the
    // vocabulary, the inverted file and the word lists go with this stage.
    variant_edges
}

/// From anchors to the run's claims: drop weak bridges, admit weak anchors
/// into the clusters the clean ones make, propagate transforms inside each
/// cluster, and corroborate the weaker direct matches inside them. The
/// anchors, the propagated pairs and the corroborated ones.
fn cluster(
    cx: &Ctx,
    items: &[Item],
    policy: &verify::Policy,
    direct: &[Edge],
    variant_edges: &[Edge],
    prop_budget: usize,
    dump: Option<&mut Dump>,
) -> (Vec<Edge>, Vec<Edge>, Vec<Edge>) {
    let progress = &cx.progress;
    let n = items.len();
    let dumping = dump.is_some();
    // One edge per pair of files. The mirrored and inverted pass can verify a
    // pair the direct pass already anchored, and two lonely files can each
    // find the other — and a pair counted twice is two links to the bridge
    // test, so a lone match between two clusters stopped being a bridge by
    // being found twice. The direct verdict is kept where there are two.
    let all: Vec<Edge> = unique_pairs(anchors(direct, policy).chain(variant_edges.iter().copied()));

    // Every anchor faces the bridge test, whichever pass produced it: a
    // mirrored match joining two clusters is exactly as consequential as a
    // direct one, and on this corpus an inverted match between a 225-pixel
    // photograph of the Earth and a beach scene was the single edge that
    // merged two whole families into 3,002 false pairs.
    progress.forecast(|f| f.edges = Some(all.len()));
    progress.begin(Stage::Propagate);
    // Clusters are made from clean anchors, and an anchor that is only an
    // anchor may add a file to one but never join two; see `admit_anchors`.
    let (clean, weak): (Vec<Edge>, Vec<Edge>) = all.into_iter().partition(|e| e.v.accepted(&policy.clean));
    let n_before = clean.len();
    // A clean anchor the bridge test drops is not thrown away but kept as a
    // weak one: it may still add a file to a cluster and counts in the cover
    // test, and joins no two clusters of its own.
    let (clean, dropped) = drop_weak_bridges(clean, n);
    let mut weak = weak;
    weak.extend(dropped);
    stage!(cx, "bridges: dropped {} lone links between clusters", n_before - clean.len());
    let n_weak = weak.len();
    let (all, refused) = admit_anchors(clean, weak, n);
    stage!(cx, "weak anchors: {} admitted, {refused} refused between clusters", n_weak - refused);

    // ---- propagate transforms inside each component
    // Propagation is iterated: each round's accepted pairs become edges the
    // next round's tree can route through, which shortens the path to files
    // that were only reachable the long way round. It converges in two or
    // three rounds.
    let mut propagated: Vec<Edge> = Vec::new();
    // Every hypothesis a round considered, kept only for `--dump`. A round
    // proposes every unmatched pair inside each component, which is quadratic
    // in the component and an order of magnitude more than it accepts, so the
    // rejected ones are dropped as soon as they have been judged unless
    // something is going to read them.
    //
    // A later round proposes again every pair an earlier one rejected, through
    // a tree that may have changed. Each pair is one hypothesis however many
    // rounds considered it, and the dump holds the last verdict on it: two
    // rows for one pair was a dump claiming more than the run had asked.
    let mut all_propagated: Vec<Edge> = Vec::new();
    let mut row_of: HashMap<(usize, usize), usize> = HashMap::new();
    let mut hypotheses: std::collections::HashSet<(usize, usize)> = Default::default();
    // A composed pair is kept only if it clears the propagated tier's own
    // overlap floor; a dump wants every hypothesis the round considered.
    let prop_min_ov = if dumping { policy.propagated.min_frame_overlap.min(0.2) } else { policy.propagated.min_frame_overlap };
    let mut pool: Vec<Edge> = all.clone();
    // Which files' components a round has to propose again: all of them in
    // the first round, and after that only those holding a pair the round
    // before accepted. A component that gained nothing has the same edges, so
    // the same tree and the same poses, and proposes exactly the hypotheses it
    // proposed last time — the accepted ones now known, the rest rejected
    // again. Components never merge here, since a composed pair joins two
    // files already in one.
    let mut dirty: Option<Vec<bool>> = None;
    let mut proposed_total = 0usize;
    // The clusters whose last proposal was a star, by lowest file, with their
    // sizes. A cluster starred in one round is the only one left to propose in
    // the next as often as not, and then fits the budget alone and is compared
    // pair by pair after all — so what to say is decided once the rounds are
    // done, from each cluster's last round, and not from its first. (A cluster
    // keeps its lowest file from round to round: components never merge here.)
    let mut still_starred: std::collections::BTreeMap<usize, usize> = Default::default();
    for round in 0..PROPAGATE_MAX_ROUNDS {
        let Propagation { found: round_all, starred, full, proposed } =
            timed!(21, propagate(items, &pool, n, prop_min_ov, dirty.as_deref(), prop_budget));
        proposed_total += proposed;
        for first in full {
            still_starred.remove(&first);
        }
        still_starred.extend(starred);
        let before = propagated.len();
        let seen: std::collections::HashSet<(usize, usize)> =
            pool.iter().map(Edge::ends).collect();
        let fresh: Vec<Edge> = round_all
            .iter()
            .filter(|e| !seen.contains(&e.ends()) && e.v.accepted(&policy.propagated))
            .copied()
            .collect();
        hypotheses.extend(round_all.iter().map(Edge::ends));
        if dumping {
            for e in round_all {
                match row_of.get(&e.ends()) {
                    Some(&k) => all_propagated[k] = e,
                    None => {
                        row_of.insert(e.ends(), all_propagated.len());
                        all_propagated.push(e);
                    }
                }
            }
        } else {
            drop(round_all);
        }
        let mut next = vec![false; n];
        for e in fresh.iter() {
            next[e.a()] = true;
            next[e.b()] = true;
        }
        dirty = Some(next);
        propagated.extend(fresh.iter().copied());
        pool.extend(fresh);
        stage!(cx, "  propagation round {}: +{} pairs", round + 1, propagated.len() - before);
        if propagated.len() == before {
            break;
        }
    }

    stage!(
        cx,
        "propagated: {} of {} composed hypotheses ({proposed_total} put to the pixels, budget {prop_budget} a round)",
        propagated.len(),
        hypotheses.len()
    );
    // Said on the console: pairs between two members of such a cluster that
    // direct matching missed are not looked for, and propagation is most of
    // the tool's recall.
    if !still_starred.is_empty() {
        let files: usize = still_starred.values().sum();
        say!(
            cx,
            "Note: {} cluster(s) ({files} files in all) are too large to compare every pair inside; their files \
             were compared with the cluster's best-connected file.",
            still_starred.len()
        );
    }
    drop(hypotheses);
    drop(row_of);

    // Weaker matches, admitted only between files an anchor already put in the
    // same cluster. They cannot merge anything, so they cost recall to refuse
    // and risk nothing to accept.
    let corroborated: Vec<Edge> = {
        let mut dsu = Dsu::new(n);
        for e in all.iter().chain(propagated.iter()) {
            dsu.union(e.a(), e.b());
        }
        let anchored: std::collections::HashSet<(usize, usize)> = all.iter().chain(propagated.iter()).map(Edge::ends).collect();
        direct
            .iter()
            .filter(|e| !anchored.contains(&e.ends()) && e.v.accepted(&policy.corroborated) && dsu.find(e.a()) == dsu.find(e.b()))
            .copied()
            .collect()
    };
    stage!(cx, "corroborated: {} more pairs inside existing clusters", corroborated.len());
    if let Some(dump) = dump {
        dump.propagated = all_propagated;
    }
    (all, propagated, corroborated)
}

/// Every verdict the matching made, for `--dump`: the direct pass's are the
/// run's own list, and these the ones nothing after their stage reads.
#[derive(Default)]
struct Dump {
    /// Every verdict the second look kept.
    variant: Vec<Edge>,
    /// The last verdict on each composed hypothesis propagation considered.
    propagated: Vec<Edge>,
}

/// Write `--dump`: one row per verdict, with the figures `replay.py` reads.
fn write_dump(cx: &Ctx, path: &Path, files: &[PathBuf], items: &[Item], direct: &[Edge], dump: Dump) -> Result<()> {
    let write = |w: &mut dyn Write| -> std::io::Result<()> {
        writeln!(w, "a,b,kind,n_match,n_in,ov_a,ov_b,scale,rot,blk,blk_n,ncc,centred,inverted,blk_min,m0,m1,m2,m3,m4,m5,aw,ah,bw,bh")?;
        for (kind, set) in [("direct", direct), ("variant", &dump.variant), ("propagated", &dump.propagated)] {
            for e in set.iter() {
                let ((a, b), v) = (e.ends(), &e.v);
                // The paths as their own bytes, quoted the way CSV quotes:
                // written with `{:?}`, a name holding a quote or a comma split
                // into two columns, and a tab or a non-UTF-8 byte came out as
                // an escape that names no file.
                use std::os::unix::ffi::OsStrExt;
                let figures = [
                    kind.to_string(),
                    v.n_match.to_string(),
                    v.n_in.to_string(),
                    format!("{:.4}", v.ov_a),
                    format!("{:.4}", v.ov_b),
                    format!("{:.5}", v.scale),
                    format!("{:.1}", v.rot_deg),
                    format!("{:.4}", v.blk),
                    v.blk_n.to_string(),
                    format!("{:.4}", v.ncc),
                    (v.centred as u8).to_string(),
                    (e.inverted() as u8).to_string(),
                    format!("{:.4}", v.blk_min),
                ];
                // The transform from `a`'s working image to `b`'s, and the two
                // working images' sizes, which the suggestion's frame tests read.
                let figures: Vec<String> = figures
                    .into_iter()
                    .chain(v.m.iter().map(|x| format!("{x:.6}")))
                    .chain([items[a].feats.w, items[a].feats.h, items[b].feats.w, items[b].feats.h].map(|x| x.to_string()))
                    .collect();
                let mut row: Vec<&[u8]> = vec![files[a].as_os_str().as_bytes(), files[b].as_os_str().as_bytes()];
                row.extend(figures.iter().map(|f| f.as_bytes()));
                report::csv_row_with(w, &row, b',')?;
            }
        }
        Ok(())
    };
    // `-` is stdout, which `stdout_has_one_reader` has kept for it alone;
    // a file is written whole or not at all, as the report is.
    if path == stdout_path() {
        let mut w = std::io::BufWriter::new(std::io::stdout().lock());
        write(&mut w).and_then(|()| w.flush())?;
        say!(cx, "dumped verdicts -> stdout");
    } else {
        report::write_whole(path, write)?;
        say!(cx, "dumped verdicts -> {}", path.display());
    }
    Ok(())
}

/// From the run's claims to what its report states: every pair, stated once
/// for every file either end answers for, and the content rule's
/// suggestion, read off the same pairs.
fn assemble(cx: &Ctx, scope: &Scope, gui: Option<&Path>, files: &[PathBuf], stats: &[walk::Stat], analysed: Analysed, claims: (Vec<Edge>, Vec<Edge>, Vec<Edge>)) -> Found {
    let args = cx.args;
    let Analysed { items, same, store, names, span_of, not_asked, n_kps, .. } = analysed;
    let (all, propagated, corroborated) = claims;
    // The pairs are the claim. `stated` collects every one of them, and
    // `group::find` reduces it to representatives — a representative plus the
    // files that matched it directly, which is neither the closure nor the
    // maximal cliques; both were measured and both are worse. So a group
    // repeats claims this run actually made rather than implying new ones,
    // and it is not an all-pairs assertion either: two members that both
    // matched the representative were never compared with each other. The
    // argument and the numbers are in `group.rs`.
    //
    // Every pair so far is between the files matched. Each is now stated for
    // every file either end answers for, with the verdict its pair was found
    // on; see `Same::expand`.
    let (all, propagated, corroborated) = (same.expand(all), same.expand(propagated), same.expand(corroborated));
    // Which pairs are stated, and from what, in the order they are first
    // met — the order `group::find` and every tie in the sort below keep. The
    // records themselves are a million strings on a large corpus, so they are
    // made afterwards on every thread rather than here on one.
    //
    // The files that are one file come first (`Same::pairs`). Two of the
    // same bytes that are not pictures at all — a pair of empty files, two
    // copies of a README — are identical, and are not a duplicate *image*,
    // which is the only claim this tool makes; only a wildcard walk can bring
    // such files in. A byte-identical pair of broken `.jpg`s is still stated:
    // those did claim to be images. Two alike only in grey are the match they
    // are, made here since no pass verified one.
    let one_file = same.pairs(|i| !not_asked[i]);
    let grey: Vec<Edge> = one_file
        .iter()
        .filter(|p| p.2 == same::Kind::SameGrey)
        .map(|&(a, b, _)| Edge::new(a, b, Verdict::perfect(n_kps[a])))
        .collect();
    let mut grey_edges = grey.iter();
    let ones = one_file.iter().map(|&(a, b, kind)| {
        let s = match kind {
            same::Kind::Identical => Stated::Identical,
            same::Kind::SamePixels => Stated::SamePixels,
            same::Kind::SameGrey => Stated::Match(grey_edges.next().expect("one edge per grey pair"), false),
        };
        (a, b, s)
    });
    let tiers = all.iter().map(|e| (e, false)).chain(propagated.iter().map(|e| (e, false))).chain(corroborated.iter().map(|e| (e, true)));
    let mut seen: std::collections::HashSet<(usize, usize)> = Default::default();
    let stated: Vec<(usize, usize, Stated)> = ones
        .chain(tiers.map(|(e, corroborated)| (e.a(), e.b(), Stated::Match(e, corroborated))))
        .filter(|&(a, b, _)| seen.insert((a, b)) && scope.speaks_of(a, b))
        .collect();
    drop(seen);
    drop(one_file);
    // What to do with each file, read off the same pairs. The other rules
    // read the groups, and are applied once those exist. Against a library,
    // its files are kept whatever the rest say, and each deletion names the
    // file it rests on. The kept scan's suggestions are the content rule's
    // whatever this run shows (`last`), so a scan kept is a scan that works
    // them out.
    let (actions, keeper) = if args.suggest != suggest::Mode::Content && !keeps_last(args, gui, scope) {
        (Vec::new(), Vec::new())
    } else {
        let cached = Cached { store: store.as_ref(), names: &names, span_of: &span_of };
        timed!(44, suggest_content(args, scope, files, &items, stats, &same, &stated, cached))
    };
    // Every colour the suggestion measured is in the cache.
    drop(store);
    // One name per file, shared by every pair that names it; see `OutPair::a`.
    let names: Vec<std::sync::Arc<str>> = files.par_iter().map(|f| f.display().to_string().into()).collect();
    let mut out_pairs: Vec<OutPair> = stated
        .par_iter()
        .map(|&(a, b, s)| OutPair {
            a: names[a].clone(),
            b: names[b].clone(),
            a_bytes: report::raw_bytes(&files[a]),
            b_bytes: report::raw_bytes(&files[b]),
            aligned_points: s.aligned_points().unwrap_or(0),
            frame_overlap: s.frame_overlap(),
            pixel_correlation: s.pixel_correlation(),
            scale: round3(file_scale(s.verdict().map_or(1.0, |v| v.scale), &items[a], &items[b])),
            mirrored: s.mirrored(),
            inverted: s.inverted(),
            identical: matches!(s, Stated::Identical),
            same_pixels: matches!(s, Stated::SamePixels),
            propagated: s.relation() == "propagated",
            corroborated: s.relation() == "corroborated",
            ia: a,
            ib: b,
        })
        .collect();
    // The same pairs as the report reads them: `group::find`'s graph, or as
    // the verdicts against a library read them (`against.rs`).
    let (graph, against_pairs): (Vec<(usize, usize)>, Vec<against::Pair>) = if scope.against {
        (Vec::new(), stated.iter().map(|&(a, b, s)| s.against_pair(a, b)).collect())
    } else {
        (stated.iter().map(|&(a, b, _)| (a, b)).collect(), Vec::new())
    };
    drop(stated);
    // Stable, as `sort_by` is, so equal names keep the order they were met in.
    out_pairs.par_sort_by(|x, y| x.a.cmp(&y.a).then(x.b.cmp(&y.b)));

    let failures: Vec<serde_json::Value> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            let e = it.err.as_ref().filter(|e| !not_asked[i] && *e != decode::CAMERA_RAW)?;
            let mut f = serde_json::json!({"path": files[i].display().to_string(), "error": e});
            if let Some(b) = report::raw_bytes(&files[i]) {
                f["path_bytes"] = b.into();
            }
            Some(f)
        })
        .collect();
    Found {
        pairs: out_pairs,
        graph,
        against_pairs,
        actions,
        keeper,
        failures,
        dims: items.iter().map(|it| it.dims).collect(),
        damaged: items.iter().map(|it| it.damaged.is_some()).collect(),
        sizes: stats.iter().map(|s| s.len).collect(),
        not_asked,
        ok: items.iter().map(|it| it.ok).collect(),
    }
}

/// Whether the scan is kept for `--last` and the window (`last`): not one
/// against a library, nor one the window runs (it keeps its own), nor one
/// with the cache off or `--no-last`.
fn keeps_last(args: &Args, gui: Option<&Path>, scope: &Scope) -> bool {
    gui.is_none() && !scope.against && !args.no_cache && !args.no_last
}

/// The cache, still open after the analysis, and where each file's record
/// sits in it: the suggestion keeps there the colours it measures.
struct Cached<'a> {
    store: Option<&'a cache::Store>,
    names: &'a [PathBuf],
    span_of: &'a [Option<cache::Span>],
}

/// The content rule's suggestion for every file, read off the stated pairs,
/// and the file each deletion rests on.
#[allow(clippy::too_many_arguments)]
fn suggest_content(
    args: &Args,
    scope: &Scope,
    files: &[PathBuf],
    items: &[Item],
    stats: &[walk::Stat],
    same: &Same,
    stated: &[(usize, usize, Stated)],
    cached: Cached,
) -> (Vec<Option<suggest::Action>>, Vec<Option<usize>>) {
    let pics: Vec<suggest::Picture> = items
        .iter()
        .zip(stats)
        .map(|(it, st)| suggest::Picture {
            dims: it.dims,
            work: (it.feats.w, it.feats.h),
            thumb: &it.thumb,
            traits: &it.traits,
            bytes: st.len,
        })
        .collect();
    let links: Vec<(usize, usize, suggest::Link)> = stated.iter().map(|&(a, b, s)| (a, b, s.link())).collect();
    // A JPEG's colour, which its decode to grey did not measure: kept
    // from an earlier run, or decoded once for this one and kept for the
    // next. Asked once a set of identical bytes, however many times the
    // rule asks.
    let measured: std::sync::Mutex<HashMap<usize, f32>> = Default::default();
    let colour = |i: usize| -> f32 {
        if let Some(c) = items[i].traits.colour_kept {
            return c;
        }
        let o = same.original(i);
        if let Some(&c) = measured.lock().unwrap().get(&o) {
            return c;
        }
        let c = decode::colour_of(&files[i]).map(cache::kept_colour);
        if let (Some(c), Some(store)) = (c, cached.store) {
            // In every record its bytes are held under.
            for &k in same.identical_to(i) {
                if let Some(span) = cached.span_of[k] {
                    store.keep_colour(span, &cached.names[k], c);
                }
            }
        }
        let c = c.unwrap_or(0.0);
        measured.lock().unwrap().insert(o, c);
        c
    };
    suggest::suggest_pinned(&pics, &links, args.min_pixel_correlation, colour, scope.pinned())
}

/// What a run found, as its report states it. Empty for a walk that found
/// nothing, which is still a finished run with a report.
#[derive(Default)]
struct Found {
    /// Every stated pair, sorted by name.
    pairs: Vec<OutPair>,
    /// Each stated pair's ends, in the order `group::find` reads them; empty
    /// against a library.
    graph: Vec<(usize, usize)>,
    /// The same pairs as `--against`'s verdicts read them; empty otherwise.
    against_pairs: Vec<against::Pair>,
    /// The content rule's suggestions and the file each deletion rests on,
    /// when they were worked out.
    actions: Vec<Option<suggest::Action>>,
    keeper: Vec<Option<usize>>,
    failures: Vec<serde_json::Value>,
    /// Each file's size as shown, whether it decoded only in part, and its
    /// length.
    dims: Vec<(u32, u32)>,
    damaged: Vec<bool>,
    sizes: Vec<u64>,
    /// Files the run was not asked about after all; see `analyse_all`.
    not_asked: Vec<bool>,
    /// Whether each file was analysed.
    ok: Vec<bool>,
}

/// Write what the run found, and say so: the one place a run ends.
fn finish(cx: &Ctx, scope: &Scope, gui: Option<&Path>, files: &[PathBuf], found: Found, problems: &Problems, started: u64) -> Result<()> {
    let (summary, went) = if scope.against { write_answers(cx, scope, files, found)? } else { write_groups(cx, scope, gui, files, found, problems, started)? };
    let summary = if files.is_empty() { "No images found.".to_string() } else { summary };
    match went {
        Some(path) => say!(cx, "{summary} -> {}", path.display()),
        None => say!(cx, "{summary}"),
    }
    prof::report();
    Ok(())
}

/// The verdicts on each scanned file against the library: the run's summary
/// line, and the file the report went to.
fn write_answers(cx: &Ctx, scope: &Scope, files: &[PathBuf], found: Found) -> Result<(String, Option<PathBuf>)> {
    let (args, library) = (cx.args, &scope.library);
    let Found { pairs, against_pairs, actions, keeper, failures, dims, damaged, sizes, not_asked, ok, .. } = found;
    let asked: Vec<usize> = (0..files.len()).filter(|&i| !library[i] && !not_asked[i]).collect();
    let answers = against::answer(&asked, library, &against_pairs, &actions, &keeper);
    cx.progress.finish();
    let runtime = cx.t.elapsed().as_secs_f64();
    let n_library = (0..files.len()).filter(|&i| library[i] && !not_asked[i]).count();
    let header = against::Header {
        config: run_config(args),
        files_enumerated: asked.len(),
        files_analysed: asked.iter().filter(|&&i| ok[i]).count(),
        library_files: n_library,
        failures: &failures,
        runtime_seconds: (runtime * 1000.0).round() / 1000.0,
    };
    let facts = against::Facts { dims: &dims, sizes: &sizes, damaged: &damaged };
    let tally = |v: against::Verdict| answers.iter().filter(|a| a.verdict == v).count();
    let summary = format!(
        "{} repeat, {} better, {} to review, {} new, of {} scanned against {} in {:.1}s",
        tally(against::Verdict::Repeat),
        tally(against::Verdict::Better),
        tally(against::Verdict::Review),
        tally(against::Verdict::New),
        count(asked.len(), "image", "images"),
        count(n_library, "library image", "library images"),
        runtime
    );
    // Where `-o` says, and `--list`'s paths on stdout. With `--list`, the
    // report goes only to a file `-o` names.
    let target = report::Target::of(args.output.as_deref(), args.format);
    let mut went = None;
    if args.list.is_empty() || args.output.as_deref().is_some_and(|p| p != stdout_path()) {
        timed!(36, against::write(&target, &header, &answers, &against_pairs, &pairs, library, files, &facts))?;
        went = target.file();
    }
    if !args.list.is_empty() {
        against::list(&answers, files, &args.list, args.null)?;
    }
    Ok((summary, went))
}

/// The groups, and each file's suggestion under the rule asked for: the
/// run's summary line, and the file the report went to.
fn write_groups(cx: &Ctx, scope: &Scope, gui: Option<&Path>, files: &[PathBuf], found: Found, problems: &Problems, started: u64) -> Result<(String, Option<PathBuf>)> {
    let args = cx.args;
    let Found { pairs, graph, actions, keeper, failures, dims, damaged, sizes, not_asked, ok, .. } = found;
    let n_ok = ok.iter().filter(|&&ok| ok).count();
    // Each deletion names the file that stays and holds it, at the end of
    // its chain of holders, not the nearest one, which may be deleted too.
    // (`--against` follows the chain itself, and says both.)
    let keeper = suggest::kept_at_end(&actions, &keeper);
    let grouping = timed!(28, group::find(files.len(), &graph));
    let membership = group::membership(&grouping);
    stage!(
        cx,
        "groups: {} representatives over {} files, {} of them in more than one group",
        grouping.len(),
        membership.len(),
        membership.values().filter(|gs| gs.len() > 1).count()
    );
    // Largest first, ties by comparing member paths in order. Paths are
    // unique and no two representatives can hold the same members — the
    // second would have had nothing left to account for — so the order is
    // total and the output is reproducible.
    let mut groups: Vec<OutGroup> = grouping
        .iter()
        .map(|g| OutGroup {
            representative: files[g.representative].display().to_string(),
            files: g.members.iter().map(|&i| files[i].display().to_string()).collect(),
            rep: g.representative,
            members: g.members.clone(),
        })
        .collect();
    groups.sort_by(|a, b| {
        b.files.len().cmp(&a.files.len()).then_with(|| a.files.cmp(&b.files))
    });

    // Cleared before anything is written to stdout, which shares the terminal.
    cx.progress.finish();
    let runtime = cx.t.elapsed().as_secs_f64();
    let (actions, kept, content) = if args.suggest == suggest::Mode::Content {
        (actions, keeper, None)
    } else {
        let corr: HashMap<(usize, usize), f32> = pairs
            .iter()
            .map(|p| ((p.ia.min(p.ib), p.ia.max(p.ib)), if p.identical || p.same_pixels { 1.0 } else { p.pixel_correlation }))
            .collect();
        let seats = groups.iter().flat_map(|g| {
            let corr = &corr;
            g.members.iter().map(move |&i| suggest::Seat {
                file: i,
                representative: i == g.rep,
                correlation: corr.get(&(i.min(g.rep), i.max(g.rep))).copied().unwrap_or(0.0),
            })
        });
        (suggest::by_group(args.suggest, files.len(), seats, args.min_pixel_correlation), Vec::new(), Some((actions, keeper)))
    };
    stage!(
        cx,
        "suggested: {} to keep, {} to delete, {} to review",
        actions.iter().filter(|a| **a == Some(suggest::Action::Keep)).count(),
        actions.iter().filter(|a| **a == Some(suggest::Action::Delete)).count(),
        actions.iter().filter(|a| **a == Some(suggest::Action::Review)).count()
    );
    let mut out = Output {
        tool: "img-fp",
        config: run_config(args),
        // A file a wildcard walk reached and that turned out not to be a
        // picture is a skip, like one the extension list turned away, and is
        // not counted by either.
        files_enumerated: files.len() - not_asked.iter().filter(|&&x| x).count(),
        files_analysed: n_ok,
        failures,
        runtime_seconds: (runtime * 1000.0).round() / 1000.0,
        groups,
        pairs,
        actions,
        kept,
    };

    let summary = format!(
        "{}, {} over {} in {:.1}s",
        count(out.groups.len(), "group", "groups"),
        count(out.pairs.len(), "pair", "pairs"),
        count(n_ok, "image", "images"),
        runtime
    );
    let facts = report::read_facts(&out, files, &dims, &damaged, &sizes);
    // Kept before `-o` is written: a report that could not be written is
    // one case `--last` is for. Not a walk that found no picture at all,
    // such as a wildcard walk of a folder of documents, nor a scan of
    // nothing, a mistyped folder say: either would replace a scan worth
    // writing out again. The window keeps by the same rule.
    if keeps_last(args, gui, scope) && out.files_enumerated > 0 {
        let roots = if args.from_file.is_some() || args.roots.iter().any(|r| r == stdout_path()) {
            // Read from a list, they are files rather than the folders the
            // window's tree starts from, and it finds those itself.
            Vec::new()
        } else {
            args.roots.iter().map(|r| std::path::absolute(r).unwrap_or_else(|_| r.clone())).collect()
        };
        let mut lines = vec![summary.clone()];
        lines.extend(problems.summary());
        let notes = last::Notes::now(problems.any(), started, runtime as u64, lines.join("\n"), roots);
        if let Err(e) = timed!(36, keep_scan(args, &mut out, content, files, &facts, &notes)) {
            say!(cx, "This scan could not be kept for --last: {e}");
        }
    }
    // The window's copy is written first. It is what the window reads to show
    // the scan at all, and a `-o` that failed — a disk full, a folder removed
    // mid-scan — used to end the run before it, so a finished scan showed as
    // a failed one with nothing to look at. Under the window, stdout is the
    // progress channel and the report goes there only if one was asked for
    // by name.
    if let Some(path) = gui {
        let target = report::Target { sink: report::Sink::File(path.to_path_buf()), format: report::Format::Json, pairs: false };
        timed!(36, report::write_with(&target, &out, files, &facts))?;
    }
    let target = report::Target::of(args.output.as_deref(), args.format);
    let mut went = None;
    if gui.is_none() || args.output.as_deref().is_some_and(|p| p != stdout_path()) {
        timed!(36, report::write_with(&target, &out, files, &facts))?;
        went = target.file();
    }
    Ok((summary, went))
}

/// Keep the scan for `--last` and the window (`last`): the report without
/// its pairs, with the content rule's suggestions whichever the run shows.
/// `content` is those suggestions, and the file each deletion rests on, when
/// `out` holds another rule's.
fn keep_scan(
    args: &Args,
    out: &mut Output,
    content: Option<(Vec<Option<suggest::Action>>, Vec<Option<usize>>)>,
    files: &[PathBuf],
    facts: &report::FileFacts,
    notes: &last::Notes,
) -> std::io::Result<()> {
    let mut held = None;
    if let Some((content, kept)) = content {
        held = Some((std::mem::replace(&mut out.actions, content), std::mem::replace(&mut out.kept, kept), out.config["suggest"].take()));
        out.config["suggest"] = "content".into();
    }
    // Every path absolute. The report names files as the run spelled them,
    // and a scan of `photos` kept as `photos/a.jpg` was read back against
    // whatever folder the reader started in: `--last` from another folder,
    // or the window from the home folder, named the files of another
    // `photos` — and the window's Trash would have moved them.
    let files: Vec<PathBuf> = files.iter().map(|f| std::path::absolute(f).unwrap_or_else(|_| f.clone())).collect();
    let spelled: Vec<(String, Vec<String>)> = out
        .groups
        .iter_mut()
        .map(|g| {
            let rep = files[g.rep].display().to_string();
            let members = g.members.iter().map(|&i| files[i].display().to_string()).collect();
            (std::mem::replace(&mut g.representative, rep), std::mem::replace(&mut g.files, members))
        })
        .collect();
    let write = |path: &Path| {
        let target = report::Target { sink: report::Sink::File(path.to_path_buf()), format: report::Format::Json, pairs: false };
        report::write_with(&target, out, &files, facts).map_err(std::io::Error::other)
    };
    let kept = last::save_with(&last::dir(args.cache.as_deref()), write, notes);
    for (g, (rep, members)) in out.groups.iter_mut().zip(spelled) {
        g.representative = rep;
        g.files = members;
    }
    if let Some((actions, kept, rule)) = held {
        out.actions = actions;
        out.kept = kept;
        out.config["suggest"] = rule;
    }
    kept
}

/// The settings a report records, which are the ones that decide its pairs.
fn run_config(args: &Args) -> serde_json::Value {
    let mut c = serde_json::json!({
        "work_size": args.work_size,
        "features": FEATURES,
        "candidates": args.candidates,
        "min_aligned_points": aligned_points(args),
        "min_frame_overlap": as_typed(args.min_frame_overlap),
        "min_pixel_correlation": as_typed(args.min_pixel_correlation),
        "stages": "anchor, propagate, corroborate",
        "mirrored_inverted": !args.skip_mirrored,
        "suggest": format!("{:?}", args.suggest).to_lowercase(),
    });
    // Only when asked for, so that a scan's report is what it always was.
    if !args.against.is_empty() {
        c["against"] = args.against.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().into();
    }
    c
}

/// A threshold as it was typed. The flags are `f32`, and JSON numbers are
/// `f64`: widened as they stand, `0.85` was written `0.8500000238418579`. The
/// shortest form that reads back as the same `f32` is the number the user
/// gave, and as an `f64` it prints as that.
fn as_typed(v: f32) -> f64 {
    v.to_string().parse().unwrap_or(v as f64)
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_scan_is_the_scope_with_no_library() {
        let plain = Scope { against: false, library: vec![false; 3] };
        assert!((0..3).all(|i| plain.asks_first(i)));
        assert!(plain.speaks_of(0, 1) && plain.speaks_of(1, 2));
        assert!(plain.pinned().iter().all(|&p| !p));
        // Files 1 and 2 are the library's.
        let against = Scope { against: true, library: vec![false, true, true] };
        assert!(against.asks_first(0) && !against.asks_first(1));
        assert!(against.speaks_of(0, 1));
        assert!(!against.speaks_of(1, 2), "nothing is said about two library files");
        assert_eq!(against.pinned(), &[false, true, true]);
    }

    /// Either file may be the one that asks, and the second look asks from
    /// the higher index as often as not: the edge is the same edge.
    #[test]
    fn a_verdict_is_the_same_edge_from_either_end() {
        let mut g = decode::Gray::new(240, 180);
        for y in 0..180 {
            for x in 0..240 {
                let v = ((x * 7 + y * 13) % 97) as f32 / 96.0;
                g.px[y * 240 + x] = v * 0.6 + ((x * y) % 31) as f32 / 120.0;
            }
        }
        let small = decode::resize_area(&g, 192, 144);
        let p = sift::Params::default();
        let items: Vec<Item> = [&g, &small]
            .into_iter()
            .map(|g| Item {
                feats: std::sync::Arc::new(sift::extract(g, &p)),
                thumb: std::sync::Arc::new(Thumb::build(g, THUMB_LONG)),
                dims: (g.w as u32, g.h as u32),
                ok: true,
                ..Default::default()
            })
            .collect();
        let pool: Vec<&[u8; DESC_LEN]> = items.iter().flat_map(|it| (0..it.feats.len()).map(|i| it.feats.d(i).try_into().unwrap())).collect();
        let vocab = Vocabulary::build(pool, &index::VocabParams::for_corpus(items.iter().map(|it| it.feats.len()).sum()));
        let lists: Vec<WordList> = items.iter().map(|it| quantise(&vocab, &it.feats)).collect();
        let policy = verify::Policy::new(MIN_ALIGNED_POINTS, 0.5, 0.6);
        let pass = Pass { items: &items, matched: &[true, true], policy: &policy, gate: (3, 0.2), dumping: false };
        let ask = |i: usize, j: usize| pass.verify(&lists, i, &items[i].feats, &lists[i], j, Variant::default(), &policy.anchor, &mut Work::default());
        let (low, a) = ask(0, 1).expect("a picture and its downscale are an anchor");
        let (high, b) = ask(1, 0).expect("asked from the other end too");
        assert!(a && b);
        assert_eq!((low.a(), low.b()), (0, 1));
        assert_eq!((high.a(), high.b()), (0, 1));
        assert!((low.v.scale - high.v.scale).abs() < 0.02 * low.v.scale, "scale {} from below, {} from above", low.v.scale, high.v.scale);
    }
}
