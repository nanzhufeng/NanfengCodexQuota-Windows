# Project Status

## Verified
- Last verified: 2026-10-11 18:09 +08:00
- Branch: codex/settings-instrument
- Code baseline: d1542a4009e1e46f9132fa866b2ccb7c43fdb627
- Verified by: Codex
- Status: Active
- Scope: Windows x64 本机 v0.1.5；源码、构建、原生界面、覆盖安装与真实只读额度。

## Current
- 独立只读额度悬浮窗，复用本机 Codex 登录；菜单图标、托盘双击、胶囊控件、可点击 GitHub 链接。
- 正常更新保留额度、连接状态、按钮文字与颜色；成功后更新时间到秒。失败和数据过期仍明确提示。
- 单请求在途保护仍有效；升级保留启动、置顶、间隔和窗口位置。

## In Progress
- 无。

## Blocked
- 无影响本次本机改动的阻塞。

## Tests
| 验证层 | 结果 | 范围 / 环境 | 证据入口 |
|---|---|---|---|
| Build | PASS | Windows MSVC；fmt/check/clippy/release/Setup | scripts/build-release.ps1 |
| Core tests | PASS | 30 个库测试及 2 个进程回归；1 个真实服务单测默认忽略 | cargo test --all-targets --locked |
| Native / UI | PASS | 更新期间保持显示；成功时间前进；双击、导航、置顶、间隔、启动、单实例、退出、位置恢复 | scripts/qa-desktop.py |
| Install / live | PASS | 最终 Setup 覆盖安装；已安装 EXE 字节及构建提交一致；真实额度读取成功 | 本机最终安装与 GUI 验收记录 |
| Formal signing | NOT RUN | 最终 EXE/Setup 为 NotSigned；缺少发行签名配置 | docs/DEVELOPMENT.md |

## Gaps
- 不同物理 DPI、多屏、睡眠唤醒和长期连续运行未扩展验证；当前未签名。
- 当前远端状态本次未核实：Git HTTPS TLS 握手失败。本次无推送或发布动作。

## Sync & Safety
- Observed at: 2026-10-11 18:09 +08:00
- Docs: OK；开发与验收规则见 DEVELOPMENT.md，README 预览明确来自 v0.1.4。
- Git: CLEAN（已验证代码基线）；本状态文件后续仅文档提交不改变该验证基线。
- Remote: LOCAL ONLY（v0.1.5）；远端当前状态 UNKNOWN。
- Rollback point: e6e3c5d9c26f498b5a3b7e9148f21340331c8d7a（已验证 v0.1.4）。
- Secrets/config risk: 本次修改未新增凭据；原设置保留，既有全仓配置风险未重新审计。

## Next
1. 用户要求发布时核实远端并发布 v0.1.5；保持准确的未签名说明。
