# Git4U

用 Rust + Tauri 2 + Vue 3 打造的跨平台桌面 Git 客户端。设计与静态原型见 [`docs/`](./docs) 与 [`prototype/`](./prototype)。

本仓库当前落地的是 **M0 垂直切片 + M1 增量**：

> M0：打开仓库 → 工作区状态 → 差异 → 暂存 → 提交 → 历史 → 分支浏览 / 本地检出
> M1 增量：设置、地址级凭证、Fetch / Pull / Push、问题抽屉、i18n、.gitignore 模板

完整范围以 [docs/14 · 实现范围与风险基线](./docs/14-实现范围与风险基线.md) 为准；`docs/` 与 `prototype/` 中的其余页面是设计参考，不代表已排期。

## 目录

```text
rust/                  # Rust 工作区（Cargo.toml / Cargo.lock / target 都在这里）
  crates/
    g4u-model/         # 领域模型 + IPC 契约（serde camelCase，无 IO）
    g4u-core/          # GitRead / GitWrite trait + 系统 git CLI 引擎
    g4u-cred/          # 地址级凭证元数据 + OS Keychain + askpass 注入
    g4u-ops/           # 仓库会话池、版本号、写互斥、用例编排
    g4u-server/        # Tauri command 薄壳（commands 子模块）
app/                   # Tauri 官方同构根目录
  src/                 # Vue 3 + TypeScript + Naive UI（Vite）
  src-tauri/           # Tauri 2 壳（窗口、能力、图标）
docs/                  # 详细设计文档
prototype/             # 33 个静态原型页面
```

依赖方向：`server → ops → core → model`。前端只经 `app/src/ipc/` 访问后端，不含 Git 语义。
`app/src-tauri` 通过 `package.workspace = "../../rust"` 加入工作区，构建产物统一在 `rust/target`。

## 运行

前置：Node 22+、pnpm 11+、Rust stable、系统 git ≥ 2.34、WebView2（Windows 自带）。

```bash
pnpm --dir app install
pnpm --dir app tauri dev         # 启动 Vite + Tauri 窗口
```

只做视觉预览（使用内存演示数据，不读磁盘仓库）：

```bash
pnpm --dir app dev               # http://localhost:1420
```

## 已验证

```bash
cargo test --manifest-path rust/Cargo.toml -p g4u-core -p g4u-ops -p g4u-cred
pnpm --dir app test              # 15 个前端测试（图布局、目录树、i18n、问题映射）
pnpm --dir app build             # vue-tsc + Vite 生产构建
pnpm --dir app tauri build --debug --no-bundle
```

## 已实现（M0 + M1 增量）

- 打开 / 移除本地仓库，仓库列表持久化到应用数据目录（`repos.json`）
- 工作区：已暂存 / 未暂存（含未跟踪）/ 冲突三组，平铺 / 树形目录切换，目录级与整文件暂存/取消暂存；`.gitignore` 模板（语言 / IDE / OS）生成、追加或覆盖
- 差异：并排 / 行内、未跟踪文件合成 diff、二进制识别、行号与增删统计；hunk/行级暂存与 discard 未实现
- 提交：空消息 / 无暂存拦截、hooks 开关、Amend、草稿持久化、Ctrl+Enter；完整提交前检查（身份/大文件/保护分支）未实现
- 历史：提交图（lane 布局）、消息/hash/作者搜索、提交详情与文件统计
- 分支：本地 / 远程 / 标签展示、upstream 与 ahead/behind、本地分支检出、基于 HEAD 新建；远程分支检出、删除、rename、merge/rebase 未实现
- 工具：顶栏「工具」（工作区之后）进入独立三栏页：左侧工作区文件树、中间只读文件预览（512 KB 截断、二进制识别、路径越界拒绝）、右侧维护工具；已提供仓库体检、大文件扫描、备份引用（可创建恢复分支）、.gitignore、忽略规则、行尾、对象/fsck、reflog、锁/中间态、环境、密钥扫描（命中行可点击定位）与清理指引；破坏性清理仍只给命令，规划见 `docs/15`
- 同步：Fetch / Pull / Push；Pull 策略（merge / rebase / ff-only）、保护分支、推送前审计、疑似密钥扫描
- 网络代理：统一代理列表（http / https / socks5）、全局默认与仓库级直连 / 指定代理
- 账户与凭证：地址级匹配、账号密码 / Access Token、OS Keychain 存储、单次 askpass 注入、只读验证
- 设置：独立设置窗口（顶栏「设置」或 Ctrl+,），分区顺序为 外观与语言 / 账户与凭证 / 代理 / 仓库默认行为 / 快捷键 / Git 配置 / 外部工具 / AI / 高级 / 工作区 / 关于；语言与主题放在第一项，乱码时也能直接切换；全局与仓库级设置持久化到 `settings.json`
- 错误：`G4uError { kind, human, causeChain, suggestions, cliEquivalent }` 全链路结构化
- 安全网（P0 部分）：commit / amend 写入 `journal.jsonl`（pre/post oid、op、备份引用）；Amend 前自动创建 `refs/git4u/backup/*`；提交后 toast 可撤销（soft reset，验 HEAD、暂存区保留、undo 前再备份）；历史已排除备份引用
- 问题处理：统一问题抽屉（状态栏 🛟 / toast「查看处理」/ 中间态横幅进入），展示结构化错误、技术细节与等价命令；`repo.refresh` / `repo.pick` / `app.retry` 建议可直接执行；中间态给出继续 / 中止 / 进度命令；认证失败可直达对应凭证的编辑框并标记「需更新」（401/403 细分与完整向导待补）
- 主题：`styles/tokens.ts` 单一来源，同时生成 CSS 变量与 Naive UI `themeOverrides`（暗/亮/跟随系统）

