# Bug/UX 循环检查 — 会话交接

**项目**: Portable AI Dev Kit (Tauri + React + Rust)
**最后会话日期**: 2026-05-22
**最后 commit**: `f9cc628`

---

## 如何接续

下次新会话开第一句话说：

> 读 HANDOFF.md 继续 BXAI 项目的 Bug/UX 循环检查与修复

我会读这份文档恢复上下文，无需重复扫描已审过的代码。

---

## 本项目已审范围（无需再扫）

- `src/main.tsx` (817 行)
- `src/styles.css` (822 行)
- `src-tauri/src/lib.rs` (205 行)
- `src-tauri/src/portable.rs` (2079 行)
- `src-tauri/src/main.rs`, `tauri.conf.json`, `vite.config.ts`
- `config/tool-manifest.json`, `Start.cmd`, `index.html`, `package.json`

---

## 已修复

### `0dff6c5` — Block path traversal in custom tool bin_name and tighten log spacing

1. **路径逃逸漏洞** — `portable.rs::add_custom_tool` 现在拒绝 `bin_name` 含 `/ \ .. :`
2. **日志面板拥挤** — `main.tsx::logText` 用 `\n` 替换 `\n\n`

### `f9cc628` — Fix concurrency races and bootstrap_kit hot-path overhead

3. **`save_state` 读写竞态** — `load_state` 现在也持 `STATE_LOCK`，消除 `rename` 期间读到空 state 的窗口
4. **`.bat` 写入 TOCTOU** — `spawn_terminal_command` 和 `add_custom_tool` 改为直接 `fs::rename`（Windows 上是原子的 `MoveFileExW + MOVEFILE_REPLACE_EXISTING`），去掉 `exists → remove → rename` 三步
5. **`bootstrap_kit` 缓存** — 用 `LazyLock<Mutex<HashSet<PathBuf>>>` 按 root 缓存，dashboard 刷新不再每次跑 14 次 `create_dir_all`

---

## 已确认为"非问题"（无需再审）

- `title={active.launchPath}` — React 对 `undefined` 不渲染 attribute
- 添加自定义工具的 modal 遮罩点击关闭 — input state 不清除，下次打开数据还在
- `marketplace_tools` 已安装检测对路径变体支持不全 — NPM 工具路径直接命中 `.cmd`，实际无影响

---

## 下一轮建议聚焦（未审）

按优先级排：

### 🟡 中优先
1. **modal 焦点陷阱缺失**
   ESC 可关闭，但 Tab 键可以跳出 modal 进入背景内容；屏幕阅读器无 `role="dialog"` / `aria-modal="true"`；打开时焦点未自动给到第一个输入框（NPM tab 用 `autoFocus`，但切到 PowerShell tab 后焦点跳到新输入需手动管理）。

2. **`prepend_portable_paths` 路径累积**
   每次调用都从 `env::var("PATH")` 读当前 PATH 并 prepend。Tauri 主进程 PATH 在 dev 模式可能已被 `npm run tauri:dev` 注入额外路径。多次 launch 会让 PATH 不断膨胀（实际不会，因为每次都是新 Command —— 但需确认无副作用）。

3. **日志 ANSI 颜色码**
   `command_output` 直接拼接 stdout/stderr。npm/cargo 输出含 ANSI 颜色码 `\x1b[...m`，在 `<pre>` 中显示为乱码。建议过滤。

4. **`MAX_LOG_ENTRIES = 80` 截断丢失上下文**
   长安装日志（npm 下载几百行）会被切到只剩 80 条。建议：合并多行单条；或允许"完整日志"按钮打开外部窗口。

### 🟢 低优先
5. **i18n 硬编码中文** — 全部 UI 字符串中文写死。如要支持英文需提取。
6. **`marketplace_tools` 列表硬编码** — 6 个工具写死在 Rust 代码。应移到 `config/marketplace.json` 或拉取远程 manifest。
7. **`flatten_single_root` 已检查 symlink，但不检查归档内 zip slip** — `Expand-Archive` 已防 zip slip，但若以后改用别的解压方式需注意。

---

## 验证命令

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml --lib
npx tsc --noEmit
```

最后一次验证通过：所有 5 个单元测试通过，无类型错误。

---

## 已知未触碰的目录

- `apps/`, `tools/`, `state/`, `workspace/` — 运行时数据，不在审计范围
- `old-portable-ai-dev-kit/` — 旧版本
- `node_modules/`, `cache/`, `dist/`, `src-tauri/target/` — 构建产物
- `.planning/`, `Microsoft/`, `.antigravitycli/` — 工具数据
- `scripts/`, `docs/`, `tasks/` — 未审，下轮可看是否相关
