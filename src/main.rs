mod application;
mod model;
mod monitor;
mod ui;
pub mod wpctl;

use gtk::prelude::*;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("wayvol v{} starting", env!("CARGO_PKG_VERSION"));

    let app = application::Application::new();
    let exit_code = app.run();
    std::process::exit(exit_code.into());
}
