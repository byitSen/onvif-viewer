use ffmpeg_sidecar::command::FfmpegCommand;
use ffmpeg_sidecar::iteration::EventIterator;
use std::process::Stdio;
use tokio::sync::mpsc;

pub struct VideoDecoder {
    width: u32,
    height: u32,
    fps: f64,
}

impl VideoDecoder {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            fps: 0.0,
        }
    }
    
    pub fn start(&mut self, rtsp_url: &str, gpu_encoder: &str) -> Result<mpsc::Receiver<Vec<u8>>, String> {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(100);
        
        // Build FFmpeg command with hardware acceleration
        let mut cmd = FfmpegCommand::new();
        
        // Add input
        cmd.input(rtsp_url)
            .arg("-rtsp_transport").arg("tcp")
            .arg("-timeout").arg("10000000")
            .arg("-fflags").arg("nobuffer")
            .arg("-flags").arg("low_delay");
        
        // Add hardware acceleration
        match gpu_encoder {
            "auto" => {
                cmd.arg("-hwaccel").arg("auto");
            }
            "cuda" => {
                cmd.arg("-hwaccel").arg("cuda")
                   .arg("-hwaccel_output_format").arg("cuda");
            }
            "qsv" => {
                cmd.arg("-hwaccel").arg("qsv");
            }
            "videotoolbox" => {
                cmd.arg("-hwaccel").arg("videotoolbox");
            }
            _ => {}
        }
        
        // Output as raw video frames
        cmd.output_format("rawvideo")
            .arg("-c:v").arg("copy")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        
        // Spawn FFmpeg process
        let mut child = cmd.spawn()
            .map_err(|e| format!("Failed to start FFmpeg: {}", e))?;
        
        // Spawn task to read frames
        tokio::spawn(async move {
            if let Some(stdout) = child.take_stdout() {
                let mut iterator = EventIterator::new(stdout);
                
                while let Ok(Some(event)) = iterator.next().await {
                    match event {
                        ffmpeg_sidecar::event::FfmpegEvent::VideoData { data, .. } => {
                            if tx.send(data.to_vec()).await.is_err() {
                                break;
                            }
                        }
                        ffmpeg_sidecar::event::FfmpegEvent::Log { message, .. } => {
                            println!("FFmpeg: {}", message);
                        }
                        _ => {}
                    }
                }
            }
        });
        
        Ok(rx)
    }
    
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl Default for VideoDecoder {
    fn default() -> Self {
        Self::new()
    }
}
