extern crate rusty_sentry;
extern crate notify;
extern crate getopts;
#[macro_use]
extern crate serde_derive;
extern crate serde_xml_rs;
extern crate url;

use std::env;
use std::path::PathBuf;
use std::process::exit;
use std::time::Duration;
use std::sync::mpsc::channel;
use std::fs::File;
use std::error::Error;
use std::io::prelude::*;
use getopts::Options;
use url::Url;
use notify::{RecommendedWatcher, Watcher, RecursiveMode, DebouncedEvent};
use rusty_sentry::updater::{PartialUpdate, guess_plugin_type, PluginType, PluginWizard, PluginGradle};
use rusty_sentry::ef_client::EFClient;
use serde_xml_rs::deserialize;


const SERVER: &str = "s";
const USERNAME: &str = "u";
const PASSWORD: &str = "p";
const PATH: &str = "path";
const SID: &str = "sid";

#[derive(Deserialize, Debug)]
struct Session {
    url: String,
    #[serde(rename="sessionId", default)]
    session_id: String,
}

#[derive(Deserialize, Debug)]
struct Sessions {
    #[serde(rename = "session", default)]
    sessions: Vec<Session>
}

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} COMMAND [options]", program);
    print!("{}", opts.usage(&brief));
}

fn print_version() {
    let developer_build = String::from("<DEVELOPER_BUILD>");
    let version = match env::var("RUSTY_SENTRY_VERSION") {
        Ok(val) => val,
        Err(_) => developer_build.clone(),
    };
    let git_hash = match env::var("RUST_SENTRY_GIT_HASH") {
        Ok(val) => val,
        Err(_) => developer_build.clone()
    };
    let date = match env::var("RUSTY_SENTRY_DATE") {
        Ok(val) => val,
        Err(_) => developer_build.clone(),
    };
    print!("Version: {}\nDate: {}\nGit Commit Hash: {}\n", version, date, git_hash);
}


fn read_sid(server: &str) -> Option<String> {
    let sid_path = match env::home_dir() {
        Some(path) => {
            let mut sid_path = path.clone();
            sid_path.push(".ecsession");
            if sid_path.exists() {
                Some(sid_path)
            } else {
                None
            }
        },
        None => None
    };

    if sid_path == None {
        return None;
    }

    let mut file: File;
    match File::open(sid_path.unwrap()) {
        Ok(f) => file = f,
        Err(_) => return None,
    };
    let mut contents = String::new();
    let result = file.read_to_string(&mut contents);
    if result.is_err() {
        return None;
    }

    let sessions: Result<Sessions, serde_xml_rs::Error> = deserialize(contents.as_bytes());
    match sessions {
        Ok(s) => {
            for session in s.sessions {
                let url = Url::parse(&session.url).unwrap();
                if url.host_str() == Some(server) {
                    return Some(session.session_id)
                }
            }
            None
        },
        Err(_) => { None }
    }
}


fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag("h", "help", "Print this help menu");
    opts.optflag("v", "version", "Show the version");

    opts.optopt(USERNAME, "username", "Provide username to connect to server", "");
    opts.reqopt(SERVER, "server", "provide server name to connect", "");
    opts.optopt("", PATH, "Provide path to the plugin folder", "PATH");
    opts.optopt(PASSWORD, "password", "provide password for the server to connect", "PASSWORD");
    opts.optopt("", SID, "provide session id to connect", "SID");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m },
        Err(f) => {
            eprintln!("{}", f.to_string());
            exit(-1);
        }
    };
    if matches.opt_present("h") {
        print_usage(&program, opts);
    }
    if matches.opt_present("v") {
        print_version();
    }

    let _command = if !matches.free.is_empty() {
        matches.free[0].clone()
    } else {
        String::from("watch")
    };

    let path: PathBuf = match matches.opt_str("path") {
        None => env::current_dir().unwrap(),
        Some(p) => PathBuf::from(p)
    };

    let ef_client = match build_client(&matches) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error while creating client: {}", e);
            exit(-1);
        }
    };

//    Fuck this shit
//    This looks like shit, but it works for now
//    Im sorry

    let plugin_type = guess_plugin_type(&path);

    match plugin_type {
        Ok(PluginType::PluginWizard) => {
            let updater = PluginWizard::build(&path, ef_client).unwrap();
            watch(&path, &updater).unwrap();
        },
        Ok(PluginType::Gradle) => {
            let updater = PluginGradle::build(&path, ef_client).unwrap();
            watch(&path, &updater).unwrap();
        },
        Err(e) => {
            eprintln!("Cannot deduce plugin type: {}", e);
            exit(1);
        }
    };

//    println!("{:?}", result)


//    let result = PluginWizard::build(&path, ef_client.clone());
//    if result.is_ok() {
//        let updater = result.unwrap();
//        if let Err(e) = watch(&path, &updater) {
//            eprintln!("Error: {:?}", e);
//        }
//    }
//    else {
//        eprintln!("Cannot build Plugin Wizard updater: {}", result.err().unwrap());
//        let result = PluginGradle::build(&path, ef_client);
//        if result.is_ok() {
//            let updater = result.unwrap();
//
//            if let Err(e) = watch(&path, &updater) {
//                eprintln!("Error: {:?}", e);
//            }
//        }
//        else {
//            eprintln!("Cannot build gradle updater: {}", result.err().unwrap());
//            exit(-1);
//        }
//    }

}


fn build_client(matches: &getopts::Matches) -> Result<EFClient, Box<Error>> {
    let server = matches.opt_str(SERVER).expect("Server must be provided");
    let username = matches.opt_str(USERNAME);
    let password = matches.opt_str(PASSWORD);
    let mut sid = matches.opt_str(SID);

    if sid.is_none() && (username.is_none() || password.is_none()) {
        sid = read_sid(&server);
    }

    let client = EFClient::new(&server,
                               username.as_ref().map(|x| &**x),
                               password.as_ref().map(|x| &**x),
                               sid.as_ref().map(|x| &**x));
    match client {
        Ok(c) => Ok(c),
        Err(e) => Err(Box::new(e))
    }
}

fn watch<T>(path: &PathBuf, plugin: &T) -> notify::Result<()> where T: PartialUpdate {
    let (tx, rx) = channel();
    let mut watcher: RecommendedWatcher = Watcher::new(tx, Duration::from_secs(1))?;
    watcher.watch(path, RecursiveMode::Recursive)?;

    println!("Started to watch {}", path.to_str().unwrap());
    loop {
        match rx.recv() {
            Ok(DebouncedEvent::Create(path)) => {
                println!("Created {}", path.to_str().unwrap());
                let result = plugin.update(&path);
                if result.is_err() {
                    eprintln!("Error while updating: {}", result.err().unwrap());
                }
            },
            Ok(DebouncedEvent::Write(path)) => {
                println!("Write: {:?}", path);
                let result = plugin.update(&path);
                if result.is_err() {
                    eprintln!("Error while updating: {}", result.err().unwrap());
                }
            },
            Ok(_) => {
            },
            Err(error) => {
                println!("Watch error: {:?}", error);
            }
        }
    }
}

