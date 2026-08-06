# Live Translator 安装部署文档（Windows + CUDA）

版本：v0.1.0 · 更新：2026-08-05
适用：在一台全新的 Windows 电脑上，以最少的步骤把「日中同传」跑起来，最好下载一个包解压就能用。

---

## 1. 一句话结论

- **推荐分发方式：绿色便携包（zip）**。解压 → 双击「启动直播翻译.cmd」→ 选模型 → 开始识别。
  包内已内置 CUDA 运行时 DLL、VC++ 运行库和全部语音/翻译模型，**无需安装 CUDA Toolkit、
  Rust、FFmpeg 或任何其他工具**。
- 目标机器只要满足：Windows 10/11 64 位 + NVIDIA 独立显卡（驱动能跑 nvidia-smi）。

---

## 2. 两种分发方式对比

| 方式 | 文件 | 大小 | 优点 | 缺点 |
|---|---|---|---|---|
| 绿色便携包（推荐） | LiveTranslator-portable-v0.1.0.zip | 约 2.7 GB（解压 3.5 GB） | 解压即用、离线可用、模型齐全 | 体积大 |
| NSIS 安装包 | Live Translator_0.1.0_x64-setup.exe | 约 36 MB | 体积小、有开始菜单/卸载 | 不含模型和 CUDA 运行时，装完还需手动放 models 并装 CUDA |
| MSI | Live Translator_0.1.0_x64_en-US.msi | 约 94 MB | 便于域/静默安装 | 同样不含模型和 CUDA 运行时 |

> 结论：对外发布请用**便携包**；MSI/NSIS 只适合「目标机器已经装好 CUDA Toolkit、你单独分发 models 目录」的情况。

---

## 3. 系统要求

| 项目 | 要求 |
|---|---|
| 操作系统 | Windows 10/11 64 位 |
| 显卡 | NVIDIA 独立显卡（GTX 10 系及以上较稳妥），显存建议 >= 4 GB |
| 驱动 | 较新的 NVIDIA 驱动（运行 nvidia-smi 能看到 GPU 即可）。CUDA 13 运行时要求驱动不能太旧；驱动过旧会报 CUDA 初始化失败 |
| 内存 | >= 8 GB（建议 16 GB；本地翻译 llama-server 约占 2 GB） |
| 磁盘 | 便携包解压后约 3.5 GB |
| 网络 | 仅「OpenAI / DeepL」云端翻译需要联网；本地 Hy-MT 翻译完全离线 |
| 浏览器扩展 | 用「浏览器音频」音源时，需要在 Chrome 里加载 browser-tab-capture 扩展（见 §7） |

> 说明：便携包已内置 CUDA 13.1 运行时（cublas64_13.dll 等）和 VC++ 2015-2022 运行库，
> 所以**目标机器不需要安装 CUDA Toolkit**。GPU 驱动是唯一必须预装的东西。

---

## 4. 安装步骤（绿色便携包）

1. 把 LiveTranslator-portable-v0.1.0.zip 拷贝到目标机器。
2. 解压到任意目录（**整包解压，不要只拷 exe**），例如 D:\LiveTranslator\。
   - 解压后目录里必须同时有：LiveTranslator.exe、cublas64_13.dll、models\ 等。
3. （推荐）双击 **启动直播翻译.cmd**；或直接双击 LiveTranslator.exe。
4. 等待悬浮窗出现。首次运行若被 SmartScreen 拦截，点「更多信息 → 仍要运行」。
5. 可选：把 models\ggml-large-v3-turbo-q5_0.bin 设为默认 ASR 模型（见 §6）。

---

