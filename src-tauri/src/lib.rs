use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::StoreExt;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelConfig {
    #[serde(rename = "rtspUrl")]
    pub rtsp_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(rename = "savePath")]
    pub save_path: Option<String>,
    #[serde(rename = "captureShortcut")]
    pub capture_shortcut: String,
    #[serde(rename = "gpuEncoder")]
    pub gpu_encoder: String,
    pub channels: Vec<ChannelConfig>,
}

pub struct AppState {
    pub ffmpeg_manager: Arc<Mutex<FFmpegManager>>,
    pub frames: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
    pub config: Arc<Mutex<AppConfig>>,
}

pub struct FFmpegManager {
    processes: HashMap<usize, std::process::Child>,
}

impl FFmpegManager {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
        }
    }

    pub fn start(&mut self, channel_id: usize, rtsp_url: &str, _port: u16, gpu_encoder: &str) -> Result<String, String> {
        self.stop(channel_id);

        let ffmpeg_path = get_ffmpeg_path();
        log::info!("Using FFmpeg: {}", ffmpeg_path);
        log::info!("GPU encoder: {}", gpu_encoder);

        let mut cmd = Command::new(&ffmpeg_path);
        
        let is_nvidia = gpu_encoder == "hevc_nvenc";
        let is_intel = gpu_encoder == "hevc_qsv";
        let is_amd = gpu_encoder == "hevc_amf";
        let is_apple = gpu_encoder == "hevc_videotoolbox";
        
        if is_nvidia || is_intel || is_amd || is_apple {
            let hwaccel = match gpu_encoder {
                "hevc_nvenc" => "cuda",
                "hevc_qsv" => "qsv",
                "hevc_amf" => "d3d11va",
                "hevc_videotoolbox" => " videotoolbox",
                _ => "auto",
            };
            
            let encoder = gpu_encoder;
            
            cmd.args([
                "-rtsp_transport", "tcp",
                "-timeout", "10000000",
                "-re",
                "-fflags", "nobuffer",
                "-flags", "low_delay",
                "-hwaccel", hwaccel,
                "-i", rtsp_url,
                "-an",
                "-c:v", encoder,
                "-preset", "fast",
                "-f", "mjpeg",
                "-",
            ]);
        } else {
            cmd.args([
                "-rtsp_transport", "tcp",
                "-timeout", "10000000",
                "-re",
                "-fflags", "nobuffer",
                "-flags", "low_delay",
                "-i", rtsp_url,
                "-an",
                "-c:v", "mjpeg",
                "-q:v", "8",
                "-s", "2560x1440",
                "-r", "30",
                "-f", "mjpeg",
                "-",
            ]);
        }
        
        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("启动 FFmpeg 失败: {}", e))?;
        
        log::info!("FFmpeg process started for channel {}", channel_id);

        let stderr = child.stderr.take();
        thread::spawn(move || {
            if let Some(stderr) = stderr {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            if !line.trim().is_empty() {
                                log::warn!("FFmpeg stderr: {}", line.trim());
                            }
                        }
                        Err(_) => break,
                    }
                    line.clear();
                }
            }
        });

        self.processes.insert(channel_id, child);
        
        Ok(format!("http://localhost:8890/mjpeg/{}", channel_id))
    }

    pub fn stop(&mut self, channel_id: usize) {
        if let Some(mut child) = self.processes.remove(&channel_id) {
            let _ = child.kill();
        }
    }

    pub fn stop_all(&mut self) {
        for (_, mut child) in self.processes.drain() {
            let _ = child.kill();
        }
    }
}

fn get_ffmpeg_path() -> String {
    if cfg!(target_os = "windows") {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let bundled = parent.join("resources")
                    .join("ffmpeg")
                    .join("bin")
                    .join("ffmpeg.exe");
                if bundled.exists() {
                    return bundled.to_string_lossy().to_string();
                }
            }
        }
        "ffmpeg.exe".to_string()
    } else {
        "ffmpeg".to_string()
    }
}

