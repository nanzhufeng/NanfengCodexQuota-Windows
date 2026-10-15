# Project Status

## Verified
- Last verified: 2026-10-15 12:28 +08:00
- Branch: codex/settings-instrument
- Code baseline: 417dd6aa641b2519d47368d39b02c8ec15d66a61
- Verified by: Codex
- Status: Active
- Scope: Windows x64 本机 v0.1.9；源码、构建、原生菜单、主界面居中、覆盖安装与真实只读额度。

## Current
- 独立只读额度悬浮窗，复用本机 Codex 登录；菜单图标、托盘双击、胶囊控件、可点击 GitHub 链接。
- 正常更新保留额度、连接状态、按钮文字与颜色；成功后更新时间到分钟。短暂网络/超时保留旧值与原时间，不显示连接失败提示；过期、未登录等仍明确。
- 单请求在途保护仍有效；升级保留启动、置顶、间隔和窗口位置。
- 参考 Mac 固定源码的深色白字红橙黄色环与190×166紧凑圆角菜单；右上方锚点、菜单关闭与屏幕避让。设计来源和平台适配见 decision-log.md。
- 托盘、圆窗双击以及菜单入口共用主界面打开路径；首次/重复打开在当前屏幕工作区居中，小工作区自动收窄。

- 菜单白卡无原生矩形边框/硬投影；独立逐像素软阴影沿圆角淡出，阴影不激活、不挡点击。Esc、外部点击和菜单选择均销毁两层窗口。受控白底视觉 QA 已检查。
- 最终安装产物及 QA：D:/CodexArtifacts/CodexQuota-refresh-v0.1.9；安装 EXE 与构建字节一致，原设置完全保留；30次菜单开关 GDI109→109、USER69→65。

- 置顶在创建时生效；可见且启用置顶时，约1秒恢复重叠外部窗口导致的遮挡或层级降级，不抢焦点；拖动期间暂停，释放后按实际工作区避让任务栏，隐藏/禁用时不强制恢复。
- 旧版覆盖及拖入任务栏两项红灯；最终安装版与重启后七项置顶验收全部通过，见 scripts/qa-topmost.py 及 final-topmost/restart-topmost 记录。

- 网络/超时同次最多两次只读尝试，耗尽后5/15/30秒起自动恢复，持续失败上限300秒；认证错误不快速重试。
- 设置/托盘/悬浮窗均移除连接失败提示、用户时间仅时分；诊断保留failure和精确sampled_at，未改写旧时间冒充成功。隔离真实GUI故障→自动恢复验收PASS；最终安装版正常60秒自动刷新读取58%且sampled_at前进，记录见automatic-live.json。

## In Progress
- 无。

## Blocked
- 无影响本次本机改动的阻塞。

## Tests
| 验证层 | 结果 | 范围 / 环境 | 证据入口 |
|---|---|---|---|
| Build | PASS | Windows MSVC；fmt/check/clippy/release/Setup | scripts/build-release.ps1 |
| Core tests | PASS | 35 个库测试及 5 个进程/恢复回归；含色环像素、居中/边界几何；1 个真实服务单测默认忽略 | cargo test --all-targets --locked |
| Native / UI | PASS | 首次/重复打开居中；双击、静默更新、配置及原有路径；菜单尺寸、圆角、锚点、Escape及30次开关无资源增长 | scripts/qa-desktop.py；scripts/qa-menu.py |
| Install / live | PASS | 最终 Setup 覆盖安装；已安装 EXE 字节及构建提交一致；真实额度读取成功 | 本机最终安装与 GUI 验收记录 |
| Formal signing | NOT RUN | 最终 EXE/Setup 为 NotSigned；缺少发行签名配置 | docs/DEVELOPMENT.md |

## Gaps
- 独占全屏、安全桌面、自动隐藏任务栏、不同物理 DPI、主题、睡眠唤醒、读屏软件和长期运行仍需扩大验证；当前未签名。已验证键盘消息 Home/Down/Enter 路径；未宣称物理键盘或 AppKit 玻璃效果验收。
- Windows 远端当前状态本次未重新核实；Mac 参考 main 源码已读取并固定提交。本次无推送或发布动作。

## Sync & Safety
- Observed at: 2026-10-15 12:28 +08:00
- Docs: OK；开发与验收规则见 DEVELOPMENT.md，README 预览来自公开 v0.1.8，尚未更新为本机 v0.1.9。
- Git: CLEAN（已验证代码基线）；本状态文件后续仅文档提交不改变该验证基线。
- Remote: LOCAL ONLY（v0.1.9）；Windows远端当前状态 UNKNOWN。
- Rollback point: 360f03cfbaddd2d69d13eed9a24dd8cf7c728084（已验证 v0.1.8）。
- Secrets/config risk: 本次修改未新增凭据；原设置保留，既有全仓配置风险未重新审计。

## Next
1. 用户要求发布时核实远端并发布 v0.1.9；保持准确的未签名说明。
