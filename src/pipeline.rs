//! One scan, from the paths asked for to the report: `run` and its stages.

use super::*;
use analysis::{Item, Planes, Read};
use decode::Refused;
use claims::{Claim, Clusters, How, Propagation};
use retrieve::{quantise, Index};
use outcome::{Against, Asked, Group, Head, Member};
use verify::Standing;
use problems::Note;
use outputs::{Outputs, To};

/// What every stage of a run reads: what it was asked, and where it says
/// what it is doing.
struct Ctx<'a> {
    args: &'a Args,
    outputs: &'a Outputs,
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

    /// The files an anchor among `edges` reached from a file that has asked
    /// and that have not asked themselves: those ask next. None in a plain
    /// scan, whose files all asked in the first round.
    ///
    /// Against a library, the scanned files ask first, and then every library
    /// file a file that has asked anchored asks in turn, round after round
    /// until none is new: the library's own pairs are what a chain from a
    /// scanned file runs through (`propagate`), and they are wanted only where
    /// the library touches the scan. A library of unique photographs is
    /// nearly all unreached and asks nothing, where the union of both sides
    /// had every file of it ask and be re-asked mirrored.
    fn reached(&self, edges: &[Edge], policy: &verify::Policy, asked: &[bool]) -> Vec<usize> {
        let mut next = vec![false; asked.len()];
        for e in edges.iter().filter(|e| policy.standing(&e.v) >= Standing::Anchor) {
            let (i, j) = e.ends();
            for (x, y) in [(i, j), (j, i)] {
                if asked[x] && self.library[y] && !asked[y] {
                    next[y] = true;
                }
            }
        }
        (0..next.len()).filter(|&i| next[i]).collect()
    }

    /// The matched files the second look asks again, mirrored and inverted,
    /// after the `direct` verdicts.
    ///
    /// A mirrored or inverted copy shares no visual words with its original,
    /// so retrieval cannot find it, and the query has to be re-asked turned
    /// (`sift::Variant`). Doing that for every image would roughly double the
    /// retrieval cost for a handful of pairs, so it is asked only where the
    /// first pass came up short: a file with no matches, or with so few that
    /// it may be hanging off the edge of its real cluster.
    ///
    /// A byte-identical copy is not counted. It is the same picture, so it says
    /// nothing about whether the picture's other versions were found — and
    /// counted, it kept every file that has a copy from ever being asked
    /// again: in a library where each photograph is there twice over, no file
    /// was, and every mirrored and inverted version in it went unfound.
    /// Ask again for images that no *pair* of matches has anchored yet. A
    /// file with one match is not safely found: that match may be the wrong
    /// one, and a mirrored query is cheap next to being wrong. Two is not a
    /// fitted threshold, it is "more than one" — the same minimal constant
    /// the bridge test uses for "more than one file", and the only place a
    /// count appears in either rule. Trying the tighter reading, "no matches
    /// at all", costs a seed apiece on three of the held-out mirror
    /// transforms.
    ///
    /// Against a library, a library file is asked as any file is once the scan
    /// has reached it: one with no match at all is no part of an answer, and
    /// asking each of those is what made matching both sides as one four times
    /// the cost against a library of unique photographs. That leaves the case
    /// those files covered: a mirror of a scanned picture that nothing else in
    /// the library matches, which matched as one scan is re-asked itself, for
    /// having no match, and here is found from the scan's side or not at all.
    /// Counting a scanned file's matches cannot tell when to look — on
    /// IMGS-AGAINST the files it caught had six — and re-asking every scanned
    /// file found them at a third more CPU. A mirror of one file of a picture's
    /// family is a mirror of the family, so each clean-anchor component that
    /// holds a scanned file is re-asked once, through its best-connected
    /// member, and propagation carries what that finds to the rest: 2,635
    /// files re-asked there against 11,024, the same finds. Clean anchors,
    /// because weak ones join families into a few large components, and one
    /// member of each found ten fewer.
    fn re_askers(&self, direct: &[Edge], policy: &verify::Policy, matched: &[bool]) -> Vec<usize> {
        let n = matched.len();
        let mut degree = vec![0u32; n];
        for e in anchors(direct, policy) {
            degree[e.a()] += 1;
            degree[e.b()] += 1;
        }
        let mut rep = vec![false; n];
        if self.against {
            let d = Clusters::joined(n, anchors(direct, policy).filter(|e| policy.standing(&e.v) == Standing::Clean));
            // Per component: whether a scanned file is in it, and its best-
            // connected member, ties to the lowest index.
            let mut best: HashMap<usize, (bool, usize)> = HashMap::new();
            for i in (0..n).filter(|&i| degree[i] > 0) {
                let e = best.entry(d.id(i)).or_insert((false, i));
                e.0 |= !self.library[i];
                if degree[i] > degree[e.1] {
                    e.1 = i;
                }
            }
            for (scanned, i) in best.into_values() {
                rep[i] |= scanned;
            }
        }
        (0..n).filter(|&i| matched[i] && (rep[i] || (degree[i] < 2 && !(self.library[i] && degree[i] == 0)))).collect()
    }

    /// Whether a matched file with `degree` anchors so far is past being
    /// asked again whatever verification finds next (`re_askers`), which is
    /// what the bar reads while it verifies. Anchors only add up, so two of
    /// them settle a file in a plain scan; against a library a component's
    /// best-connected member is asked however many it has.
    fn settled(&self, degree: u32) -> bool {
        !self.against && degree >= 2
    }
}

/// One scan, from the paths asked for to the report: each stage below hands
/// the next what it found, and lets go of what nothing after it reads.
pub(crate) fn run(args: &Args, outputs: &Outputs, log: &Log, problems: &mut Problems, gui: Option<&Path>) -> Result<()> {
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
    let cx = Ctx { args, outputs, log, progress: progress::Progress::new(), t: t_start };

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
                Err(e) => problems.note(Note::Cache, format_args!("could not delete {}: {e}", p.display())),
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
    let matched: Vec<bool> = (0..n).map(|i| items[i].read.ok() && same.answers_for(i) == i).collect();
    let n_same = (0..n).filter(|&i| items[i].read.ok() && !same.is_copy(i) && same.answers_for(i) != i).count();
    if n_same > 0 {
        stage!(cx, "same pixels: {n_same} files decode to a picture another file already holds");
    }

    say!(cx, "Analysis complete. Matching {}...", count(analysed.n_ok, "image", "images"));
    let policy = verify::Policy::new(aligned_points(args), args.min_frame_overlap, args.min_pixel_correlation);
    // What a verdict must already have before its pixels are worth reading:
    // what the lowest standing asks (`Policy::gate`), or everything a dump
    // would record.
    //
    // A dump widens the gate and never narrows it: the bars the run's own
    // tiers use still decide, so a run with `--dump` finds the pairs the same
    // run without it finds. (It used to *set* the gate to (3, 0.2), which is
    // tighter than a `--min-frame-overlap` below 0.2 and so lost pairs.)
    let mut dump = args.dump.is_some().then(Dump::default);
    let gate = policy.gate();
    let gate = if dump.is_some() { (gate.0.min(3), gate.1.min(0.2)) } else { gate };
    let pass = Pass { items, matched: &matched, policy: &policy, gate, dumping: dump.is_some() };
    let index = index_all(&cx, &scope, &pass);
    let (mut direct, verified) = retrieve_and_verify(&cx, &scope, &pass, &index);
    let turned = second_look(&cx, &scope, &pass, index, &direct);
    direct.extend(turned);

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

    let claims = cluster(&cx, items, &policy, &direct, verified, dump.as_mut());
    if let (Some(to), Some(dump)) = (&outputs.dump, dump) {
        write_dump(&cx, to, &files, items, &direct, dump)?;
    }
    // Every verdict the matching made, and what the dump alone reads: nothing
    // below does. Held to the end, they were 65 MB on 27,000 pictures through
    // the assembly, which is where such a run peaks.
    drop(direct);

    let found = assemble(&cx, &scope, gui, &files, &stats, analysed, claims);
    let done = finish(&cx, &scope, gui, &files, found, problems, started);
    // What each stage took, which is what the bar's priors are read from
    // (`progress::Stage::prior`).
    for line in cx.progress.paces() {
        stage!(cx, "{line}");
    }
    done
}

