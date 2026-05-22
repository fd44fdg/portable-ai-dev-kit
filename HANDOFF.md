# Bug/UX 循环检查 — 会话交接

**项目**: Portable AI Dev Kit (Tauri + React + Rust)
**最后会话日期**: 2026-05-22
**最后 commit**: `811872b`

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

### `811872b` — Strip terminal escapes from log output and make modals keyboard-accessible

6. **日志 ANSI / CR 乱码** — `command_output` 调用新的 `strip_terminal_escapes` 过滤 CSI / OSC 序列与孤立 `\r`，npm/cargo 进度条不再显示为乱码
7. **modal 可访问性** — 新 `useFocusTrap` hook + `role="dialog"` `aria-modal="true"` `aria-labelledby`，Tab 在 modal 内循环，打开聚焦首个可交互元素，关闭返还焦点到触发器

---

## 已确认为"非问题"（无需再审）

- `title={active.launchPath}` — React 对 `undefined` 不渲染 attribute
- 添加自定义工具的 modal 遮罩点击关闭 — input state 不清除，下次打开数据还在
- `marketplace_tools` 已安装检测对路径变体支持不全 — NPM 工具路径直接命中 `.cmd`，实际无影响
- `prepend_portable_paths` 路径累积 — 只设置 child `Command.env("PATH")`，父进程 PATH 不变，无累积
- `MAX_LOG_ENTRIES = 80` 截断 — 每条 = 一个命令的完整输出（非每行一条），80 个命令历史足够

---

## 下一轮建议聚焦（未审）

按优先级排：

### 🟢 低优先
1. **i18n 硬编码中文** — 全部 UI 字符串中文写死。如要支持英文需提取。
2. **`marketplace_tools` 列表硬编码** — 6 个工具写死在 Rust 代码。应移到 `config/marketplace.json` 或拉取远程 manifest。
3. **`flatten_single_root` 已检查 symlink，但不检查归档内 zip slip** — `Expand-Archive` 已防 zip slip，但若以后改用别的解压方式需注意。

剩余项目偏架构/产品决策，非 bug；下一轮若继续可考虑扫一遍 `scripts/`、`docs/`、`tasks/` 目录看是否有相关代码。

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
