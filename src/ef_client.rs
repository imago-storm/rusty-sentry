use reqwest::{ClientBuilder, Method, Client};
use reqwest::header::{Headers, ContentType, Cookie};
use std::io::{Error,ErrorKind};
use std::io::Read;
use std::collections::HashMap;
use url::percent_encoding::{utf8_percent_encode, DEFAULT_ENCODE_SET};
use serde_json;

const PORT: &str = "443";

#[derive(Debug, Clone)]
pub struct EFClient {
    server: String,
    username: Option<String>,
    password: Option<String>,
    sid: Option<String>,
    port: String,
    client: Client,
    debug_level: i8
}

#[derive(Deserialize, Debug)]
struct PropertyResponse {
    property: Property
}

#[derive(Deserialize, Debug, Serialize)]
pub struct Property {
    #[serde(rename="propertyName")]
    property_name: Option<String>,
    value: String,
}

#[derive(Deserialize, Debug) ]
struct PluginResponse {
    plugin: Plugin
}

#[derive(Deserialize, Debug)]
pub struct Plugin {
    #[serde(rename="pluginName")]
    pub plugin_name: String,
    #[serde(rename="pluginVersion")]
    pub plugin_version: String
}

impl EFClient {
    pub fn new(server: &str, username: Option<&str>, password: Option<&str>, sid: Option<&str>) -> Result<EFClient, Error> {
        let client = match ClientBuilder::new()
            .danger_disable_certificate_validation_entirely()
            .build() {
            Ok(c) => Ok(c),
            Err(e) => Err(Error::new(ErrorKind::Other, format!("Cannot create client: {}", e)))
        }?;

        if sid == None && (username == None || password == None) {
            return Err(Error::new(ErrorKind::Other, "Either username & password or sid must be provided"));
        }
        Ok(EFClient {
            server: String::from(server),
            username: username.map(str::to_string),
            password: password.map(str::to_string),
            sid: sid.map(str::to_string),
            client,
            port: String::from(PORT),
            debug_level: 0
        })
    }

    fn debug(&self, message: &str) {
        if self.debug_level > 0 {
            println!("[DEBUG] {}", message);
        }
    }

    pub fn set_debug_level(&mut self, level: i8) {
        self.debug_level = level;
    }

    pub fn set_port(&mut self, port: &str) {
        self.port = String::from(port);
    }


    fn request_json<'a>(&self, uri: &'a str, method: Method, payload: Option<&'a HashMap<&str, &str>>) -> Result<String, Error> {
        let url= format!("https://{}:{}/rest/v1.0/{}", &self.server, &self.port, uri);
        let mut req = self.client.request(method, &url);

        let mut headers = Headers::new();
//        Auth
        if self.username != None && self.password != None {
            let username = self.username.clone().unwrap();
            req.basic_auth(
                username,
                self.password.clone()
            );
        }
        else {
            let mut cookie = Cookie::new();
            let sid = self.sid.clone().expect("either username & password or sid must be provided");
            cookie.append("sessionId", sid);
            headers.set(cookie);
        }

        headers.set(ContentType::json());
        req.headers(headers);
        match payload {
            Some(body) => {
                self.debug(&format!("Body: {:?}", body));
                req.json(&body.clone());
            },
            None => {},
        };
        let mut res = req.send().expect("Error happened while sending a request");
        if res.status().is_success() {
            let mut body: String = String::new();
            let _ = res.read_to_string(&mut body)?;
            return Ok(body);
        }
        else {
            let mut body: String = String::new();
            res.read_to_string(&mut body)?;
            return Err(Error::new(ErrorKind::Other, format!("Request failed: status code {}, body: {}", res.status(), body)))
        }
    }

    pub fn set_property(&self, name: &str, value: &str) -> Result<Property, Error> {
        let uri = format!("properties/{}", utf8_percent_encode(name, DEFAULT_ENCODE_SET).to_string());
        let mut payload = HashMap::new();
        payload.insert("value", value);
        let res = &self.request_json(&uri, Method::Put, Some(&payload))?;
        self.debug(&format!("API Response: {:?}", res));
        let property: PropertyResponse = serde_json::from_str(&res)?;
        Ok(property.property)
    }

    pub fn set_procedure_command(&self, project_name: &str, procedure_name: &str, step_name: &str, command: &str) -> Result<(), Error> {
        let uri = format!("projects/{}/procedures/{}/steps/{}", project_name, procedure_name, step_name);
        let mut payload = HashMap::new();
        payload.insert("command", command);
        let _res = &self.request_json(&uri, Method::Put, Some(&payload))?;
        Ok(())
    }

    pub fn get_plugin(&self, plugin_name: &str) -> Result<Plugin, Error> {
        let uri  = format!("plugins/{}", plugin_name);
        let res = &self.request_json(&uri, Method::Get, None)?;
        let plugin: PluginResponse = serde_json::from_str(&res)?;
        Ok(plugin.plugin)
    }


    pub fn get_property(&self, name: &str) -> Result<Property, Error> {
        let uri = format!("properties/{}", utf8_percent_encode(name, DEFAULT_ENCODE_SET).to_string());
        let res = &self.request_json(&uri, Method::Get, None)?;
        let property: PropertyResponse = serde_json::from_str(&res)?;
        Ok(property.property)
    }

    pub fn status(&self) -> () {
        let uri = "server/status";
        let res = &self.request_json(&uri, Method::Get, None);
        self.debug(&format!("{:?}", res));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_client() -> EFClient {
        let ef_client = EFClient::new("vivarium", Some("admin"),
                                      Some("changeme"), None).unwrap();
        ef_client
    }

    #[test]
    fn get_plugin_test() {
        let client = build_client();
        let plugin = client.get_plugin("EC-OpenShift");
        println!("{:?}", plugin);
        assert!(plugin.is_ok());
    }

    #[test]
    fn set_procedure_command() {
        let client = build_client();
        let plugin = client.get_plugin("EC-OpenShift").unwrap();
        let project_name = plugin.plugin_name;
        let result = client.set_procedure_command(&project_name, "Discover", "discover", "test");
        assert!(result.is_ok());
    }
}