/// What the analysis hands on: every file's analysis, which files are one
/// file, and the cache, still open for the colours the suggestion measures.
struct Analysed {
    items: Vec<Item>,
    same: Same,
    store: Option<cache::Store>,
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
    // What the bar knows of the run before anything is read; every stage
    // below tells it more as soon as it has counted it.
    progress.learn(|k| {
        k.files = files.len();
        k.cache_bytes = cache_path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map_or(0, |m| m.len());
        k.cache_write = cache_path.is_some();
        k.work_size = args.work_size;
        k.candidates = args.candidates;
        if args.skip_mirrored {
            k.ask_again = Some((0, 0));
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
    // A copy is answered by its original: the most there is to describe
    // until the cache has said what it answers.
    let originals = (0..files.len()).filter(|&i| !same.is_copy(i)).count();
    progress.learn(|k| k.to_describe = Some(originals));

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
    let mut store = cache_path.as_ref().map(|p| {
        let names = cache_names(files, canonical);
        progress.begin(Stage::CacheRead);
        // Only this run's paths are unpacked; the rest of the machine's
        // cache is read for where it is, not for what it says. See
        // `cache::open`.
        //
        // Each original the cache is seen to answer as it reads is one less
        // to describe, for the bar: on a rescan the read is the first tenth
        // of the run, and until it ends every file was a file to describe.
        let met: Vec<std::sync::atomic::AtomicBool> = (0..n).map(|_| Default::default()).collect();
        let answered = |i: usize| {
            if !met[same.original(i)].swap(true, Ordering::Relaxed) {
                progress.learn(|k| k.to_describe = k.to_describe.map(|t| t.saturating_sub(1)));
            }
        };
        cache::open(p, settings, names.into_iter().zip(stats.iter().map(|s| s.key)).collect(), &answered, problems)
    });
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
    if let Some(s) = store.as_ref().filter(|s| s.usable() > 0) {
        stage!(cx, "cache: {} usable records", s.usable());
    }
    // Every walked file's record comes *out* of the store rather than being
    // copied from it. The analysis is the largest thing the run holds, and
    // cloning it here made it exist twice over — once in the cache's map and
    // once in `items` — for the length of the phase that already sets the
    // peak, and then charged a second bill to free the originals. On the
    // found corpus that was 1.9 s of dropping a nine-thousand-record map,
    // about as much again cloning into it, and 164 MB of peak.
    //
    // The exact pass has already grouped the files whose bytes hash the same,
    // and the analysis depends on nothing but those bytes. Describing the
    // second copy of a file is not a cheaper way to reach the same answer, it
    // is the same work done twice: each group's original (`Same::of_bytes`)
    // is described and the rest are copied from it. Those groups are already
    // claimed as duplicates in the output, so sharing one analysis between
    // them asserts nothing the run does not assert anyway.
    //
    // The bytes are the same, so the analysis is: the first record any of
    // them has serves the original, and stays in the file under the name of
    // the copy it was made for, where the next run finds it again. Any other
    // copy's is not taken, and so not kept (`cache::Store::take`).
    let mine: Vec<Option<cache::Record>> = (0..n)
        .map(|i| {
            let s = store.as_mut().filter(|_| !same.is_copy(i))?;
            same.identical_to(i).iter().find_map(|&k| s.take(k))
        })
        .collect();
    // What each file still to be analysed is expected to cost, for the bar
    // and nothing else. Read from headers, in parallel, and only for files the
    // cache and the exact pass have not already answered, so a cached run
    // opens nothing here.
    let todo = (0..n).filter(|&i| !same.is_copy(i) && mine[i].is_none()).count();
    // Whether the cache will have anything to write, as far as it can be
    // told yet.
    let rewrite = store.as_ref().is_some_and(|s| args.prune_cache || s.stale());
    progress.learn(|k| {
        k.to_describe = Some(todo);
        k.cache_write = rewrite;
    });
    let headers = progress.begin_counted(Stage::Headers, n as u64, n, "files");
    // The headers are kept as well: the decode claims its share of the
    // decode budget from them before it reads the file.
    let (cost, header): (Vec<progress::FileCost>, Vec<Option<decode::Probe>>) = (0..n)
        .into_par_iter()
        .map(|i| {
            headers.tick();
            if same.is_copy(i) || mine[i].is_some() {
                return Default::default();
            }
            let probe = decode::probe(&files[i]);
            (progress::file_cost(probe.as_ref(), stats[i].len, args.work_size, sp.upsample_below), probe)
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
    let bar = progress.begin_describe(&(0..n).filter(|&i| !same.is_copy(i) && mine[i].is_none()).map(|i| cost[i]).collect::<Vec<_>>());
    let planes = Planes::default();
    let made: Vec<Option<Item>> = mine
        .into_par_iter()
        .enumerate()
        .map(|(i, rec)| {
            if same.is_copy(i) {
                return None;
            }
            if let Some(rec) = rec {
                return Some(Item::from(rec));
            }
            let t0 = Instant::now();
            let (it, decode) = analysis::analyse(&files[i], args.work_size, &sp, header[i].as_ref(), &planes);
            // Each record goes into the cache file as soon as it is made,
            // which is what lets an interrupt keep the analysis without
            // writing anything; see `cache::Store`.
            if let (Some(s), Read::Whole) = (&store, &it.read) {
                s.append(i, it.dims, &it.traits, &it.feats, &it.thumb);
            }
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            bar.done(&cost[i], decode, t0.elapsed());
            if d % 250 == 0 {
                stage!(cx, "  described {d}");
            }
            Some(it)
        })
        .collect();
    drop(cost);
    drop(header);
    drop(planes);
    // A copy of a file that would not decode fails as its original did, and
    // is reported under its own path by `tell_problems`. It used to be
    // decoded again, here, on one thread, one copy after another — the same
    // bytes through the same decoder, for the same error. (Both were read
    // whole by the exact pass, so an original that could not be *opened* is
    // never in a group to begin with.)
    let items: Vec<Item> = (0..n).map(|i| made[same.original(i)].clone().expect("every original is analysed")).collect();
    drop(made);
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
            problems.note(Note::Cache, "--prune-cache skipped: the walk was incomplete");
            false
        });
    progress.learn(|k| {
        // What the matching stages will be handed: originals, not copies.
        k.images = Some((0..n).filter(|&i| items[i].read.ok() && same.answers_for(i) == i).count());
        k.descriptors = Some((0..n).filter(|&i| same.answers_for(i) == i).map(|i| items[i].feats.len()).sum());
    });
    if let (Some(p), Some(store)) = (cache_path, store.as_mut()) {
        finish_cache(cx, p, store, prune, problems);
    }
    // The store stays open until the suggestion, which keeps in it the colour
    // of each JPEG it has to decode to learn it (`cache::Store::keep_colour`).
    // Nothing more is analysed or read from the cache. What was written stays
    // readable for as long as it is held.
    spill::end();
    stage!(
        cx,
        "cpu: decode {:.0}s, features {:.0}s",
        analysis::T_DECODE.load(Ordering::Relaxed) as f64 / 1e6,
        analysis::T_SIFT.load(Ordering::Relaxed) as f64 / 1e6
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
    let n_ok = items.iter().filter(|i| i.read.ok()).count();
    let n_desc: usize = items.iter().map(|i| i.feats.len()).sum();
    stage!(cx, "described {n_ok}/{n} images, {n_desc} descriptors");
    Analysed { items, same, store, not_asked, n_kps, n_ok }
}

/// Settle the cache file (`cache::Store::finish`), and say what it kept.
fn finish_cache(cx: &Ctx, p: &Path, store: &mut cache::Store, prune: bool, problems: &mut Problems) {
    cx.progress.begin(Stage::CacheWrite);
    let kept = store.finish(prune);
    if let Some(e) = kept.failure {
        // Not fatal — the pairs this run reports are the same pairs — but
        // the next run will pay for this one's analysis all over again,
        // which is exactly what a problem is here.
        problems.note(Note::Cache, format_args!("could not write {}: {e}", p.display()));
    }
    if kept.carried > 0 {
        stage!(cx, "cache: {} records kept from other scans", kept.carried);
    }
    if kept.other_sizes > 0 {
        stage!(cx, "cache: {} records kept from other working sizes", kept.other_sizes);
    }
    if prune && kept.dropped > 0 {
        say!(cx, "pruned {} cached record(s) this scan did not use", kept.dropped);
    }
    match kept.compacted {
        cache::Compacted::Unneeded { records, unused } => stage!(cx, "cache: {records} records, {unused} of them unused, not compacted"),
        cache::Compacted::Busy => stage!(cx, "cache: not compacted, another run has it open"),
        cache::Compacted::Done => {}
        cache::Compacted::Failed(e) => problems.note(Note::Cache, format_args!("could not write {}: {e}", p.display())),
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
    let not_asked: Vec<bool> = files
        .iter()
        .zip(items)
        .map(|(f, it)| match it.read {
            Read::Refused(Refused::MacMetadata) => true,
            Read::Refused(Refused::NotAnImage) => !wanted.is_a_guess_at_images() && !extensions::names_an_image(f),
            _ => false,
        })
        .collect();
    for ((f, it), &not_asked) in files.iter().zip(items).zip(&not_asked) {
        let f = f.display().to_string();
        match &it.read {
            Read::Refused(Refused::MacMetadata) => problems.note(Note::MacMetadata, &f),
            // A raw file is a picture, not read: a skip, and still in its
            // exact group, since a copy of its bytes is a copy of it.
            Read::Refused(Refused::CameraRaw) => problems.note(Note::CameraRaw, &f),
            // A file a wildcard walk reached that is no picture at all never
            // claimed to be one; see `extensions.rs`.
            Read::Refused(_) if not_asked => problems.note(Note::NotImageContent, &f),
            Read::Refused(r) => problems.note(Note::Unreadable, format_args!("{f}: {r}")),
            Read::Failed(e) => problems.note(Note::Unreadable, format_args!("{f}: {e}")),
            // Decoded, described, and described to nothing. Nothing failed —
            // a blank picture really has no local features — but the file is
            // in the corpus and the matcher has nothing to say about it, and
            // that is worth a line for the same reason a refusal is: the run
            // did not answer the question it was asked about this file.
            Read::Whole | Read::Damaged(_) if it.feats.len() == 0 => problems.note(Note::Featureless, &f),
            Read::Whole | Read::Damaged(_) => {}
        }
        if let Some(why) = it.read.damaged() {
            problems.note(Note::Damaged, format_args!("{f}: {why}"));
        }
    }
    not_asked
}

/// Candidate pairs verified per slice; see `retrieve_and_verify`. A few
/// seconds of work across the workers, so the barrier between slices costs
/// nothing measurable, and a few megabytes of verdicts, so a slice's own
/// collect is no transient worth the name.
const VERIFY_SLICE: usize = 1 << 16;

/// What both verification passes read: the files, which of them are
/// matched (`run`), the acceptance bars, and the gate a verdict must clear
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
    /// as it is). The edge, from the lower index to the higher, when it
    /// stands at all (`Standing::Corroborating`) or `--dump` records it.
    #[allow(clippy::too_many_arguments)]
    fn verify(&self, lists: &[WordList], i: usize, fi: &Features, wi: &WordList, j: usize, variant: Variant, (cands, matches, scratch): &mut Work) -> Option<Edge> {
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
        if self.policy.standing(&v) == Standing::Rejected && !(self.dumping && v.n_in >= 3) {
            return None;
        }
        // Every edge runs from the lower index to the higher, and its verdict
        // with it: stored the other way round, the report's `scale` for the
        // pair was the reciprocal of the scale between the files it names.
        if i < j { Some(Edge::new(i, j, v)) } else { Some(Edge::new(j, i, v.reversed()?)) }
    }
}

/// Anchors only: a pair believed on its own evidence. These are what decide
/// which files end up in one cluster. They are read out of the direct
/// verdicts where they lie rather than copied: they are nearly all of them,
/// and a copy was another hundred-odd megabytes held through the second look,
/// which is where a large corpus's run peaks.
fn anchors<'a>(direct: &'a [Edge], policy: &'a verify::Policy) -> impl Iterator<Item = Edge> + 'a {
    direct.iter().filter(|e| policy.standing(&e.v) >= Standing::Anchor).copied()
}

/// The vocabulary, built from the corpus, and every matched file's words:
/// the index every file asks.
fn index_all<'a>(cx: &Ctx, scope: &Scope, pass: &Pass<'a>) -> Index<'a> {
    let progress = &cx.progress;
    let (items, matched) = (pass.items, pass.matched);
    let n = items.len();
    let n_match = matched.iter().filter(|&&m| m).count();
    let n_desc_match: usize = (0..n).filter(|&i| matched[i]).map(|i| items[i].feats.len()).sum();

    // Vocabulary from the corpus itself, at a depth the corpus chooses. A
    // copy's descriptors are its original's, so counting them would weigh
    // that picture twice in the sample and size the tree for words that are
    // not there.
    let vp = index::VocabParams::for_corpus(n_desc_match);
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

    // Inverted file, of every word; see `InvertedFile::build`. Against a
    // library, not every file asks, so each list's idf mass is kept for
    // reading its answers both ways round; see `retrieve::both_ways`.
    progress.begin(Stage::InvertedFile);
    let index = Index::new(items, matched, vocab, lists, scope.against);
    stage!(cx, "inverted file");
    index
}

/// Every file that asks first asks the index as it is, and its candidates are
/// verified: the direct verdicts, and how many candidates were verified,
/// which is what a round of propagation may spend.
fn retrieve_and_verify(cx: &Ctx, scope: &Scope, pass: &Pass, index: &Index) -> (Vec<Edge>, usize) {
    let (args, progress) = (cx.args, &cx.progress);
    let (items, matched) = (pass.items, pass.matched);
    let n = items.len();
    // Retrieval and verification are separate passes so that a pair proposed
    // from both sides is verified once. Containment scoring is asymmetric —
    // the crop finds the photograph much more readily than the reverse — so
    // both directions genuinely have to be asked.
    // Against a library, the scanned files ask first, and then the library
    // files they reach, round after round until none is new (`Scope::reached`).
    let mut asked = vec![false; n];
    let mut ask: Vec<usize> = (0..n).filter(|&i| matched[i] && scope.asks_first(i)).collect();
    let mut seen: std::collections::HashSet<(u32, u32)> = Default::default();
    let mut all_direct: Vec<Edge> = Vec::new();
    let mut verified = 0usize;
    let mut round = 0usize;
    // Anchors each file has so far, and the files and descriptors they have
    // put past the second look (`Scope::settled`): the bar's bound on it.
    let mut degree = vec![0u32; n];
    let mut settled = (0usize, 0usize);
    loop {
        for &i in ask.iter() {
            asked[i] = true;
        }
        let first = round == 0;
        let descriptors: usize = ask.iter().map(|&i| items[i].feats.len()).sum();
        let bar = progress.begin_counted(Stage::Candidates, descriptors as u64, ask.len(), "images");
        let mut cand_pairs: Vec<(u32, u32)> = ask
            .par_iter()
            .map_init(
                || index.asking(),
                |a, &i| {
                    let cands = index.candidates(i, &[&index.lists()[i]], args.candidates, a);
                    bar.add(items[i].feats.len() as u64);
                    let i = i as u32;
                    cands.iter().map(|&(j, ..)| if j < i { (j, i) } else { (i, j) }).collect::<Vec<_>>()
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
        let vbar = progress.begin_counted(Stage::Verify, cand_pairs.len() as u64, cand_pairs.len(), "pairs");
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
                    vbar.tick();
                    let (i, j) = (i as usize, j as usize);
                    pass.verify(index.lists(), i, &items[i].feats, &index.lists()[i], j, Variant::SAME, work)
                }))
                .flatten()
                .collect();
            for e in anchors(&part, pass.policy) {
                for i in [e.a(), e.b()] {
                    degree[i] += 1;
                    if matched[i] && degree[i] == 2 && scope.settled(degree[i]) {
                        settled = (settled.0 + 1, settled.1 + items[i].feats.len());
                    }
                }
            }
            progress.learn(|k| k.settled = settled);
            all_direct.extend(part);
        }
        verified += cand_pairs.len();
        drop(cand_pairs);
        ask = scope.reached(&all_direct[from..], pass.policy, &asked);
        if ask.is_empty() {
            break;
        }
        round += 1;
    }
    let n_edges = anchors(&all_direct, pass.policy).count();
    stage!(cx, "anchors: {} of {} verified pairs", n_edges, all_direct.len());
    progress.learn(|k| k.anchors = Some(n_edges));
    (all_direct, verified)
}

