#![allow(unused)]

use lazy_static::lazy_static;
use serde::de::DeserializeOwned;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{HeaderMap, HeaderValue, self};
use reqwest::blocking::Client;
use reqwest::{Method, Url};
use std::sync::RwLock;
use serde::Deserialize;

use crate::errors::RobloxAPIResponseErrors;

lazy_static! {
    pub static ref HTTP: RwLock<HttpClient> = {
        let client = HttpClient::new();
        RwLock::new(client)
    };
}

pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Option<HeaderMap>,
    pub body: Option<String>
}

pub struct HttpClient {
    pub client: Client
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self { client: Client::new() }
    }
}

pub trait HttpClientExt {
    fn set_cookie(&self, cookie: &str) -> Result<(), &str>;
    fn remove_cookie(&self);
    fn request<T>(&self, data: HttpRequest) -> Result<T, String>
        where T: DeserializeOwned;
}

impl HttpClientExt for RwLock<HttpClient> {
    fn set_cookie(&self, cookie: &str) -> Result<(), &str> {
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, HeaderValue::from_str(cookie).unwrap());

        let res = Client::new()
            .post("https://auth.roblox.com/v2/logout")
            .body("")
            .headers(headers.clone())
            .send()
            .unwrap();

        if !res.status().is_success() && res.status().as_u16() != 403 {
            return Err("Invalid cookie");
        }

        let csrf = res.headers().get("x-csrf-token");

        if csrf.is_none() {
            return Err("Failed to fetch X-CSRF-TOKEN");
        }

        headers.insert("X-CSRF-TOKEN", csrf.unwrap().to_owned());
        self.write().expect("Failed to modify HTTP client").client = Client::builder()
            .default_headers(headers)
            .cookie_store(true)
            .build()
            .expect("Failed to build HTTP client");

        Ok(())
    }

    fn remove_cookie(&self) {
        self.write().expect("Failed to modify HTTP client").client = Client::new();
    }

    fn request<T>(&self, data: HttpRequest) -> Result<T, String>
        where T: DeserializeOwned
    {
        let res = self
            .read()
            .expect("Failed to read HTTP client")
            .client
            .request(data.method, format!("https://{}", data.url))
            .body(data.body.unwrap_or_default())
            .headers(data.headers.unwrap_or_default())
            .send();

        match res {
            Ok(res) => {
                let status = res.status();

                if status.is_success() {
                    let body = res.json::<T>();
                    match body {
                        Ok(body) => Ok(body),
                        Err(err) => Err(err.to_string()),
                    }
                } else {
                    let body = res.json::<RobloxAPIResponseErrors>();
                    match body {
                        Ok(body) => {
                            let errors = body.errors;
                            let error = errors
                                .first()
                                .expect("Unknown error");

                            Err(error.message.to_string())
                        }
                        Err(_) => Err(status.to_string())
                    }
                }
            },
            Err(err) => Err(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tokio_test::{assert_err, assert_ok};
    use super::*;

    const ENDPOINT_GET: &str = "httpbin.org/get";
    const ENDPOINT_404: &str = "httpbin.org/status/404";
    const ENDPOINT_ROBLOX: &str = "users.roblox.com/v1/users/0"; // Intentionally invalid user ID

    #[test]
    fn ok_req() {
        let req = HttpRequest {
            method: Method::GET,
            url: ENDPOINT_GET.to_string(),
            headers: None,
            body: None
        };

        let res = HTTP.request::<Value>(req);
        assert_ok!(res);
    }

    #[test]
    fn err_req() {
        let req = HttpRequest {
            method: Method::GET,
            url: ENDPOINT_404.to_string(),
            headers: None,
            body: None
        };

        let res = HTTP.request::<Value>(req);
        assert_err!(res);
    }

    #[test]
    fn roblox_err() {
        let req = HttpRequest {
            method: Method::GET,
            url: ENDPOINT_ROBLOX.to_string(),
            headers: None,
            body: None
        };

        let res = HTTP.request::<String>(req);

        assert_err!(&res);
        assert_eq!(res.unwrap_err(), "The user id is invalid.");
    }
}