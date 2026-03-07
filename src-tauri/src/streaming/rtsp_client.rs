use msf_rtsp::RtspConnection;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct RtspClient {
    url: String,
    connected: bool,
}

impl RtspClient {
    pub async fn new(url: &str) -> Result<Self, String> {
        println!("Connecting to RTSP stream: {}", url);
        
        // Parse RTSP URL
        let (host, port, path) = Self::parse_url(url)?;
        
        // Connect to camera
        let addr = format!("{}:{}", host, port);
        let mut stream = TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("Failed to connect to {}: {}", addr, e))?;
        
        // Send OPTIONS request
        let options = format!(
            "OPTIONS {} RTSP/1.0\r\n\
             CSeq: 1\r\n\
             User-Agent: ONVIF-Client\r\n\r\n",
            path
        );
        
        stream.write_all(options.as_bytes())
            .await
            .map_err(|e| format!("Failed to send OPTIONS: {}", e))?;
        
        // Read response
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        
        println!("RTSP connection established to {}", url);
        
        Ok(Self {
            url: url.to_string(),
            connected: true,
        })
    }
    
    fn parse_url(url: &str) -> Result<(String, u16, String), String> {
        // Parse rtsp://host:port/path
        let url = url.trim_start_matches("rtsp://");
        let parts: Vec<&str> = url.split('/').collect();
        
        let host_port = parts[0];
        let path = if parts.len() > 1 {
            format!("/{}", parts[1..].join("/"))
        } else {
            "/".to_string()
        };
        
        let (host, port) = if host_port.contains(':') {
            let p: Vec<&str> = host_port.split(':').collect();
            (p[0].to_string(), p[1].parse().unwrap_or(554))
        } else {
            (host_port.to_string(), 554)
        };
        
        Ok((host, port, path))
    }
    
    pub fn is_connected(&self) -> bool {
        self.connected
    }
}
