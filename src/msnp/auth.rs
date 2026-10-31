//! Escargot's Passport-compatible ticket exchange.
//!
//! Observed live flow (October 2026):
//!   GET https://msnmsgr.escargot.chat/rdr/pprdr.asp
//!   -> PassportURLs: DALogin=https://ds.escargot.nina.chat/login
//!   GET DALogin with Authorization: Passport1.4 sign-in=...,pwd=...
//!   -> Authentication-Info: Passport1.4 da-status=success,from-PP='t=...'
//! The 20 character token is then sent as `USR <trid> TWN S t=<token>`.

use std::fmt;
use std::time::Duration;

use urlencoding::encode;

const USER_AGENT: &str = "MSN Messenger/7.5.0324";

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

pub trait HttpTransport: Send + Sync {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, AuthError>;
}

impl<T: HttpTransport + ?Sized> HttpTransport for std::sync::Arc<T> {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, AuthError> {
        (**self).get(url, headers)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("Incorrect email or password.")]
    InvalidCredentials,
    #[error("The sign-in server did not answer correctly: {0}")]
    Protocol(String),
    #[error("Could not reach the sign-in server: {0}")]
    Network(String),
}

pub struct PassportAuth {
    login_host: String,
    transport: Box<dyn HttpTransport>,
}

impl PassportAuth {
    pub fn new(login_host: impl Into<String>) -> Self {
        Self { login_host: login_host.into(), transport: Box::new(UreqTransport) }
    }

    pub fn with_transport(login_host: impl Into<String>, transport: Box<dyn HttpTransport>) -> Self {
        Self { login_host: login_host.into(), transport }
    }

    pub fn fetch_ticket(&self, email: &str, password: &str) -> Result<String, AuthError> {
        let login_host = self.login_host.trim_end_matches('/');
        let nexus = format!("{login_host}/rdr/pprdr.asp");
        let response = self.transport.get(&nexus, &[("User-Agent", USER_AGENT)])?;

        if response.status != 200 {
            return Err(AuthError::Protocol(format!(
                "Nexus returned HTTP {}",
                response.status
            )));
        }

        let passport_urls = response
            .header("PassportURLs")
            .ok_or_else(|| AuthError::Protocol("Nexus response had no PassportURLs header".into()))?;
        let urls = parse_passport_urls(passport_urls);
        let dalogin = urls
            .get("DALogin")
            .ok_or_else(|| AuthError::Protocol("PassportURLs had no DALogin entry".into()))?;

        let authorization = build_authorization_header(email, password);
        let response = self.transport.get(
            dalogin,
            &[("User-Agent", USER_AGENT), ("Authorization", &authorization)],
        )?;

        if response.status == 401 {
            return Err(AuthError::InvalidCredentials);
        }
        if response.status != 200 {
            return Err(AuthError::Protocol(format!(
                "Passport returned HTTP {}",
                response.status
            )));
        }

        let auth_info = response.header("Authentication-Info").ok_or_else(|| {
            AuthError::Protocol("Passport response had no Authentication-Info header".into())
        })?;

        extract_ticket(auth_info)
    }
}

pub fn parse_passport_urls(value: &str) -> std::collections::HashMap<String, String> {
    value
        .split(',')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

pub fn build_authorization_header(email: &str, password: &str) -> String {
    format!(
        "Passport1.4 OrgVerb=GET,OrgURL=http%3A%2F%2Fmessenger.msn.com,sign-in={},pwd={},lc=1033,id=507",
        encode(email),
        encode(password)
    )
}

pub fn extract_ticket(auth_info: &str) -> Result<String, AuthError> {
    let start = auth_info
        .find("from-PP='")
        .map(|index| index + "from-PP='".len())
        .or_else(|| auth_info.find("t="))
        .ok_or_else(|| AuthError::Protocol("Authentication-Info had no ticket".into()))?;

    let rest = &auth_info[start..];
    let token = rest.split('\'').next().unwrap_or(rest).trim();
    let token = token.strip_prefix("t=").unwrap_or(token);

    if token.len() < 8 {
        return Err(AuthError::Protocol("Authentication-Info ticket was too short".into()));
    }

    Ok(token.to_string())
}

struct UreqTransport;

impl HttpTransport for UreqTransport {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<HttpResponse, AuthError> {
        let mut request = ureq::get(url).timeout(Duration::from_secs(25));
        for (name, value) in headers {
            request = request.set(name, value);
        }

        match request.call() {
            Ok(response) => Ok(to_response(response.status(), &response)),
            Err(ureq::Error::Status(status, response)) => Ok(to_response(status, &response)),
            Err(error) => Err(AuthError::Network(error.to_string())),
        }
    }
}

fn to_response(status: u16, response: &ureq::Response) -> HttpResponse {
    let headers = response
        .headers_names()
        .into_iter()
        .filter_map(|name| response.header(&name).map(|value| (name, value.to_string())))
        .collect();
    HttpResponse { status, headers, body: Vec::new() }
}

impl fmt::Debug for PassportAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PassportAuth").field("login_host", &self.login_host).finish_non_exhaustive()
    }
}
