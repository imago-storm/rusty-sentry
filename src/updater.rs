use std::path::{Path, PathBuf};
use regex::Regex;
use std::collections::HashMap;
use std::path;
use std::io::{Error, ErrorKind};
use std::fs;
use std::time::SystemTime;
use std::fs::File;
use std::io::prelude::*;
use serde_xml_rs::deserialize;
use ef_client::EFClient;
use serde_xml_rs;

#[derive(Debug)]
pub struct Updater {
    pub plugin_name: String,
    pub version: String,
    pub plugin_key: String,
    pub path: PathBuf,
    ef_client: EFClient,
}

#[derive(Debug, Deserialize)]
struct Plugin {
    key: String,
    version: String
}

impl Updater {
    pub fn new(folder: &PathBuf, ef_client: EFClient) -> Result<Updater, Error> {
        let plugin = Self::read_metadata(folder)?;
        println!("Found plugin: {}, version {}", plugin.key, plugin.version);
        let updater = Updater {
            plugin_name: format!("{}-{}", plugin.key, plugin.version),
            plugin_key: String::from(plugin.key),
            version: String::from(plugin.version),
            path: folder.clone(),
            ef_client,
        };
        Ok(updater)
    }

    fn read_metadata(folder: &PathBuf) -> Result<Plugin, Error> {
        let mut metadata_path = folder.clone();
        metadata_path.push("META-INF");
        metadata_path.push("plugin.xml");
        println!("Trying {}", metadata_path.to_str().unwrap());
        let mut f = File::open(metadata_path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        let plugin: Result<Plugin, serde_xml_rs::Error> = deserialize(contents.as_bytes());
        match plugin {
            Ok(plugin) => Ok(plugin),
            Err(error) => Err(Error::new(ErrorKind::Other, "Cannot parse plugin.xml")),
        }
    }

    pub fn update(&self, path: &PathBuf) {
        let path_str = path.to_str().unwrap();
        if self.is_property(path_str) {
            println!("{} is a property!", path_str);
            let res = self.update_property(path);
            match res {
                Err(err) => eprintln!("Error: {:?}", err),
                Ok(_) => println!("Processed: {}", path_str)
            };
        }
        else if self.is_step_code(path_str) {
            println!("{} is a step command, pls rebuild", path_str);
        }
        else if self.is_form_xml(path_str) {
            println!("{} is a form.xml, pls rebuild", path_str);
        }
    }


    fn is_step_code(&self, path: &str) -> bool {
        let regexp_str = format!("dsl{}procedures{}[\\w\\s]+{}steps", path::MAIN_SEPARATOR, path::MAIN_SEPARATOR, path::MAIN_SEPARATOR);
        let reg = Regex::new(&regexp_str).unwrap();
        reg.is_match(path)
    }

    fn is_form_xml(&self, path: &str) -> bool {
        Regex::new("form\\.xml$").unwrap().is_match(path)
    }

    fn is_procedure_dsl(&self, path: &str) -> bool {
        Regex::new("procedure\\.dsl$").unwrap().is_match(path)
    }

    fn is_property(&self, path: &str) -> bool {
        let regexp_str = format!("dsl{}properties{}", path::MAIN_SEPARATOR, path::MAIN_SEPARATOR);
        let reg = Regex::new(&regexp_str).unwrap();
        reg.is_match(path)
    }

    fn is_form(&self, path: &Path) -> bool {
        false
    }

    fn update_property(&self, path: &PathBuf) -> Result<(), Error> {
        if !path.is_absolute() {
            return Err(Error::new(ErrorKind::Other, "Path should be absolute!"))
        }
        let value = self.get_file_content(path)?;
        let mut prefix = self.path.clone();
        prefix.push("dsl");
        prefix.push("properties");
        let path = path.strip_prefix(&prefix).unwrap();
        let re = Regex::new("\\..+$").unwrap();
        let mut property_name: String = String::from(re.replace_all(path.to_str().unwrap(), ""));
        property_name = format!("/plugins/{}/project/{}", self.plugin_key, property_name);
        println!("Property name: {}", property_name);
        self.ef_client.set_property(&property_name, &value)?;
        Ok(())
    }


    fn get_file_content(&self, path: &Path) -> Result<String, Error> {
        let res = File::open(path);
        let mut f: File;
        match res {
            Ok(file) => f = file,
            Err(e) => {
                let err = format!("Cannot open {}: {}", path.to_str().unwrap(), e);
                return Err(Error::new(ErrorKind::Other, err));
            }
        };
//        let mut f = File::open(path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        contents = contents.replace("@PLUGIN_NAME@", &self.plugin_name);
        contents = contents.replace("@PLUGIN_VERSION@", &self.version);
        contents = contents.replace("@PLUGIN_KEY@", &self.plugin_key);
        Ok(contents)
    }
}
