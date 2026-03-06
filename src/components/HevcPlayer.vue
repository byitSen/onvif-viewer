<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch } from 'vue'

const props = defineProps<{
  src: string
}>()

const videoRef = ref<HTMLVideoElement | null>(null)
const error = ref('')
const isLoading = ref(true)

const checkSupport = (): boolean => {
  const video = document.createElement('video')
  return video.canPlayType('video/mp4; codecs="hevc.1.e08"') !== '' ||
         video.canPlayType('video/mp4; codecs="hevc.1.m09"') !== ''
}

const initPlayer = async () => {
  isLoading.value = true
  error.value = ''
  
  if (!videoRef.value) return
  
  if (!checkSupport()) {
    error.value = '您的浏览器不支持 H.265/HEVC 播放。请使用 Safari 浏览器或安装 H.265 解码器。'
    isLoading.value = false
    return
  }
  
  try {
    videoRef.value.src = props.src
    videoRef.value.addEventListener('loadeddata', () => {
      isLoading.value = false
    })
    videoRef.value.addEventListener('error', () => {
      error.value = '视频加载失败'
      isLoading.value = false
    })
    
    await videoRef.value.play()
  } catch (e) {
    error.value = '播放失败: ' + String(e)
    isLoading.value = false
  }
}

onMounted(() => {
  initPlayer()
})

onUnmounted(() => {
  if (videoRef.value) {
    videoRef.value.pause()
    videoRef.value.src = ''
  }
})

watch(() => props.src, (newSrc) => {
  if (videoRef.value && newSrc) {
    initPlayer()
  }
})
</script>

<template>
  <div class="hevc-player">
    <video
      ref="videoRef"
      autoplay
      playsinline
      controls
      muted
    />
    <div v-if="isLoading" class="loading">加载中...</div>
    <div v-if="error" class="error">{{ error }}</div>
  </div>
</template>

<style scoped>
.hevc-player {
  width: 100%;
  height: 100%;
  background: #000;
  display: flex;
  align-items: center;
  justify-content: center;
  position: relative;
}

video {
  width: 100%;
  height: 100%;
  object-fit: contain;
}

.loading {
  position: absolute;
  color: #fff;
  font-size: 14px;
}

.error {
  position: absolute;
  color: #ff4444;
  padding: 20px;
  text-align: center;
  background: rgba(0,0,0,0.8);
}
</style>
