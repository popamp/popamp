//! Popamp: a Winamp-style skinnable audio player for Linux and Windows.

// A window program on Windows, so none opens a console of its own; output
// for the command line goes to the console it was started from.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod bar;
mod bind;
mod canvas;
mod cli;
mod dialogs;
mod edition;
mod fileinfo;
mod filetypes;
mod font;
mod gallery;
mod headless;
mod history;
mod hold;
mod hotkeys;
mod ipc;
#[cfg(feature = "visualizers")]
mod keep_awake;
mod library;
mod media;
mod menu;
mod model;
mod museum;
mod panels;
mod pen;
mod platform;
#[cfg(feature = "plugins")]
mod plugins;
mod render;
mod scale;
mod scan;
mod skinwin;
mod ui;
mod vis;
#[cfg(feature = "visualizers")]
mod visualizer;
mod widgets;

use cli::Remote;
use model::{AppEvent, Model};
use popamp_core::config::{Config, Paths};
use winit::event_loop::EventLoop;

fn main() {
    #[cfg(windows)]
    platform::use_parent_console();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let opts = match cli::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("popamp: {e}");
            std::process::exit(2);
        }
    };
    if opts.help {
        print!("{}", cli::HELP);
        return;
    }
    if opts.version {
        println!("{}", edition::version_line());
        return;
    }
    if let Some(dir) = &opts.render {
        let files = opts.remotes.iter().flat_map(|r| match r {
            Remote::Open(f) | Remote::Add(f) => f.clone(),
            _ => Vec::new(),
        });
        match headless::render_all(dir, opts.skin.clone(), files.collect()) {
            Ok(()) => println!("rendered to {}", dir.display()),
            Err(e) => {
                eprintln!("popamp: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let class = opts.instance_class.clone().unwrap_or_else(|| "default".into());
    let socket = ipc::socket_path(&class);

    // Forward to a running instance unless /NEW.
    if !opts.new_instance {
        let mut remotes = opts.remotes.clone();
        if opts.kill {
            remotes = vec![Remote::Quit];
        }
        let forwarded = if remotes.is_empty() { vec![Remote::Show] } else { remotes };
        match ipc::send(&socket, &forwarded) {
            Ok(reply) => {
                if forwarded.contains(&Remote::Status) {
                    println!("{reply}");
                }
                return;
            }
            Err(e) => {
                log::debug!("no instance at {}: {e}", socket.display());
                if opts.kill || forwarded.iter().all(|r| matches!(r, Remote::Quit | Remote::Status)) {
                    eprintln!("popamp: no running instance");
                    std::process::exit(1);
                }
            }
        }
    }

    if let Err(e) = popamp_audio::output::available() {
        log::error!("{e} — audio output will not work");
    }
    let paths = Paths::resolve(opts.ini_dir.clone());
    let ini = opts.config_file.clone().unwrap_or_else(|| paths.ini_file());
    let config = Config::load(ini);
    if opts.del_m3u {
        let _ = std::fs::remove_file(paths.playlist_file());
    }

    let mut event_loop = EventLoop::<AppEvent>::with_user_event();
    #[cfg(linux_like)]
    platform::choose_window_system(&mut event_loop, config.settings.wayland);
    let event_loop = match event_loop.build() {
        Ok(el) => el,
        Err(e) => {
            eprintln!("popamp: cannot open a display: {e}");
            std::process::exit(1);
        }
    };
    let proxy = event_loop.create_proxy();
    let mut model = Model::new(paths, config, proxy.clone());
    model.load_session();

    let server_proxy = proxy.clone();
    let socket_for_server = if opts.new_instance && opts.instance_class.is_none() {
        ipc::socket_path(&format!("new-{}", std::process::id()))
    } else {
        socket.clone()
    };
    if let Err(e) = ipc::serve(socket_for_server.clone(), move |req| {
        let _ = server_proxy.send_event(AppEvent::Remote(req));
    }) {
        log::warn!("cannot create control socket {}: {e}", socket_for_server.display());
    }

    let reply = model.on_remote(opts.remotes.clone());
    if opts.remotes.contains(&Remote::Status) {
        println!("{reply}");
    }

    let media = media::spawn(proxy.clone(), model.engine.shared().clone(), &class);
    let hotkeys_proxy = proxy.clone();
    let hotkeys = hotkeys::Hotkeys::new(std::sync::Arc::new(move |c| media::send(&hotkeys_proxy, vec![cli::Remote::Cmd(c)])));
    let mut app = app::App::new(model, opts.start_minimized, media, hotkeys);
    if let Err(e) = event_loop.run_app(&mut app) {
        log::error!("event loop: {e}");
    }
    let _ = std::fs::remove_file(&socket_for_server);
}
