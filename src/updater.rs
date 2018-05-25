use std::path::{Path, PathBuf};
use regex::{Regex, RegexBuilder};
use std::collections::HashMap;
use std::path;
use std::io::{Error, ErrorKind};
use std::fs::File;
use std::io::prelude::*;
use serde_xml_rs::deserialize;
use ef_client::EFClient;
use serde_xml_rs;

#[derive(Debug, Deserialize, PartialEq)]
enum PluginType {
    PluginWizard,
    Gradle
}

#[derive(Debug)]
pub struct Updater {
    pub plugin_name: String,
    pub version: String,
    pub plugin_key: String,
    pub path: PathBuf,
    ef_client: EFClient,
    plugin_type: PluginType
}

#[derive(Debug, Deserialize)]
struct Plugin {
    key: String,
    version: String,
    plugin_type: PluginType
}

struct Gradle {
    key: String,
    version: String,
    manifest_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename = "fileset")]
struct Manifest {
    #[serde(rename = "file")]
    fileset: Vec<ManifestFile>
}

#[derive(Debug, Deserialize)]
#[serde(rename = "file")]
struct ManifestFile {
    path: PathBuf,
    xpath: String,
}

//TODO traits

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
            plugin_type: plugin.plugin_type
        };
        Ok(updater)
    }

    fn read_metadata(folder: &PathBuf) -> Result<Plugin, Error> {
        let result = Self::read_plugin_wizard_metadata(folder);
        if result.is_ok() {
            return result;
        }
        let r = Self::read_gradle_metadata(folder);
        r
    }

    fn read_gradle_metadata(folder: &PathBuf) -> Result<Plugin, Error> {
        println!("Reading gradle metadata\n");
        let mut gradle_path = folder.clone();
        gradle_path.push("build.gradle");
        let mut f = File::open(gradle_path)?;
        let mut gradle_contents = String::new();
        f.read_to_string(&mut gradle_contents)?;
        let reg = Regex::new("version\\s*=\\s*[\"']([\\d\\.]+)[\"']").unwrap();
        let caps = reg.captures(&gradle_contents);
        let mut version = "";
        if caps.is_some() {
            version = caps.unwrap().get(1).unwrap().as_str();
        }
        let name_re = Regex::new("description\\s*=\\s*['\"]Plugins\\s*:\\s*([\\w-]+)").unwrap();
        let caps = name_re.captures(&gradle_contents);
        let mut plugin_name = "";
        if caps.is_some() {
            plugin_name = caps.unwrap().get(1).unwrap().as_str();
        }
        Ok(Plugin{
            key: String::from(plugin_name),
            version: String::from(version),
            plugin_type: PluginType::Gradle
        })
    }

    fn read_plugin_wizard_metadata(folder: &PathBuf) -> Result<Plugin, Error> {
        let mut metadata_path = folder.clone();
        metadata_path.push("META-INF");
        metadata_path.push("plugin.xml");
        println!("Trying {}", metadata_path.to_str().unwrap());
        let mut f = File::open(metadata_path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        let plugin: Result<Plugin, serde_xml_rs::Error> = deserialize(contents.as_bytes());
        match plugin {
            Ok(mut plugin) => {
                plugin.plugin_type = PluginType::PluginWizard;
                Ok(plugin)
            },
            Err(error) => Err(Error::new(ErrorKind::Other, "Cannot parse plugin.xml")),
        }
    }


    pub fn update(&self, path: &PathBuf) {
        let path_str = path.to_str().unwrap();
        if self.plugin_type == PluginType::PluginWizard {
            self.update_plugin_wizard(path);
        }
        else {
            let manifest = self.read_manifest();
            if manifest.is_ok() {
                println!("Scanned manifest");
                let manifest = manifest.unwrap();
                for file in manifest.fileset {
                    if path.ends_with(file.path) {
                        println!("Xpath: {}", file.xpath);
                        self.update_by_xpath(&file.xpath, path);
                    }
                }
            }
            else {
                println!("Cannot read manifest: {}", manifest.unwrap_err());
            }
        }
    }

    fn update_by_xpath(&self, xpath: &str, file_path: &PathBuf) {
        let tokens = xpath.split("/");
        let mut path = Vec::new();
        let mut procedure_name: Option<&str> = None;
        let mut step_name: Option<&str> = None;
        for token in tokens {
            if token.starts_with("property") {
//                get property name
                println!("Property: {}", token);
                let re = Regex::new("propertyName=[\"'](.+?)[\"']").unwrap();
                println!("{:?}", re.captures(token));
                let caps = re.captures(token);
                if caps.is_some() {
                    let property_name = caps.unwrap().get(1).unwrap().as_str();
                    path.push(property_name);
                }
            }
            else if token.starts_with("propertySheet") {
                println!("Property sheet");
            }
            else if token.starts_with("procedure") {
                println!("Procedure: {}", token);
                let re = Regex::new("procedureName=[\"'](\\w+)[\"']").unwrap();
                let caps = re.captures(token);
                if caps.is_some() {
                    procedure_name = Some(caps.unwrap().get(1).unwrap().as_str());
                }
            }
            else if token.starts_with("step") {
                println!("Step: {}", token);
                let re = Regex::new("stepName=[\"'](\\w+)[\"']").unwrap();
                let caps = re.captures(token);
                if caps.is_some() {
                    step_name = Some(caps.unwrap().get(1).unwrap().as_str());
                }
            }
        }
        let value = &self.get_file_content(file_path).expect("Cannot read file content");
        if procedure_name == None {
            let property_name = format!("/plugins/{}/project/{}", self.plugin_key, path.join("/"));
            println!("Property name: {}", property_name);
            self.ef_client.set_property(&property_name, &value);
        }
        else {
            if step_name == None {
                let property_name = format!("/plugins/{}/project/procedures/{}/{}", self.plugin_key, procedure_name.unwrap(), path.join("/"));
                println!("Property name: {}", property_name);
                self.ef_client.set_property(&property_name, &value);
            }
            else {
                println!("Command update is not supported yet: {:?}, {:?}", procedure_name, step_name);
            }
        }
    }

    fn update_plugin_wizard(&self, path: &PathBuf) {
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

    fn read_manifest(&self) -> Result<Manifest, Error> {
        let mut manifest_path = self.path.clone();
        manifest_path.push("src");
        manifest_path.push("main");
        manifest_path.push("resources");
        manifest_path.push("project");
        manifest_path.push("manifest.xml");
        println!("Manifest: {}", manifest_path.display());
        let mut f = File::open(manifest_path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        let manifest: Result<Manifest, serde_xml_rs::Error> = deserialize(contents.as_bytes());
        match manifest {
            Ok(m) => Ok(m),
            Err(e) => Err(Error::new(ErrorKind::Other, format!("Cannot parse manifest: {}", e)))
        }
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