/// The second look at files the direct pass came up short on
/// (`Scope::re_askers`): each asks again mirrored and inverted. Its verdicts,
/// kept as the direct pass's are; the index goes with it.
fn second_look(cx: &Ctx, scope: &Scope, pass: &Pass, mut index: Index, direct: &[Edge]) -> Vec<Edge> {
    let (args, progress) = (cx.args, &cx.progress);
    let (items, policy) = (pass.items, pass.policy);
    // The postings go until the second look asks; see `Index::let_go`.
    index.let_go();
    // `--skip-mirrored` asks it of nobody, and everything below then does
    // nothing: no word lists made, no postings rebuilt, no verdicts.
    let askers: Vec<usize> = if args.skip_mirrored { Vec::new() } else { scope.re_askers(direct, policy, pass.matched) };
    // Each variant is asked the same retrieval question it always was, and
    // then the three answers are spent as one: a candidate is verified once,
    // under the variant it scored highest for, and only the best `-k` of the
    // merged list are verified at all (`Index::candidates`).
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
    let weight: Vec<u64> = askers.iter().map(|&i| progress.ask_again_cost(items[i].feats.len())).collect();
    let total_weight: u64 = weight.iter().sum();
    let bar = progress.begin_counted(Stage::Variants, total_weight, askers.len(), "images");
    index.turn(&askers);
    let index = &index;
    let turned: Vec<Edge> = askers
        .par_iter()
        .enumerate()
        .zip(weight.par_iter())
        .map_init(
            || (index.asking(), Work::default()),
            |(a, work), ((at, &i), &w)| timed!(38, {
                let seen: [Features; 3] = timed!(40, std::array::from_fn(|v| items[i].feats.seen_as(Variant::TURNED[v])));
                let words = index.turned(at, &seen);
                let mut out: Vec<Edge> = Vec::new();
                for &(j, _, v) in index.candidates(i, &[&words[0], &words[1], &words[2]], args.candidates, a) {
                    let v = v as usize;
                    out.extend(pass.verify(index.lists(), i, &seen[v], &words[v], j as usize, Variant::TURNED[v], work));
                }
                bar.add(w);
                out
            }),
        )
        .flatten()
        .collect();
    if args.skip_mirrored {
        stage!(cx, "variants: skipped");
    } else {
        stage!(cx, "variants: {} more pairs from {} unmatched images", anchors(&turned, policy).count(), askers.len());
    }
    // Nothing after this matches a descriptor against another, so the
    // vocabulary, the inverted file and the word lists go with this stage.
    turned
}

