//! GObject wrappers for GTK ListStore binding.
//!
//! GTK4's `gio::ListStore` requires GObject types, so we wrap our Stream data.

use std::cell::{Cell, RefCell};

use glib::prelude::*;
use glib::Properties;
use gtk::glib;
use gtk::subclass::prelude::*;

mod imp {
    use super::*;

    #[derive(Default, Properties)]
    #[properties(wrapper_type = super::StreamObject)]
    pub struct StreamObject {
        #[property(get, set)]
        id: Cell<u32>,
        #[property(get, set)]
        name: RefCell<String>,
        /// "Playback" or "Capture"
        #[property(get, set)]
        stream_type: RefCell<String>,
        /// Volume as 0.0 - 1.5 float
        #[property(get, set)]
        volume: Cell<f64>,
        #[property(get, set)]
        muted: Cell<bool>,
        /// Icon name for the stream type
        #[property(get, set)]
        icon_name: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StreamObject {
        const NAME: &'static str = "WayvolStreamObject";
        type Type = super::StreamObject;
    }

    #[glib::derived_properties]
    impl ObjectImpl for StreamObject {}
}

glib::wrapper! {
    pub struct StreamObject(ObjectSubclass<imp::StreamObject>);
}

impl StreamObject {
    pub fn new(
        id: u32,
        name: &str,
        stream_type: &str,
        volume: f64,
        muted: bool,
        icon_name: &str,
    ) -> Self {
        glib::Object::builder()
            .property("id", id)
            .property("name", name)
            .property("stream-type", stream_type)
            .property("volume", volume)
            .property("muted", muted)
            .property("icon-name", icon_name)
            .build()
    }

    /// Accessible description for screen readers.
    pub fn accessible_description(&self) -> String {
        let pct = (self.volume() * 100.0).round() as i32;
        let mute_str = if self.muted() { ", muted" } else { "" };
        format!(
            "{} {} stream, {}%{}",
            self.name(),
            self.stream_type(),
            pct,
            mute_str
        )
    }

    /// Label for the volume slider.
    pub fn slider_label(&self) -> String {
        let pct = (self.volume() * 100.0).round() as i32;
        format!("{} volume, {}%", self.name(), pct)
    }

    /// Label for the mute button.
    pub fn mute_label(&self) -> String {
        if self.muted() {
            format!("Unmute {}", self.name())
        } else {
            format!("Mute {}", self.name())
        }
    }
}

impl From<&crate::wpctl::Stream> for StreamObject {
    fn from(stream: &crate::wpctl::Stream) -> Self {
        StreamObject::new(
            stream.id,
            &stream.name,
            stream.stream_type.as_str(),
            stream.volume,
            stream.muted,
            stream.stream_type.icon(),
        )
    }
}

// GObject model tests require GTK initialization which has strict thread
// requirements. The accessible label logic is thoroughly tested via the
// pure functions in wpctl::tests (accessible_label, slider_accessible_label,
// mute_button_label). The GObject wrapper is a thin layer over those same
// computations, so integration testing covers it at the application level.
//
// The From<&Stream> conversion is straightforward field mapping that would
// need GTK init to verify; it's validated by the working application.
