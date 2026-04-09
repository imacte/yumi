// src/router/index.ts
import { createRouter, createWebHashHistory } from 'vue-router'
import HomeView from '../views/HomeView.vue'
import AppRulesView from '../views/AppRulesView.vue'
import ConfigEditorView from '../views/ConfigEditorView.vue'
import LogViewerView from '../views/LogViewerView.vue' // [新增] 引入日志页面

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/', name: 'home', component: HomeView },
    { path: '/apps', name: 'apps', component: AppRulesView },
    { path: '/config', name: 'config', component: ConfigEditorView },
    { path: '/log', name: 'log', component: LogViewerView } // [新增] 日志路由
  ]
})

export default router