/// From anchors to the run's claims: drop weak bridges, admit weak anchors
/// into the clusters the clean ones make, propagate transforms inside each
/// cluster, and corroborate the weaker direct matches inside them: the
/// run's claims, in that order.
fn cluster(
    cx: &Ctx,
    items: &[Item],
    policy: &verify::Policy,
    direct: &[Edge],
    prop_budget: usize,
    dump: Option<&mut Dump>,
) -> Vec<Claim> {
    let progress = &cx.progress;
    let n = items.len();
    let dumping = dump.is_some();
    // One edge per pair of files. The second look can verify a pair the
    // direct pass already anchored, and two lonely files can each find the
    // other — and a pair counted twice is two links to the bridge test, so a
    // lone match between two clusters stopped being a bridge by being found
    // twice. The direct pass's verdicts come first, and one is kept where
    // there are two.
    let all: Vec<Edge> = claims::unique_pairs(anchors(direct, policy));

    // Every anchor faces the bridge test, whichever pass produced it: a
    // mirrored match joining two clusters is exactly as consequential as a
    // direct one, and on this corpus an inverted match between a 225-pixel
    // photograph of the Earth and a beach scene was the single edge that
    // merged two whole families into 3,002 false pairs.
    progress.learn(|k| k.anchors = Some(all.len()));
    progress.begin(Stage::Propagate);
    // Clusters are made from clean anchors, and an anchor that is only an
    // anchor may add a file to one but never join two; see `claims::admit`.
    let mut clean: Vec<bool> = all.iter().map(|e| policy.standing(&e.v) == Standing::Clean).collect();
    let demoted = claims::demote_bridges(&all, &mut clean, n);
    stage!(cx, "bridges: dropped {demoted} lone links between clusters");
    let n_weak = clean.iter().filter(|&&c| !c).count();
    let (clusters, mut claimed, refused) = claims::admit(all, &clean, n);
    drop(clean);
    stage!(cx, "weak anchors: {} admitted, {refused} refused between clusters", n_weak - refused);

    // ---- propagate transforms inside each cluster
    // Propagation is iterated: each round's accepted pairs become edges the
    // next round's tree can route through, which shortens the path to files
    // that were only reachable the long way round. It converges in two or
    // three rounds. What is claimed so far is the anchors, then each round's
    // composed pairs.
    let n_anchors = claimed.len();
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
    // A composed pair is kept only if it clears the overlap floor; a dump
    // wants every hypothesis the round considered.
    let prop_min_ov = if dumping { policy.min_frame_overlap().min(0.2) } else { policy.min_frame_overlap() };
    // Which files' clusters a round has to propose again: all of them in
    // the first round, and after that only those holding a pair the round
    // before accepted. A cluster that gained nothing has the same edges, so
    // the same tree and the same poses, and proposes exactly the hypotheses it
    // proposed last time — the accepted ones now known, the rest rejected
    // again.
    let mut dirty: Option<Vec<bool>> = None;
    let mut proposed_total = 0usize;
    // The clusters whose last proposal was a star, by lowest file, with their
    // sizes. A cluster starred in one round is the only one left to propose in
    // the next as often as not, and then fits the budget alone and is compared
    // pair by pair after all — so what to say is decided once the rounds are
    // done, from each cluster's last round, and not from its first.
    let mut still_starred: std::collections::BTreeMap<usize, usize> = Default::default();
    for round in 0..PROPAGATE_MAX_ROUNDS {
        let Propagation { found: round_all, starred, full, proposed } =
            timed!(21, claims::propagate(items, &claimed, &clusters, prop_min_ov, dirty.as_deref(), prop_budget));
        proposed_total += proposed;
        for first in full {
            still_starred.remove(&first);
        }
        still_starred.extend(starred);
        // `propagate` proposes no pair already claimed.
        let fresh: Vec<Edge> = round_all.iter().filter(|e| policy.composed(&e.v)).copied().collect();
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
        stage!(cx, "  propagation round {}: +{} pairs", round + 1, fresh.len());
        if fresh.is_empty() {
            break;
        }
        claimed.extend(fresh);
    }

    stage!(
        cx,
        "propagated: {} of {} composed hypotheses ({proposed_total} put to the pixels, budget {prop_budget} a round)",
        claimed.len() - n_anchors,
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
    //
    // A turned verdict is not one of them. Mirrored, a subject with two
    // sides alike (a butterfly, a face) matches another of its kind as well
    // as it matches itself, and below an anchor's bar that is what such a
    // match mostly is: let in, the turned verdicts added 437 true pairs on
    // IMGS-ALL, IMGS3 and IMGS5 (recall +0.02 to +0.03 points, and 16 false
    // ones inside IMGS5's merges) and on the found corpus 12 pairs, every
    // one two butterflies of a species.
    let anchored: std::collections::HashSet<(usize, usize)> = claimed.iter().map(Edge::ends).collect();
    let corroborated = direct.iter().filter(|e| {
        e.v.variant == Variant::SAME && !anchored.contains(&e.ends()) && policy.standing(&e.v) >= Standing::Corroborating && clusters.same(e.a(), e.b())
    });
    let how = |k: usize| if k < n_anchors { How::Direct } else { How::Propagated };
    let n_claimed = claimed.len();
    let claims: Vec<Claim> = claimed
        .into_iter()
        .enumerate()
        .map(|(k, e)| Claim { e, how: how(k) })
        .chain(corroborated.map(|&e| Claim { e, how: How::Corroborated }))
        .collect();
    stage!(cx, "corroborated: {} more pairs inside existing clusters", claims.len() - n_claimed);
    if let Some(dump) = dump {
        dump.propagated = all_propagated;
    }
    claims
}

/// Every verdict the matching made, for `--dump`: those of the files' own
/// questions are the run's own list, and these the ones nothing after their
/// stage reads.
#[derive(Default)]
struct Dump {
    /// The last verdict on each composed hypothesis propagation considered.
    propagated: Vec<Edge>,
}

/// A verdict as `--dump` writes it: the edge, which question made it, and
/// the run's files, which its row names.
struct Dumped<'a> {
    e: &'a Edge,
    kind: &'static str,
    files: &'a [PathBuf],
    items: &'a [Item],
}

