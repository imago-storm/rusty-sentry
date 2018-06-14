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
pub enum PluginType {
    PluginWizard,
    Gradle,
}

#[derive(Debug)]
pub struct Updater {
    pub plugin_name: String,
    pub version: String,
    pub plugin_key: String,
    pub path: PathBuf,
    ef_client: EFClient,
    plugin_type: PluginType,
}

#[derive(Debug, Deserialize)]
struct Plugin {
    key: String,
    version: String,
    plugin_type: PluginType
}

#[derive(Debug, Deserialize)]
struct PluginMETAINF {
    key: String,
    version: String
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

#[derive(Debug)]
pub struct PluginMeta {
    key: String,
    version: String,
    folder: PathBuf,
}

#[derive(Debug)]
pub struct PluginWizard {
    meta: PluginMeta,
    ef_client: EFClient,
}

#[derive(Debug)]
pub struct PluginGradle {
    meta: PluginMeta,
    manifest_path: PathBuf,
    ef_client: EFClient,
}

pub trait PartialUpdate {
    type PluginType;
    fn update(&self, file: &PathBuf) -> Result<(), Error>;
    fn build(plugin_folder: &PathBuf, ef_client: EFClient) -> Result<Self::PluginType, Error>;

    fn get_file_content(&self, path: &Path, meta: &PluginMeta) -> Result<String, Error> {
        let res = File::open(path);
        let mut f: File;
        match res {
            Ok(file) => f = file,
            Err(e) => {
                let err = format!("Cannot open {}: {}", path.to_str().unwrap(), e);
                return Err(Error::new(ErrorKind::Other, err));
            }
        };
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        let plugin_name = format!("{}-{}", &meta.key, &meta.version);
        contents = contents.replace("@PLUGIN_NAME@", &plugin_name);
        contents = contents.replace("@PLUGIN_VERSION@", &meta.version);
        contents = contents.replace("@PLUGIN_KEY@", &meta.key);
        Ok(contents)
    }
}

impl PartialUpdate for PluginGradle {
    type PluginType = PluginGradle;

    fn update(&self, path: &PathBuf) -> Result<(), Error> {
        let manifest = self.read_manifest()?;
        let xpath = self.find_xpath(path, &manifest);
        if xpath.is_some() {
            let xpath = xpath.unwrap();
            self.update_by_xpath(&xpath, path)?;
        }
        Ok(())
    }

    fn build(folder: &PathBuf, ef_client: EFClient) -> Result<Self::PluginType, Error> {
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
        let metadata = PluginMeta {
            key: String::from(plugin_name),
            version: String::from(version),
            folder: folder.clone(),
        };
        let manifest_path = folder.join("src").join("main").join("resources").join("project").join("manifest.xml");

        Ok(PluginGradle {
            meta: metadata,
            manifest_path,
            ef_client,
        })
    }
}

impl PluginGradle {
    fn read_manifest(&self) -> Result<Manifest, Error> {
        let mut f = File::open(&self.manifest_path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        let manifest: Result<Manifest, serde_xml_rs::Error> = deserialize(contents.as_bytes());
        match manifest {
            Ok(m) => Ok(m),
            Err(e) => Err(Error::new(ErrorKind::Other, format!("Cannot parse manifest: {}", e)))
        }
    }

    fn find_xpath(&self, path: &PathBuf, manifest: &Manifest) -> Option<String> {
        for file in manifest.fileset.iter() {
            if path.ends_with(&file.path) {
                let xpath = file.xpath.clone();
                return Some(xpath);
            }
        }
        None
    }

    fn update_by_xpath(&self, xpath: &str, file_path: &PathBuf) -> Result<(), Error> {
        let tokens = xpath.split("/");
        let mut path = Vec::new();
        let mut procedure_name: Option<&str> = None;
        let mut step_name: Option<&str> = None;
        for token in tokens {
            if token.starts_with("property") {
                let re = Regex::new("propertyName=[\"'](.+?)[\"']").unwrap();
                let caps = re.captures(token);
                if caps.is_some() {
                    let property_name = caps.unwrap().get(1).unwrap().as_str();
                    path.push(property_name);
                }
            } else if token.starts_with("propertySheet") {
                println!("Property sheet");
            } else if token.starts_with("procedure") {
                println!("Procedure: {}", token);
                let re = Regex::new("procedureName=[\"'](\\w+)[\"']").unwrap();
                let caps = re.captures(token);
                if caps.is_some() {
                    procedure_name = Some(caps.unwrap().get(1).unwrap().as_str());
                }
            } else if token.starts_with("step") {
                println!("Step: {}", token);
                let re = Regex::new("stepName=[\"'](\\w+)[\"']").unwrap();
                let caps = re.captures(token);
                if caps.is_some() {
                    step_name = Some(caps.unwrap().get(1).unwrap().as_str());
                }
            }
        }
        let value = &self.get_file_content(file_path, &self.meta)?;
        let plugin_name = &self.meta.key;
        if procedure_name == None {
            let property_name = format!("/plugins/{}/project/{}", plugin_name, path.join("/"));
            println!("Property name: {}", property_name);
            self.ef_client.set_property(&property_name, &value)?;
            return Ok(());
        } else {
            if step_name == None {
                let property_name = format!("/plugins/{}/project/procedures/{}/{}", plugin_name, procedure_name.unwrap(), path.join("/"));
                println!("Property name: {}", property_name);
                self.ef_client.set_property(&property_name, &value)?;
                return Ok(());
            } else {
                println!("Command update is not supported yet: {:?}, {:?}", procedure_name, step_name);
            }
        }
        Ok(())
    }
}

impl PartialUpdate for PluginWizard {
    type PluginType = PluginWizard;

