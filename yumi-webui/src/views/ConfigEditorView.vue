<script setup lang="ts">
import { ref, onMounted, h, defineComponent } from 'vue';
import { useI18n } from 'vue-i18n';
import { Bridge } from '@/utils/bridge';
// 1. 从 vant 中移除 showToast
import { Collapse, CollapseItem, Cell, Switch, Icon, showConfirmDialog } from 'vant';
// 2. 引入原生内核层的 toast
import { toast } from '@/kernelsu';

const { t } = useI18n();
const activeTab = ref('rules');
const currentData = ref<any>({});
const loading = ref(false);
const activeNames = ref<string[]>([]);

const showEditDialog = ref(false);
const editingKeyPath = ref('');
const editingValue = ref('');
const editingType = ref<'string' | 'number' | 'array'>('string');

const modeActions = [{ name: 'powersave', color: '#4CAF50' }, { name: 'balance', color: '#2196F3' }, { name: 'performance', color: '#FF9800' }, { name: 'fast', color: '#F44336' }];
const appModeActions = [{ name: 'powersave', color: '#4CAF50' }, { name: 'balance', color: '#2196F3' }, { name: 'performance', color: '#FF9800' }, { name: 'fast', color: '#F44336' }, { name: 'fas', color: '#E91E63' }, { name: '删除该规则', color: '#FF0000', isDelete: true }];
const languageActions = [{ name: 'zh' }, { name: 'en' }];
const loglevelActions = [{ name: 'OFF' }, { name: 'ERROR' }, { name: 'WARN' }, { name: 'INFO' }, { name: 'DEBUG' }, { name: 'TRACE' }];

const showModeSheet = ref(false);
const showAppModeSheet = ref(false);
const showLanguageSheet = ref(false);
const showLoglevelSheet = ref(false);

const isObj = (v: any) => v && typeof v === 'object' && !Array.isArray(v);

const collectExpandPaths = (obj: any, basePath: string): string[] => {
  const result: string[] = [];
  if (!isObj(obj)) return result;
  Object.entries(obj).forEach(([key, val]) => {
    if (isObj(val)) {
      const p = basePath ? `${basePath}/${key}` : key;
      const depth = p.split('/').length - 1;
      if (depth < 2) {
        result.push(p);
        result.push(...collectExpandPaths(val, p));
      }
    }
  });
  return result;
};

const loadData = async () => {
  loading.value = true;
  try {
    currentData.value = activeTab.value === 'rules'
      ? await Bridge.getRulesConfig()
      : await Bridge.getMainConfig();
    activeNames.value = collectExpandPaths(currentData.value, '');
  } catch (e) {
    // 3. 替换为原生 toast
    toast(t('load_failed'));
  } finally {
    loading.value = false;
  }
};

onMounted(loadData);

const saveConfig = async () => {
  try {
    if (activeTab.value === 'rules') await Bridge.saveRulesConfig(currentData.value);
    else await Bridge.saveMainConfig(currentData.value);
    // 3. 替换为原生 toast
    toast(t('saved'));
  } catch (e) {
    // 3. 替换为纯文本的原生 toast
    toast(t('save_failed'));
  }
};

const setDeepValue = (obj: any, path: string, value: any) => {
  const keys = path.split('/');
  let current = obj;
  for (let i = 0; i < keys.length - 1; i++) {
    const k = keys[i] as string;
    if (!current[k]) current[k] = {};
    current = current[k];
  }
  current[keys[keys.length - 1] as string] = value;
};

const handleItemClick = (fullPath: string, value: any) => {
  if (typeof value === 'boolean') return;

  editingKeyPath.value = fullPath;

  if (fullPath === 'global_mode') { showModeSheet.value = true; return; }
  if (fullPath.startsWith('app_modes/')) { showAppModeSheet.value = true; return; }
  if (fullPath === 'meta/language') { showLanguageSheet.value = true; return; }
  if (fullPath === 'meta/loglevel') { showLoglevelSheet.value = true; return; }

  const isArrayField = Array.isArray(value) || fullPath === 'ignored_apps' || fullPath.endsWith('fps_gears') || fullPath.endsWith('target_fps') || fullPath.endsWith('cluster_profiles');

  if (isArrayField) { 
    editingType.value = 'array'; 
    if (fullPath.endsWith('cluster_profiles') && Array.isArray(value)) {
      editingValue.value = value.map((v: any) => v.capacity_weight ?? 1.0).join(', ');
    } else {
      editingValue.value = Array.isArray(value) ? value.join(', ') : ''; 
    }
  }
  else if (typeof value === 'number') { 
    editingType.value = 'number'; 
    editingValue.value = String(value); 
  }
  else { 
    editingType.value = 'string'; 
    editingValue.value = String(value); 
  }
  showEditDialog.value = true;
};

const confirmEdit = () => {
  let val: any = editingValue.value;
  if (editingType.value === 'number') val = Number(val);
  
  if (editingType.value === 'array') {
    const strArray = val.split(',').map((s: string) => s.trim()).filter((s: string) => s !== '');
    
    if (editingKeyPath.value.includes('fps')) {
      val = strArray.map(Number).filter((n: number) => !isNaN(n));
    } else if (editingKeyPath.value.endsWith('cluster_profiles')) {
      val = strArray.map(Number).filter((n: number) => !isNaN(n)).map((n: number) => ({ capacity_weight: n }));
    } else {
      val = strArray;
    }
  }
  setDeepValue(currentData.value, editingKeyPath.value, val);
  saveConfig();
  showEditDialog.value = false;
};