## 5. 便携包目录结构

    LiveTranslator-portable/
    ├─ LiveTranslator.exe          主程序
    ├─ 启动直播翻译.cmd            推荐入口（把包内 CUDA/ffmpeg 加入 PATH 后启动）
    ├─ 使用说明.txt                包内快速说明
    ├─ cublas64_13.dll             内置 CUDA 运行时（勿删）
    ├─ cublasLt64_13.dll           内置 CUDA 运行时（勿删，约 458 MB）
    ├─ cudart64_13.dll             内置 CUDA 运行时（勿删）
    ├─ msvcp140*.dll / vcruntime*.dll  内置 VC++ 运行库（勿删）
    ├─ models/
    │  ├─ ggml-large-v3-turbo-q5_0.bin  默认推荐的日文识别模型（547 MB）
    │  ├─ ggml-small.bin                备用小模型（465 MB）
    │  ├─ ggml-silero-v6.2.0.bin        VAD 断句模型（必需）
    │  └─ hymt/                         本地离线翻译（Hy-MT）
    │     ├─ HY-MT1.5-1.8B-Q4_K_M.gguf  翻译模型（约 1.08 GB）
    │     └─ llama-server.exe + *.dll   本地翻译服务
    └─ ffmpeg/                     可选，仅用于「播放本地文件/网络流」音源

> 主程序按以下顺序找模型目录：环境变量 LIVE_TRANSLATOR_MODEL_DIR → exe 同目录的 models\ →
> 应用数据目录。便携包走的是「exe 同目录 models」。

---

## 6. 首次使用

1. **选 ASR 模型**：点设置（齿轮）→「ASR 模型」→ 选 ggml-large-v3-turbo-q5_0.bin。
   - 切换立即生效（运行中切换会自动重启识别会话），无需重启 App。
2. **选翻译服务**：设置 →「翻译」：
   - 本地 Hy-MT（默认推荐）：完全离线，适合隐私/无网环境。
   - OpenAI Compatible / DeepL：需要网络和 API Key。
3. **选音源**：顶栏「音频来源」→ 系统音频 / 麦克风 / 浏览器音频。
4. 点 ▶ 开始识别；字幕与中文翻译显示在悬浮窗里。

---

## 7. 浏览器音频（可选，需 Chrome 扩展）

「浏览器音频」需要加载 browser-tab-capture/ 下的 Chrome MV3 扩展：

1. Chrome 打开 chrome://extensions，开启「开发者模式」。
2. 「加载已解压的扩展程序」→ 选择 browser-tab-capture 目录。
3. 在网页里点扩展图标开始捕获；应用里选「浏览器音频」→ ▶。

---

## 8. 验证安装是否正常

- nvidia-smi 能列出 GPU 且无报错（驱动正常）。
- 启动 App 后任务管理器里能看到进程，且无「缺少 DLL」弹窗。
- 开始识别后，exe 目录下的 diagnostics.log 会打印 [asr]、[asr:commit]、[translate] 行。
- 本地翻译健康：确认 models\hymt\ 里 GGUF + llama-server.exe + 所有 DLL 齐全，
  选中「本地 Hy-MT」后翻译行有输出即正常。

---

## 9. 常见问题（FAQ）

| 现象 | 原因 / 处理 |
|---|---|
| 双击没反应或闪退 | 先用「启动直播翻译.cmd」；仍不行：nvidia-smi 看驱动是否过旧/缺失 |
| 报缺少 cublas64_13.dll 等 | exe 同目录被误删了 CUDA DLL；重新解压完整包 |
| 报「模型找不到」 | models\ 与 exe 不在同一目录；确认整包解压 |
| 本地翻译（Hy-MT）无输出 | models\hymt\ 不完整（缺 GGUF 或 llama-server.exe 或 DLL） |
| 播放文件/串流报 ffmpeg 错误 | 便携包未含 ffmpeg（制作时未加 -IncludeFfmpeg）；自行装 ffmpeg 并加 PATH |
| 识别很慢 | 检查是否误选了 CPU：设置里模型选大模型；GPU 驱动正常时默认走 CUDA |
| 杀毒/SmartScreen 拦截 | 本工具未签名；选择「仍要运行」，或加入白名单 |
| 端口 8090 被占用 | 本地 Hy-MT 固定用 127.0.0.1:8090；关掉占用程序或换机器 |

---

