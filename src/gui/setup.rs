//! The first page: what to scan, how, and the scan's progress.
//!
//! Two tabs. General holds what most scans change — the folders, the options
//! that decide what counts as a duplicate, and where a report goes. Advanced
//! holds the rest of img-fp's command line: threads, which files a walk takes,
//! the cache, and a log file. The progress bar, the log and Scan/Cancel sit
//! below both.

use crate::labels as l;
use crate::results::Results;
use crate::scan::{self, Event, Scan};
use crate::settings::{ReportFormat, Settings};
use crate::App;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How long a cancelled scan has to answer before it is killed. img-fp answers
/// Ctrl-C in well under a tenth of a second; this is for a disk that has
/// stopped answering it.
const KILL_AFTER: Duration = Duration::from_secs(3);

/// A list of folders with Add and Remove, for the folders to scan and the
/// ones to leave out.
struct PathList {
    root: gtk::Box,
    list: gtk::ListBox,
    paths: RefCell<Vec<PathBuf>>,
    add: gtk::Button,
}

impl PathList {
    fn new(window: &gtk::ApplicationWindow, add_label: &str, remove_label: &str, empty: &str, height: i32) -> Rc<PathList> {
        let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::Browse).build();
        list.add_css_class("boxed-list");
        list.set_placeholder(Some(&gtk::Label::builder().label(empty).css_classes(["dim-label"]).margin_top(12).margin_bottom(12).build()));
        let scroll = gtk::ScrolledWindow::builder()
            .child(&list)
            .min_content_height(height)
            .hexpand(true)
            .has_frame(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let add = gtk::Button::with_mnemonic(add_label);
        let remove = gtk::Button::with_mnemonic(remove_label);
        let buttons = gtk::Box::new(gtk::Orientation::Vertical, 6);
        buttons.append(&add);
        buttons.append(&remove);
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        root.append(&scroll);
        root.append(&buttons);
        let me = Rc::new(PathList { root, list, paths: RefCell::new(Vec::new()), add: add.clone() });

        let window = window.downgrade();
        add.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                let (Some(me), Some(window)) = (me.upgrade(), window.upgrade()) else { return };
                glib::spawn_future_local(async move {
                    let dialog = gtk::FileDialog::builder().title("Choose folders").modal(true).build();
                    if let Ok(files) = dialog.select_multiple_folders_future(Some(&window)).await {
                        let paths = files.iter::<gio::File>().filter_map(|f| f.ok()?.path());
                        me.add_paths(paths);
                    }
                });
            }
        });
        remove.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.remove_selected();
                }
            }
        });
        // Delete removes the selected folder, as it would in a file manager.
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed({
            let me = Rc::downgrade(&me);
            move |_, key, _, _| {
                if matches!(key, gdk::Key::Delete | gdk::Key::KP_Delete | gdk::Key::BackSpace) {
                    if let Some(me) = me.upgrade() {
                        me.remove_selected();
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        });
        me.list.add_controller(keys);
        // Folders dropped from a file manager.
        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        drop.connect_drop({
            let me = Rc::downgrade(&me);
            move |_, value, _, _| {
                let (Some(me), Ok(files)) = (me.upgrade(), value.get::<gdk::FileList>()) else { return false };
                me.add_paths(files.files().iter().filter_map(|f| f.path()));
                true
            }
        });
        me.root.add_controller(drop);
        me
    }

    fn add_paths(&self, paths: impl Iterator<Item = PathBuf>) {
        let mut v = self.paths.borrow().clone();
        for p in paths {
            if !v.contains(&p) {
                v.push(p);
            }
        }
        self.set(v);
    }

    fn set(&self, v: Vec<PathBuf>) {
        while let Some(row) = self.list.row_at_index(0) {
            self.list.remove(&row);
        }
        for p in &v {
            let label = gtk::Label::builder()
                .label(p.display().to_string())
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::Middle)
                .tooltip_text(p.display().to_string())
                .margin_start(8)
                .margin_end(8)
                .margin_top(4)
                .margin_bottom(4)
                .build();
            self.list.append(&label);
        }
        *self.paths.borrow_mut() = v;
    }

    fn remove_selected(&self) {
        let Some(row) = self.list.selected_row() else { return };
        let i = row.index() as usize;
        let mut v = self.paths.borrow().clone();
        if i < v.len() {
            v.remove(i);
        }
        self.set(v);
        // Keep the keyboard where it was: on the next row, or on Add.
        let n = self.paths.borrow().len();
        if n == 0 {
            self.add.grab_focus();
        } else if let Some(r) = self.list.row_at_index(i.min(n - 1) as i32) {
            self.list.select_row(Some(&r));
            r.grab_focus();
        }
    }

    fn paths(&self) -> Vec<PathBuf> {
        self.paths.borrow().clone()
    }
}

