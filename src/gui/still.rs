//! A picture whose size does not depend on what it shows.
//!
//! `gtk::Picture` asks for its texture's own size, so a texture arriving
//! queues a resize, and a resize climbs to the window: the whole page is
//! measured and laid out again. With a group's pictures and a strip of seven
//! hundred groups arriving a few at a time from the decoders, that held the
//! main loop for 40 to 120 ms at a time, many times a second, measured on
//! IMGS-ALL, until the last of them had come. A `Still` is told its size by
//! its parent (or holds a fixed one), fills it the way `ContentFit::Cover`
//! does, and a new texture only redraws it.

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct Still {
        pub texture: RefCell<Option<gdk::Texture>>,
        /// What it asks for, both ways: (0, 0) to take what it is given.
        pub size: Cell<(i32, i32)>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Still {
        const NAME: &'static str = "ImgFpStill";
        type Type = super::Still;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("still");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Still {}

    impl WidgetImpl for Still {
        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let (w, h) = self.size.get();
            let n = if orientation == gtk::Orientation::Horizontal { w } else { h };
            (n, n, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let Some(t) = self.texture.borrow().clone() else { return };
            let w = self.obj().width() as f32;
            let h = self.obj().height() as f32;
            let (tw, th) = (t.width() as f32, t.height() as f32);
            if w <= 0.0 || h <= 0.0 || tw <= 0.0 || th <= 0.0 {
                return;
            }
            // Cover: as small as fills both ways, centred, the rest cut.
            let s = (w / tw).max(h / th);
            let (dw, dh) = (tw * s, th * s);
            snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, w, h));
            snapshot.append_scaled_texture(&t, gsk::ScalingFilter::Linear, &graphene::Rect::new((w - dw) / 2.0, (h - dh) / 2.0, dw, dh));
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct Still(ObjectSubclass<imp::Still>) @extends gtk::Widget, @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Still {
    /// `width` x `height` always, or (0, 0) for whatever the parent gives.
    pub fn new(width: i32, height: i32) -> Still {
        let s: Still = glib::Object::new();
        s.imp().size.set((width, height));
        s
    }

    pub fn set_texture(&self, t: Option<&gdk::Texture>) {
        *self.imp().texture.borrow_mut() = t.cloned();
        self.queue_draw();
    }

    /// What a screen reader says for it.
    pub fn set_label(&self, text: &str) {
        self.update_property(&[gtk::accessible::Property::Label(text)]);
    }
}

/// A texture whose size is said in the desktop's units, not its pixels: at
/// a scale factor of 2, a 4000-pixel picture is 2000 wide, so a picture
/// shown at that size has one of its pixels on each pixel of the screen.
/// GTK reads a texture's pixels as units, which is twice that.
mod actual {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct ActualSize {
        pub texture: RefCell<Option<gdk::Texture>>,
        pub scale: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ActualSize {
        const NAME: &'static str = "ImgFpActualSize";
        type Type = super::ActualSize;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for ActualSize {}

    impl PaintableImpl for ActualSize {
        fn flags(&self) -> gdk::PaintableFlags {
            gdk::PaintableFlags::STATIC_SIZE | gdk::PaintableFlags::STATIC_CONTENTS
        }

        fn intrinsic_width(&self) -> i32 {
            self.texture.borrow().as_ref().map_or(0, |t| (t.width() + self.scale.get().max(1) - 1) / self.scale.get().max(1))
        }

        fn intrinsic_height(&self) -> i32 {
            self.texture.borrow().as_ref().map_or(0, |t| (t.height() + self.scale.get().max(1) - 1) / self.scale.get().max(1))
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let (Some(t), Some(s)) = (self.texture.borrow().clone(), snapshot.downcast_ref::<gtk::Snapshot>()) else { return };
            // Pixel for pixel, so nearest: a sharp copy beside a soft one is
            // what this is looked at for.
            s.append_scaled_texture(&t, gsk::ScalingFilter::Nearest, &graphene::Rect::new(0.0, 0.0, width as f32, height as f32));
        }
    }
}

glib::wrapper! {
    pub struct ActualSize(ObjectSubclass<actual::ActualSize>) @implements gdk::Paintable;
}

impl ActualSize {
    /// `t` shown at `scale` pixels of the screen to a unit of the desktop.
    pub fn new(t: &gdk::Texture, scale: i32) -> ActualSize {
        let a: ActualSize = glib::Object::new();
        *a.imp().texture.borrow_mut() = Some(t.clone());
        a.imp().scale.set(scale.max(1));
        a
    }

    /// Its size in the desktop's units.
    pub fn size(&self) -> (i32, i32) {
        (self.intrinsic_width(), self.intrinsic_height())
    }
}
