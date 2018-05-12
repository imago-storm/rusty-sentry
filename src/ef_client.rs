use reqwest::{ClientBuilder, Method, Client};
use reqwest::header::{Headers, ContentType, Cookie};
use std::io::{Error,ErrorKind};
use std::io::Read;
use std::collections::HashMap;
use url::percent_encoding::{utf8_percent_encode, DEFAULT_ENCODE_SET};
use serde_json;

const PORT: &str = "443";

#[derive(Debug)]
pub struct EFClient {
    server: String,
    username: Option<String>,
    password: Option<String>,
    sid: Option<String>,
    port: String,
    client: Client,
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
        })
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
                println!("{:?}", body);
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
        println!("{:?}", res);
        let property: PropertyResponse = serde_json::from_str(&res)?;
        Ok(property.property)
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
        println!("{:?}", res);
    }
}

