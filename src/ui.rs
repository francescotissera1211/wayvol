//! Main application window with accessible volume controls.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::SignalHandlerId;
use gtk::glib;

use crate::wpctl;

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
    #[template(string = r#"
        <interface>
            <template class="WayvolWindow" parent="AdwApplicationWindow">
                <property name="title">wayvol - Volume Mixer</property>
                <property name="default-width">550</property>
                <property name="default-height">500</property>
                <child>
                    <object class="GtkBox" id="root_box">
                        <property name="orientation">vertical</property>
                        <child>
                            <object class="AdwHeaderBar">
                                <property name="title-widget">
                                    <object class="AdwWindowTitle">
                                        <property name="title">wayvol</property>
                                        <property name="subtitle">Volume Mixer</property>
                                    </object>
                                </property>
                            </object>
                        </child>
                        <child>
                            <object class="GtkScrolledWindow" id="scroll">
                                <property name="vexpand">true</property>
                                <property name="hscrollbar-policy">never</property>
                                <child>
                                    <object class="AdwClamp">
                                        <property name="maximum-size">700</property>
                                        <child>
                                            <object class="GtkBox" id="content_box">
                                                <property name="orientation">vertical</property>
                                                <property name="spacing">12</property>
                                                <property name="margin-top">12</property>
                                                <property name="margin-bottom">12</property>
                                                <property name="margin-start">12</property>
                                                <property name="margin-end">12</property>
                                            </object>
                                        </child>
                                    </object>
                                </child>
                            </object>
                        </child>
                    </object>
                </child>
            </template>
        </interface>
    "#)]
    pub struct Window {
        #[template_child]
        pub content_box: TemplateChild<gtk::Box>,

        pub sink_dropdown: RefCell<Option<gtk::DropDown>>,
        pub source_dropdown: RefCell<Option<gtk::DropDown>>,
        pub streams_group: RefCell<Option<adw::PreferencesGroup>>,
        /// Track stream rows so we can remove them on refresh.
        pub stream_rows: RefCell<Vec<adw::ActionRow>>,
        /// Track current stream IDs to avoid unnecessary rebuilds.
        /// Only rebuild rows when the set of streams actually changes
        /// (app starts/stops playing), NOT on every pw-dump event.
        /// This prevents Orca from re-announcing every row on refresh.
        pub current_stream_ids: RefCell<Vec<u32>>,
        /// Signal handler ID for sink dropdown selection changes.
        pub sink_handler: RefCell<Option<SignalHandlerId>>,
        /// Signal handler ID for source dropdown selection changes.
        pub source_handler: RefCell<Option<SignalHandlerId>>,
    }

    impl Default for Window {
        fn default() -> Self {
            Self {
                content_box: Default::default(),
                sink_dropdown: RefCell::new(None),
                source_dropdown: RefCell::new(None),
                streams_group: RefCell::new(None),
                stream_rows: RefCell::new(Vec::new()),
                current_stream_ids: RefCell::new(Vec::new()),
                sink_handler: RefCell::new(None),
                source_handler: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "WayvolWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            log::debug!("Window::constructed - calling parent");
            self.parent_constructed();
            log::debug!("Window::constructed - calling setup_ui");
            self.obj().setup_ui();
            log::debug!("Window::constructed - done");
        }
    }

    impl WidgetImpl for Window {}
    impl WindowImpl for Window {}
    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionMap, gio::ActionGroup, gtk::Root;
}

impl Window {
    pub fn new(app: &adw::Application) -> Self {
        log::debug!("Window::new - building");
        let window: Self = glib::Object::builder()
            .property("application", app)
            .build();
        log::debug!("Window::new - built successfully");
        window
    }

    fn imp(&self) -> &imp::Window {
        imp::Window::from_obj(self)
    }

    /// Build the full UI layout.
    fn setup_ui(&self) {
        log::debug!("setup_ui - start");
        let content_box = &self.imp().content_box;

        // Device selection section
        let device_group = self.build_device_section();
        content_box.append(&device_group);
        log::debug!("setup_ui - device section added");

        // Streams section
        let streams_group = self.build_streams_section();
        content_box.append(&streams_group);
        log::debug!("setup_ui - streams section added, done");
    }

