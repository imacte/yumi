<script setup lang="ts">
import { onMounted, computed } from 'vue';
import { useSchedulerStore } from '@/stores/scheduler';
import { useI18n } from 'vue-i18n'; 
import { toast } from '@/kernelsu'; // 引入原生 ksu toast 替代 Vant

const store = useSchedulerStore();
const { t, locale } = useI18n(); 

// 语言切换逻辑
const toggleLanguage = () => {
  const newLang = locale.value === 'zh' ? 'en' : 'zh';
  locale.value = newLang;
  localStorage.setItem('app_lang', newLang); 
};

// 模式列表 (响应式翻译)
const modes = computed(() => [
  { key: 'powersave', name: t('mode_powersave'), desc: t('desc_powersave'), icon: 'shield-o', color: '#4CAF50' },
  { key: 'balance', name: t('mode_balance'), desc: t('desc_balance'), icon: 'balance-o', color: '#2196F3' },
  { key: 'performance', name: t('mode_performance'), desc: t('desc_performance'), icon: 'fire', color: '#FF9800' },
  { key: 'fast', name: t('mode_fast'), desc: t('desc_fast'), icon: 'upgrade', color: '#F44336' },
]);

onMounted(() => {
  store.initData();
});

const handleModeSelect = async (modeKey: string) => {
  await store.switchMode(modeKey);
  // 删除了 Vant 的 showToast，底层 Bridge.setMode 已经自带了原生 toast 提示
};

// 点击复制 QQ 群号
const copyQQGroup = async () => {
  try {
    // 现代浏览器剪贴板 API
    await navigator.clipboard.writeText('103609137');
    // 使用原生的 ksu toast，避免 Vant 白块 bug
    toast(t('copied'));
  } catch (err) {
    // 兼容性降级处理
    toast('QQ: 103609137');
  }
};
</script>

<template>
  <div class="home-container">
    
    <div class="top-header">
      <div class="lang-btn" @click="toggleLanguage">
        <van-icon name="exchange" size="16" />
        <span>{{ locale === 'zh' ? 'EN' : '中' }}</span>
      </div>
    </div>

    <div class="welcome-card">
      <div class="welcome-content">
        <h2>{{ t('welcome') }}</h2>
        <van-icon name="smile-o" size="36" color="rgba(255,255,255,0.8)"/>
      </div>
    </div>
    
    <div class="header-cards">
      <div class="status-card daemon-card" :style="{ background: store.isDaemonRunning ? '#10b981' : '#9ca3af' }">
        <van-icon :name="store.isDaemonRunning ? 'checked' : 'warning-o'" size="32" />
        <div class="info">
          <h2>yumi</h2>
          <p>{{ store.isDaemonRunning ? t('daemon_running') : t('daemon_stopped') }}</p>
        </div>
      </div>

      <div class="status-card mode-card" :style="{ background: modes.find(m => m.key === store.currentMode)?.color || '#2196F3' }">
        <van-icon :name="modes.find(m => m.key === store.currentMode)?.icon || 'balance-o'" size="32" />
        <div class="info">
          <h2>{{ modes.find(m => m.key === store.currentMode)?.name || t('unknown_mode') }}</h2>
          <p>{{ t('current_status') }}</p>
        </div>
      </div>
    </div>

    <div class="section-title">{{ t('global_mode') }}</div>
    <van-grid :column-num="2" :gutter="12" :border="false" class="mode-grid">
      <van-grid-item v-for="mode in modes" :key="mode.key">
        <div class="mode-card-content" 
             :class="{ 'is-active': store.currentMode === mode.key }" 
             :style="store.currentMode === mode.key ? { backgroundColor: mode.color } : {}"
             @click="handleModeSelect(mode.key)">
          <van-icon :name="mode.icon" size="26" :color="store.currentMode === mode.key ? '#fff' : mode.color" />
          <span class="mode-name" :style="{ color: store.currentMode === mode.key ? '#fff' : '#323233' }">{{ mode.name }}</span>
          <span class="mode-desc" :style="{ color: store.currentMode === mode.key ? 'rgba(255,255,255,0.8)' : '#969799' }">{{ mode.desc }}</span>
        </div>
      </van-grid-item>
    </van-grid>
    
    <div class="section-title">{{ t('about') }}</div>
    <div class="about-card">
      <van-cell-group inset :border="false">
        <van-cell 
          :title="t('qq_group')" 
          value="103609137" 
          icon="qq" 
          clickable 
          @click="copyQQGroup" 
        />
        <van-cell 
          :title="t('tg_group')" 
          :value="t('click_to_join')" 
          icon="chat-o" 
          is-link 
          url="https://t.me/+gp4adLJAsXYzMjc1" 
        />
        <van-cell 
          :title="t('github_repo')" 
          :value="t('click_to_view')" 
          icon="cluster-o" 
          is-link 
          url="https://github.com/imacte/YukiCtrl" 
        />
      </van-cell-group>
    </div>

    <div class="section-title">{{ t('more_features') }}</div>
    <div class="grid-menu">
        <van-grid clickable :column-num="3" :gutter="12" :border="false">
            <van-grid-item icon="apps-o" :text="t('app_management')" to="/apps" />
            <van-grid-item icon="setting-o" :text="t('detailed_config')" to="/config" />
            <van-grid-item icon="notes-o" :text="t('view_log')" to="/log" />
        </van-grid>
    </div>
  </div>
