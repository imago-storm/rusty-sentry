use std::path::{Path, PathBuf};
use regex::{Regex, escape};
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
            version: String::from(format!("{}.0", version)),
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
                let re = Regex::new("procedureName=[\"']([\\w\\s]+)[\"']").unwrap();
                let caps = re.captures(token);
                if caps.is_some() {
                    procedure_name = Some(caps.unwrap().get(1).unwrap().as_str());
                }
            } else if token.starts_with("step") {
                println!("Step: {}", token);
                let re = Regex::new("stepName=[\"']([\\w\\s]+)[\"']").unwrap();
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
                let procedure_name = procedure_name.expect("procedure name is not found");
                let step_name = step_name.expect("step name is not found");
                let plugin = self.ef_client.get_plugin(&self.meta.key)?;
                println!("Procedure name: {}, step name: {}", procedure_name, step_name);
                let _res = self.ef_client.set_procedure_command(&plugin.plugin_name, &procedure_name, &step_name, &value)?;
                println!("Updated step");
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
            self.update_step(path)?;
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
        let path = match path.strip_prefix(&prefix) {
            Err(e) => {
                return Err(Error::new(ErrorKind::Other, format!("Cannot strip prefix: {}", e)));
            },
            Ok(path) => path
        };
        let re = Regex::new("\\..+$").unwrap();
        let mut property_name: String = String::from(re.replace_all(path.to_str().expect("Cannot remove file extension from property"), ""));
        let re = Regex::new("\\\\").expect("Cannot compile regexp");
        property_name = String::from(re.replace_all(&property_name, "/"));
        let plugin_name = &self.meta.key;
        property_name = format!("/plugins/{}/project/{}", plugin_name, property_name);
        println!("Property name: {}", property_name);
        self.ef_client.set_property(&property_name, &value)?;
        Ok(())
    }

    fn is_step_code(&self, path: &str) -> bool {
        let sep = escape(&path::MAIN_SEPARATOR.to_string());
        let regexp_str = format!("dsl{}procedures{}[\\w\\s]+{}steps", sep, sep, sep);
        let reg = Regex::new(&regexp_str).unwrap();
        reg.is_match(path)
    }

    fn is_form_xml(&self, path: &str) -> bool {
        Regex::new("form\\.xml$").unwrap().is_match(path)
    }


    fn is_property(&self, path: &str) -> bool {
        let separator = escape(&path::MAIN_SEPARATOR.to_string());
        let regexp_str = format!("dsl{}properties{}", separator, separator);
        let reg = Regex::new(&regexp_str).expect("Failed to compile regexp for property checking");
        reg.is_match(path)
    }

    fn update_step(&self, path: &PathBuf) -> Result<(), Error> {
        let res = self.get_procedure_and_step_name(path);
        match res {
            Ok((procedure_name, step_name)) => {
                println!("Procedure name: {}, step name: {}", procedure_name, step_name);
                let plugin = &self.ef_client.get_plugin(&self.meta.key)?;
                let command = &self.get_file_content(path,&self.meta)?;
                &self.ef_client.set_procedure_command(
                    &plugin.plugin_name,
                    &procedure_name, &step_name,
                    &command);
            },
            Err(e) => {
                eprintln!("Cannot deduce procedure or step name from {}: {}", path.display(), e);
            }
        }
        Ok(())
    }

    pub fn get_procedure_and_step_name(&self, path: &PathBuf) -> Result<(String, String), Error> {
        let path_part = self.meta.folder.join("dsl/procedures");
        let relative_path = path.strip_prefix(&self.meta.folder);
        let path = path.strip_prefix(&path_part);
        if path.is_err() || relative_path.is_err() {
            return Err(Error::new(ErrorKind::Other, "Cannot strip prefix"));
        }
        let relative_path = relative_path.unwrap();
        let path = path.unwrap();
        let procedure_folder_name = path.iter().next();
        if procedure_folder_name .is_none() {
            return Err(Error::new(ErrorKind::Other, "Cannot get procedure folder name"));
        }
        let procedure_folder_name = procedure_folder_name.unwrap();
        let procedure_dsl_path = self.meta.folder.join("dsl/procedures").join(procedure_folder_name).join("procedure.dsl");

        let mut f = File::open(procedure_dsl_path)?;
        let mut contents = String::new();
        f.read_to_string(&mut contents)?;
        let file_name = relative_path.file_name();
        if file_name.is_none() {
            return Err(Error::new(ErrorKind::Other, "Relative path cannot be calculated"));
        }
        let step_name = Self::deduce_step_name(&contents, file_name.unwrap().to_str().unwrap());
        if step_name.is_none() {
            return Err(Error::new(ErrorKind::Other, "Cannot deduce step name"));
        }
        let step_name = step_name.unwrap();

        let procedure_name = Self::deduce_procedure_name(&contents);
        if procedure_name.is_none() {
            return Err(Error::new(ErrorKind::Other, "Cannot deduce procedure name"));
        }
        let procedure_name = procedure_name.unwrap();

        Ok((procedure_name, step_name))
    }


    fn deduce_step_name(fragment: &str, relative_path: &str) -> Option<String> {
        let step_name_re = Regex::new(&relative_path).unwrap();
        let first = step_name_re.split(fragment).next();
        let first = match first {
            Some(f) => f,
            None => return None
        };

        let re = Regex::new("step\\s+['\"]([\\w\\s\\-_]+)['\"]").unwrap();
        let step_name = match re.captures_iter(first).last() {
            Some(caps) => {
                let step_name = caps.iter().last().unwrap().unwrap().as_str();
                Some(String::from(step_name))
            },
            None => return None,
        };
        step_name
    }


    fn deduce_procedure_name(fragment: &str) -> Option<String> {
//        First attempt
        let re = Regex::new("procedure\\s+['\"]([\\w\\s\\-_]+)['\"]").unwrap();
        let caps = re.captures(fragment);
        match caps {
            Some(caps) => {
                let procedure_name = caps.iter().last().unwrap().unwrap().as_str();
                return Some(String::from(procedure_name));
            },
            None => ()
        };

//        Second attempt

        let re = Regex::new("procedure\\s+([\\w_]+)").unwrap();
        let caps = re.captures(fragment);
        let var_name = match caps {
            Some(caps) => {
                let var_name = caps.iter().last().unwrap().unwrap().as_str();
                var_name
            },
            None => {
                return None;
            }
        };

        let re = Regex::new(&format!("{}\\s*=\\s*['\"]([\\w\\s\\-_]+)[\"']", var_name)).unwrap();
        let caps = re.captures(fragment);
        match caps {
            Some(caps) => {
                let procedure_name = caps.iter().last().unwrap().unwrap().as_str();
                return Some(String::from(procedure_name));
            },
            None =>  None
        }
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
//
//        plugin.unwrap().get_procedure_and_step_name(&PathBuf::from("/Users/imago/Documents/ecloud/plugins/containers/EC-Kubernetes/dsl/procedures/checkCluster/steps/checkCluster.groovy"));

    }

    static FRAGMENT1: &str = "\
         procedure 'procName', {\
         step 'stepName', { command = new File('dsl/procedures/procName/steps/stepName.groovy')},\
         step 'another step', command: new File('dsl/procedures/procName/steps/anotherStepName.groovy')";

    static FRAGMENT2: &str = "def procName = 'procedure',\
    procedure procName, {";


    #[test]
    fn deduce_step_name_test() {
        let step_name = PluginWizard::deduce_step_name(FRAGMENT1,
       "dsl/procedures/procName/steps/anotherStepName.groovy");
        assert!(step_name.is_some());
        assert_eq!(step_name.unwrap(), "another step");
    }

    #[test]
    fn deduce_procedure_name_test() {
        let procedure_name = PluginWizard::deduce_procedure_name(FRAGMENT1);
        assert!(procedure_name.is_some());
        assert_eq!(procedure_name.unwrap(), "procName");
    }

    #[test]
    fn deduce_procedure_name_in_var_test() {
        let procedure_name = PluginWizard::deduce_procedure_name(FRAGMENT2);
        assert!(procedure_name.is_some());
        assert_eq!(procedure_name.unwrap(), "procedure");
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


}