    /// Build the output/input device selection dropdowns.
    fn build_device_section(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::builder()
            .title("Devices")
            .build();
        group.update_property(&[gtk::accessible::Property::Label("Audio device selection")]);

        // Output device row
        let sink_row = adw::ActionRow::builder()
            .title("Output Device")
            .build();
        sink_row.update_property(&[gtk::accessible::Property::Label("Output device selector")]);

        let sink_model = gtk::StringList::new(&[]);
        let sink_dropdown = gtk::DropDown::builder()
            .model(&sink_model)
            .valign(gtk::Align::Center)
            .build();
        sink_dropdown
            .update_property(&[gtk::accessible::Property::Label("Select output device")]);
        sink_row.add_suffix(&sink_dropdown);
        group.add(&sink_row);

        *self.imp().sink_dropdown.borrow_mut() = Some(sink_dropdown);

        // Input device row
        let source_row = adw::ActionRow::builder()
            .title("Input Device")
            .build();
        source_row
            .update_property(&[gtk::accessible::Property::Label("Input device selector")]);

        let source_model = gtk::StringList::new(&[]);
        let source_dropdown = gtk::DropDown::builder()
            .model(&source_model)
            .valign(gtk::Align::Center)
            .build();
        source_dropdown
            .update_property(&[gtk::accessible::Property::Label("Select input device")]);
        source_row.add_suffix(&source_dropdown);
        group.add(&source_row);

        *self.imp().source_dropdown.borrow_mut() = Some(source_dropdown);

        group
    }

    /// Build the streams section. Rows are added dynamically by refresh_streams.
    fn build_streams_section(&self) -> adw::PreferencesGroup {
        let group = adw::PreferencesGroup::builder()
            .title("Streams")
            .description("Active audio streams")
            .build();

        *self.imp().streams_group.borrow_mut() = Some(group.clone());
        group
    }

    /// Refresh all UI data from wpctl.
    pub fn refresh(&self) {
        log::debug!("refresh - start");
        self.refresh_devices();
        self.refresh_streams();
        log::debug!("refresh - done");
    }

    /// Refresh device dropdowns.
    fn refresh_devices(&self) {
        log::debug!("refresh_devices - fetching status");
        let status = match wpctl::get_status() {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to get wpctl status: {e}");
                return;
            }
        };