</template>

<style scoped>
.home-container {
  padding-bottom: 50px;
  background-color: #f7f8fa;
  min-height: 100vh;
}

/* 顶栏与语言切换按钮 */
.top-header { display: flex; justify-content: flex-end; padding: 16px 16px 0; }
.lang-btn {
  display: flex; align-items: center; gap: 4px;
  background: #fff; padding: 6px 12px; border-radius: 20px;
  font-size: 13px; font-weight: 600; color: #333;
  box-shadow: 0 2px 8px rgba(0,0,0,0.05); cursor: pointer; transition: all 0.2s;
}
.lang-btn:active { background: #f0f0f0; }

/* 欢迎卡片 */
.welcome-card {
  margin: 16px 16px 4px; padding: 24px 20px; border-radius: 16px;
  background: linear-gradient(135deg, #1989fa 0%, #005ce6 100%);
  color: white; box-shadow: 0 6px 16px rgba(25, 137, 250, 0.2);
}
.welcome-content { display: flex; justify-content: space-between; align-items: center; }
.welcome-content h2 { margin: 0; font-size: 20px; font-weight: 600; letter-spacing: 0.5px; }

/* 顶部双卡片布局 */
.header-cards { display: flex; gap: 12px; margin: 16px; }
.status-card {
  flex: 1; padding: 16px 8px; border-radius: 16px; color: white;
  display: flex; flex-direction: column; align-items: center; justify-content: center;
  box-shadow: 0 6px 16px rgba(0,0,0,0.12); transition: all 0.3s ease; text-align: center;
}
.status-card .info { margin-top: 8px; }
.status-card h2 { margin: 0; font-size: 16px; font-weight: 600; }
.status-card p { margin: 4px 0 0; opacity: 0.9; font-size: 12px; }

/* 强制清除 Vant 的默认边距 */
:deep(.van-grid-item__content) { padding: 0 !important; background-color: transparent !important; }

/* 模式卡片 */
.mode-card-content {
  width: 100%; height: 96px; display: flex; flex-direction: column;
  align-items: center; justify-content: center; background-color: #fff;
  border-radius: 12px; box-shadow: 0 2px 8px rgba(0,0,0,0.04);
  transition: all 0.3s cubic-bezier(0.25, 0.8, 0.25, 1); box-sizing: border-box;
  cursor: pointer;
}
.mode-card-content:active { transform: scale(0.95); opacity: 0.9; }
.mode-card-content.is-active { box-shadow: 0 6px 16px rgba(0,0,0,0.15); transform: translateY(-2px); }
.mode-name { margin-top: 8px; font-size: 14px; font-weight: 600; }
.mode-desc { margin-top: 4px; font-size: 11px; }

/* 关于卡片修复自带阴影 */
.about-card :deep(.van-cell-group--inset) {
  margin: 0 16px;
  box-shadow: 0 2px 8px rgba(0,0,0,0.04);
}

.section-title {
  margin: 20px 16px 10px; font-size: 14px; color: #969799; font-weight: 500;
}
</style>