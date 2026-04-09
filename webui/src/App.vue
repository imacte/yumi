<script setup lang="ts">
// 可以在这里做全局初始化，目前暂时留空
</script>

<template>
  <van-config-provider>
    <router-view />
  </van-config-provider>
</template>

<style>
/* 全局重置样式 */
body {
  margin: 0;
  padding: 0;
  background-color: #f7f8fa; /* 浅灰色背景，类似 MIUI 设置页 */
  font-family: -apple-system, BlinkMacSystemFont, 'Helvetica Neue', Helvetica, Segoe UI, Arial, Roboto, 'PingFang SC', 'miui', 'Hiragino Sans GB', 'Microsoft Yahei', sans-serif;
}
</style>