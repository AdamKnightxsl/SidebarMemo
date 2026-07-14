# Sidebar Memo - 极简 Windows 侧边栏备忘录

一款轻量级 Windows 侧边栏备忘录，采用毛玻璃设计，全局快捷键唤出。

## 技术栈

- **前端**: Vue 3 + TypeScript + Vite
- **后端**: Tauri 2 + Rust
- **存储**: SQLite (本地)

## 开发环境搭建

### 前置要求

1. **Node.js** >= 18
2. **Rust** >= 1.70 (安装 [rustup](https://rustup.rs/))
3. **Visual Studio Build Tools** (Windows C++ 构建工具)
4. **WebView2** (Windows 10/11 自带)

### 安装依赖

```bash
# 安装前端依赖
npm install

# Rust 依赖会在首次构建时自动下载
```

### 开发模式

```bash
npm run tauri dev
```

### 构建发布版

```bash
npm run tauri build
```

构建产物在 `src-tauri/target/release/bundle/` 目录下。

## 功能特性

- ✅ 全局快捷键唤出/隐藏 (默认 Alt+Space，可自定义)
- ✅ 屏幕右侧滑出，失焦自动隐藏
- ✅ 系统托盘常驻，不占任务栏
- ✅ 毛玻璃/亚克力界面效果
- ✅ 快速输入框 (Enter 发送)
- ✅ 自动保存 (500ms 防抖)
- ✅ 卡片式列表，按时间倒序
- ✅ 置顶 / 删除 / 颜色标记
- ✅ 完成状态 (横线 + 变暗)
- ✅ 拖拽排序 (置顶与非置顶分组)
- ✅ 搜索过滤
- ✅ 创建时间 + 编辑时间显示

## 项目结构

```
sidebar-memo/
├── src/                    # Vue 前端
│   ├── components/         # UI 组件
│   ├── composables/        # 组合式函数
│   ├── views/              # 页面视图
│   └── assets/             # 样式
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── main.rs         # 入口
│   │   ├── lib.rs          # Tauri 逻辑
│   │   └── db.rs           # SQLite 数据库
│   ├── icons/              # 应用图标
│   └── Cargo.toml          # Rust 依赖
├── package.json
└── vite.config.ts
```