#[allow(dead_code)]
fn parse_mp4_frames(data: &[u8]) -> Option<Vec<u8>> {
    let start_marker = &[0x00, 0x00, 0x00, 0x18, 0x66, 0x74, 0x79, 0x70];
    
    if let Some(start_pos) = data.windows(8).position(|w| w == start_marker) {
        if data.len() > start_pos + 8 {
            let mdat_start = &data[start_pos + 4..];
            if let Some(mdat_pos) = mdat_start.windows(4).position(|w| w == &[0x6D, 0x64, 0x61, 0x74]) {
                let mdat_data = &mdat_start[mdat_pos + 4..];
                if mdat_data.len() > 8 {
                    let size = u32::from_be_bytes([mdat_data[0], mdat_data[1], mdat_data[2], mdat_data[3]]) as usize;
                    if size > 8 && size <= mdat_data.len() - 4 {
                        let frame_data = &mdat_data[4..size];
                        return Some(frame_data.to_vec());
                    }
                }
            }
        }
    }
    None
}

#[tauri::command]
async fn start_stream(
    channel_id: usize,
    rtsp_url: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    log::info!("start_stream called: channel={}, url={}", channel_id, rtsp_url);
    
    let frames = state.frames.clone();
    let manager = state.ffmpeg_manager.clone();
    let config = state.config.clone();
    
    let gpu_encoder = {
        let cfg = config.lock().unwrap();
        cfg.gpu_encoder.clone()
    };
    
    let stdout = {
        let mut mgr = manager.lock().unwrap();
        let _url = mgr.start(channel_id, &rtsp_url, 8890, &gpu_encoder)?;
        
        if let Some(child) = mgr.processes.get_mut(&channel_id) {
            child.stdout.take()
        } else {
            None
        }
    };
    
    let frames_clone = frames.clone();
    let rtsp_url_clone = rtsp_url.clone();
    let ch = channel_id;
    
    if let Some(stdout) = stdout {
        thread::spawn(move || {
            read_ffmpeg_output(ch, frames_clone, stdout, rtsp_url_clone);
        });
    }

    Ok(format!("http://localhost:8890/mjpeg/{}", channel_id))
}

fn read_ffmpeg_output(channel_id: usize, frames: Arc<Mutex<HashMap<usize, Vec<u8>>>>, stdout: std::process::ChildStdout, _rtsp_url: String) {
    log::info!("Starting FFmpeg output reader for channel {}", channel_id);
    
    let mut frame_count = 0;
    let mut buffer = Vec::new();
    let mut reader = BufReader::new(stdout);
    let mut chunk = vec![0u8; 65536];
    
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => {
                log::warn!("FFmpeg stdout EOF, attempting to reconnect...");
                break;
            }
            Ok(n) => {
                let data = &chunk[..n];
                buffer.extend_from_slice(data);
                
                while let Some((frame, consumed)) = extract_next_jpeg(&buffer) {
                    if !frame.is_empty() {
                        let mut frames_lock = frames.lock().unwrap();
                        frames_lock.insert(channel_id, frame.clone());
                        frame_count += 1;
                        if frame_count % 30 == 0 {
                            log::info!("Frame stored! size={}", frame.len());
                        }
                    }
                    buffer.drain(0..consumed);
                }
            }
            Err(e) => {
                log::info!("FFmpeg read error: {}", e);
                break;
            }
        }
    }
    
    log::info!("FFmpeg reader thread ended");
}

fn extract_next_jpeg(buffer: &[u8]) -> Option<(Vec<u8>, usize)> {
    let jpeg_start = [0xFF, 0xD8];
    let jpeg_end = [0xFF, 0xD9];
    
    if let Some(start_idx) = buffer.windows(2).position(|w| w == &jpeg_start) {
        if let Some(end_idx) = buffer[start_idx..].windows(2).position(|w| w == &jpeg_end) {
            let end_idx = start_idx + end_idx + 2;
            let frame = buffer[start_idx..end_idx].to_vec();
            return Some((frame, end_idx));
        }
    }
    None
}