/// A label on the left of a grid, whose mnemonic reaches `target`.
fn label_for(text: &str, target: &impl IsA<gtk::Widget>) -> gtk::Label {
    let label = gtk::Label::with_mnemonic(text);
    label.set_mnemonic_widget(Some(target));
    label.set_xalign(0.0);
    label
}

fn hint(text: &str) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).wrap(true).css_classes(["dim-label", "caption"]).build()
}

fn spin(min: f64, max: f64, step: f64, digits: u32) -> gtk::SpinButton {
    let s = gtk::SpinButton::with_range(min, max, step);
    s.set_digits(digits);
    s.set_numeric(true);
    // Enter starts the scan, as it does in an entry. (GTK 4.14 has a
    // property for this; the window asks for 4.10.)
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(|c, key, _, mods| {
        if matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) && !mods.contains(gdk::ModifierType::CONTROL_MASK) {
            if let Some(s) = c.widget().and_downcast::<gtk::SpinButton>() {
                s.update();
                let _ = s.activate_action("default.activate", None);
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    s.add_controller(keys);
    s.set_valign(gtk::Align::Center);
    s
}

fn entry() -> gtk::Entry {
    gtk::Entry::builder().hexpand(true).activates_default(true).build()
}

fn grid() -> gtk::Grid {
    gtk::Grid::builder()
        .row_spacing(8)
        .column_spacing(12)
        .margin_top(16)
        .margin_bottom(16)
        .margin_start(16)
        .margin_end(16)
        .build()
}

fn section(text: &str) -> gtk::Label {
    let l = gtk::Label::builder().label(text).xalign(0.0).margin_top(8).build();
    l.add_css_class("heading");
    l
}

pub struct Setup {
    pub root: gtk::Box,
    app: Rc<App>,
    results: Rc<Results>,

    folders: Rc<PathList>,
    recursive: gtk::CheckButton,
    work_size: gtk::SpinButton,
    candidates: gtk::SpinButton,
    min_points: gtk::SpinButton,
    min_overlap: gtk::SpinButton,
    min_correlation: gtk::SpinButton,
    report: gtk::CheckButton,
    report_path: gtk::Entry,
    report_format: gtk::DropDown,

    threads: gtk::SpinButton,
    extensions: gtk::Entry,
    exclude: Rc<PathList>,
    symlinks: gtk::CheckButton,
    use_cache: gtk::CheckButton,
    cache_path: gtk::Entry,
    clear_cache: gtk::CheckButton,
    prune_cache: gtk::CheckButton,
    log: gtk::CheckButton,
    log_path: gtk::Entry,

    notebook: gtk::Notebook,
    scan_button: gtk::Button,
    cancel_button: gtk::Button,
    back_button: gtk::Button,
    defaults_button: gtk::Button,
    progress: gtk::ProgressBar,
    status: gtk::Label,
    elapsed: gtk::Label,

    scan: RefCell<Option<Scan>>,
    started: Cell<Option<Instant>>,
    cancelling: Cell<bool>,
}

impl Setup {
    pub fn new(app: Rc<App>, settings: Settings, results: Rc<Results>) -> Rc<Setup> {
        let window = &app.window;

        // ---- General
        let folders = PathList::new(window, l::ADD_FOLDER, l::REMOVE_FOLDER, "No folders yet. Add one, or drop it here.", 130);
        let recursive = gtk::CheckButton::with_mnemonic(l::RECURSIVE);
        let work_size = spin(0.0, 4096.0, 32.0, 0);
        let candidates = spin(1.0, 2000.0, 10.0, 0);
        let min_points = spin(0.0, 200.0, 1.0, 0);
        let min_overlap = spin(0.0, 1.0, 0.05, 2);
        let min_correlation = spin(0.0, 1.0, 0.05, 2);
        let report = gtk::CheckButton::with_mnemonic(l::REPORT);
        let report_path = entry();
        report_path.set_placeholder_text(Some("results.txt, results.csv or results.json"));
        let report_choose = gtk::Button::with_mnemonic(l::REPORT_CHOOSE);
        let report_format = gtk::DropDown::from_strings(&ReportFormat::ALL.map(|f| f.label()));

        let g = grid();
        let folders_label = label_for(l::FOLDERS, &folders.list);
        folders_label.set_valign(gtk::Align::Start);
        g.attach(&folders_label, 0, 0, 1, 1);
        g.attach(&folders.root, 1, 0, 2, 1);
        g.attach(&recursive, 1, 1, 2, 1);

        g.attach(&section("What counts as a duplicate"), 0, 2, 3, 1);
        let rows: [(&str, &gtk::SpinButton, &str); 5] = [
            (l::WORK_SIZE, &work_size, "Pixels on the long side each image is analysed at. Higher finds more, especially small pictures inside larger ones such as slides and collages, but is slower. 640 is a good choice when that matters. 0 does not shrink images at all, which is much slower on large photos."),
            (l::MIN_POINTS, &min_points, "Matching points two images must share. Higher is stricter."),
            (l::MIN_OVERLAP, &min_overlap, "How much of one image must lie inside the other, from 0 to 1. Higher is stricter."),
            (l::MIN_CORRELATION, &min_correlation, "How closely the pixels of that shared area must agree, from 0 to 1. Higher is stricter."),
            (l::CANDIDATES, &candidates, "Possible matches checked for each image."),
        ];
        for (i, (text, w, tip)) in rows.into_iter().enumerate() {
            let r = 3 + i as i32;
            let label = label_for(text, w);
            label.set_tooltip_text(Some(tip));
            w.set_tooltip_text(Some(tip));
            g.attach(&label, 0, r, 1, 1);
            g.attach(w, 1, r, 1, 1);
            g.attach(&hint(tip), 2, r, 1, 1);
        }
        g.attach(&section("Report"), 0, 8, 3, 1);
        g.attach(&report, 0, 9, 1, 1);
        let report_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        report_box.append(&report_path);
        report_box.append(&report_choose);
        g.attach(&report_box, 1, 9, 2, 1);
        g.attach(&label_for(l::REPORT_FORMAT, &report_format), 0, 10, 1, 1);
        report_format.set_halign(gtk::Align::Start);
        g.attach(&report_format, 1, 10, 2, 1);
        let general = gtk::ScrolledWindow::builder().child(&g).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();

        // ---- Advanced
        let threads = spin(0.0, 1024.0, 1.0, 0);
        let extensions = entry();
        let exclude = PathList::new(window, l::ADD_EXCLUDE, l::REMOVE_EXCLUDE, "Nothing left out.", 90);
        let symlinks = gtk::CheckButton::with_mnemonic(l::SYMLINKS);
        let use_cache = gtk::CheckButton::with_mnemonic(l::USE_CACHE);
        let cache_path = entry();
        cache_path.set_placeholder_text(Some("the default: ~/.cache/img-fp/analysis.bin"));
        let cache_choose = gtk::Button::with_mnemonic(l::CACHE_CHOOSE);
        let clear_cache = gtk::CheckButton::with_mnemonic(l::CLEAR_CACHE);
        let prune_cache = gtk::CheckButton::with_mnemonic(l::PRUNE_CACHE);
        let log = gtk::CheckButton::with_mnemonic(l::LOG_FILE);
        let log_path = entry();
        let log_choose = gtk::Button::with_mnemonic(l::LOG_CHOOSE);

        let a = grid();
        a.attach(&label_for(l::THREADS, &threads), 0, 0, 1, 1);
        let threads_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        threads_box.append(&threads);
        threads_box.append(&hint("Worker threads. 0 uses every core."));
        a.attach(&threads_box, 1, 0, 2, 1);
        a.attach(&section("Which files"), 0, 1, 3, 1);
        a.attach(&label_for(l::EXTENSIONS, &extensions), 0, 2, 1, 1);
        a.attach(&extensions, 1, 2, 2, 1);
        a.attach(
            &hint("Extensions a folder is searched for, comma-separated. * takes every file, and !gif leaves GIFs out of everything else."),
            1, 3, 2, 1,
        );
        let exclude_label = label_for(l::EXCLUDE, &exclude.list);
        exclude_label.set_valign(gtk::Align::Start);
        a.attach(&exclude_label, 0, 4, 1, 1);
        a.attach(&exclude.root, 1, 4, 2, 1);
        a.attach(&symlinks, 1, 5, 2, 1);
        a.attach(&section("Cache"), 0, 6, 3, 1);
        a.attach(&use_cache, 0, 7, 3, 1);
        a.attach(&hint("Keeps each image's analysis, so a folder scanned again only analyses what changed."), 0, 8, 3, 1);
        a.attach(&label_for(l::CACHE_FILE, &cache_path), 0, 9, 1, 1);
        let cache_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        cache_box.append(&cache_path);
        cache_box.append(&cache_choose);
        a.attach(&cache_box, 1, 9, 2, 1);
        a.attach(&clear_cache, 1, 10, 2, 1);
        a.attach(&prune_cache, 1, 11, 2, 1);
        a.attach(&hint("Both apply to the next scan only."), 1, 12, 2, 1);
        a.attach(&section("Log"), 0, 13, 3, 1);
        a.attach(&log, 0, 14, 1, 1);
        let log_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        log_box.append(&log_path);
        log_box.append(&log_choose);
        a.attach(&log_box, 1, 14, 2, 1);
        a.attach(&hint("Every skipped file, problem and stage timing, in full."), 1, 15, 2, 1);
        let advanced = gtk::ScrolledWindow::builder().child(&a).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();

        let notebook = gtk::Notebook::new();
        notebook.append_page(&general, Some(&gtk::Label::with_mnemonic(l::TAB_GENERAL)));
        notebook.append_page(&advanced, Some(&gtk::Label::with_mnemonic(l::TAB_ADVANCED)));
        notebook.set_vexpand(true);

        // ---- The bar at the bottom
        let progress = gtk::ProgressBar::builder().hexpand(true).show_text(true).text("").valign(gtk::Align::Center).build();
        let status = gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).hexpand(true).build();
        let elapsed = gtk::Label::builder().css_classes(["dim-label", "numeric"]).build();
        let scan_button = gtk::Button::with_mnemonic(l::SCAN);
        scan_button.add_css_class("suggested-action");
        scan_button.set_tooltip_text(Some("Start the scan (Ctrl+Enter)"));
        let cancel_button = gtk::Button::with_mnemonic(l::CANCEL);
        cancel_button.add_css_class("destructive-action");
        cancel_button.set_tooltip_text(Some("Stop now (Escape). Images analysed so far are kept in the cache."));
        cancel_button.set_visible(false);
        let back_button = gtk::Button::with_mnemonic(l::BACK_TO_RESULTS);
        back_button.set_visible(false);
        let defaults_button = gtk::Button::with_mnemonic(l::DEFAULTS);
        defaults_button.set_tooltip_text(Some("Every option back to its default. The folders are kept."));
        let log_button = gtk::Button::with_mnemonic(l::SCAN_LOG);

        let bottom = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_start(16)
            .margin_end(16)
            .margin_top(8)
            .margin_bottom(12)
            .build();
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        bar.append(&progress);
        bar.append(&elapsed);
        bottom.append(&bar);
        bottom.append(&status);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.append(&defaults_button);
        buttons.append(&log_button);
        let spacer = gtk::Box::builder().hexpand(true).build();
        buttons.append(&spacer);
        buttons.append(&gtk::Label::builder().label("F1 keyboard shortcuts").css_classes(["dim-label", "caption"]).build());
        buttons.append(&back_button);
        buttons.append(&cancel_button);
        buttons.append(&scan_button);
        bottom.append(&buttons);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&notebook);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&bottom);
        window.set_default_widget(Some(&scan_button));

        let me = Rc::new(Setup {
            root,
            app: app.clone(),
            results,
            folders,
            recursive,
            work_size,
            candidates,
            min_points,
            min_overlap,
            min_correlation,
            report,
            report_path,
            report_format,
            threads,
            extensions,
            exclude,
            symlinks,
            use_cache,
            cache_path,
            clear_cache,
            prune_cache,
            log,
            log_path,
            notebook,
            scan_button,
            cancel_button,
            back_button,
            defaults_button,
            progress,
            status,
            elapsed,
            scan: RefCell::new(None),
            started: Cell::new(None),
            cancelling: Cell::new(false),
        });
        me.show(&settings);
        me.idle(if me.folders.paths().is_empty() { "Add the folders to scan, then press Scan." } else { "Press Scan to start." });

        // What depends on what.
        for (check, deps) in [
            (&me.report, vec![me.report_path.clone().upcast::<gtk::Widget>(), report_choose.clone().upcast(), me.report_format.clone().upcast()]),
            (&me.use_cache, vec![me.cache_path.clone().upcast(), cache_choose.clone().upcast(), me.prune_cache.clone().upcast()]),
            (&me.log, vec![me.log_path.clone().upcast(), log_choose.clone().upcast()]),
        ] {
            let sync = move |c: &gtk::CheckButton| {
                for d in &deps {
                    d.set_sensitive(c.is_active());
                }
            };
            sync(check);
            check.connect_toggled(sync);
        }
        report_choose.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.choose_file(me.report_path.clone(), "Save the report as", "results.txt");
                }
            }
        });
        cache_choose.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.choose_file(me.cache_path.clone(), "Cache file", "analysis.bin");
                }
            }
        });
        log_choose.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.choose_file(me.log_path.clone(), "Write the log to", "img-fp.log");
                }
            }
        });
        me.scan_button.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.start();
                }
            }
        });
        me.cancel_button.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    me.cancel();
                }
            }
        });
        me.back_button.connect_clicked({
            let app = app.clone();
            let results = me.results.clone();
            move |_| {
                app.show_results();
                results.focus();
            }
        });
        me.defaults_button.connect_clicked({
            let me = Rc::downgrade(&me);
            move |_| {
                if let Some(me) = me.upgrade() {
                    let keep = me.read();
                    me.show(&Settings { folders: keep.folders, exclude: keep.exclude, ..Settings::default() });
                }
            }
        });
        log_button.connect_clicked({
            let app = app.clone();
            move |_| app.show_log()
        });

        // Escape cancels a running scan, and Ctrl+Enter starts one from
        // anywhere on the page.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let me = Rc::downgrade(&me);
            move |_, key, _, mods| {
                let Some(me) = me.upgrade() else { return glib::Propagation::Proceed };
                if me.app.stack.visible_child_name().as_deref() != Some("setup") {
                    return glib::Propagation::Proceed;
                }
                if key == gdk::Key::Escape && me.scanning() {
                    me.cancel();
                    return glib::Propagation::Stop;
                }
                let enter = matches!(key, gdk::Key::Return | gdk::Key::KP_Enter);
                if enter && mods.contains(gdk::ModifierType::CONTROL_MASK) && !me.scanning() {
                    me.start();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        });
        app.window.add_controller(keys);
        me
    }

    /// Put the Scan button (or Cancel) under the keyboard.
    pub fn focus_scan(&self) {
        if self.scanning() {
            self.cancel_button.grab_focus();
        } else if self.folders.paths().is_empty() {
            self.folders.add.grab_focus();
        } else {
            self.scan_button.grab_focus();
        }
    }

    /// Whether "Back to results" is offered.
    pub fn results_available(&self, yes: bool) {
        self.back_button.set_visible(yes && !self.scanning());
    }

    fn scanning(&self) -> bool {
        self.scan.borrow().is_some()
    }

    /// Stop a running scan without waiting for it: the window is closing.
    pub fn stop_scan(&self) {
        if let Some(s) = self.scan.borrow().as_ref() {
            s.interrupt();
        }
    }

    fn show(&self, s: &Settings) {
        self.folders.set(s.folders.clone());
        self.recursive.set_active(s.recursive);
        self.work_size.set_value(s.work_size as f64);
        self.candidates.set_value(s.candidates as f64);
        self.min_points.set_value(s.min_aligned_points as f64);
        self.min_overlap.set_value(s.min_frame_overlap as f64);
        self.min_correlation.set_value(s.min_pixel_correlation as f64);
        self.report.set_active(s.report);
        self.report_path.set_text(&s.report_path);
        self.report_format.set_selected(ReportFormat::ALL.iter().position(|f| *f == s.report_format).unwrap_or(0) as u32);
        self.threads.set_value(s.threads as f64);
        self.extensions.set_text(&s.extensions);
        self.exclude.set(s.exclude.clone());
        self.symlinks.set_active(s.follow_symlinks);
        self.use_cache.set_active(s.use_cache);
        self.cache_path.set_text(&s.cache_path);
        self.clear_cache.set_active(s.clear_cache);
        self.prune_cache.set_active(s.prune_cache);
        self.log.set_active(s.log);
        self.log_path.set_text(&s.log_path);
    }

    fn read(&self) -> Settings {
        // A value typed but not yet committed with Enter or Tab is still the
        // value meant.
        for s in [&self.work_size, &self.candidates, &self.min_points, &self.min_overlap, &self.min_correlation, &self.threads] {
            s.update();
        }
        Settings {
            folders: self.folders.paths(),
            recursive: self.recursive.is_active(),
            work_size: self.work_size.value() as usize,
            candidates: self.candidates.value() as usize,
            min_aligned_points: self.min_points.value() as u32,
            min_frame_overlap: self.min_overlap.value() as f32,
            min_pixel_correlation: self.min_correlation.value() as f32,
            report: self.report.is_active(),
            report_path: self.report_path.text().to_string(),
            report_format: ReportFormat::ALL[(self.report_format.selected() as usize).min(ReportFormat::ALL.len() - 1)],
            threads: self.threads.value() as usize,
            extensions: self.extensions.text().to_string(),
            exclude: self.exclude.paths(),
            follow_symlinks: self.symlinks.is_active(),
            use_cache: self.use_cache.is_active(),
            cache_path: self.cache_path.text().to_string(),
            clear_cache: self.clear_cache.is_active(),
            prune_cache: self.prune_cache.is_active(),
            log: self.log.is_active(),
            log_path: self.log_path.text().to_string(),
        }
    }

    fn choose_file(self: &Rc<Self>, target: gtk::Entry, title: &str, name: &str) {
        let window = self.app.window.clone();
        let dialog = gtk::FileDialog::builder().title(title).modal(true).initial_name(name).build();
        let current = target.text();
        if !current.is_empty() {
            dialog.set_initial_file(Some(&gio::File::for_path(current.as_str())));
        }
        glib::spawn_future_local(async move {
            if let Ok(f) = dialog.save_future(Some(&window)).await {
                if let Some(p) = f.path() {
                    target.set_text(&p.display().to_string());
                }
            }
            target.grab_focus();
        });
    }

    fn alert(&self, message: &str, detail: &str) {
        let d = gtk::AlertDialog::builder().message(message).detail(detail).modal(true).build();
        d.show(Some(&self.app.window));
    }

    /// Nothing running: the options can be changed and Scan pressed.
    fn idle(&self, status: &str) {
        self.scan.replace(None);
        self.cancelling.set(false);
        self.started.set(None);
        self.notebook.set_sensitive(true);
        self.defaults_button.set_sensitive(true);
        self.scan_button.set_visible(true);
        self.cancel_button.set_visible(false);
        self.cancel_button.set_sensitive(true);
        self.back_button.set_visible(self.results.has_results());
        self.status.set_text(status);
        let refocus = self.cancel_button.has_focus() || GtkWindowExt::focus(&self.app.window).is_none();
        if refocus {
            self.scan_button.grab_focus();
        }
    }

    fn start(self: &Rc<Self>) {
        if self.scanning() {
            return;
        }
        let settings = self.read().with_absolute_paths();
        // Shown where they were typed, so the path a file is written to is the
        // path on screen.
        self.report_path.set_text(&settings.report_path);
        self.log_path.set_text(&settings.log_path);
        self.cache_path.set_text(&settings.cache_path);
        if settings.folders.is_empty() {
            self.alert("Nothing to scan", "Add at least one folder to scan.");
            self.folders.add.grab_focus();
            return;
        }
        let argv = settings.argv();
        if let Err(e) = img_fp::check_args(argv.iter().cloned()) {
            let e = e.trim_start_matches("error: ").lines().next().unwrap_or("").to_string();
            self.alert("These options cannot be used together", &e);
            return;
        }
        settings.save();
        let (scan, events) = match Scan::start(&argv) {
            Ok(x) => x,
            Err(e) => {
                self.alert("Could not start the scan", &e.to_string());
                return;
            }
        };
        self.app.log.set_text("");
        let shown: Vec<String> = argv.iter().skip(1).map(|a| shell_word(&a.to_string_lossy())).collect();
        self.app.log_line(&format!("$ img-fp {}", shown.join(" ")));
        self.scan.replace(Some(scan));
        self.started.set(Some(Instant::now()));
        self.notebook.set_sensitive(false);
        self.defaults_button.set_sensitive(false);
        self.back_button.set_visible(false);
        self.scan_button.set_visible(false);
        self.cancel_button.set_visible(true);
        self.cancel_button.grab_focus();
        self.progress.set_fraction(0.0);
        self.progress.set_text(Some("0%"));
        self.status.set_text("Starting…");
        self.elapsed.set_text("0:00");

        // The clock beside the bar, twice a second while the scan runs.
        glib::timeout_add_local(Duration::from_millis(500), {
            let me = Rc::downgrade(self);
            move || {
                let Some(me) = me.upgrade() else { return glib::ControlFlow::Break };
                let Some(t) = me.started.get() else { return glib::ControlFlow::Break };
                let s = t.elapsed().as_secs();
                me.elapsed.set_text(&format!("{}:{:02}", s / 60, s % 60));
                glib::ControlFlow::Continue
            }
        });

        let me = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let mut stderr: Vec<String> = Vec::new();
            while let Ok(ev) = events.recv().await {
                let Some(me) = me.upgrade() else { return };
                match ev {
                    Event::Progress(p, m) => {
                        if !me.cancelling.get() {
                            me.progress.set_fraction(p.clamp(0.0, 1.0));
                            me.progress.set_text(Some(&format!("{:.0}%", p * 100.0)));
                            me.status.set_text(&capitalise(&m));
                        }
                    }
                    Event::Log(line) => me.app.log_line(&line),
                    Event::Stderr(line) => {
                        me.app.log_line(&line);
                        stderr.push(line);
                    }
                    Event::Exited { code, signal } => {
                        me.finished(code, signal, &stderr);
                        return;
                    }
                }
            }
        });
    }

    fn cancel(self: &Rc<Self>) {
        let scan = self.scan.borrow();
        let Some(s) = scan.as_ref() else { return };
        if self.cancelling.replace(true) {
            return;
        }
        s.interrupt();
        self.status.set_text("Cancelling…");
        self.cancel_button.set_sensitive(false);
        // Only this scan: by the time this fires, it has almost always ended
        // and another may have started.
        let id = s.id;
        glib::timeout_add_local_once(KILL_AFTER, {
            let me = Rc::downgrade(self);
            move || {
                if let Some(me) = me.upgrade() {
                    if let Some(s) = me.scan.borrow().as_ref().filter(|s| s.id == id) {
                        s.kill();
                    }
                }
            }
        });
    }

    fn finished(self: &Rc<Self>, code: Option<i32>, signal: Option<i32>, stderr: &[String]) {
        let scan = self.scan.borrow_mut().take();
        let secs = self.started.get().map_or(0, |t| t.elapsed().as_secs());
        let took = format!("{}:{:02}", secs / 60, secs % 60);
        // The one-off cache requests have been carried out, or given up on.
        self.clear_cache.set_active(false);
        self.prune_cache.set_active(false);
        match code {
            Some(0) | Some(scan::EXIT_PROBLEMS) => {
                let found = scan.as_ref().map(|s| scan::read_report(&s.result)).unwrap_or(Err("the scan left no results".into()));
                drop(scan);
                match found {
                    Ok(found) => {
                        self.progress.set_fraction(1.0);
                        self.progress.set_text(Some("100%"));
                        let problems = code == Some(scan::EXIT_PROBLEMS);
                        let summary = format!(
                            "Done in {took}: {} group{} among {} images.{}",
                            found.groups.len(),
                            if found.groups.len() == 1 { "" } else { "s" },
                            found.analysed,
                            if problems { " It had problems with some files; see the scan log." } else { "" }
                        );
                        self.idle(&summary);
                        self.results.show(found, problems);
                        self.app.show_results();
                        self.results.focus();
                    }
                    Err(e) => {
                        self.idle("The scan finished, but its results could not be read.");
                        self.alert("Could not read the results", &e);
                    }
                }
            }
            Some(scan::EXIT_INTERRUPTED) => {
                drop(scan);
                let kept = stderr.iter().rev().find(|l| l.starts_with("Interrupted")).cloned().unwrap_or_default();
                let kept = kept.trim_start_matches("Interrupted.").trim();
                self.progress.set_text(Some("Cancelled"));
                self.idle(format!("Cancelled after {took}. {kept}").trim());
            }
            _ => {
                drop(scan);
                self.progress.set_text(Some("Failed"));
                if signal == Some(9) {
                    self.idle("Stopped: the scan did not answer the cancel, and was ended.");
                    return;
                }
                let error = stderr
                    .iter()
                    .rev()
                    .find(|l| l.starts_with("Error"))
                    .map(|l| l.trim_start_matches("Error:").trim().to_string())
                    .unwrap_or_else(|| match (code, signal) {
                        (_, Some(s)) => format!("The scan was ended by signal {s}."),
                        (Some(c), _) => format!("The scan exited with code {c}."),
                        _ => "The scan ended unexpectedly.".to_string(),
                    });
                self.idle("The scan failed.");
                self.alert("The scan failed", &error);
            }
        }
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// A command-line word as a shell would need it written, for the log.
fn shell_word(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=,:@%+".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}
