// src/utils/mock.ts
const mockRules = {
  yumi_scheduler: true,
  dynamic_enabled: true,
  global_mode: "powersave",
  app_modes: {
    'com.miHoYo.GenshinImpact': 'fas',
    'com.tencent.tmgp.sgame': 'fas',
    'com.tencent.tmgp.speedmobile': 'fas'
  },
  ignored_apps: ['com.android.systemui'],
  fas_rules: {
    fps_gears: [30.0, 60.0, 90.0, 120.0, 144.0],
    fps_margin: 3.0,
    per_app_profiles: {
      "com.miHoYo.GenshinImpact": { target_fps: [30, 60], fps_margin: 4.0 },
      "com.tencent.tmgp.sgame": { target_fps: [60, 90, 120], fps_margin: 3.0 }
    },
    pid: { kp: 0.035, ki: 0.015, kd: 0.005 },
    auto_capacity_weight: true,
    cluster_profiles: [ { capacity_weight: 1.0 }, { capacity_weight: 1.5 }, { capacity_weight: 2.5 }, { capacity_weight: 3.5 } ],
    perf_floor: 0.22,
    perf_ceil: 1.0
  }
};

const mockConfig = {
  meta: { name: "default_config", author: "yuki", language: "en", loglevel: "INFO" },
  function: { CpuIdleScalingGovernor: false, IOOptimization: true },
  IO_Settings: { Scheduler: "none", read_ahead_kb: "128", nomerges: "2", iostats: "0" },
  CpuIdle: { current_governor: "" },
  powersave: {
    cpu_load_governor: { up_threshold: 0.85, down_threshold: 0.60, smoothing_up: 0.40, smoothing_down: 0.50, down_rate_limit_ticks: 2, headroom_factor: 1.10, perf_floor: 0.10, perf_ceil: 0.70, perf_init: 0.30 }
  },
  balance: {
    cpu_load_governor: { up_threshold: 0.80, down_threshold: 0.50, smoothing_up: 0.60, smoothing_down: 0.30, down_rate_limit_ticks: 3, headroom_factor: 1.25, perf_floor: 0.15, perf_ceil: 1.0, perf_init: 0.50 }
  },
  performance: {
    cpu_load_governor: { up_threshold: 0.65, down_threshold: 0.40, smoothing_up: 0.80, smoothing_down: 0.20, down_rate_limit_ticks: 5, headroom_factor: 1.40, perf_floor: 0.35, perf_ceil: 1.0, perf_init: 0.60 }
  },
  fast: {
    cpu_load_governor: { up_threshold: 0.01, down_threshold: 0.01, smoothing_up: 1.0, smoothing_down: 0.01, down_rate_limit_ticks: 10, headroom_factor: 2.0, perf_floor: 1.0, perf_ceil: 1.0, perf_init: 1.0 }
  }
};

const mockApps = ['com.android.chrome', 'com.tencent.mm', 'com.miHoYo.GenshinImpact'];
const delay = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
let simulatedModeTxt = "balance";

export const MockBridge = {
  async isDaemonRunning(): Promise<boolean> { await delay(100); return true; },
  async getCurrentMode(): Promise<string> { await delay(200); return simulatedModeTxt; },
  async setMode(mode: string): Promise<void> { await delay(200); mockRules.global_mode = mode; setTimeout(() => { simulatedModeTxt = mode; }, 800); },
  async getInstalledApps(): Promise<string[]> { await delay(500); return mockApps; },
  async getAppRules(): Promise<Record<string, string>> { await delay(300); return mockRules.app_modes; },
  async saveAppRule(pkg: string, mode: string): Promise<void> { 
    await delay(200); 
    
    // 更新或删除应用模式
    if (mode === '') {
      delete (mockRules.app_modes as any)[pkg];
    } else {
      (mockRules.app_modes as any)[pkg] = mode; 
    }

    // 同步初始化 per_app_profiles 逻辑
    if (mode === 'fas') {
      if (!mockRules.fas_rules) (mockRules.fas_rules as any) = {};
      if (!mockRules.fas_rules.per_app_profiles) (mockRules.fas_rules.per_app_profiles as any) = {};
      
      if (!(mockRules.fas_rules.per_app_profiles as any)[pkg]) {
        (mockRules.fas_rules.per_app_profiles as any)[pkg] = {
          target_fps: [30, 60, 90, 120],
          fps_margin: 3.0
        };
      }
    }
  },  async getRulesConfig(): Promise<any> { await delay(300); return JSON.parse(JSON.stringify(mockRules)); },
  async saveRulesConfig(config: any): Promise<void> { await delay(400); Object.assign(mockRules, config); },
  async getMainConfig(): Promise<any> { await delay(300); return JSON.parse(JSON.stringify(mockConfig)); },
  async saveMainConfig(config: any): Promise<void> { await delay(400); Object.assign(mockConfig, config); },
  async getDaemonLog(): Promise<string> {
    await delay(300);
    return `[2026-02-23 02:31:07] [INFO] [yumi] daemon is running smoothly.\n[2026-02-23 02:48:18] [INFO] [Scheduler] Active mode: ${simulatedModeTxt}`;
  },
  async getCpuPolicies(): Promise<number[]> { return []; },
  async getAvailableFreqs(policyNum: number): Promise<string[]> { return []; },
  async getAvailableGovernors(policyNum: number): Promise<string[]> { return []; }
};