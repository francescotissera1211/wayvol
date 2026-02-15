//! GTK Application subclass that manages the window and monitor thread.

use std::cell::RefCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use async_channel::Receiver;
use gtk::{gio, glib};

use crate::monitor::{MonitorEvent, MonitorThread};
use crate::ui::Window;

mod imp {
    use super::*;

    pub struct Application {
        pub monitor: RefCell<Option<MonitorThread>>,
    }

    impl Default for Application {
        fn default() -> Self {
            Self {
                monitor: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "WayvolApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self) {
            self.parent_activate();
            self.obj().on_activate();
        }

        fn startup(&self) {
            self.parent_startup();
            self.obj().on_startup();
        }

        fn shutdown(&self) {
            if let Some(mut monitor) = self.monitor.borrow_mut().take() {
                log::info!("Shutting down monitor thread");
                monitor.shutdown();
            }
            self.parent_shutdown();
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl Default for Application {
    fn default() -> Self {
        Self::new()
    }
}

impl Application {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", "com.github.wayvol")
            .property("flags", gio::ApplicationFlags::default())
            .build()
    }

    fn imp(&self) -> &imp::Application {
        imp::Application::from_obj(self)
    }

    fn on_startup(&self) {
        log::info!("wayvol starting up");

        // Set up quit action
        let quit_action = gio::SimpleAction::new("quit", None);
        let app = self.clone();
        quit_action.connect_activate(move |_, _| {
            app.quit();
        });
        self.add_action(&quit_action);
        self.set_accels_for_action("app.quit", &["<Control>q"]);
    }

    fn on_activate(&self) {
        // Create or re-present existing window
        let window = if let Some(win) = self.active_window() {
            log::debug!("Re-activating existing window");
            win.downcast::<Window>()
                .expect("Active window should be WayvolWindow")
        } else {
            log::debug!("Creating new window");
            let window = Window::new(self.upcast_ref());

            // Present window FIRST so it appears immediately,
            // then schedule data loading on idle so the main loop
            // can process the present request before we block on wpctl.
            window.present();
            log::debug!("Window presented");

            let win_clone = window.clone();
            glib::idle_add_local_once(move || {
                log::debug!("Idle callback: starting initial refresh");
                win_clone.refresh();
                log::debug!("Idle callback: initial refresh complete");
            });

            // Start monitor thread
            self.start_monitor(&window);

            return;
        };

        window.present();
    }

    /// Start the background monitor thread and wire events to the window.
    fn start_monitor(&self, window: &Window) {
        let (event_tx, event_rx) = async_channel::unbounded::<MonitorEvent>();

        match MonitorThread::spawn(event_tx) {
            Ok(monitor) => {
                *self.imp().monitor.borrow_mut() = Some(monitor);
                self.process_monitor_events(event_rx, window);
                log::info!("Monitor thread started");
            }
            Err(e) => {
                log::error!("Failed to start monitor thread: {e}");
                window.announce("Warning: could not start audio monitor. Streams will not update automatically.");
            }
        }
    }

    /// Process monitor events on the GTK main loop with debouncing.
    /// Multiple rapid StreamsChanged events are coalesced — at most one
    /// refresh per second.
    fn process_monitor_events(&self, rx: Receiver<MonitorEvent>, window: &Window) {
        let window = window.clone();
        let refresh_scheduled = std::rc::Rc::new(std::cell::Cell::new(false));

        glib::spawn_future_local(async move {
            while let Ok(event) = rx.recv().await {
                match event {
                    MonitorEvent::StreamsChanged => {
                        if refresh_scheduled.get() {
                            continue;
                        }

                        refresh_scheduled.set(true);
                        let win = window.clone();
                        let flag = refresh_scheduled.clone();
                        let rx_drain = rx.clone();

                        // Wait 500ms, then refresh, then hold a 500ms cooldown.
                        // This coalesces bursts of pw-dump events into one refresh
                        // per ~1 second.
                        glib::timeout_add_local_once(
                            std::time::Duration::from_millis(500),
                            move || {
                                // Drain any events that queued during the wait
                                while rx_drain.try_recv().is_ok() {}

                                log::debug!("Monitor: debounced refresh firing");
                                win.refresh_streams();

                                // Keep the flag set for a cooldown period
                                glib::timeout_add_local_once(
                                    std::time::Duration::from_millis(500),
                                    move || {
                                        flag.set(false);
                                    },
                                );
                            },
                        );
                    }
                    MonitorEvent::Error(msg) => {
                        log::error!("Monitor error: {msg}");
                        window.announce(&format!("Audio monitor error: {msg}"));
                    }
                }
            }
            log::debug!("Monitor event channel closed");
        });
    }
}