#[tauri::command]
async fn stop_stream(channel_id: usize, state: State<'_, AppState>) -> Result<(), String> {
    let mut manager = state.ffmpeg_manager.lock().unwrap();
    manager.stop(channel_id);
    let mut frames = state.frames.lock().unwrap();
    frames.remove(&channel_id);
    Ok(())
}

#[tauri::command]
async fn start_all_streams(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let config = state.config.lock().unwrap().clone();
    let gpu_encoder = config.gpu_encoder.clone();
    let mut results = Vec::new();

    for (id, channel) in config.channels.iter().enumerate() {
        if !channel.rtsp_url.is_empty() {
            let mut manager = state.ffmpeg_manager.lock().unwrap();
            match manager.start(id, &channel.rtsp_url, 8890, &gpu_encoder) {
                Ok(url) => results.push(url),
                Err(e) => log::error!("Channel {} failed: {}", id, e),
            }
        }
    }

    Ok(results)
}

#[tauri::command]
async fn stop_all_streams(state: State<'_, AppState>) -> Result<(), String> {
    let mut manager = state.ffmpeg_manager.lock().unwrap();
    manager.stop_all();
    let mut frames = state.frames.lock().unwrap();
    frames.clear();
    Ok(())
}

#[tauri::command]
async fn save_image(
    parent_path: String,
    date_str: String,
    time_str: String,
    filename: String,
    base64_data: String,
) -> Result<String, String> {
    log::info!("save_image called: parent={}, date={}, time={}, filename={}", parent_path, date_str, time_str, filename);
    log::info!("base64_data length: {}, prefix: {}", base64_data.len(), &base64_data[..30.min(base64_data.len())]);
    
    let dir_path = PathBuf::from(&parent_path)
        .join(&date_str)
        .join(&time_str);

    log::info!("Creating directory: {:?}", dir_path);
    std::fs::create_dir_all(&dir_path)
        .map_err(|e| {
            log::error!("创建目录失败: {}", e);
            format!("创建目录失败: {}", e)
        })?;

    let file_path = dir_path.join(&filename);
    let base64_clean = base64_data
        .trim_start_matches("data:image/jpeg;base64,")
        .trim_start_matches("data:image/png;base64,");

    log::info!("Decoding base64, length after trim: {}", base64_clean.len());
    let image_data = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        base64_clean,
    )
    .map_err(|e| {
        log::error!("Base64 解码失败: {}", e);
        format!("Base64 解码失败: {}", e)
    })?;

    log::info!("Writing file: {:?}, size: {}", file_path, image_data.len());
    std::fs::write(&file_path, image_data)
        .map_err(|e| {
            log::error!("保存文件失败: {}", e);
            format!("保存文件失败: {}", e)
        })?;

    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn capture_frame(
    channel_id: usize,
    parent_path: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    log::info!("capture_frame called for channel {}", channel_id);
    
    let now = chrono::Local::now();
    let date_str = now.format("%Y-%m-%d").to_string();
    let time_str = now.format("%H-%M-%S").to_string();
    let filename = format!("{}_通道{}.jpg", time_str, channel_id + 1);
    
    let frames = state.frames.lock().unwrap();
    let frame_data = frames.get(&channel_id).cloned();
    
    let frame = frame_data.ok_or("没有可用的视频帧")?;
    log::info!("Got frame size: {}", frame.len());
    drop(frames);
    
    let dir_path = PathBuf::from(&parent_path)
        .join(&date_str)
        .join(&time_str);
    
    std::fs::create_dir_all(&dir_path)
        .map_err(|e| format!("创建目录失败: {}", e))?;
    
    let file_path = dir_path.join(&filename);
    std::fs::write(&file_path, &frame)
        .map_err(|e| format!("保存文件失败: {}", e))?;
    
    log::info!("Frame saved to: {:?}", file_path);
    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
fn check_gpu() -> GpuInfo {
    check_gpu_support()
}

#[tauri::command]
fn get_home_path() -> Result<String, String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "无法获取 home 目录".to_string())
}

