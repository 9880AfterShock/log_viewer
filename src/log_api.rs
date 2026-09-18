use std::{
    collections::HashMap, io::{Error, Read, Write}, net::{Ipv4Addr, SocketAddrV4, TcpStream}, sync::{Arc, atomic::AtomicU64}, time::Duration,
};

use egui::Context;
use serde::Deserialize;

const CONNECTION_IP: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(192, 168, 43, 1), 5535);

pub fn read_log_with_reporting(log_name: impl Into<String>, progress: Arc<AtomicU64>, ctx: Context) -> std::io::Result<String> {
    let log_name = log_name.into();
    let mut stream = TcpStream::connect(CONNECTION_IP)?;
    stream.write_all(format!("{{\"command\":\"Read\",\"data\":\"{log_name}\"}}").as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut bytes = Vec::new();
    let mut buf = [0u8; 8192];
    while let Ok(size) = stream.read(&mut buf) && size != 0 {
        progress.update(std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst, |a| a+size as u64);
        bytes.extend(&buf[..size]);
        ctx.request_repaint();
    }
    String::from_utf8(bytes).map_err(|_| Error::new(std::io::ErrorKind::InvalidData, "Invalid UTF-8"))
}

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
    
    pub fn list_logs(&mut self) -> std::io::Result<Vec<FileSpecifier>> {
        if let Some(list) = &self.list {
            return Ok(list.clone())
        }
        let mut stream = TcpStream::connect_timeout(&std::net::SocketAddr::V4(CONNECTION_IP), Duration::from_millis(1))?;
        stream.write_all(b"{\"command\":\"List\"}")?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut str = String::new();
        stream.read_to_string(&mut str)?;
        let list = serde_json::from_str::<Vec<FileSpecifier>>(&str)?;
        self.list = Some(list.clone());
        Ok(list)
    }

    pub fn read_log(&mut self, log_name: impl Into<String>) -> std::io::Result<String> {
        let log_name = log_name.into();
        if let Some(contents) = self.cache.get(&log_name) {
            return Ok(contents.clone());
        }
        let mut stream = TcpStream::connect(CONNECTION_IP)?;
        stream.write_all(format!("{{\"command\":\"Read\",\"data\":\"{log_name}\"}}").as_bytes())?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut str = String::new();
        stream.read_to_string(&mut str)?;
        Ok(str)
    }

    pub fn cache_log(&mut self, log_name: impl Into<String>, data: String) {
        self.cache.insert(log_name.into(), data);
    }

    pub fn delete_log(&self, log_name: impl Into<String>) -> std::io::Result<()> {
        let mut stream = TcpStream::connect(CONNECTION_IP)?;
        stream.write_all(
            format!(
                "{{\"command\":\"Delete\",\"data\":\"{}\"}}",
                log_name.into()
            )
            .as_bytes(),
        )?;
        stream.shutdown(std::net::Shutdown::Write)?;
        let mut str = String::new();
        stream.read_to_string(&mut str)?;
        match serde_json::from_str::<DeleteResult>(&str)? {
            DeleteResult::Success { success: _ } => Ok(()),
            DeleteResult::Error { error } => Err(Error::new(std::io::ErrorKind::NotFound, error)),
        }
    }
}