/// `--dump`'s columns: the two files, then the figures `replay.py` reads,
/// then the transform from `a`'s working image to `b`'s and the two working
/// images' sizes, which the suggestion's frame tests read. Each path is its
/// own bytes, quoted the way CSV quotes: written with `{:?}`, a name holding
/// a quote or a comma split into two columns, and a tab or a non-UTF-8 byte
/// came out as an escape that names no file.
fn dump_columns<'a>() -> table::Table<Dumped<'a>> {
    use table::{Cell, Column, Json};
    fn col<'a>(name: &'static str, cell: for<'r> fn(&'r Dumped<'a>) -> Cell<'r>) -> Column<Dumped<'a>> {
        Column { name, csv: true, json: Json::Never, cell }
    }
    fn m<'r, const K: usize>(r: &'r Dumped) -> Cell<'r> {
        Cell::Fixed(r.e.v.m[K], 6)
    }
    table::Table(vec![
        col("a", |r| Cell::Path(&r.files[r.e.ends().0])),
        col("b", |r| Cell::Path(&r.files[r.e.ends().1])),
        col("kind", |r| Cell::Text(r.kind.into())),
        col("n_match", |r| Cell::Count(r.e.v.n_match.into())),
        col("n_in", |r| Cell::Count(r.e.v.n_in.into())),
        col("ov_a", |r| Cell::Fixed(r.e.v.ov_a, 4)),
        col("ov_b", |r| Cell::Fixed(r.e.v.ov_b, 4)),
        col("scale", |r| Cell::Fixed(r.e.v.scale, 5)),
        col("rot", |r| Cell::Fixed(r.e.v.rot_deg, 1)),
        col("blk", |r| Cell::Fixed(r.e.v.blk, 4)),
        col("blk_n", |r| Cell::Count(r.e.v.blk_n.into())),
        col("ncc", |r| Cell::Fixed(r.e.v.ncc, 4)),
        col("centred", |r| Cell::Count(r.e.v.centred.into())),
        col("inverted", |r| Cell::Count(r.e.inverted().into())),
        col("blk_min", |r| Cell::Fixed(r.e.v.blk_min, 4)),
        col("m0", m::<0>),
        col("m1", m::<1>),
        col("m2", m::<2>),
        col("m3", m::<3>),
        col("m4", m::<4>),
        col("m5", m::<5>),
        col("aw", |r| Cell::Count(r.items[r.e.ends().0].feats.w.into())),
        col("ah", |r| Cell::Count(r.items[r.e.ends().0].feats.h.into())),
        col("bw", |r| Cell::Count(r.items[r.e.ends().1].feats.w.into())),
        col("bh", |r| Cell::Count(r.items[r.e.ends().1].feats.h.into())),
    ])
}