        // Update sinks dropdown — filter out unplugged devices
        let mut sinks = wpctl::parse_devices(&status, wpctl::DeviceType::Sink);
        wpctl::enrich_device_availability(&mut sinks, wpctl::DeviceType::Sink);
        sinks.retain(|d| d.available != Some(false));
        log::debug!("refresh_devices - found {} available sinks", sinks.len());
        if let Some(dropdown) = self.imp().sink_dropdown.borrow().as_ref() {
            // Disconnect old handler before updating to avoid triggering it
            if let Some(handler_id) = self.imp().sink_handler.borrow_mut().take() {
                dropdown.disconnect(handler_id);
            }

            update_device_dropdown(dropdown, &sinks);

            // Connect new selection handler
            let sinks_clone = sinks.clone();
            let handler_id = dropdown.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                if let Some(device) = sinks_clone.get(idx) {
                    log::debug!("Sink selection changed to: {} (id={})", device.name, device.id);
                    if let Err(e) = wpctl::set_default(device.id) {
                        log::error!("Failed to set default sink: {e}");
                    }
                }
            });
            *self.imp().sink_handler.borrow_mut() = Some(handler_id);
        }

        // Update sources dropdown — filter out unplugged devices
        let mut sources = wpctl::parse_devices(&status, wpctl::DeviceType::Source);
        wpctl::enrich_device_availability(&mut sources, wpctl::DeviceType::Source);
        sources.retain(|d| d.available != Some(false));
        log::debug!("refresh_devices - found {} available sources", sources.len());
        if let Some(dropdown) = self.imp().source_dropdown.borrow().as_ref() {
            // Disconnect old handler before updating
            if let Some(handler_id) = self.imp().source_handler.borrow_mut().take() {
                dropdown.disconnect(handler_id);
            }

            update_device_dropdown(dropdown, &sources);

            let sources_clone = sources.clone();
            let handler_id = dropdown.connect_selected_notify(move |dd| {
                let idx = dd.selected() as usize;
                if let Some(device) = sources_clone.get(idx) {
                    log::debug!("Source selection changed to: {} (id={})", device.name, device.id);
                    if let Err(e) = wpctl::set_default(device.id) {
                        log::error!("Failed to set default source: {e}");
                    }
                }
            });
            *self.imp().source_handler.borrow_mut() = Some(handler_id);
        }
    }

    /// Refresh the streams list.
    ///
    /// Only rebuilds rows when the set of stream IDs actually changes
    /// (new app starts playing, app closes). Skips entirely if the same
    /// streams are present — this prevents Orca from re-announcing every
    /// row on each pw-dump event.
    pub fn refresh_streams(&self) {
        log::debug!("refresh_streams - fetching status");
        let group = match self.imp().streams_group.borrow().clone() {
            Some(g) => g,
            None => return,
        };

        let status = match wpctl::get_status() {
            Ok(s) => s,
            Err(e) => {
                log::error!("Failed to get wpctl status: {e}");
                return;
            }
        };

        let streams = wpctl::parse_streams(&status);
        log::debug!("refresh_streams - parsed {} streams", streams.len());

        // Check if the set of streams has actually changed.
        let new_ids: Vec<u32> = streams.iter().map(|s| s.id).collect();
        if *self.imp().current_stream_ids.borrow() == new_ids {
            log::debug!("refresh_streams - same streams, skipping rebuild");
            return;
        }

        let old_count = self.imp().current_stream_ids.borrow().len() as u32;
        log::debug!("refresh_streams - stream set changed, rebuilding");

        // Fetch actual volumes for each stream
        let streams: Vec<wpctl::Stream> = streams
            .into_iter()
            .map(|mut s| {
                if let Ok(vol_info) = wpctl::get_volume(s.id) {
                    s.volume = vol_info.volume;
                    s.muted = vol_info.muted;
                }
                s
            })
            .collect();

        // Remove old rows
        for row in self.imp().stream_rows.borrow().iter() {
            group.remove(row);
        }
        self.imp().stream_rows.borrow_mut().clear();

        // Add new rows
        let new_count = streams.len() as u32;
        for stream in &streams {
            let row = build_stream_action_row(stream);
            group.add(&row);
            self.imp().stream_rows.borrow_mut().push(row);
        }

        // Store current IDs for next comparison
        *self.imp().current_stream_ids.borrow_mut() = new_ids;

        // Update description
        if new_count == 0 {
            group.set_description(Some("No active audio streams"));
        } else {
            group.set_description(Some(&format!(
                "{new_count} active audio stream{}",
                if new_count == 1 { "" } else { "s" }
            )));
        }

        // Announce only when streams appear or disappear
        if new_count > old_count && old_count > 0 {
            let diff = new_count - old_count;
            let msg = if diff == 1 {
                "New audio stream appeared".to_string()
            } else {
                format!("{diff} new audio streams appeared")
            };
            self.announce(&msg);
        } else if new_count < old_count {
            let diff = old_count - new_count;
            let msg = if diff == 1 {
                "Audio stream removed".to_string()
            } else {
                format!("{diff} audio streams removed")
            };
            self.announce(&msg);
        }
    }

    /// Announce a message to screen readers via AT-SPI.
    pub fn announce(&self, message: &str) {
        log::info!("Announce: {message}");
        self.upcast_ref::<gtk::Widget>()
            .announce(message, gtk::AccessibleAnnouncementPriority::Medium);
    }
}

