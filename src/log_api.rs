use std::{
    collections::HashMap, io::{Read, Write}, net::{Ipv4Addr, SocketAddrV4, TcpStream}, str::FromStr, sync::{Arc, atomic::AtomicU64}, time::Duration,
};

use egui::Context;
use reqwest::{Client, Error, Method, Request, Url, header::HeaderValue};
use serde::Deserialize;

const CONNECTION_IP: &str = "http://192.168.43.1:5535/";

pub struct LogGrabber {
    cache: HashMap<String, String>,
    list: Option<Vec<FileSpecifier>>
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum DeleteResult {
    Success { success: bool },
    Error { error: String },
}

#[derive(Debug, Deserialize, Clone)]
pub struct FileSpecifier {
    pub name: String,
    pub size: u64
}

impl LogGrabber {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            list: None
        }
    }

    pub fn clear_caches(&mut self) {
        self.cache = HashMap::new();
        self.list = None;
    }
    
    pub async fn list_logs(&self) -> reqwest::Result<Vec<FileSpecifier>> {
        if let Some(list) = &self.list {
            return Ok(list.clone())
        }
        let request = reqwest::get(format!("{}list",CONNECTION_IP)).await?;
        let list = serde_json::from_str::<Vec<FileSpecifier>>(&request.text().await?).expect("invalid json");
        Ok(list)
    }

    pub async fn read_log(&self, log_name: impl Into<String>) -> reqwest::Result<String> {
        let log_name = log_name.into();
        if let Some(contents) = self.cache.get(&log_name) {
            return Ok(contents.clone());
        }
        let mut request = Request::new(Method::GET, Url::from_str(&format!("{}read",CONNECTION_IP)).expect("?????"));
        request.headers_mut().insert("target", HeaderValue::from_bytes(log_name.as_bytes()).expect("invalid bytes?"));
        let stream = Client::new().execute(request).await?;
        stream.text().await
    }

    pub fn cache_log(&mut self, log_name: impl Into<String>, data: impl Into<String>) {
        self.cache.insert(log_name.into(), data.into());
    }

    pub fn cache_list(&mut self, list: Vec<FileSpecifier>, data: String) {
        self.list = Some(list);
    }

    pub async fn delete_log(&self, log_name: impl Into<String>) -> reqwest::Result<bool> {
        let mut request = Request::new(Method::GET, Url::from_str(&format!("{}delete",CONNECTION_IP)).expect("?????"));
        request.headers_mut().insert("target", HeaderValue::from_bytes(log_name.into().as_bytes()).expect("invalid bytes?"));
        let stream = Client::new().execute(request).await?;
        match serde_json::from_str::<DeleteResult>(&stream.text().await?).expect("invalid json") {
            DeleteResult::Success { success } => Ok(true),
            DeleteResult::Error { error } => Ok(false),
        }
    }
}