## 10. 如何更新 / 重新打包（开发侧）

源码改完后重新构建并打便携包（在**开发机**上执行）：

    # 1) 构建可分发的 exe（含 MSI + NSIS）
    powershell -ExecutionPolicy Bypass -File build_cuda.ps1 -Mode tauri
    # 2) 刷新根目录 exe
    Copy-Item target\release\live-translator-desktop.exe LiveTranslator.exe -Force
    # 3) 生成便携包（可选 -IncludeAllModels 带全部模型；默认带推荐模型）
    powershell -ExecutionPolicy Bypass -File scripts\make_portable.ps1 -IncludeFfmpeg
    #    输出：dist\LiveTranslator-portable-v<版本>.zip

构建前置条件（仅开发机需要）：Rust nightly + MSVC 14.44 + CUDA Toolkit 13.1（bin\x64\ 进 PATH）
+ CMake/Ninja + FFmpeg。见 work/handoff.md 的 Windows 构建说明。

---

## 11. 分发与许可提醒

- **NVIDIA CUDA 运行时 DLL**（cublas64_13.dll 等）可随应用再分发，需遵守 NVIDIA 的
  CUDA 最终用户许可协议（EULA）；发布时建议附带 NVIDIA 许可说明。
- **模型文件**：Whisper（MIT）、Silero VAD（MIT）、Hy-MT（Tencent，注意其仓库许可）、
  speakrs（MIT）——发布时按各自许可证附上版权/许可文本（见仓库 THIRD_PARTY_NOTICES.md）。
- 当前构建面向 **sm_86（RTX 30 系）** 优化。若目标机器是 RTX 20 系（sm_75）或 40 系
  （sm_89），CUDA 架构需在构建脚本里调整 CMAKE_CUDA_ARCHITECTURES 后重新构建
  （30 系默认向下兼容运行，40 系通常也可用，但建议按目标机型重新编译）。


---

## 12. 轻量版（Lite）与首次运行自动下载

如果你想让**初始下载更小**，用轻量版：

- 文件：LiveTranslator-portable-lite-v0.1.0.zip（约 0.4 GB，完整版约 2.7 GB）
- 轻量版不含语音/翻译模型；首次运行时在设置里点按钮联网补齐：
  - 【下载 ASR 模型】约 1 GB：Whisper small + large-v3-turbo q5_0 + Silero VAD（识别必需）
  - 【下载离线翻译】约 1.1 GB：Hy-MT（只有用「本地 Hy-MT」翻译才需要）
- 下载带进度显示、自动 SHA-256 校验、断点可重试；下载到 exe 同目录的 models\ 里。
- 不做识别/只用云端翻译的用户可以只下 ASR 模型。

首次使用流程（轻量版）：
1. 解压 → 双击「启动直播翻译.cmd」。
2. 打开设置（齿轮），在「模型文件」区点【下载 ASR 模型】，等进度到 100%。
3. 选模型（建议 large-v3-turbo q5_0）→ 选翻译 → 选音源 → ▶ 开始识别。

镜像切换：默认从 hf-mirror.com 下载（国内快）。若网络环境不同，可在系统环境变量里设置
LIVE_TRANSLATOR_MODEL_MIRROR=https://huggingface.co 后重启应用。

---

## 13. 打包产物清单（v0.1.0）

| 产物 | 路径（开发机） | 大小 | 用途 |
|---|---|---|---|
| 完整便携 zip | dist\LiveTranslator-portable-v0.1.0.zip | 2.7 GB | 解压即用，离线可用（推荐） |
| 轻量便携 zip | dist\LiveTranslator-portable-lite-v0.1.0.zip | 0.4 GB | 首次运行联网下载模型 |
| NSIS 安装包 | target\release\bundle\nsis\Live Translator_0.1.0_x64-setup.exe | ~36 MB | 仅 App，不含模型/CUDA |
| MSI | target\release\bundle\msi\Live Translator_0.1.0_x64_en-US.msi | ~94 MB | 同上 |