    fn update(&self, path: &PathBuf) -> Result<(), Error> {
        let path_str = path.to_str().unwrap();
        if self.is_property(path_str) {
            println!("{} is a property!", path_str);
            self.update_property(path)?;
        } else if self.is_step_code(path_str) {
            println!("{} is a step command, pls rebuild", path_str);
        } else if self.is_form_xml(path_str) {
            println!("{} is a form.xml, pls rebuild", path_str);
        }
        Ok(())
    }

    fn build(folder: &PathBuf, ef_client: EFClient) -> Result<Self::PluginType, Error> {
        let metadata_path = folder.join("META-INF").join("plugin.xml");
        println!("Trying {}", metadata_path.to_str().unwrap());
        let mut f = File::open(&metadata_path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        println!("Contents: {}", contents);
        let plugin: Result<PluginMETAINF, serde_xml_rs::Error> = deserialize(contents.as_bytes());
        match plugin {
            Ok(p) => {
                let metadata = PluginMeta{
                    key: p.key,
                    version: p.version,
                    folder: folder.clone(),
                };
                Ok(PluginWizard{
                    meta: metadata,
                    ef_client
                })
            }
            Err(error) => Err(Error::new(ErrorKind::Other, format!("Cannot parse {}: {}", metadata_path.display(), error)))
        }
    }

}

impl PluginWizard {
    fn update_property(&self, path: &PathBuf) -> Result<(), Error> {
        if !path.is_absolute() {
            return Err(Error::new(ErrorKind::Other, "Path should be absolute!"));
        }
        let value = self.get_file_content(path, &self.meta)?;
        let prefix = self.meta.folder.join("dsl").join("properties");
        let path = path.strip_prefix(&prefix).unwrap();
        let re = Regex::new("\\..+$").unwrap();
        let mut property_name: String = String::from(re.replace_all(path.to_str().unwrap(), ""));
        let plugin_name = &self.meta.key;
        property_name = format!("/plugins/{}/project/{}", plugin_name, property_name);
        println!("Property name: {}", property_name);
        self.ef_client.set_property(&property_name, &value)?;
        Ok(())
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
}


pub fn guess_plugin_type(path: &PathBuf) -> Result<PluginType, Error> {
    let plugin_manifest = path.join("META-INF").join("plugin.xml");
    if plugin_manifest.exists() {
        return Ok(PluginType::PluginWizard);
    }
    let build_gradle = path.join("build.gradle");
    if build_gradle.exists() {
        return Ok(PluginType::Gradle);
    }
    Err(Error::new(ErrorKind::Other, format!("Cannot determine plugin type: {}", path.display())))
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
            plugin_type: plugin.plugin_type,
        };
        Ok(updater)
    }