/// Write `--dump`: one row per verdict (`dump_columns`).
fn write_dump(cx: &Ctx, to: &To, files: &[PathBuf], items: &[Item], direct: &[Edge], dump: Dump) -> Result<()> {
    let write = |w: &mut dyn Write| -> std::io::Result<()> {
        let columns = dump_columns();
        columns.csv_header(w, b',')?;
        let kind = |e: &Edge| if e.v.variant != Variant::SAME { "variant" } else { "direct" };
        let rows = direct.iter().map(|e| (e, kind(e))).chain(dump.propagated.iter().map(|e| (e, "propagated")));
        rows.map(|(e, kind)| Dumped { e, kind, files, items }).try_for_each(|r| columns.csv(w, &r, b','))
    };
    to.write("the dump", write)?;
    say!(cx, "dumped verdicts -> {}", to.name());
    Ok(())
}

/// From the run's claims to what its report states: every pair, stated once
/// for every file either end answers for, and the content rule's
/// suggestion, read off the same pairs.
fn assemble(cx: &Ctx, scope: &Scope, gui: Option<&Path>, files: &[PathBuf], stats: &[walk::Stat], analysed: Analysed, claims: Vec<Claim>) -> Found {
    let args = cx.args;
    cx.progress.begin(Stage::Assemble);
    let Analysed { items, same, store, not_asked, n_kps, .. } = analysed;
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
    let claims = same.expand(claims);
    // Which pairs are stated, and from what, each once in the order first
    // met (`stated::unique`): the order every tie in the sort below keeps.
    // What each says is worked out afterwards, on every thread
    // (`Stated::pair`).
    //
    // The files that are one file come first (`Same::pairs`). Two of the
    // same bytes that are not pictures at all — a pair of empty files, two
    // copies of a README — are identical, and are not a duplicate *image*,
    // which is the only claim this tool makes; only a wildcard walk can bring
    // such files in. A byte-identical pair of broken `.jpg`s is still stated:
    // those did claim to be images. Two alike only in grey are the match they
    // are, made here since no pass verified one.
    let one_file = same.pairs(|i| !not_asked[i]);
    let grey: Vec<Claim> = one_file
        .iter()
        .filter(|p| p.2 == same::Kind::SameGrey)
        .map(|&(a, b, _)| Claim { e: Edge::new(a, b, Verdict::perfect(n_kps[a])), how: How::Direct })
        .collect();
    let mut grey = grey.iter();
    let ones = one_file.iter().map(|&(a, b, kind)| {
        let s = match kind {
            same::Kind::Identical => Stated::Identical,
            same::Kind::SamePixels => Stated::SamePixels,
            same::Kind::SameGrey => Stated::Claim(grey.next().expect("one claim per grey pair")),
        };
        (a, b, s)
    });
    let stated = stated::unique(ones.chain(claims.iter().map(|c| (c.e.a(), c.e.b(), Stated::Claim(c)))))
        .into_iter()
        .filter(|&(a, b, _)| scope.speaks_of(a, b))
        .collect::<Vec<_>>();
    drop(one_file);
    // What to do with each file, read off the same pairs. The other rules
    // read the groups, and are applied once those exist. Against a library,
    // its files are kept whatever the rest say, and each deletion names the
    // file it rests on. The kept scan's suggestions are the content rule's
    // whatever this run shows (`last`), so a scan kept is a scan that works
    // them out.
    let (actions, keeper) = if args.suggest != suggest::Mode::Content && !keeps_last(args, gui) {
        (Vec::new(), Vec::new())
    } else {
        timed!(44, suggest_content(args, scope, files, &items, stats, &same, &stated, store.as_ref()))
    };
    // Every colour the suggestion measured is in the cache.
    drop(store);
    // What each pair says, which is all that is read of it from here on.
    let mut pairs: Vec<Pair> = stated.par_iter().map(|&(a, b, s)| s.pair(a, b, &items)).collect();
    drop(stated);
    drop(claims);
    // The same pairs as the report reads them: the groups `group::find`
    // makes of them, or as the verdicts against a library read them
    // (`against.rs`).
    let (grouping, asked) = if scope.against {
        let scanned: Vec<usize> = (0..files.len()).filter(|&i| !scope.library[i] && !not_asked[i]).collect();
        (None, against::asked(&scanned, &scope.library, &pairs, &actions, &keeper))
    } else {
        let graph: Vec<(usize, usize)> = pairs.iter().map(Pair::ends).collect();
        (Some(timed!(28, group::find(files.len(), &graph))), Vec::new())
    };
    // By name, as the JSON lists them; stable, so names that read alike keep
    // the order they were met in.
    let names: Vec<String> = files.par_iter().map(|f| f.display().to_string()).collect();
    pairs.par_sort_by(|x, y| names[x.a as usize].cmp(&names[y.a as usize]).then_with(|| names[x.b as usize].cmp(&names[y.b as usize])));

    let failures: Vec<serde_json::Value> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| {
            let e = it.read.error().filter(|_| !not_asked[i] && it.read != Read::Refused(Refused::CameraRaw))?;
            let mut f = serde_json::json!({"path": files[i].display().to_string(), "error": e});
            if let Some(b) = table::raw_bytes(&files[i]) {
                f["path_bytes"] = b.into();
            }
            Some(f)
        })
        .collect();
    Found {
        pairs,
        grouping,
        asked,
        actions,
        keeper,
        failures,
        dims: items.iter().map(|it| it.dims).collect(),
        damaged: items.iter().map(|it| it.read.damaged().is_some()).collect(),
        sizes: stats.iter().map(|s| s.len).collect(),
        not_asked,
        ok: items.iter().map(|it| it.read.ok()).collect(),
    }
}

