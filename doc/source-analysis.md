# Harpoon Zellij 插件源码分析

## 项目概述

Harpoon 是一个 Zellij 终端复用器的 WASM 插件，灵感来自 Neovim 的 [harpoon](https://github.com/ThePrimeagen/harpoon) 插件。它允许用户手动管理一个"收藏面板"列表，实现在常用 pane 之间快速跳转。

## 项目结构

```
harpoon/
├── src/
│   ├── main.rs          # 插件主逻辑（状态管理、事件处理、UI 渲染）
│   └── persistence.rs   # 持久化模块（书签的磁盘读写与恢复匹配）
├── Cargo.toml           # 依赖：serde, serde_json, zellij-tile
├── plugin-dev-workspace.kdl  # 开发用 Zellij 布局
└── .cargo/config.toml   # 构建目标配置
```

## 核心依赖

| 依赖 | 用途 |
|------|------|
| `zellij-tile` (0.42.2) | Zellij 插件 SDK，提供事件订阅、权限请求、UI 渲染等 API |
| `serde` / `serde_json` | 书签数据的序列化与反序列化 |

## 数据模型

### `Pane`（main.rs）

```rust
struct Pane {
    pane_info: PaneInfo,  // Zellij 提供的面板信息（id, title, is_plugin 等）
    tab_info: TabInfo,    // 面板所属的标签页信息（name, position, active 等）
}
```

- `PaneInfo.id` 对于非插件面板在整个 session 中唯一，是面板的稳定标识符。
- `Display` trait 实现格式：`tab_name | pane_title`。

### `PaneBookmark`（persistence.rs）

```rust
struct PaneBookmark {
    tab_name: String,
    pane_title: String,
}
```

持久化到磁盘的轻量级书签，仅保存名称信息用于跨 session 恢复匹配。

### `State`（main.rs）

```rust
struct State {
    selected: usize,              // 当前选中的列表索引
    panes: Vec<Pane>,             // 收藏的面板列表
    focused_pane: Option<Pane>,   // 用户打开 harpoon 前聚焦的面板
    tab_info: Option<Vec<TabInfo>>,
    pane_manifest: Option<PaneManifest>,
    session_name: Option<String>,
    persistence: Persistence,
}
```

## 执行流程

### 1. 插件加载（`load`）

```
用户按下快捷键 (如 Ctrl+y)
  → Zellij 加载 harpoon.wasm
  → 调用 State::load()
    → 请求权限：RunCommands, ReadApplicationState, ChangeApplicationState
    → 订阅事件：Key, TabUpdate, PaneUpdate, PermissionRequestResult,
                SessionUpdate, RunCommandResult
```

### 2. 初始化序列

插件加载后，Zellij 会依次推送事件完成初始化：

```
PermissionRequestResult(Granted)
  → 重命名插件面板为 "harpoon"

SessionUpdate(session_infos)
  → 获取当前 session 名称
  → 触发 load_from_disk()：执行 shell 命令读取持久化文件

RunCommandResult (source="load")
  → 解析 JSON 为 Vec<PaneBookmark>
  → 存入 pending_bookmarks 等待与实际面板匹配

TabUpdate / PaneUpdate
  → 缓存最新的 tab_info / pane_manifest
  → 调用 update_panes() 进行面板同步
```

### 3. 面板同步（`update_panes`）

每次收到 `TabUpdate` 或 `PaneUpdate` 时执行：

```
update_panes()
  ├── get_valid_panes()
  │     遍历已保存面板，在最新 manifest 中按 pane_id 查找
  │     移除已关闭的面板，更新已移动面板的 tab 信息
  │
  ├── match_pending_bookmarks()
  │     将磁盘加载的书签按 (tab_name, pane_title) 匹配到实际面板
  │     匹配成功的从 pending 列表移除，加入 panes 列表
  │
  ├── 更新 focused_pane（当前活跃的非插件面板）
  │
  ├── 将 selected 光标移到 focused_pane 对应的列表位置
  │
  └── 如果列表有变化，save_to_disk() 持久化
```

### 4. 用户交互（键盘事件）

| 按键 | 行为 |
|------|------|
| `a` | 将当前聚焦面板加入列表，按 tab 位置排序后隐藏插件 |
| `A` | 将所有标签页的所有终端面板加入列表，隐藏插件 |
| `d` | 删除选中项，持久化保存 |
| `j` / `Down` | 选中项下移（循环） |
| `k` / `Up` | 选中项上移（循环） |
| `Enter` / `l` | 跳转到选中面板（`focus_terminal_pane`），隐藏插件 |
| `Esc` / `c` | 隐藏插件 |

### 5. UI 渲染（`render`）

```
render(rows, cols)
  ├── 居中显示标题 "==== N panes ===="
  ├── 逐行渲染面板列表，选中项高亮（.selected()）
  └── 底部显示快捷键提示（根据窗口宽度自适应 3 种布局）
```

提示栏根据列宽选择不同详细程度：
- `> 75` 列：完整提示
- `> 50` 列：缩写提示
- `≤ 50` 列：最简提示

## 持久化机制

### 存储路径

```
$XDG_DATA_HOME/zellij-harpoon/<session_name>.json
# 默认: ~/.local/share/zellij-harpoon/<session_name>.json
```

### 读取流程

```
load_from_disk()
  → run_command(["sh", "-c", "cat <path> 2>/dev/null || echo '[]'"])
  → 异步等待 RunCommandResult 事件
  → on_load_command() 解析 JSON → pending_bookmarks
```

### 写入流程

```
save_to_disk()
  → 将 panes 转为 Vec<PaneBookmark> → JSON
  → run_command(["sh", "-c", "mkdir -p <dir> && printf '%s' \"$1\" > <path>", "_", json])
```

### 变更检测

`has_changed()` 比较当前面板列表的 `(tab_name, pane_title)` 元组与上次保存的状态，避免无意义的磁盘写入。

### 恢复匹配

`match_pending_bookmarks()` 在每次 `update_panes()` 时尝试将未匹配的书签与实际面板关联：
- 按 `tab_name` 找到对应标签页
- 在该标签页中按 `pane_title` 找到匹配的非插件面板
- 避免重复匹配已在列表中的面板

## 完整生命周期时序图

```
┌─────────┐     ┌─────────┐     ┌──────────────┐     ┌──────┐
│  User   │     │ Zellij  │     │   Harpoon    │     │ Disk │
└────┬────┘     └────┬────┘     └──────┬───────┘     └──┬───┘
     │  Ctrl+y       │                 │                 │
     │──────────────>│  load()         │                 │
     │               │────────────────>│                 │
     │               │  PermGranted    │                 │
     │               │────────────────>│ rename pane     │
     │               │  SessionUpdate  │                 │
     │               │────────────────>│ load_from_disk  │
     │               │                 │────────────────>│
     │               │  RunCmdResult   │                 │
     │               │────────────────>│ parse bookmarks │
     │               │  TabUpdate      │                 │
     │               │────────────────>│ update_panes()  │
     │               │  PaneUpdate     │                 │
     │               │────────────────>│ update_panes()  │
     │               │                 │ match bookmarks │
     │               │  render()       │                 │
     │               │<────────────────│                 │
     │  显示 UI      │                 │                 │
     │<──────────────│                 │                 │
     │  按 'a'       │                 │                 │
     │──────────────>│  Key('a')       │                 │
     │               │────────────────>│ add pane        │
     │               │                 │ save_to_disk    │
     │               │                 │────────────────>│
     │               │                 │ hide_self()     │
     │               │<────────────────│                 │
     │  按 Enter     │                 │                 │
     │──────────────>│  Key(Enter)     │                 │
     │               │────────────────>│ focus_pane      │
     │               │                 │ hide_self()     │
     │<──────────────│<────────────────│                 │
     │  切换到目标面板│                 │                 │
```

## 设计要点

1. **面板标识**：使用 `PaneInfo.id`（非插件面板在 session 内唯一）作为运行时标识，使用 `(tab_name, pane_title)` 作为持久化标识。
2. **异步 I/O**：WASM 环境无法直接访问文件系统，通过 `run_command` 执行 shell 命令实现异步读写。
3. **自动清理**：每次事件更新时自动移除已关闭的面板，保持列表与实际状态同步。
4. **跨 session 恢复**：通过名称匹配（而非 ID）实现 session 重启后的书签恢复。
5. **最小权限**：仅请求必要的三项权限（RunCommands、ReadApplicationState、ChangeApplicationState）。
