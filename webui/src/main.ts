// src/main.ts
import { createApp } from 'vue'
import { createPinia } from 'pinia'
import Vant from 'vant'
import 'vant/lib/index.css'

import App from './App.vue'
import router from './router'
import i18n from './i18n' // [新增] 引入 i18n

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.use(Vant)
app.use(i18n) // [新增] 挂载 i18n

app.mount('#app')