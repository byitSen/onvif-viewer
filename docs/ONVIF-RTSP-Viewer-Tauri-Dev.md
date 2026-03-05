# ONVIF/RTSP 视频流工具 - Rust + Tauri 开发文档

## 目录
1. [项目概述](#1-项目概述)
2. [技术栈选型](#2-技术栈选型)
3. [项目结构](#3-项目结构)
4. [核心功能实现](#4-核心功能实现)
5. [配置与持久化](#5-配置与持久化)
6. [全局快捷键](#6-全局快捷键)
7. [拍照功能](#7-拍照功能)
8. [前端 UI 实现](#8-前端-ui-实现)
9. [Tauri 配置](#9-tauri-配置)
10. [构建与发布](#10-构建与发布)
11. [完整项目初始化脚本](#11-完整项目初始化脚本)
12. [关键实现要点](#12-关键实现要点)
13. [FFmpeg 打包配置](#14-ffmpeg-打包配置)
14. [完整构建命令](#15-完整构建命令)
15. [验证清单](#16-验证清单)
16. [常见问题排查](#17-常见问题排查)

---

## 1. 项目概述

### 1.1 功能需求
- **3路视频流播放**: 同时接收并播放3路 ONVIF/RTSP 实时视频流
- **一键拍照**: 全局快捷键支持后台运行拍照，保存到本地
- **一键连接/断开**: 批量管理3路视频流连接状态
- **配置持久化**: 保存 RTSP URL、快捷键、保存路径等配置

### 1.2 路径规范
```
保存路径格式: {父目录}/{YYYY-MM-DD}/{HH-mm-ss}/{文件名}
示例: /Pictures/2026-03-05/15-30-00/通道1.jpg
```
**关键点**: 所有通道在同一批次拍照时使用统一的"时-分-秒"时间戳，避免59秒进位导致创建多个文件夹。

---

## 2. 技术栈选型

### 2.1 核心技术
| 组件 | 技术选型 | 说明 |
|------|----------|------|
| 桌面框架 | Tauri 2.x | Rust 原生，性能好，打包体积小 |
| 前端框架 | Vue 3 + Vite | 轻量级响应式 UI |
| 视频流处理 | FFmpeg | RTSP 转 MJPEG 流 |
| ONVIF 协议 | onvif-rs 或手动解析 | 设备发现与媒体获取 |
| 全局快捷键 | tauri-plugin-global-shortcut | 后台响应快捷键 |
| 状态存储 | tauri-plugin-store | JSON 配置持久化 |
| 文件操作 | tauri-plugin-fs | 路径选择与文件保存 |

### 2.2 依赖版本
```toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
tauri-plugin-global-shortcut = "2"
tauri-plugin-store = "2"
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

---

## 3. 项目结构

```
rtsp-viewer/
├── src/                      # Rust 后端源码
│   ├── main.rs              # 入口文件
│   ├── lib.rs               # 库文件
│   ├── commands/            # Tauri 命令
│   │   ├── mod.rs
│   │   ├── stream.rs       # 视频流管理
│   │   ├── capture.rs      # 拍照功能
│   │   └── config.rs        # 配置管理
│   ├── services/            # 核心服务
│   │   ├── mod.rs
│   │   ├── ffmpeg.rs        # FFmpeg 进程管理
│   │   └── http_server.rs   # MJPEG HTTP 服务器
│   └── models/              # 数据模型
│       ├── mod.rs
│       ├── config.rs        # 配置结构
│       └── stream.rs        # 流状态
├── src-tauri/               # Tauri 配置目录
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   └── icons/
├── src-ui/                  # 前端源码
│   ├── src/
│   │   ├── main.ts
│   │   ├── App.vue
│   │   ├── components/
│   │   │   ├── VideoPanel.vue
│   │   │   ├── Toolbar.vue
│   │   │   └── StatusBar.vue
│   │   └── styles/
│   │       └── main.css
│   ├── index.html
│   ├── vite.config.ts
│   └── package.json
└── README.md
```

---

## 4. 核心功能实现

### 4.1 视频流播放

#### 4.1.1 FFmpeg 进程管理 (Rust)

```rust
// src/services/ffmpeg.rs

use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::io::Read;

pub struct FfmpegManager {
    processes: Arc<Mutex<HashMap<usize, Command>>>,
    http_port: u16,
}

impl FfmpegManager {
    pub fn new() -> Self {
        Self {
            processes: Arc::new(Mutex::new(HashMap::new())),
            http_port: 8889,
        }
    }

    pub fn start_stream(&self, channel_id: usize, rtsp_url: &str) -> Result<String, String> {
        // 停止已存在的流
        self.stop_stream(channel_id);

        let port = self.http_port + channel_id as u16;
        let output_url = format!("http://0.0.0.0:{}/mjpeg/{}", port, channel_id);

        // 构建 FFmpeg 命令
        let mut cmd = Command::new("ffmpeg");
        cmd.args([
            "-rtsp_transport", "tcp",
            "-timeout", "5000000",
            "-i", rtsp_url,
            "-c:v", "mjpeg",
            "-q:v", "8",
            "-f", "image2pipe",
            "-"
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

        // 启动进程（实际实现需要处理输出流）
        
        let mut processes = self.processes.lock().unwrap();
        processes.insert(channel_id, cmd);

        Ok(output_url)
    }

    pub fn stop_stream(&self, channel_id: usize) {
        let mut processes = self.processes.lock().unwrap();
        if let Some(mut cmd) = processes.remove(&channel_id) {
            let _ = cmd.kill();
        }
    }

    pub fn stop_all(&self) {
        let mut processes = self.processes.lock().unwrap();
        for (_, mut cmd) in processes.drain() {
            let _ = cmd.kill();
        }
    }
}
```

#### 4.1.2 MJPEG HTTP 服务器

```rust
// src/services/http_server.rs

use std::sync::Arc;
use std::collections::HashMap;
use std::io::Write;
use std::net::TcpListener;
use tokio::sync::Mutex;

pub struct MjpegServer {
    frames: Arc<Mutex<HashMap<usize, Vec<u8>>>>,
    port: u16,
}

impl MjpegServer {
    pub fn new(port: u16) -> Self {
        Self {
            frames: Arc::new(Mutex::new(HashMap::new())),
            port,
        }
    }

    pub async fn start(&self) {
        let listener = TcpListener::bind(format!("0.0.0.0:{}", self.port)).unwrap();
        
        loop {
            if let Ok((mut stream, _)) = listener.accept() {
                let frames = self.frames.clone();
                tokio::spawn(async move {
                    Self::handle_connection(&mut stream, frames).await;
                });
            }
        }
    }

    async fn handle_connection(
        stream: &mut std::net::TcpStream,
        frames: Arc<Mutex<HashMap<usize, Vec<u8>>>>
    ) {
        // 实现 MJPEG 流响应
        let response_header = "HTTP/1.1 200 OK\r\n\
            Content-Type: multipart/x-mixed-replace; boundary=--jpegboundary\r\n\
            Cache-Control: no-cache\r\n\r\n";
        
        let _ = stream.write_all(response_header.as_bytes());
        
        loop {
            let frames_lock = frames.lock().await;
            if let Some(frame) = frames_lock.get(&0) {
                let _ = stream.write_all(b"--jpegboundary\r\n");
                let _ = stream.write_all(b"Content-Type: image/jpeg\r\n\r\n");
                let _ = stream.write_all(frame);
                let _ = stream.write_all(b"\r\n");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    pub async fn update_frame(&self, channel_id: usize, data: Vec<u8>) {
        let mut frames = self.frames.lock().await;
        frames.insert(channel_id, data);
    }
}
```

### 4.2 Tauri 命令层

```rust
// src/commands/mod.rs

use tauri::State;
use crate::services::ffmpeg::FfmpegManager;
use crate::AppState;

#[tauri::command]
pub async fn start_stream(
    channel_id: usize,
    rtsp_url: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let url = state.ffmpeg.start_stream(channel_id, &rtsp_url)?;
    Ok(url)
}

#[tauri::command]
pub async fn stop_stream(
    channel_id: usize,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.ffmpeg.stop_stream(channel_id);
    Ok(())
}

#[tauri::command]
pub async fn start_all_streams(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let config = state.config.lock().unwrap();
    let mut results = Vec::new();
    
    for (id, url) in config.rtsp_urls.iter().enumerate() {
        if !url.is_empty() {
            match state.ffmpeg.start_stream(id, url) {
                Ok(url) => results.push(url),
                Err(e) => eprintln!("Channel {} failed: {}", id, e),
            }
        }
    }
    
    Ok(results)
}

#[tauri::command]
pub async fn stop_all_streams(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.ffmpeg.stop_all();
    Ok(())
}
```

---

## 5. 配置与持久化

### 5.1 配置数据结构

```rust
// src/models/config.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(rename = "savePath")]
    pub save_path: Option<String>,
    
    #[serde(rename = "captureShortcut")]
    pub capture_shortcut: String,
    
    pub channels: Vec<ChannelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    #[serde(rename = "rtspUrl")]
    pub rtsp_url: String,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            rtsp_url: String::new(),
        }
    }
}
```

### 5.2 配置持久化 (使用 tauri-plugin-store)

```rust
// src/commands/config.rs

use tauri::State;
use crate::AppState;
use crate::models::config::AppConfig;

const CONFIG_FILE: &str = "rtsp-config.json";

#[tauri::command]
pub async fn load_config(
    state: State<'_, AppState>,
) -> Result<AppConfig, String> {
    let store = state.store.lock().unwrap();
    
    if let Some(config) = store.get(CONFIG_FILE) {
        Ok(config.clone())
    } else {
        Ok(AppConfig {
            capture_shortcut: "CommandOrControl+Shift+P".to_string(),
            channels: vec![ChannelConfig::default(); 3],
            ..Default::default()
        })
    }
}

#[tauri::command]
pub async fn save_config(
    config: AppConfig,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut store = state.store.lock().unwrap();
    store.set(CONFIG_FILE, config);
    store.save()?;
    Ok(())
}

#[tauri::command]
pub async fn select_save_path(
    app: tauri::AppHandle,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    
    let result = app.dialog()
        .file()
        .set_title("选择保存路径")
        .blocking_pick_folder();
    
    Ok(result.map(|p| p.to_string()))
}
```

---

## 6. 全局快捷键

### 6.1 注册全局快捷键

```rust
// src/main.rs

use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // 注册默认快捷键
            let shortcut: Shortcut = "CommandOrControl+Shift+P".parse().unwrap();
            
            app.global_shortcut().on_shortcut(shortcut, |app, _shortcut, _event| {
                // 发送事件到前端触发拍照
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.emit("global-capture", ());
                }
            })?;
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

### 6.2 前端监听全局快捷键

```typescript
// src-ui/src/App.vue

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { listen } from '@tauri-apps/api/event'

const captureAll = async () => {
  const now = new Date()
  const dateStr = now.toISOString().split('T')[0]
  const hours = String(now.getHours()).padStart(2, '0')
  const minutes = String(now.getMinutes()).padStart(2, '0')
  const seconds = String(now.getSeconds()).padStart(2, '0')
  const timeStr = `${hours}-${minutes}-${seconds}`
  
  for (const channel of channels.value) {
    if (channel.connected) {
      await captureImage(channel.index, dateStr, timeStr)
    }
  }
}

onMounted(async () => {
  // 监听全局快捷键
  await listen('global-capture', () => {
    captureAll()
  })
})
</script>
```

---

## 7. 拍照功能

### 7.1 Rust 端保存图片

```rust
// src/commands/capture.rs

use std::path::PathBuf;
use std::fs;
use base64::{Engine as _, engine::general_purpose::STANDARD};

#[tauri::command]
pub async fn save_image(
    parent_path: String,
    date_str: String,      // YYYY-MM-DD
    time_str: String,      // HH-mm-ss
    filename: String,      // 通道名.jpg
    base64_data: String,
) -> Result<String, String> {
    // 构建完整路径: 父目录/日期/时间/文件名
    let dir_path = PathBuf::from(&parent_path)
        .join(&date_str)
        .join(&time_str);
    
    // 创建目录
    fs::create_dir_all(&dir_path)
        .map_err(|e| format!("创建目录失败: {}", e))?;
    
    // 保存文件
    let file_path = dir_path.join(&filename);
    let base64_clean = base64_data
        .trim_start_matches("data:image/jpeg;base64,")
        .trim_start_matches("data:image/png;base64,");
    
    let image_data = STANDARD.decode(base64_clean)
        .map_err(|e| format!("Base64 解码失败: {}", e))?;
    
    fs::write(&file_path, image_data)
        .map_err(|e| format!("保存文件失败: {}", e))?;
    
    Ok(file_path.to_string_lossy().to_string())
}
```

### 7.2 前端拍照逻辑

```typescript
// src-ui/src/App.vue

<script setup lang="ts">
const captureAll = async () => {
  // 1. 获取统一时间戳（避免59秒进位导致多个文件夹）
  const now = new Date()
  const dateStr = now.toISOString().split('T')[0]
  const hours = String(now.getHours()).padStart(2, '0')
  const minutes = String(now.getMinutes()).padStart(2, '0')
  const seconds = String(now.getSeconds()).padStart(2, '0')
  const timeStr = `${hours}-${minutes}-${seconds}`
  
  // 2. 对所有已连接通道拍照
  for (const channel of channels.value) {
    if (channel.connected) {
      await captureChannel(channel.index, dateStr, timeStr)
    }
  }
}

const captureChannel = async (index: number, dateStr: string, timeStr: string) => {
  const imgEl = document.getElementById(`video-${index}`) as HTMLImageElement
  if (!imgEl) return
  
  // 绘制到 Canvas 获取 Base64
  const canvas = document.createElement('canvas')
  canvas.width = imgEl.naturalWidth
  canvas.height = imgEl.naturalHeight
  const ctx = canvas.getContext('2d')!
  ctx.drawImage(imgEl, 0, 0)
  
  const base64 = canvas.toDataURL('image/jpeg', 0.95)
  const filename = `${timeStr}_通道${index + 1}.jpg`
  
  // 调用 Rust 保存
  await invoke('save_image', {
    parentPath: savePath.value,
    dateStr,
    timeStr,
    filename,
    base64Data: base64
  })
}
</script>
```

---

## 8. 前端 UI 实现

### 8.1 App.vue 主组件

```vue
<!-- src-ui/src/App.vue -->

<template>
  <div class="app">
    <!-- 顶部工具栏 -->
    <header class="toolbar">
      <h1>ONVIF Viewer - 视频监控</h1>
      <div class="toolbar-actions">
        <button class="btn btn-success" @click="connectAll">一键连接</button>
        <button class="btn btn-danger" @click="disconnectAll">一键断开</button>
        <button class="btn btn-primary" @click="selectSavePath">保存路径</button>
        <input 
          type="text" 
          v-model="captureShortcut" 
          @change="updateShortcut"
          placeholder="快捷键"
          class="shortcut-input"
        />
      </div>
    </header>
    
    <!-- 视频区域 -->
    <main class="video-container">
      <div class="channel" v-for="(channel, index) in channels" :key="index">
        <div class="channel-header">
          <span>通道 {{ index + 1 }}</span>
        </div>
        <div class="channel-body">
          <div class="video-wrapper">
            <img 
              v-if="channel.connected" 
              :src="channel.streamUrl" 
              :id="'video-' + index"
            />
            <div class="placeholder" v-else>未连接</div>
          </div>
          <div class="channel-controls">
            <input 
              type="text" 
              class="rtsp-input"
              v-model="channel.rtspUrl"
              placeholder="rtsp://用户名:密码@IP:端口/stream"
              :disabled="channel.connected"
            />
            <div class="channel-actions">
              <button 
                class="btn-connect"
                :class="{ connected: channel.connected }"
                @click="toggleConnection(index)"
              >
                {{ channel.connected ? '断开' : '连接' }}
              </button>
              <button 
                class="btn-snap"
                @click="captureImage(index)"
                :disabled="!channel.connected"
              >
                拍照
              </button>
            </div>
          </div>
        </div>
      </div>
    </main>
    
    <!-- 底部控制栏 -->
    <footer class="bottom-bar">
      <div class="save-path">
        保存路径: <span>{{ savePath || '未设置' }}</span>
      </div>
      <button 
        class="btn-capture"
        @click="captureAll"
        :disabled="!hasAnyConnected"
      >
        📷 一键拍照
      </button>
    </footer>
  </div>
</template>

<script setup lang="ts">
import { ref, reactive, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'

// 状态
const savePath = ref('')
const captureShortcut = ref('CommandOrControl+Shift+P')

const channels = reactive([
  { rtspUrl: '', connected: false, streamUrl: '', index: 0 },
  { rtspUrl: '', connected: false, streamUrl: '', index: 1 },
  { rtspUrl: '', connected: false, streamUrl: '', index: 2 },
])

const hasAnyConnected = computed(() => channels.some(c => c.connected))

// 加载配置
onMounted(async () => {
  try {
    const config = await invoke('load_config')
    if (config.savePath) savePath.value = config.savePath
    if (config.channels) {
      config.channels.forEach((c: any, i: number) => {
        if (channels[i]) channels[i].rtspUrl = c.rtspUrl
      })
    }
    if (config.captureShortcut) {
      captureShortcut.value = config.captureShortcut
    }
  } catch (e) {
    console.error('加载配置失败:', e)
  }
  
  // 监听全局快捷键
  await listen('global-capture', () => {
    captureAll()
  })
})

// 保存配置
async function saveConfig() {
  await invoke('save_config', {
    config: {
      savePath: savePath.value,
      captureShortcut: captureShortcut.value,
      channels: channels.map(c => ({ rtspUrl: c.rtspUrl }))
    }
  })
}

// 选择保存路径
async function selectSavePath() {
  const result = await open({ directory: true, title: '选择保存路径' })
  if (result) {
    savePath.value = result as string
    await saveConfig()
  }
}

// 更新快捷键
async function updateShortcut() {
  await invoke('update_shortcut', { shortcut: captureShortcut.value })
  await saveConfig()
}

// 切换连接状态
async function toggleConnection(index: number) {
  const channel = channels[index]
  
  if (channel.connected) {
    await invoke('stop_stream', { channelId: index })
    channel.connected = false
    channel.streamUrl = ''
  } else {
    if (!channel.rtspUrl) return
    
    try {
      const url = await invoke('start_stream', { 
        channelId: index, 
        rtspUrl: channel.rtspUrl 
      })
      channel.streamUrl = url
      channel.connected = true
    } catch (e) {
      console.error('连接失败:', e)
    }
  }
  await saveConfig()
}

// 一键连接
async function connectAll() {
  for (const channel of channels) {
    if (channel.rtspUrl && !channel.connected) {
      await toggleConnection(channel.index)
      await new Promise(r => setTimeout(r, 500))
    }
  }
}

// 一键断开
async function disconnectAll() {
  for (const channel of channels) {
    if (channel.connected) {
      await toggleConnection(channel.index)
      await new Promise(r => setTimeout(r, 300))
    }
  }
}

// 拍照
async function captureAll() {
  const now = new Date()
  const dateStr = now.toISOString().split('T')[0]
  const hours = String(now.getHours()).padStart(2, '0')
  const minutes = String(now.getMinutes()).padStart(2, '0')
  const seconds = String(now.getSeconds()).padStart(2, '0')
  const timeStr = `${hours}-${minutes}-${seconds}`
  
  for (const channel of channels) {
    if (!channel.connected) continue
    
    const imgEl = document.getElementById(`video-${channel.index}`) as HTMLImageElement
    if (!imgEl) continue
    
    const canvas = document.createElement('canvas')
    canvas.width = imgEl.naturalWidth
    canvas.height = imgEl.naturalHeight
    canvas.getContext('2d')!.drawImage(imgEl, 0, 0)
    
    const base64 = canvas.toDataURL('image/jpeg', 0.95)
    const filename = `${timeStr}_通道${channel.index + 1}.jpg`
    
    await invoke('save_image', {
      parentPath: savePath.value,
      dateStr,
      timeStr,
      filename,
      base64Data: base64
    })
  }
}

// 单通道拍照
async function captureImage(index: number) {
  await captureAll()
}
</script>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
  background: #1a1a2e;
  color: #fff;
  overflow: hidden;
}

.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
}

.toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 20px;
  background: #16213e;
  border-bottom: 1px solid #0f3460;
}

.toolbar h1 { font-size: 18px; font-weight: 600; }

.toolbar-actions { display: flex; gap: 10px; align-items: center; }

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
  transition: all 0.2s;
}

.btn-success { background: #22c55e; color: #fff; }
.btn-danger { background: #ef4444; color: #fff; }
.btn-primary { background: #0f3460; color: #fff; }

.shortcut-input {
  width: 150px;
  padding: 8px;
  background: #1a1a2e;
  border: 1px solid #0f3460;
  border-radius: 6px;
  color: #fff;
  font-size: 13px;
}

.video-container {
  flex: 1;
  display: flex;
  gap: 10px;
  padding: 15px;
  overflow: auto;
}

.channel {
  flex: 1;
  display: flex;
  flex-direction: column;
  background: #16213e;
  border-radius: 8px;
  overflow: hidden;
  min-width: 300px;
}

.channel-header {
  padding: 10px 15px;
  background: #0f3460;
  font-weight: 600;
}

.channel-body { padding: 10px; display: flex; flex-direction: column; flex: 1; }

.video-wrapper {
  flex: 1;
  background: #000;
  border-radius: 6px;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
}

.video-wrapper img { max-width: 100%; max-height: 100%; object-fit: contain; }

.placeholder { color: #666; font-size: 14px; }

.channel-controls { margin-top: 10px; }

.rtsp-input {
  width: 100%;
  padding: 8px 12px;
  background: #1a1a2e;
  border: 1px solid #0f3460;
  border-radius: 6px;
  color: #fff;
  font-size: 13px;
  margin-bottom: 8px;
}

.channel-actions { display: flex; gap: 8px; }

.btn-connect {
  flex: 1;
  padding: 8px;
  background: #0f3460;
  color: #fff;
  border: none;
  border-radius: 6px;
  cursor: pointer;
}

.btn-connect.connected { background: #e94560; }

.btn-snap {
  padding: 8px 12px;
  background: #0f3460;
  color: #fff;
  border: none;
  border-radius: 6px;
  cursor: pointer;
}

.bottom-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 15px 20px;
  background: #16213e;
  border-top: 1px solid #0f3460;
}

.save-path { font-size: 13px; color: #a0a0a0; }
.save-path span { color: #fff; }

.btn-capture {
  padding: 15px 30px;
  background: #e94560;
  color: #fff;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  font-size: 16px;
  font-weight: 600;
}

.btn-capture:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
```

---

## 9. Tauri 配置

### 9.1 tauri.conf.json

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "ONVIF Viewer",
  "version": "1.0.0",
  "identifier": "com.onvifviewer.app",
  "build": {
    "devtools": true,
    "frontendDist": "../src-ui/dist",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "title": "ONVIF Viewer",
        "width": 1280,
        "height": 800,
        "minWidth": 800,
        "minHeight": 600,
        "resizable": true,
        "fullscreen": false,
        "center": true
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  },
  "plugins": {
    "global-shortcut": {},
    "store": {},
    "dialog": {},
    "fs": {
      "scope": ["**"]
    }
  }
}
```

### 9.2 capabilities 配置

```json
// src-tauri/capabilities/main.json

{
  "$schema": "https://schemas.tauri.app/config/2/capabilities",
  "identifier": "main-capability",
  "description": "主窗口权限",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:event:default",
    "core:window:default",
    "core:window:allow-close",
    "core:window:allow-minimize",
    "core:window:allow-maximize",
    "global-shortcut:default",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister",
    "store:default",
    "dialog:default",
    "dialog:allow-open",
    "fs:default",
    "fs:allow-write",
    "fs:allow-read"
  ]
}
```

---

## 10. 构建与发布

### 10.1 开发环境准备

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装 Node.js (建议 v18+)
# 使用 nvm 安装: nvm install 18

# 初始化 Tauri 项目
npm create tauri-app@latest rtsp-viewer -- --template vue-ts --manager npm

# 进入项目目录
cd rtsp-viewer
```

### 10.2 安装依赖

```bash
# 安装前端依赖
cd src-ui
npm install

# 添加 Tauri 插件
cd ../src-tauri
cargo add tauri-plugin-global-shortcut
cargo add tauri-plugin-store
cargo add tauri-plugin-dialog
cargo add tauri-plugin-fs
cargo add tokio --features full
cargo add base64
```

### 10.3 构建命令

```bash
# 开发模式
npm run tauri dev

# 构建发布
npm run tauri build
```

### 10.4 Windows 打包配置 (electron-builder 对比 Tauri)

```json
// package.json (在 src-ui 目录)
{
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-dialog": "^2",
    "@tauri-apps/plugin-fs": "^2",
    "@tauri-apps/plugin-global-shortcut": "^2",
    "@tauri-apps/plugin-store": "^2"
  }
}
```

---

## 11. 完整项目初始化脚本

```bash
#!/bin/bash

# 创建项目
npm create tauri-app@latest onvif-viewer -- --template vue-ts

cd onvif-viewer

# 安装前端依赖
npm install

# 添加 Tauri 插件
npm install @tauri-apps/plugin-dialog @tauri-apps/plugin-fs @tauri-apps/plugin-global-shortcut @tauri-apps/plugin-store

# Rust 端添加依赖
cd src-tauri
cargo add tauri-plugin-global-shortcut
cargo add tauri-plugin-store  
cargo add tauri-plugin-dialog
cargo add tauri-plugin-fs
cargo add tokio --features full
cargo add base64
```

---

## 12. 关键实现要点

### 12.1 统一时间戳问题

用户特别强调：59秒进位时不能创建多个文件夹。

**解决方案**：
```typescript
// 在 captureAll() 开始时一次性获取时间
const now = new Date()
const timeStr = `${hours}-${minutes}-${seconds}`  // 包含秒

// 所有通道使用同一个 timeStr
```

### 12.2 后台运行拍照

全局快捷键在 Tauri 中通过 `tauri-plugin-global-shortcut` 实现，即使窗口不在焦点也能响应。

### 12.3 FFmpeg 路径处理

```rust
fn get_ffmpeg_path() -> String {
    if cfg!(target_os = "windows") {
        // Windows: 优先使用打包的 ffmpeg
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("ffmpeg.exe")))
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "ffmpeg".to_string())
    } else {
        // macOS/Linux: 使用系统 PATH 中的 ffmpeg
        "ffmpeg".to_string()
    }
}
```

---

## 14. FFmpeg 打包配置

### 14.1 方案选择

将 FFmpeg 打包进 Tauri 应用有三种常用方案：

| 方案 | 优点 | 缺点 |
|------|------|------|
| **静态编译 FFmpeg** | 单文件发行，无需额外依赖 | 编译耗时长，体积较大 |
| **下载预编译二进制** | 简单快捷 | 需要网络，版本固定 |
| **使用 ffmpeg-nextgen 库** | 纯 Rust，无需外部二进制 | 功能可能有局限 |

### 14.2 推荐方案：下载预编译二进制

这是最简单可靠的方式，类似当前 Electron 版本的实现。

#### 14.2.1 创建构建脚本

```javascript
// scripts/download-ffmpeg.js (在 src-ui 目录下创建)

const https = require('https');
const fs = require('fs');
const path = require('path');

const FFmpeg_VERSION = '7.1';
const FFmpeg_URLS = {
  win64: `https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl-shared.zip`,
  macos: `https://evermeet.cx/ffmpeg/getRelease/ffmpeg`,
  linux: `https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz`
};

function downloadFile(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    https.get(url, (response) => {
      response.pipe(file);
      file.on('finish', () => {
        file.close();
        resolve();
      });
    }).on('error', (err) => {
      fs.unlink(dest, () => {});
      reject(err);
    });
  });
}

async function main() {
  const platform = process.platform;
  const buildDir = path.join(__dirname, '../build-resources', platform === 'win32' ? 'win' : platform === 'darwin' ? 'mac' : 'linux');
  
  if (!fs.existsSync(buildDir)) {
    fs.mkdirSync(buildDir, { recursive: true });
  }
  
  const url = FFmpeg_URLS[platform === 'win32' ? 'win64' : platform === 'darwin' ? 'macos' : 'linux'];
  const dest = path.join(buildDir, platform === 'win32' ? 'ffmpeg.zip' : 'ffmpeg.tar.xz');
  
  console.log(`Downloading FFmpeg from ${url}...`);
  await downloadFile(url, dest);
  console.log('Download complete!');
}

main().catch(console.error);
```

#### 14.2.2 修改 package.json

```json
{
  "scripts": {
    "download-ffmpeg": "node scripts/download-ffmpeg.js",
    "dev": "vite",
    "build": "vite build && npm run download-ffmpeg",
    "tauri": "tauri"
  }
}
```

### 14.3 Rust 端获取 FFmpeg 路径

```rust
// src/utils/ffmpeg_path.rs

use std::path::PathBuf;

#[cfg(target_os = "windows")]
pub fn get_ffmpeg_path() -> String {
    // 1. 优先使用打包在 resources 中的 ffmpeg
    if let Ok(resources) = std::env::current_exe() {
        if let Some(parent) = resources.parent() {
            let bundled = parent.join("resources").join("ffmpeg.exe");
            if bundled.exists() {
                return bundled.to_string_lossy().to_string();
            }
            
            // 也检查同级目录
            let side_by_side = parent.join("ffmpeg.exe");
            if side_by_side.exists() {
                return side_by_side.to_string_lossy().to_string();
            }
        }
    }
    
    // 2. 回退到系统 PATH
    "ffmpeg".to_string()
}

#[cfg(target_os = "macos")]
pub fn get_ffmpeg_path() -> String {
    if let Ok(resources) = std::env::current_exe() {
        if let Some(parent) = resources.parent() {
            let bundled = parent.join("Resources").join("ffmpeg");
            if bundled.exists() {
                return bundled.to_string_lossy().to_string();
            }
        }
    }
    "ffmpeg".to_string()
}

#[cfg(target_os = "linux")]
pub fn get_ffmpeg_path() -> String {
    if let Ok(resources) = std::env::current_exe() {
        if let Some(parent) = resources.parent() {
            let bundled = parent.join("lib").join("ffmpeg");
            if bundled.exists() {
                return bundled.to_string_lossy().to_string();
            }
        }
    }
    "ffmpeg".to_string()
}
```

### 14.4 Tauri 配置中添加资源

```json
// src-tauri/tauri.conf.json

{
  "bundle": {
    "resources": {
      "build-resources/win/ffmpeg.exe": "resources/ffmpeg.exe",
      "build-resources/mac/ffmpeg": "resources/ffmpeg",
      "build-resources/linux/ffmpeg": "resources/ffmpeg"
    },
    "externalBin": [
      "build-resources/win/ffmpeg.exe",
      "build-resources/mac/ffmpeg",
      "build-resources/linux/ffmpeg"
    ]
  }
}
```

### 14.5 使用 static_ffmpeg 方案（可选）

如果希望完全静态链接，不依赖外部文件，可以使用 `static_ffmpeg` crate：

```toml
# Cargo.toml

[dependencies]
static_ffmpeg = "4"
```

```rust
// main.rs

fn main() {
    // 初始化 FFmpeg（自动下载如果不存在）
    static_ffmpeg::init().expect("Failed to initialize FFmpeg");
    
    // 之后可以正常使用 FFmpeg 命令
    // ...
}
```

**注意**：static_ffmpeg 方案会增加约 80MB 的包体积。

---

## 15. 完整构建命令

```bash
#!/bin/bash

# ============ 项目初始化 ============

# 1. 创建 Tauri 项目
npm create tauri-app@latest onvif-viewer -- --template vue-ts

cd onvif-viewer

# 2. 安装前端依赖
npm install

# 3. 添加 Tauri 插件
npm install @tauri-apps/plugin-dialog @tauri-apps/plugin-fs @tauri-apps/plugin-global-shortcut @tauri-apps/plugin-store

# 4. Rust 端添加依赖
cd src-tauri
cargo add tauri-plugin-global-shortcut
cargo add tauri-plugin-store
cargo add tauri-plugin-dialog
cargo add tauri-plugin-fs
cargo add tokio --features full
cargo add base64

# ============ 开发环境 ============

# 5. 下载 FFmpeg（Windows）
mkdir -p build-resources/win
# 手动下载 ffmpeg.exe 放到 build-resources/win/ 目录

# 6. 运行开发模式
cd ..
npm run tauri dev

# ============ 构建发布 ============

# 7. 构建
npm run tauri build
```

---

## 16. 验证清单

构建完成后验证以下功能：

- [ ] 3 路 RTSP 流可以同时播放
- [ ] 一键连接/断开正常工作
- [ ] 一键拍照保存到正确路径
- [ ] 全局快捷键在后台也能触发拍照
- [ ] 快捷键可自定义并保存
- [ ] 配置重启后自动加载
- [ ] FFmpeg 随应用一起打包

---

## 17. 常见问题排查

### 17.1 FFmpeg 找不到
```bash
# 检查打包后的资源
# Windows: release/bundle/nsis/*.exe 中包含 ffmpeg.exe
```

### 17.2 权限问题
```json
// src-tauri/capabilities/main.json 添加
{
  "permissions": [
    "fs:allow-execute",
    "shell:allow-execute"
  ]
}
```

### 17.3 RTSP 连接超时
```rust
// 增加超时时间
let ffmpeg_args = [
    "-rtsp_transport", "tcp",
    "-timeout", "10000000",  // 10秒
    "-reconnect", "1",
    "-reconnect_streamed", "1",
    "-reconnect_delay_max", "5",
    // ...
];
```