## 明确边界

| 项 | 状态 | 说明 |
|---|---|---|
| Git 引擎 | 仅系统 git CLI | 读/写都在 `GitRead`/`GitWrite` 后；gitoxide 读路径、git2 写路径按设计后续接入 |
| 实时刷新 | 5s 轮询 + 操作后刷新 | `notify` watcher（300ms 防抖）未接 |
| Journal / 备份引用 / Undo | 部分（commit / amend） | Amend 前创建备份 ref；commit/amend 写 JSONL Journal；提交后 toast 可 soft undo；reset / 删分支 / stash / push、恢复界面与 Precheck 待接 |
| 行级 / 块级暂存 | 未实现（P1） | 需补丁引擎（`git apply --cached --unidiff-zero`）与 hunk/行级操作 |
| 克隆 / Tag CRUD | 未实现（P1） | 克隆继承系统凭证；Tag 当前只读展示，无创建/删除/推送 |
| Stash / 冲突向导 | 未实现（P2） | Stash 与一次一处的合并向导均未接入；冲突目前只显示文件列表 |
| Fetch / Pull / Push | 基础实现 | 默认单 remote；无 force push；推送审计覆盖保护分支 / 落后远端 / 大文件 / 常见密钥模式；支持代理 |
| 设置窗口 | 独立设置窗口 | 主窗口 `Ctrl+,` / 顶栏打开 `settings` WebviewWindow；浏览器预览回退到 `/settings` 路由 |
| 网络代理 | 基础实现 | 统一代理列表 + 仓库级直连 / 指定代理；通过环境变量注入，不写 `.gitconfig` |
| 账户与凭证 | 基础实现 | 地址级 HTTPS 匹配 + Keychain + askpass 单次注入；仓库显式绑定、克隆提示、失败来源联动待接 |
| 其余原型页面 | 未实现 | 冲突向导、Stash、Shelf、Rebase、急救中心、Blame、LFS、Hooks、终端等按 [docs/14](./docs/14-实现范围与风险基线.md) 推进 |
| 工具页 | 只读诊断 + 安全恢复 | 目录树 + 预览 + 体检/大文件/备份/ignore/EOL/对象/fsck/reflog/锁/环境/密钥扫描；恢复分支已接；移动分支、锁删除、gc、未跟踪清理需 P0 安全层 |
| 国际化 | 简体中文 / English 界面切换 | 前端文案已覆盖；后端错误文案仍为中文 |
| 问题处理 | 统一问题抽屉 + 凭证直达（P0 + P2 部分） | 中间态命令指引、错误建议动作、认证失败直达凭证编辑框；继续/中止、冲突、401/403 细分、来源联动待接入 |
| CI / CSP | 未接 / 开发期关闭 | 无 CI、无差分测试；CSP 为 `null`，生产前必须收紧 |

## M1 剩余关键路径

1. **P0**：备份引用创建 + Journal + 有限 Undo + Precheck；CI/黄金仓库/契约测试；CSP 与进程清理。
2. **P1**：克隆、hunk/行级暂存、Discard、完整提交前检查、远程分支检出、安全删分支、基本 Tag。
3. **P2**：合并/冲突向导、Stash、中间态 continue/abort、急救 S01–S08/S13、分叉拉取与 force push 门禁。

已交付的设置、凭证、同步、i18n、问题抽屉与 .gitignore 模板不重做；后续只在同一安全模型内收敛。完整裁决见 [docs/14](./docs/14-实现范围与风险基线.md)。

## 新增功能时的边界

- 所有写操作只加在 `g4u-ops`；不要在 `g4u-server` 或前端拼 git 命令。
- 新错误必须补齐四件套，并在前端展示可执行建议。
- 前端颜色只引用语义 token，不写裸色值；大数组状态用 `shallowRef` 整段替换。
- Rust 命令统一在 `rust/` 下执行；Tauri 命令统一在 `app/` 下执行。
