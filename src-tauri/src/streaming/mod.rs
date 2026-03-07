use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

mod rtsp_client;
mod decoder;
mod renderer;

pub use rtsp_client::RtspClient;
pub use decoder::VideoDecoder;
pub use renderer::VideoRenderer;

pub struct StreamManager {
    clients: HashMap<usize, Arc<Mutex<Option<RtspClient>>>>,
    decoders: HashMap<usize, Arc<Mutex<Option<VideoDecoder>>>>,
    renderers: HashMap<usize, Arc<Mutex<Option<VideoRenderer>>>>,
    frames: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
}

impl StreamManager {
    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            decoders: HashMap::new(),
            renderers: HashMap::new(),
            frames: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    pub fn get_frame(&self, channel_id: usize) -> Option<Vec<u8>> {
        let frames = self.frames.lock().unwrap();
        frames.get(&channel_id).cloned()
    }
}
