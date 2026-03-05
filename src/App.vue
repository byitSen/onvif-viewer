<script setup lang="ts">
import { ref, reactive, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'

interface Channel {
  rtspUrl: string
  connected: boolean
  streamUrl: string
  index: number
}

const savePath = ref('')
const captureShortcut = ref('CommandOrControl+Shift+P')
const toastMessage = ref('')

function showToast(msg: string) {
  toastMessage.value = msg
  setTimeout(() => {
    toastMessage.value = ''
  }, 3000)
}

const channels = reactive<Channel[]>([
  { rtspUrl: '', connected: false, streamUrl: '', index: 0 },
  { rtspUrl: '', connected: false, streamUrl: '', index: 1 },
  { rtspUrl: '', connected: false, streamUrl: '', index: 2 },
])

  const hasAnyConnected = computed(() => channels.some(c => c.connected))

console.log('=== App.vue script setup ===')

onMounted(async () => {
  console.log('>>> onMounted fired')
  try {
    console.log('Loading config...')
    const config = await invoke<any>('load_config')
    console.log('Config loaded:', config)
    if (config.savePath) savePath.value = config.savePath
    if (config.channels) {
      config.channels.forEach((c: any, i: number) => {
        if (channels[i]) channels[i].rtspUrl = c.rtspUrl || ''
      })
    }
    if (config.captureShortcut) {
      captureShortcut.value = config.captureShortcut
    }
  } catch (e) {
    console.error('加载配置失败:', e)
  }

  await listen('global-capture', () => {
    console.log('Global capture event received')
    captureAll()
  })
})

async function saveConfig() {
  try {
    await invoke('save_config', {
      config: {
        savePath: savePath.value,
        captureShortcut: captureShortcut.value,
        channels: channels.map(c => ({ rtspUrl: c.rtspUrl }))
      }
    })
  } catch (e) {
    console.error('保存配置失败:', e)
  }
}

async function selectSavePath() {
  try {
    const result = await open({ directory: true, title: '选择保存路径' })
    if (result) {
      savePath.value = result as string
      await saveConfig()
    }
  } catch (e) {
    console.error('选择路径失败:', e)
  }
}

async function updateShortcut() {
  await invoke('update_shortcut', { shortcut: captureShortcut.value })
  await saveConfig()
}

function handleConnect(index: number) {
  console.log('>>> handleConnect called with index:', index)
  toggleConnection(index)
}

async function toggleConnection(index: number) {
  console.log('=== toggleConnection START ===', index)
  const channel = channels[index]
  console.log('Channel:', channel)

  if (channel.connected) {
    console.log('Stopping stream...')
    try {
      await invoke('stop_stream', { channelId: index })
      channel.connected = false
      channel.streamUrl = ''
      console.log('Stream stopped')
    } catch (e) {
      console.error('Stop failed:', e)
    }
  } else {
    if (!channel.rtspUrl) {
      console.log('No RTSP URL, returning')
      return
    }

    try {
      console.log('Starting stream with URL:', channel.rtspUrl)
      const url = await invoke<string>('start_stream', {
        channelId: index,
        rtspUrl: channel.rtspUrl
      })
      console.log('Stream started, URL:', url)
      channel.streamUrl = url
      channel.connected = true
      console.log('Channel connected set to true')
    } catch (e) {
      console.error('连接失败:', e)
      alert('连接失败: ' + e)
    }
  }
  await saveConfig()
}

async function connectAll() {
  for (const channel of channels) {
    if (channel.rtspUrl && !channel.connected) {
      await toggleConnection(channel.index)
      await new Promise(r => setTimeout(r, 500))
    }
  }
}

async function disconnectAll() {
  for (const channel of channels) {
    if (channel.connected) {
      await toggleConnection(channel.index)
      await new Promise(r => setTimeout(r, 300))
    }
  }
}

async function captureAll() {
  let targetPath = savePath.value
  if (!targetPath) {
    try {
      const homePath = await invoke<string>('get_home_path')
      targetPath = homePath + '/Pictures/RTSP_Viewer'
    } catch (e) {
      return
    }
  }

  for (const channel of channels) {
    if (!channel.connected) continue

    try {
      await invoke<string>('capture_frame', {
        channelId: channel.index,
        parentPath: targetPath
      })
      showToast(`通道 ${channel.index + 1} 拍照成功`)
    } catch (e) {
      console.error('保存图片失败:', e)
    }
  }
}
</script>

<template>
  <div class="app">
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

    <div v-if="toastMessage" class="toast">{{ toastMessage }}</div>

    <main class="video-container">
      <div class="channel" v-for="(channel, idx) in channels" :key="channel.index">
        <div class="channel-header">
          <span>通道 {{ channel.index + 1 }}</span>
        </div>
        <div class="channel-body">
          <div class="video-wrapper">
            <img
              v-if="channel.connected"
              :src="channel.streamUrl"
              :id="'video-' + channel.index"
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
                @click="handleConnect(channel.index)"
              >
                {{ channel.connected ? '断开' : '连接' }}
              </button>
              <button
                class="btn-snap"
                @click="captureAll"
                :disabled="!channel.connected"
              >
                拍照
              </button>
            </div>
          </div>
        </div>
      </div>
    </main>

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

<style scoped>
* { margin: 0; padding: 0; box-sizing: border-box; }

.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  background: #1a1a2e;
  color: #fff;
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
.btn-success:hover { background: #16a34a; }
.btn-danger { background: #ef4444; color: #fff; }
.btn-danger:hover { background: #dc2626; }
.btn-primary { background: #0f3460; color: #fff; }
.btn-primary:hover { background: #1a4a7a; }

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
  position: relative;
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

.rtsp-input:focus { outline: none; border-color: #e94560; }

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

.btn-connect:hover { background: #1a4a7a; }
.btn-connect.connected { background: #e94560; }

.btn-snap {
  padding: 8px 12px;
  background: #0f3460;
  color: #fff;
  border: none;
  border-radius: 6px;
  cursor: pointer;
}

.btn-snap:hover { background: #1a4a7a; }
.btn-snap:disabled { opacity: 0.5; cursor: not-allowed; }

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

.btn-capture:hover { background: #ff5a75; }
.btn-capture:disabled { opacity: 0.5; cursor: not-allowed; }

.toast {
  position: fixed;
  top: 80px;
  left: 50%;
  transform: translateX(-50%);
  background: #22c55e;
  color: #fff;
  padding: 12px 24px;
  border-radius: 8px;
  font-size: 14px;
  z-index: 1000;
  animation: fadeIn 0.3s ease;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateX(-50%) translateY(-10px); }
  to { opacity: 1; transform: translateX(-50%) translateY(0); }
}
</style>
