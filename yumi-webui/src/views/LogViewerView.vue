<script setup lang="ts">
import { ref, onMounted, nextTick, computed } from 'vue';
import { Bridge } from '@/utils/bridge';
import { useI18n } from 'vue-i18n';

const { t } = useI18n();
const logContent = ref('');
const loading = ref(false);
const terminalBody = ref<HTMLElement | null>(null);

const fetchLog = async () => {
  loading.value = true;
  try {
    const text = await Bridge.getDaemonLog();
    logContent.value = text || '';
    
    // 自动滚动到底部
    await nextTick();
    if (terminalBody.value) {
      terminalBody.value.scrollTop = terminalBody.value.scrollHeight;
    }
  } finally {
    loading.value = false;
  }
};

// 使用正则对日志内容进行高亮解析
const formattedLog = computed(() => {
  if (!logContent.value) return `<div class="log-empty">${t('log_empty')}</div>`;
  
  return logContent.value.split('\n').map(line => {
    // 过滤掉空行
    if (!line.trim()) return '';

    // 1. 高亮时间戳 [2026-02-23 02:31:07]
    let html = line.replace(/\[\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\]/g, match => `<span class="log-time">${match}</span>`);
    
    // 2. 高亮日志等级 [INFO], [WARN], [ERROR]
    html = html.replace(/\[INFO\]/g, `<span class="log-info">[INFO]</span>`);
    html = html.replace(/\[WARN\]/g, `<span class="log-warn">[WARN]</span>`);
    html = html.replace(/\[ERROR\]/g, `<span class="log-error">[ERROR]</span>`);
    
    // 3. 高亮模块标签 [yumi::xxx] 或 [Scheduler] 等
    html = html.replace(/\[(yumi[^\]]*|Scheduler|AppDetect|Screen|Boot)\]/g, match => `<span class="log-tag">${match}</span>`);

    return `<div class="log-line">${html}</div>`;
  }).join('');
});

onMounted(() => {
  fetchLog();
});
</script>

<template>
  <div class="log-viewer">
    <van-nav-bar
      :title="t('view_log')"
      left-arrow
      @click-left="$router.back()"
      fixed
      placeholder
      z-index="100"
    >
      <template #right>
        <van-icon name="replay" size="18" @click="fetchLog" />
      </template>
    </van-nav-bar>

    <van-loading v-if="loading && !logContent" class="loading-center" vertical>{{ t('loading') }}</van-loading>

    <div v-else class="terminal-card">
      <div class="terminal-header">
        <div class="mac-buttons">
          <span class="btn close"></span>
          <span class="btn minimize"></span>
          <span class="btn maximize"></span>
        </div>
        <div class="terminal-title">{{ t('log_terminal_title', { file: 'daemon.log', shell: 'bash' }) }}</div>
      </div>
      
      <div class="terminal-body" ref="terminalBody">
        <div class="log-container" v-html="formattedLog"></div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.log-viewer {
  min-height: 100vh;
  /* 背景颜色与主页保持一致，让卡片凸显出来 */
  background-color: #f7f8fa; 
  padding-bottom: 20px;
}

.loading-center {
  padding-top: 100px;
}

/* 终端卡片主体 */
.terminal-card {
  margin: 16px;
  background-color: #1e1e1e; /* 终端深黑灰 */
  border-radius: 12px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  /* 高度适配：屏幕高度减去导航栏和外边距 */
  height: calc(100vh - 100px); 
}

/* 伪 Mac 窗口控制栏 */
.terminal-header {
  background-color: #2d2d2d;
  height: 36px;
  display: flex;
  align-items: center;
  padding: 0 16px;
  position: relative;
}

.mac-buttons {
  display: flex;
  gap: 8px;
}

.mac-buttons .btn {
  width: 12px;
  height: 12px;
  border-radius: 50%;
  display: inline-block;
}

.mac-buttons .close { background-color: #ff5f56; }
.mac-buttons .minimize { background-color: #ffbd2e; }
.mac-buttons .maximize { background-color: #27c93f; }

.terminal-title {
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
  color: #999;
  font-size: 13px;
  font-family: sans-serif;
  font-weight: 500;
}

/* 终端日志内容区 */
.terminal-body {
  flex: 1;
  padding: 12px 16px;
  overflow-y: auto;
  /* 定制滚动条 */
  scrollbar-width: thin;
  scrollbar-color: #555 #1e1e1e;
}

.terminal-body::-webkit-scrollbar {
  width: 6px;
}
.terminal-body::-webkit-scrollbar-track {
  background: #1e1e1e;
}
.terminal-body::-webkit-scrollbar-thumb {
  background-color: #555;
  border-radius: 10px;
}

.log-container {
  font-family: 'JetBrains Mono', 'Fira Code', 'Courier New', Consolas, monospace;
  font-size: 12px;
  line-height: 1.6;
  color: #d4d4d4; /* 默认普通文本颜色 */
  word-wrap: break-word;
  white-space: pre-wrap;
}

/* === 语法高亮 CSS === */
:deep(.log-empty) { color: #888; font-style: italic; }
:deep(.log-line) { margin-bottom: 2px; }
:deep(.log-time) { color: #6a9955; /* 绿色系时间戳 */ }
:deep(.log-info) { color: #569cd6; font-weight: bold; /* 蓝色 INFO */ }
:deep(.log-warn) { color: #dcdcaa; font-weight: bold; /* 黄色 WARN */ }
:deep(.log-error) { color: #f44747; font-weight: bold; /* 红色 ERROR */ }
:deep(.log-tag) { color: #c586c0; /* 紫色模块标签 */ }
</style>
