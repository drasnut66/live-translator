import { useEffect, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AudioLines,
  History,
  Languages,
  Lock,
  Minus,
  Pause,
  Play,
  Settings2,
  Unlock,
  X,
} from "lucide-react";
import "./App.css";

type RuntimeStatus = {
  platform: string;
  modelReady: boolean;
  vadReady: boolean;
  whisperReady: boolean;
};

type SubtitleSnapshot = {
  stableJa: string;
  unstableJa: string;
};

const fallbackSnapshot: SubtitleSnapshot = {
  stableJa: "パスタっていろんな味にできるし、",
  unstableJa: "よくない？",
};

const defaultStatus: RuntimeStatus = {
  platform: "desktop",
  modelReady: false,
  vadReady: false,
  whisperReady: false,
};

function App() {
  const [running, setRunning] = useState(true);
  const [locked, setLocked] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [source, setSource] = useState("browser");
  const [provider, setProvider] = useState("openai");
  const [opacity, setOpacity] = useState(92);
  const [status, setStatus] = useState(defaultStatus);
  const [subtitle, setSubtitle] = useState(fallbackSnapshot);

  useEffect(() => {
    Promise.all([
      invoke<RuntimeStatus>("runtime_status"),
      invoke<SubtitleSnapshot>("demo_subtitle"),
    ])
      .then(([nextStatus, nextSubtitle]) => {
        setStatus(nextStatus);
        setSubtitle(nextSubtitle);
      })
      .catch(() => {
        // Browser preview uses deterministic local sample data.
      });
  }, []);

  const ready = status.modelReady && status.vadReady && status.whisperReady;

  async function minimizeWindow() {
    try {
      await getCurrentWindow().minimize();
    } catch {
      // No native window is present in browser preview mode.
    }
  }

  async function closeWindow() {
    try {
      await getCurrentWindow().close();
    } catch {
      // No native window is present in browser preview mode.
    }
  }

  return (
    <main
      className={`overlay ${locked ? "is-locked" : ""}`}
      style={{ "--overlay-opacity": opacity / 100 } as CSSProperties}
    >
      <header className="toolbar" data-tauri-drag-region>
        <div className="identity" data-tauri-drag-region>
          <span className={`status-light ${ready ? "is-ready" : ""}`} />
          <Languages size={16} strokeWidth={2.2} />
          <strong>日中同传</strong>
          <span className="runtime">JA → ZH · {status.platform}</span>
        </div>

        <div className="controls">
          <label className="source-select" title="音频来源">
            <AudioLines size={15} aria-hidden="true" />
            <select
              aria-label="音频来源"
              data-testid="audio-source"
              value={source}
              onChange={(event) => setSource(event.target.value)}
            >
              <option value="browser">浏览器音频</option>
              <option value="system">系统音频</option>
              <option value="microphone">麦克风</option>
            </select>
          </label>

          <button
            className={`icon-button ${running ? "is-active" : ""}`}
            type="button"
            title={running ? "暂停识别" : "继续识别"}
            aria-label={running ? "暂停识别" : "继续识别"}
            data-testid="recognition-toggle"
            onClick={() => setRunning((value) => !value)}
          >
            {running ? <Pause size={16} /> : <Play size={16} />}
          </button>
          <button
            className={`icon-button ${historyOpen ? "is-active" : ""}`}
            type="button"
            title="字幕历史"
            aria-label="字幕历史"
            aria-expanded={historyOpen}
            data-testid="history-toggle"
            onClick={() => {
              setHistoryOpen((value) => !value);
              setSettingsOpen(false);
            }}
          >
            <History size={16} />
          </button>
          <button
            className={`icon-button ${settingsOpen ? "is-active" : ""}`}
            type="button"
            title="设置"
            aria-label="设置"
            aria-expanded={settingsOpen}
            data-testid="settings-toggle"
            onClick={() => {
              setSettingsOpen((value) => !value);
              setHistoryOpen(false);
            }}
          >
            <Settings2 size={16} />
          </button>
          <button
            className={`icon-button ${locked ? "is-active" : ""}`}
            type="button"
            title={locked ? "解锁窗口" : "锁定窗口"}
            aria-label={locked ? "解锁窗口" : "锁定窗口"}
            data-testid="lock-toggle"
            onClick={() => setLocked((value) => !value)}
          >
            {locked ? <Lock size={16} /> : <Unlock size={16} />}
          </button>
          <span className="control-separator" />
          <button className="icon-button" type="button" title="最小化" aria-label="最小化" onClick={minimizeWindow}>
            <Minus size={16} />
          </button>
          <button className="icon-button danger" type="button" title="退出" aria-label="退出" onClick={closeWindow}>
            <X size={16} />
          </button>
        </div>
      </header>

      <section className="subtitle-surface" aria-live="polite">
        <div className="language-marker ja">JA</div>
        <p className="subtitle japanese">
          <span>{subtitle.stableJa}</span>
          <span className="partial">{subtitle.unstableJa}</span>
        </p>

        <div className="language-marker zh">ZH</div>
        <p className="subtitle chinese">意大利面可以搭配各种口味，不是很好吗？</p>
      </section>

      <footer className="statusbar">
        <span>{running ? "识别中" : "已暂停"}</span>
        <span className="status-divider" />
        <span>large-v3-turbo q5</span>
        <span className="status-divider" />
        <span>RTF 0.11</span>
        <span className="spacer" />
        <span className="latency">1.2s</span>
      </footer>

      {settingsOpen && (
        <aside className="quick-settings" aria-label="快速设置">
          <label>
            <span>翻译</span>
            <select
              aria-label="翻译服务"
              value={provider}
              onChange={(event) => setProvider(event.target.value)}
            >
              <option value="openai">OpenAI Compatible</option>
              <option value="deepl">DeepL</option>
            </select>
          </label>
          <label className="opacity-control">
            <span>透明度</span>
            <input
              type="range"
              min="55"
              max="100"
              value={opacity}
              aria-label="窗口透明度"
              onInput={(event) => setOpacity(Number(event.currentTarget.value))}
            />
            <output>{opacity}%</output>
          </label>
        </aside>
      )}

      {historyOpen && (
        <aside className="subtitle-history" aria-label="字幕历史记录">
          <div className="history-time">00:02:14</div>
          <div>
            <p lang="ja">パスタっていろんな味にできるし、よくない？</p>
            <p lang="zh-CN">意大利面可以搭配各种口味，不是很好吗？</p>
          </div>
        </aside>
      )}
    </main>
  );
}

export default App;