const onSelectMode = (a: any) => { currentData.value.global_mode = a.name; saveConfig(); showModeSheet.value = false; };
const onSelectAppMode = (a: any) => {
  const pkg = editingKeyPath.value.split('/').pop() || '';
  if (a.isDelete) delete currentData.value.app_modes[pkg];
  else currentData.value.app_modes[pkg] = a.name;
  saveConfig(); showAppModeSheet.value = false;
};
const onSelectLanguage = (a: any) => { setDeepValue(currentData.value, 'meta/language', a.name); saveConfig(); showLanguageSheet.value = false; };
const onSelectLoglevel = (a: any) => { setDeepValue(currentData.value, 'meta/loglevel', a.name); saveConfig(); showLoglevelSheet.value = false; };

const RecursiveItem = defineComponent({
  name: 'RecursiveItem',
  props: ['name', 'value', 'path'],
  setup(props) {
    return () => {
      if (isObj(props.value)) {
        const pathParts = props.path.split('/');
        const isPerAppProfileNode = pathParts.length >= 2 && pathParts[pathParts.length - 2] === 'per_app_profiles';

        const children = Object.entries(props.value).map(([subKey, subVal]) =>
          h(RecursiveItem, { key: subKey, name: subKey, value: subVal, path: `${props.path}/${subKey}` })
        );

        const collapseProps: any = { title: props.name, name: props.path, class: 'nested-group' };
        const slots: any = { default: () => children };

        if (isPerAppProfileNode) {
          slots.value = () => h('div', {
            onClick: (e: Event) => e.stopPropagation()
          }, [
            h(Icon, {
              name: 'delete-o',
              color: '#ee0a24',
              size: '18',
              style: { padding: '4px', cursor: 'pointer' },
              onClick: () => {
                showConfirmDialog({
                  title: '确认删除',
                  message: `确定要删除 ${props.name} 的专属配置吗？`,
                  confirmButtonColor: '#ee0a24'
                }).then(() => {
                  const keys = props.path.split('/');
                  const targetKey = keys.pop() as string;
                  let obj = currentData.value;
                  for (const k of keys) obj = obj[k];
                  delete obj[targetKey];
                  saveConfig();
                }).catch(() => {});
              }
            })
          ]);
        }

        return h(CollapseItem, collapseProps, slots);
      }

      return h(Cell, {
        title: props.name,
        center: true,
        isLink: typeof props.value !== 'boolean',
        onClick: () => handleItemClick(props.path, props.value)
      }, {
        value: () => {
          if (typeof props.value === 'boolean') return null;
          let displayVal = '';
          if (Array.isArray(props.value)) {
            if (props.path.endsWith('cluster_profiles')) {
              displayVal = `[${props.value.map((v: any) => v.capacity_weight ?? '?').join(', ')}]`;
            } else {
              displayVal = `[${props.value.join(', ')}]`;
            }
          } else {
            displayVal = String(props.value);
          }
          return h('span', displayVal);
        },
        'right-icon': () => typeof props.value === 'boolean' ? h(Switch, {
          modelValue: props.value,
          size: '20px',
          'onUpdate:modelValue': (newVal: boolean) => {
            setDeepValue(currentData.value, props.path, newVal);
            saveConfig();
          }
        }) : null
      });
    };
  }
});
</script>

<template>
  <div class="config-editor">
    <van-nav-bar title="详细配置" left-arrow @click-left="$router.back()" fixed placeholder>
      <template #right><van-icon name="replay" size="18" @click="loadData" /></template>
    </van-nav-bar>

    <div class="tab-container">
      <van-tabs v-model:active="activeTab" type="card" animated @change="loadData" color="#1989fa">
        <van-tab title="调度规则 (Rules)" name="rules" />
        <van-tab title="核心配置 (Config)" name="config" />
      </van-tabs>
    </div>

    <van-loading v-if="loading" class="loading-center" vertical>加载中...</van-loading>

    <div v-else class="config-content">
      <van-collapse v-model="activeNames" :border="false">
        <RecursiveItem v-for="(val, key) in currentData" :key="key" :name="String(key)" :value="val" :path="String(key)" />
      </van-collapse>
      <div style="height: 60px;"></div>
    </div>

    <van-dialog v-model:show="showEditDialog" title="编辑" show-cancel-button @confirm="confirmEdit">
      <div class="dialog-content">
        <div class="path-hint">{{ editingKeyPath.replace(/\//g, ' > ') }}</div>
        <van-field v-model="editingValue" :type="editingType === 'number' ? 'number' : 'text'" input-align="center" border autofocus />
      </div>
    </van-dialog>

    <van-action-sheet v-model:show="showModeSheet" :actions="modeActions" cancel-text="取消" @select="onSelectMode" />
    <van-action-sheet v-model:show="showAppModeSheet" :actions="appModeActions" cancel-text="取消" @select="onSelectAppMode" />
    <van-action-sheet v-model:show="showLanguageSheet" :actions="languageActions" cancel-text="取消" @select="onSelectLanguage" />
    <van-action-sheet v-model:show="showLoglevelSheet" :actions="loglevelActions" cancel-text="取消" @select="onSelectLoglevel" />
  </div>
</template>

<style scoped>
/* 样式保持不变 */
.config-editor { min-height: 100vh; background: #f7f8fa; }
.tab-container { padding: 12px 16px; background: #fff; margin-bottom: 8px; }
.loading-center { padding-top: 100px; }
.config-content { padding: 0 12px; }
.nested-group { margin-bottom: 4px; border-radius: 8px; overflow: hidden; }
:deep(.van-collapse-item__content) { padding: 0 0 0 16px; background: #fafafa; }
:deep(.van-cell) { margin-bottom: 1px; border-radius: 4px; }
.dialog-content { padding: 20px 16px; }
.path-hint { font-size: 11px; color: #999; text-align: center; margin-bottom: 12px; }
:deep(.van-action-sheet__subname) { font-size: 11px; color: #999; }
</style>