/// Whether the scan is kept for `--last` and the window (`last`): not one
/// the window runs (it keeps its own), nor one with the cache off or
/// `--no-last`.
fn keeps_last(args: &Args, gui: Option<&Path>) -> bool {
    gui.is_none() && !args.no_cache && !args.no_last
}

/// The content rule's suggestion for every file, read off the stated pairs,
/// and the file each deletion rests on. `store` is the cache, still open
/// after the analysis: the suggestion keeps there the colours it measures.
#[allow(clippy::too_many_arguments)]
fn suggest_content(
    args: &Args,
    scope: &Scope,
    files: &[PathBuf],
    items: &[Item],
    stats: &[walk::Stat],
    same: &Same,
    stated: &[(usize, usize, Stated)],
    store: Option<&cache::Store>,
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
        if let (Some(c), Some(store)) = (c, store) {
            // In every record its bytes are held under.
            for &k in same.identical_to(i) {
                store.keep_colour(k, c);
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
    pairs: Vec<Pair>,
    /// The groups `group::find` makes of the stated pairs; `None` against a
    /// library, and for a walk that found nothing.
    grouping: Option<Vec<group::Group>>,
    /// Against a library, each scanned file's pairs as its verdict reads
    /// them (`against::asked`); empty otherwise.
    asked: Vec<Asked>,
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
///
/// What it found is an `Outcome`: groups, or against a library each scanned
/// file. It is kept for `--last` before `-o` is written, since a report that
/// could not be written is one case `--last` is for, and the window's copy
/// is written first: a `-o` that failed — a disk full, a folder removed
/// mid-scan — used to end the run before it, so a finished scan showed as a
/// failed one with nothing to look at.
fn finish(cx: &Ctx, scope: &Scope, gui: Option<&Path>, files: &[PathBuf], mut found: Found, problems: &Problems, started: u64) -> Result<()> {
    let args = cx.args;
    let n = files.len();
    // A file a wildcard walk reached and that turned out not to be a picture
    // is a skip, like one the extension list turned away, and is not counted
    // by either. Nor is the library, which the run was not asked about.
    let counted = |i: usize| !found.not_asked[i];
    let scanned: Vec<usize> = (0..n).filter(|&i| counted(i) && !scope.library[i]).collect();
    let n_library = (0..n).filter(|&i| counted(i) && scope.library[i]).count();
    let n_ok = scanned.iter().filter(|&&i| found.ok[i]).count();
    let grouping = (!scope.against).then(|| {
        let grouping = found.grouping.take().unwrap_or_default();
        let membership = group::membership(&grouping);
        stage!(
            cx,
            "groups: {} representatives over {} files, {} of them in more than one group",
            grouping.len(),
            membership.len(),
            membership.values().filter(|gs| gs.len() > 1).count()
        );
        grouping
    });
    // Cleared before anything is written to stdout, which shares the terminal.
    cx.progress.finish();
    let runtime = cx.t.elapsed().as_secs_f64();
    let head = Head {
        config: run_config(args),
        files_enumerated: scanned.len(),
        files_analysed: n_ok,
        failures: Vec::new(),
        runtime_seconds: (runtime * 1000.0).round() / 1000.0,
    };
    let (scan, pairs) = timed!(36, outcome(args, files, &scope.library, grouping, n_library, head, found));
    let answers = against::answers(&scan);
    let summary = if scan.against.is_some() {
        format!(
            "{}, of {} scanned against {} in {runtime:.1}s",
            against::tally(&answers),
            count(scanned.len(), "image", "images"),
            count(n_library, "library image", "library images"),
        )
    } else {
        let actions = scan.actions(args.suggest);
        stage!(
            cx,
            "suggested: {} to keep, {} to delete, {} to review",
            actions.iter().filter(|a| **a == Some(suggest::Action::Keep)).count(),
            actions.iter().filter(|a| **a == Some(suggest::Action::Delete)).count(),
            actions.iter().filter(|a| **a == Some(suggest::Action::Review)).count()
        );
        format!("{}, {} over {} in {runtime:.1}s", count(scan.groups.len(), "group", "groups"), count(pairs.len(), "pair", "pairs"), count(n_ok, "image", "images"))
    };
    // Not a walk that found no picture at all, such as a wildcard walk of a
    // folder of documents, nor a scan of nothing, a mistyped folder say:
    // either would replace a scan worth writing out again. The window keeps
    // by the same rule.
    if keeps_last(args, gui) && scan.head.files_enumerated > 0 {
        let roots = if args.from_file.is_some() || args.roots.iter().any(|r| r == Path::new("-")) {
            // Read from a list, they are files rather than the folders the
            // window's tree starts from, and it finds those itself.
            Vec::new()
        } else {
            args.roots.iter().map(|r| std::path::absolute(r).unwrap_or_else(|_| r.clone())).collect()
        };
        let mut lines = vec![summary.clone()];
        lines.extend(problems.summary());
        let notes = last::Notes::now(problems.any(), started, runtime as u64, lines.join("\n"), roots);
        let save = |path: &Path| scan.save(path).map_err(std::io::Error::other);
        if let Err(e) = timed!(36, last::save_with(&last::dir(args.cache.as_deref()), save, &notes)) {
            say!(cx, "This scan could not be kept for --last: {e}");
        }
    }
    if let Some(path) = gui {
        timed!(36, scan.save(path))?;
    }
    // Where `outputs::plan` says.
    let mut went = None;
    if let Some(r) = &cx.outputs.report {
        timed!(36, report::write_with(r, &scan, args.suggest, Some(report::Pairs { files, list: &pairs })))?;
        went = r.to.file();
    }
    if cx.outputs.list {
        against::list(&scan, &answers, &args.list, args.null)?;
    }
    let summary = if files.is_empty() { "No images found.".to_string() } else { summary };
    match went {
        Some(path) => say!(cx, "{summary} -> {}", path.display()),
        None => say!(cx, "{summary}"),
    }
    prof::report();
    Ok(())
}

/// Each file's number in an `Outcome`, in the order they are first named,
/// and the file each number is.
#[derive(Default)]
struct Numbers {
    number: HashMap<usize, usize>,
    run_of: Vec<usize>,
}

impl Numbers {
    fn name(&mut self, i: usize) -> usize {
        *self.number.entry(i).or_insert_with(|| {
            self.run_of.push(i);
            self.run_of.len() - 1
        })
    }
}

/// What the run found, as everything after it reads it (`Outcome`), and its
/// stated pairs, which only the JSON lists: the groups `grouping` makes, or
/// with none, against a library, each scanned file.
fn outcome(args: &Args, files: &[PathBuf], library: &[bool], grouping: Option<Vec<group::Group>>, library_files: usize, mut head: Head, found: Found) -> (Outcome, Vec<Pair>) {
    let Found { pairs, asked, actions, keeper, failures, dims, damaged, sizes, .. } = found;
    head.failures = failures;
    let mut numbers = Numbers::default();
    let groups = grouping.map(|g| numbered_groups(files, g, &pairs, &mut numbers));
    let against = groups.is_none().then(|| {
        let mut name = |m: Member| Member { file: numbers.name(m.file), ..m };
        // The scanned files first, in the run's order, then what each names.
        let asked: Vec<Asked> = asked.into_iter().map(|a| Asked { file: name(Member { file: a.file, with: None, at: None }).file, ..a }).collect();
        let asked = asked
            .into_iter()
            .map(|a| Asked {
                holder: a.holder.map(&mut name),
                library: a.library.into_iter().map(|m| outcome::Match { file: name(Member { file: m.file, with: None, at: None }).file, ..m }).collect(),
                ..a
            })
            .collect();
        Against { library_files, asked }
    });
    // Each deletion names the file that stays and holds it, at the end of
    // its chain of holders, not the nearest one, which may be deleted too.
    // A kept file is in a group with what it holds, so this numbers nothing
    // new there; it is here so that a kept copy always has a number.
    let keeper = suggest::kept_at_end(&actions, &keeper);
    let held: Vec<usize> = numbers.run_of.iter().filter_map(|&i| keeper.get(i).copied().flatten()).collect();
    for k in held {
        numbers.name(k);
    }
    let Numbers { number, run_of } = numbers;
    // The size as shown is the analysis's own (`dims`): the picture decoded,
    // with its orientation applied. The header is still asked of a file the
    // analysis has no size for, which is a file that would not decode: a
    // byte-identical pair of broken pictures is reported, and its header may
    // still say how large it claims to be. The length is the walk's own.
    let scan_files: Vec<outcome::File> = run_of
        .par_iter()
        .map(|&i| {
            let p = &files[i];
            let dims = match dims.get(i) {
                Some(&(w, h)) if w > 0 && h > 0 => Some((w, h)),
                _ => decode::probe(p).map(|pr| (pr.w, pr.h)),
            };
            outcome::File { path: p.clone(), dims, bytes: sizes.get(i).copied(), damaged: damaged.get(i).copied().unwrap_or(false), library: library[i] }
        })
        .collect();
    let (content, kept) = if actions.is_empty() {
        (Vec::new(), Vec::new())
    } else {
        (run_of.iter().map(|&i| actions[i]).collect(), run_of.iter().map(|&i| keeper[i].map(|k| number[&k])).collect())
    };
    let scan = match (groups, against) {
        (Some(groups), _) => Outcome::new(head, args.min_pixel_correlation, scan_files, groups, content, kept),
        (None, against) => Outcome::of_against(head, args.min_pixel_correlation, scan_files, against.unwrap_or_default(), content, kept),
    };
    (scan, pairs)
}

/// The groups as an `Outcome` holds them, each member with its pair with
/// its representative, and the files numbered as the groups first name them.
///
/// The groups are sorted largest first, ties by comparing member paths in
/// order. Paths are unique and no two representatives can hold the same
/// members — the second would have had nothing left to account for — so the
/// order is total and the output is reproducible.
fn numbered_groups(files: &[PathBuf], grouping: Vec<group::Group>, pairs: &[Pair], numbers: &mut Numbers) -> Vec<Group> {
    let names: Vec<Vec<String>> = grouping.iter().map(|g| g.members.iter().map(|&i| files[i].display().to_string()).collect()).collect();
    let mut order: Vec<usize> = (0..grouping.len()).collect();
    order.sort_by(|&a, &b| names[b].len().cmp(&names[a].len()).then_with(|| names[a].cmp(&names[b])));
    drop(names);
    let mut want: HashMap<(usize, usize), Option<&Pair>> = HashMap::new();
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::with_capacity(order.len());
    for &k in &order {
        let g = &grouping[k];
        let rep = g.representative;
        let members: Vec<usize> = std::iter::once(rep).chain(g.members.iter().copied().filter(|&i| i != rep)).collect();
        for &i in &members[1..] {
            want.insert((i.min(rep), i.max(rep)), None);
        }
        for &i in &members {
            numbers.name(i);
        }
        groups.push((rep, members));
    }
    for p in pairs {
        if let Some(e) = want.get_mut(&p.key()) {
            *e = Some(p);
        }
    }
    groups
        .into_iter()
        .map(|(rep, members)| Group {
            members: members
                .into_iter()
                .map(|i| {
                    let p = (i != rep).then(|| want[&(i.min(rep), i.max(rep))].expect("a member's pair with its representative is stated"));
                    Member { file: numbers.number[&i], with: p.map(|p| p.ev), at: p.and_then(|p| p.place_from(rep)) }
                })
                .collect(),
        })
        .collect()
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
    use verify::Thumb;

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
                ..Default::default()
            })
            .collect();
        let pool: Vec<&[u8; DESC_LEN]> = items.iter().flat_map(|it| (0..it.feats.len()).map(|i| it.feats.d(i).try_into().unwrap())).collect();
        let vocab = Vocabulary::build(pool, &index::VocabParams::for_corpus(items.iter().map(|it| it.feats.len()).sum()));
        let lists: Vec<WordList> = items.iter().map(|it| quantise(&vocab, &it.feats)).collect();
        let policy = verify::Policy::new(MIN_ALIGNED_POINTS, 0.5, 0.6);
        let pass = Pass { items: &items, matched: &[true, true], policy: &policy, gate: (3, 0.2), dumping: false };
        let ask = |i: usize, j: usize| pass.verify(&lists, i, &items[i].feats, &lists[i], j, Variant::SAME, &mut Work::default());
        let low = ask(0, 1).expect("a picture and its downscale match");
        let high = ask(1, 0).expect("asked from the other end too");
        assert!(policy.standing(&low.v) >= Standing::Anchor && policy.standing(&high.v) >= Standing::Anchor, "an anchor from either end");
        assert_eq!((low.a(), low.b()), (0, 1));
        assert_eq!((high.a(), high.b()), (0, 1));
        assert!((low.v.scale - high.v.scale).abs() < 0.02 * low.v.scale, "scale {} from below, {} from above", low.v.scale, high.v.scale);
    }
}