/// Update a DropDown widget with new device list.
fn update_device_dropdown(dropdown: &gtk::DropDown, devices: &[wpctl::Device]) {
    let model = gtk::StringList::new(&[]);
    let mut default_idx = 0u32;

    for (i, device) in devices.iter().enumerate() {
        model.append(&device.name);
        if device.is_default {
            default_idx = i as u32;
        }
    }

    dropdown.set_model(Some(&model));
    dropdown.set_selected(default_idx);
}

/// Build an AdwActionRow for a single audio stream.
///
/// Uses libadwaita's native row widget so keyboard navigation (Tab/arrows)
/// and screen reader focus work correctly without nested ListBox hacks.
fn build_stream_action_row(stream: &wpctl::Stream) -> adw::ActionRow {
    let pct = (stream.volume * 100.0).round() as i32;
    let mute_str = if stream.muted { " · Muted" } else { "" };

    let row = adw::ActionRow::builder()
        .title(&stream.name)
        .subtitle(format!("{}{}", stream.stream_type.as_str(), mute_str))
        .build();
    row.update_property(&[gtk::accessible::Property::Label(
        &wpctl::accessible_label(stream),
    )]);

    // Stream type icon as prefix
    let icon = gtk::Image::builder()
        .icon_name(stream.stream_type.icon())
        .pixel_size(24)
        .build();
    row.add_prefix(&icon);

    // Volume slider (0% to 150%)
    let adjustment = gtk::Adjustment::new(
        stream.volume * 100.0, // value
        0.0,                   // lower
        150.0,                 // upper (150% for boost)
        1.0,                   // step increment
        10.0,                  // page increment
        0.0,                   // page size
    );

    let slider = gtk::Scale::builder()
        .orientation(gtk::Orientation::Horizontal)
        .adjustment(&adjustment)
        .hexpand(true)
        .draw_value(false)
        .width_request(200)
        .focusable(true)
        .build();
    slider.update_property(&[gtk::accessible::Property::Label(
        &wpctl::slider_accessible_label(stream),
    )]);

    // Add marks at 0%, 100%, 150%
    slider.add_mark(0.0, gtk::PositionType::Bottom, Some("0%"));
    slider.add_mark(100.0, gtk::PositionType::Bottom, Some("100%"));
    slider.add_mark(150.0, gtk::PositionType::Bottom, Some("150%"));

    // Volume percentage label
    let vol_label = gtk::Label::builder()
        .label(format!("{pct}%"))
        .width_chars(5)
        .xalign(1.0)
        .build();

    // Connect slider value changes
    let stream_id = stream.id;
    let vol_label_clone = vol_label.clone();
    adjustment.connect_value_changed(move |adj| {
        let new_pct = adj.value().round() as i32;
        vol_label_clone.set_label(&format!("{new_pct}%"));

        let level = adj.value() / 100.0;
        if let Err(e) = wpctl::set_volume(stream_id, level) {
            log::error!("Failed to set volume for stream {stream_id}: {e}");
        }
    });

    // Mute toggle button
    let mute_icon = if stream.muted {
        "audio-volume-muted-symbolic"
    } else {
        "audio-volume-high-symbolic"
    };
    let mute_btn = gtk::ToggleButton::builder()
        .icon_name(mute_icon)
        .active(stream.muted)
        .valign(gtk::Align::Center)
        .focusable(true)
        .css_classes(vec!["flat".to_string()])
        .build();
    mute_btn.update_property(&[gtk::accessible::Property::Label(
        &wpctl::mute_button_label(stream),
    )]);

    let stream_name = stream.name.clone();
    mute_btn.connect_toggled(move |btn| {
        let is_muted = btn.is_active();
        let icon = if is_muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        btn.set_icon_name(icon);

        let label = if is_muted {
            format!("Unmute {stream_name}")
        } else {
            format!("Mute {stream_name}")
        };
        btn.update_property(&[gtk::accessible::Property::Label(&label)]);

        if let Err(e) = wpctl::toggle_mute(stream_id) {
            log::error!("Failed to toggle mute for stream {stream_id}: {e}");
        }
    });

    row.add_suffix(&slider);
    row.add_suffix(&vol_label);
    row.add_suffix(&mute_btn);

    row
}
