# Project Status

## Verified
- Last verified: 2026-10-12 10:02 +08:00
- Branch: codex/settings-instrument
- Code baseline: 23a3ebf081696c4db1b772a5c0239ea4f96c86ee
- Verified by: Codex
- Status: Active
- Scope: Windows x64 本机 v0.1.7；源码、构建、原生菜单、主界面居中、覆盖安装与真实只读额度。

## Current
- 独立只读额度悬浮窗，复用本机 Codex 登录；菜单图标、托盘双击、胶囊控件、可点击 GitHub 链接。
- 正常更新保留额度、连接状态、按钮文字与颜色；成功后更新时间到秒。失败和数据过期仍明确提示。
- 单请求在途保护仍有效；升级保留启动、置顶、间隔和窗口位置。
- 参考 Mac 固定源码的深色白字红橙黄色环与190×166紧凑圆角菜单；右上方锚点、菜单关闭与屏幕避让。设计来源和平台适配见 decision-log.md。
- 托盘、圆窗双击以及菜单入口共用主界面打开路径；首次/重复打开在当前屏幕工作区居中，小工作区自动收窄。

- 菜单白卡无原生矩形边框/硬投影；独立逐像素软阴影沿圆角淡出，阴影不激活、不挡点击。Esc、外部点击和菜单选择均销毁两层窗口。受控白底视觉 QA 已检查。
- 最终安装产物及 QA：D:/CodexArtifacts/CodexQuota-shadow-v0.1.7；安装 EXE 与构建字节一致，原设置完全保留；30次菜单开关 GDI26→26、USER22→22。

## In Progress
- 无。

## Blocked
- 无影响本次本机改动的阻塞。

## Tests
| 验证层 | 结果 | 范围 / 环境 | 证据入口 |
|---|---|---|---|
| Build | PASS | Windows MSVC；fmt/check/clippy/release/Setup | scripts/build-release.ps1 |
| Core tests | PASS | 34 个库测试及 2 个进程回归；含色环像素、居中/边界几何；1 个真实服务单测默认忽略 | cargo test --all-targets --locked |
| Native / UI | PASS | 首次/重复打开居中；双击、静默更新、配置及原有路径；菜单尺寸、圆角、锚点、Escape及30次开关无资源增长 | scripts/qa-desktop.py；scripts/qa-menu.py |
| Install / live | PASS | 最终 Setup 覆盖安装；已安装 EXE 字节及构建提交一致；真实额度读取成功 | 本机最终安装与 GUI 验收记录 |
| Formal signing | NOT RUN | 最终 EXE/Setup 为 NotSigned；缺少发行签名配置 | docs/DEVELOPMENT.md |

## Gaps
- 不同物理 DPI、主题、睡眠唤醒、读屏软件和长期运行仍需扩大验证；当前未签名。已验证键盘消息 Home/Down/Enter 路径；未宣称物理键盘或 AppKit 玻璃效果验收。
- Windows 远端当前状态本次未重新核实；Mac 参考 main 源码已读取并固定提交。本次无推送或发布动作。

## Sync & Safety
- Observed at: 2026-10-12 10:02 +08:00
- Docs: OK；开发与验收规则见 DEVELOPMENT.md，README 预览明确来自 v0.1.4。
- Git: CLEAN（已验证代码基线）；本状态文件后续仅文档提交不改变该验证基线。
- Remote: LOCAL ONLY（v0.1.7）；Windows远端当前状态 UNKNOWN。
- Rollback point: e43a852daa083b1cbd41ebcbe37b4dc211d18db6（已验证 v0.1.6）。
- Secrets/config risk: 本次修改未新增凭据；原设置保留，既有全仓配置风险未重新审计。

## Next
1. 用户要求发布时核实远端并发布 v0.1.7；保持准确的未签名说明。