const STORE_PATH: &str = "config.json";

#[tauri::command]
async fn load_config(app: AppHandle, state: State<'_, AppState>) -> Result<AppConfig, String> {
    log::info!("load_config called");
    
    // Try to load from store
    if let Ok(store) = app.store(STORE_PATH) {
        if let Some(config_val) = store.get("app_config") {
            if let Ok(config) = serde_json::from_value::<AppConfig>(config_val.clone()) {
                log::info!("Loaded config from store: {:?}", config);
                // Also update state
                let mut state_config = state.config.lock().unwrap();
                *state_config = config.clone();
                return Ok(config);
            }
        }
    }
    
    // Return default config
    let config = state.config.lock().unwrap().clone();
    log::info!("Returning default config: {:?}", config);
    Ok(config)
}

#[tauri::command]
async fn save_config(app: AppHandle, config: AppConfig, state: State<'_, AppState>) -> Result<(), String> {
    log::info!("save_config called: {:?}", config);
    
    // Update in-memory state
    let mut current_config = state.config.lock().unwrap();
    *current_config = config.clone();
    
    // Save to store
    if let Ok(store) = app.store(STORE_PATH) {
        let config_json = serde_json::to_value(&config).map_err(|e| e.to_string())?;
        store.set("app_config", config_json);
        store.save().map_err(|e| e.to_string())?;
        log::info!("Config saved to store");
    }
    
    Ok(())
}

#[tauri::command]
async fn select_save_path(app: AppHandle) -> Result<Option<String>, String> {
    log::info!("select_save_path called");
    use tauri_plugin_dialog::DialogExt;

    let result = app
        .dialog()
        .file()
        .set_title("选择保存路径")
        .blocking_pick_folder();

    Ok(result.map(|p| p.to_string()))
}

#[tauri::command]
async fn update_shortcut(
    app: AppHandle,
    shortcut: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let mut config = state.config.lock().unwrap();
        config.capture_shortcut = shortcut.clone();
    }

    let app_handle = app.clone();
    register_shortcut(&app_handle, &shortcut)?;

    Ok(())
}

fn register_shortcut(app: &AppHandle, shortcut_str: &str) -> Result<(), String> {
    let shortcut: Shortcut = shortcut_str.parse().map_err(|_| "无效的快捷键")?;

    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                log::info!("Global shortcut triggered");
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("global-capture", ());
                }
            }
        })
        .map_err(|e| format!("注册快捷键失败: {}", e))?;

    Ok(())
}

