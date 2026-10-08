# 开发与发行

| 模块 | 职责 |
|---|---|
| discovery / codex / provider | 定位 CLI、执行只读 RPC、解析并分类失败；不复制登录凭据 |
| monitor | 唯一额度快照、过期与错误展示口径 |
| worker | 单请求在途、刷新退避、取消和子进程回收 |
| config | 原子设置持久化，损坏文件保留原样 |
| desktop / weekly_quota_render | 原生窗口、托盘、系统消息、设置与圆窗绘制 |
| desktop/settings_view | 原生设置控件与仪表绘制；常规、关于两页；保留键盘及辅助技术语义 |
| packaging / build-release | 安装、卸载、签名身份核验和最终产物 |

核心回归包含窗口缺失、分数百分比、失败保留与恢复、透明通道缩放、损坏配置、取消查询和子进程回收。真实服务测试默认忽略，只有获准的已登录本地环境单独执行。GUI 脚本只用于本地桌面。

设置窗口依赖清单中的 Common Controls v6；自绘使用 GDI+，原生控件保持即时设置通知。`qa-desktop.py` 验证两页导航及已删除入口、置顶、间隔、启动、单实例、退出及位置恢复。窗口布局单测覆盖最小、默认和宽屏尺寸；实际 DPI 和多屏验收仍需独立完成。

## 签名

先读取完整环境命名空间 `NANFENG_CODEX_QUOTA_PFX_PATH`、`NANFENG_CODEX_QUOTA_PFX_PASSWORD`、`NANFENG_CODEX_QUOTA_SIGN_EXPECTED_SUBJECT`、`NANFENG_CODEX_QUOTA_TIMESTAMP_URL`。只配置部分即停止；没有任何环境字段时，才读取用户级 `.config/nanfeng-signing/NanfengCodexQuota-Windows.json` 的 pfxPath、pfxPassword、expectedSubject、timestampUrl。

凭据和 PFX 必须在仓库外。发行证书须有私钥、符合期望主体、不是自签身份；EXE 与 Setup 均须 Authenticode Valid 且发行主体匹配。没有凭据时默认失败，只有明确接受本次未签名交付后使用 `-AllowUnsigned`，并在 README/Release 说明真实签名状态。

## 安装与验收

单个 x64 Setup EXE、当前用户安装、独立应用标识。安装检查运行中的应用，不强杀；升级保留已有启动选择；卸载仅删除指向本次安装目录的具名启动项，保留设置与 Codex 登录。

发行前验证精确安装包的安装、已安装 EXE 字节、真实额度、窗口响应、关于中的构建提交、保留设置的覆盖安装和卸载。合成 Windows 消息不能替代物理多屏、真实 DPI、完整睡眠及长期运行证据。

README 预览只放当前版本真实页面；Release 只放安装包。main、标签、构建提交与远端附件需一致。上传失败保留同一草稿，检查远端资产再重试，不覆盖已发布版本。
