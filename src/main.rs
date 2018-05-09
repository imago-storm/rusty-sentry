extern crate rusty_sentry;
extern crate notify;
extern crate docopt;
#[macro_use]
extern crate serde_derive;
use docopt::Docopt;
use std::env;
use std::path::PathBuf;
use notify::{RecommendedWatcher, Watcher, RecursiveMode, DebouncedEvent};
use rusty_sentry::updater::Updater;
use rusty_sentry::ef_client::EFClient;
use std::time::Duration;
use std::sync::mpsc::channel;



#[derive(Debug, Deserialize)]
struct Args {
    cmd_watch: bool,
    arg_path: Option<String>,
    flag_version: bool,
}

const USAGE: &'static str = "
Rusty Sentry.

Usage:
    rusty_sentry watch [--path <path>] [--username <username>] [--password <password>]
    rusty_sentry (-h | --help)
    rusty_sentry --version

Options:
    --version    Show version.
";

fn main() {
    let mut version: String;
    match env::var("RUSTY_SENTRY_VERSION") {
        Ok(val) => version = val,
        Err(_) => version = String::from("no version set")
    };
    println!("Version: {}", version);
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    println!("{:?}", args);
    let path = match args.arg_path {
        Some(path) => PathBuf::from(path),
        None => env::current_dir().unwrap(),
    };
    println!("Path: {}", path.to_str().unwrap());

    let path = PathBuf::from("/Users/imago/Documents/ecloud/plugins/containers/EC-Kubernetes");
    let ef_client = EFClient::new("ubuntu-esxi", "admin", "changeme");
    let updater = Updater::new(&path, ef_client).unwrap();
    println!("{:?}", updater);

    if let Err(e) = watch(&path, &updater) {
        println!("Error: {:?}", e);
    }
}

fn watch(path: &PathBuf, updater: &Updater) -> notify::Result<()> {
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx, Duration::from_secs(2))?;
    watcher.watch(path, RecursiveMode::Recursive)?;
    println!("Started to watch {}", path.to_str().unwrap());
    loop {
        match rx.recv() {
            Ok(DebouncedEvent::Create(path)) => {
                println!("Created {}", path.to_str().unwrap());
                updater.update(&path);
            },
            Ok(DebouncedEvent::Write(path)) => {
                println!("Write: {:?}", path);
                updater.update(&path);
            },
            Ok(_) => {
            },
            Err(error) => {
                println!("Watch error: {:?}", error);
            }
        }
    }
}