fn setup_logging() {
    use std::io::Write;
    
    let log_dir = if cfg!(target_os = "windows") {
        if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data).join("ONVIF Viewer").join("logs")
        } else {
            PathBuf::from(env::temp_dir()).join("ONVIF Viewer").join("logs")
        }
    } else {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from(env::temp_dir()))
            .join("ONVIF Viewer")
            .join("logs")
    };
    
    let _ = fs::create_dir_all(&log_dir);
    
    let log_file_path = log_dir.join("app.log");
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
        .ok();
    
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .format(|buf, record| {
            writeln!(
                buf,
                "{} [{}] {} - {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();
    
    if let Some(mut file) = log_file {
        let _ = writeln!(file, "\n=== Application started at {} ===", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
    
    log::info!("Logging initialized. Log file: {:?}", log_file_path);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub encoders: Vec<String>,
    pub nvidia: bool,
    pub intel: bool,
    pub amd: bool,
    pub apple: bool,
}

fn check_gpu_support() -> GpuInfo {
    let ffmpeg_path = get_ffmpeg_path();
    
    let output = Command::new(&ffmpeg_path)
        .args(["-hide_banner", "-encoders"])
        .output();
    
    let mut encoders = Vec::new();
    let mut nvidia = false;
    let mut intel = false;
    let mut amd = false;
    let mut apple = false;
    
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            
            if stdout.contains("hevc_nvenc") || stdout.contains("h264_nvenc") {
                nvidia = true;
                encoders.push("hevc_nvenc".to_string());
            }
            if stdout.contains("hevc_qsv") || stdout.contains("h264_qsv") {
                intel = true;
                encoders.push("hevc_qsv".to_string());
            }
            if stdout.contains("hevc_amf") || stdout.contains("h264_amf") {
                amd = true;
                encoders.push("hevc_amf".to_string());
            }
            if stdout.contains("hevc_videotoolbox") || stdout.contains("h264_videotoolbox") {
                apple = true;
                encoders.push("hevc_videotoolbox".to_string());
            }
            
            log::info!("GPU support check: nvidia={}, intel={}, amd={}, apple={}", nvidia, intel, amd, apple);
            log::info!("Available encoders: {:?}", encoders);
        }
        Err(e) => {
            log::error!("Failed to check GPU support: {}", e);
        }
    }
    
    GpuInfo { encoders, nvidia, intel, amd, apple }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    setup_logging();
    log::info!("Starting ONVIF Viewer");

    let app_state = AppState {
        ffmpeg_manager: Arc::new(Mutex::new(FFmpegManager::new())),
        frames: Arc::new(Mutex::new(HashMap::new())),
        config: Arc::new(Mutex::new(AppConfig {
            capture_shortcut: "CommandOrControl+Shift+P".to_string(),
            gpu_encoder: "".to_string(),
            channels: vec![ChannelConfig::default(); 3],
            ..Default::default()
        })),
    };

    let frames = app_state.frames.clone();
    let http_port = 8890;
    thread::spawn(move || {
        start_mjpeg_server(http_port, frames);
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(app_state)
        .setup(|app| {
            let shortcut_str = "CommandOrControl+Shift+P";
            if let Err(e) = register_shortcut(app.handle(), shortcut_str) {
                log::error!("Failed to register default shortcut: {}", e);
            }

            log::info!("App setup complete");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_stream,
            stop_stream,
            start_all_streams,
            stop_all_streams,
            save_image,
            capture_frame,
            load_config,
            save_config,
            select_save_path,
            update_shortcut,
            get_home_path,
            check_gpu,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn start_mjpeg_server(port: u16, frames: Arc<Mutex<HashMap<usize, Vec<u8>>>>) {
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).unwrap();
    log::info!("MJPEG server listening on port {}", port);

    for stream in listener.incoming() {
        if let Ok(mut stream) = stream {
            let frames_clone = frames.clone();
            thread::spawn(move || {
                handle_mjpeg_connection(&mut stream, frames_clone);
            });
        }
    }
}

fn handle_mjpeg_connection(
    stream: &mut std::net::TcpStream,
    frames: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
) {
    let mut buffer = [0u8; 256];
    if let Ok(n) = stream.read(&mut buffer) {
        let request = String::from_utf8_lossy(&buffer[..n]);
        log::info!("MJPEG request: {}", request.lines().next().unwrap_or(""));
        
        let channel_id = if let Some(path) = request.lines().next().and_then(|l| l.split_whitespace().nth(1)) {
            path.split('/').last().and_then(|s| s.parse().ok()).unwrap_or(0)
        } else {
            0
        };
        log::info!("Serving channel: {}", channel_id);
        
        let response = "HTTP/1.1 200 OK\r\n\
            Content-Type: multipart/x-mixed-replace; boundary=jpegboundary\r\n\
            Cache-Control: no-cache\r\n\r\n";

        if let Err(e) = stream.write_all(response.as_bytes()) {
            log::error!("Failed to write response header: {}", e);
            return;
        }

        loop {
            let frames_lock = frames.lock().unwrap();
            if let Some(frame) = frames_lock.get(&channel_id) {
                if frame.is_empty() {
                    drop(frames_lock);
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                if stream.write_all(b"--jpegboundary\r\n").is_err() {
                    return;
                }
                if stream.write_all(b"Content-Type: image/jpeg\r\n\r\n").is_err() {
                    return;
                }
                if stream.write_all(frame).is_err() {
                    return;
                }
                if stream.write_all(b"\r\n").is_err() {
                    return;
                }
            }
            drop(frames_lock);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}