    fn read_metadata(folder: &PathBuf) -> Result<Plugin, Error> {
        let result = Self::read_plugin_wizard_metadata(folder);
        if result.is_ok() {
            return result;
        }
        else {
            println!("Cannot read plugin.xml: {}", result.err().unwrap());
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
        Ok(Plugin {
            key: String::from(plugin_name),
            version: String::from(version),
            plugin_type: PluginType::Gradle,
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
//                plugin.plugin_type = PluginType::PluginWizard;
                Ok(plugin)
            }
            Err(error) => Err(Error::new(ErrorKind::Other, format!("Cannot parse plugin.xml: {}", error))),
        }
    }


    pub fn update(&self, path: &PathBuf) {
        let path_str = path.to_str().unwrap();
        if self.plugin_type == PluginType::PluginWizard {
            self.update_plugin_wizard(path);
        } else {
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
            } else {
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
            } else if token.starts_with("propertySheet") {
                println!("Property sheet");
            } else if token.starts_with("procedure") {
                println!("Procedure: {}", token);
                let re = Regex::new("procedureName=[\"'](\\w+)[\"']").unwrap();
                let caps = re.captures(token);
                if caps.is_some() {
                    procedure_name = Some(caps.unwrap().get(1).unwrap().as_str());
                }
            } else if token.starts_with("step") {
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
        } else {
            if step_name == None {
                let property_name = format!("/plugins/{}/project/procedures/{}/{}", self.plugin_key, procedure_name.unwrap(), path.join("/"));
                println!("Property name: {}", property_name);
                self.ef_client.set_property(&property_name, &value);
            } else {
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
        } else if self.is_step_code(path_str) {
            println!("{} is a step command, pls rebuild", path_str);
        } else if self.is_form_xml(path_str) {
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
            return Err(Error::new(ErrorKind::Other, "Path should be absolute!"));
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


#[cfg(test)]
mod tests {
    use super::*;

    fn read_gradle_plugin() -> Result<PluginGradle, Error> {
        let plugin_path = "/Users/imago/Documents/ecloud/plugins/EC-WebLogic";
        let ef_client = EFClient::new("ubuntu-esxi", Some("admin"),
        Some("changeme"), None).unwrap();
        let plugin = PluginGradle::build(
            &PathBuf::from(plugin_path),
            ef_client
        );
        plugin
    }


    #[test]
    fn read_gradle_metadata() {
        let plugin = read_gradle_plugin();
        assert!(plugin.is_ok());
        assert_eq!(plugin.unwrap().meta.version, "3.3.0");
    }

//    #[test]
//    fn test_update_file() {
//        let plugin = read_gradle_plugin();
//        let plugin = plugin.unwrap();
//        let file = plugin.meta.folder.join("src").join("main").join("resources").join("project").join("jython").join("add_server_to_cluster.jython");
//        println!("{:?}", file);
//        plugin.update(&file);
//        assert!(true);
//    }

    #[test]
    fn show_xpath() {
        let plugin = read_gradle_plugin();
        let plugin = plugin.unwrap();
        let file = plugin.meta.folder.join("src").join("main").join("resources").join("project").join("jython").join("add_server_to_cluster.jython");
        let manifest = plugin.read_manifest().unwrap();
        let xpath = plugin.find_xpath(&file, &manifest);
        assert!(xpath.is_some());
        let xpath = xpath.unwrap();
        println!("{}", xpath);
        assert_eq!(xpath, "//property[propertyName=\"jython\"]/propertySheet/property[propertyName=\"add_server_to_cluster.jython\"]/value");
    }

    fn build_ef_client() -> EFClient {
        let ef_client = EFClient::new("ubuntu-esxi", Some("admin"),
                                      Some("changeme"), None).unwrap();
        ef_client
    }

    #[test]
    fn test_plugin_wizard() {
        let plugin_path = "/Users/imago/Documents/ecloud/plugins/containers/EC-Kubernetes";
        let ef_client = EFClient::new("ubuntu-esxi", Some("admin"),
                                      Some("changeme"), None).unwrap();
        let plugin = PluginWizard::build(
            &PathBuf::from(plugin_path),
            ef_client
        );
        println!("{:?}", plugin);

        assert!(plugin.is_ok());
    }


    fn watch_placeholder<T>(plugin: &T) where T: PartialUpdate {
        let path = PathBuf::new();
        let res = plugin.update(&path);
        println!("{:?}", res);
    }

    #[test]
    fn test_watch_placeholder() {
        let plugin_path = "/Users/imago/Documents/ecloud/plugins/containers/EC-Kubernetes";
        let ef_client = EFClient::new("ubuntu-esxi", Some("admin"),
                                      Some("changeme"), None).unwrap();
        let plugin = PluginWizard::build(
            &PathBuf::from(plugin_path),
            ef_client
        ).unwrap();
        watch_placeholder(&plugin);
    }

    #[test]
    fn build_generic_plugin() {
        let plugin_path = PathBuf::from("/Users/imago/Documents/ecloud/plugins/containers/EC-Kubernetes");
        let mut plugin: PartialUpdate;
        let res = PluginWizard::build(plugin_path);
        plugin = res.unwrap();
        assert!(true);
    }


}