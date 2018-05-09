use reqwest::{ClientBuilder, Method, Client};
use reqwest::header::{Headers, ContentType};
use std::io::{Error,ErrorKind};
use std::io::Read;
use std::collections::HashMap;
use url::percent_encoding::{utf8_percent_encode, DEFAULT_ENCODE_SET};
use serde_json;

#[derive(Debug)]
pub struct EFClient {
    server: String,
    username: String,
    password: String,
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
    pub fn new(server: &str, username: &str, password: &str) -> EFClient {
        let client = ClientBuilder::new()
            .danger_disable_certificate_validation_entirely()
            .build()
            .unwrap();
        let port = String::from("443");

        EFClient {
            server: String::from(server),
            username: String::from(username),
            password: String::from(password),
            client,
            port
        }
    }

    pub fn set_port(&mut self, port: &str) {
        self.port = String::from(port);
    }


    fn request_json<'a>(&self, uri: &'a str, method: Method, payload: Option<&'a HashMap<&str, &str>>) -> Result<String, Error> {
        let url= format!("https://{}:{}/rest/v1.0/{}", &self.server, &self.port, uri);
        let mut req = self.client.request(method, &url);
        req.basic_auth(
            self.username.clone(),
            Some(self.password.clone())
        );
        let mut headers = Headers::new();
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

