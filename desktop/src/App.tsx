import { useEffect, useState, type CSSProperties } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
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

type SessionStatusPayload = {
  state: "loading" | "running" | "stopped" | "error";
  message?: string;
};

type SubtitlePartialPayload = {
  segmentId: number;
  revision: number;
  stableJa: string;
  unstableJa: string;
  inputMs: number;
  decodeMs: number;
};

type SubtitleCommitPayload = {
  segmentId: number;
  startMs: number;
  endMs: number;
  textJa: string;
};

type TranslationPayload = {
  segmentId: number;
  startMs: number;
  endMs: number;
  textJa: string;
  textZh: string;
};

type HistoryEntry = {
  time: string;
  japanese: string;
  chinese: string;
  segmentId: number;
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
  const [running, setRunning] = useState(false);
  const [locked, setLocked] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [source, setSource] = useState("test");
  const [provider, setProvider] = useState("openai");
  const [opacity, setOpacity] = useState(92);
  const [status, setStatus] = useState(defaultStatus);
  const [subtitle, setSubtitle] = useState(fallbackSnapshot);
  const [translation, setTranslation] = useState("意大利面可以搭配各种口味，不是很好吗？");
  const [sessionState, setSessionState] = useState<SessionStatusPayload["state"]>("stopped");
  const [sessionMessage, setSessionMessage] = useState("");
  const [latencyMs, setLatencyMs] = useState<number | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([
    {
      time: "00:02:14",
      japanese: "パスタっていろんな味にできるし、よくない？",
      chinese: "意大利面可以搭配各种口味，不是很好吗？",
      segmentId: 0,
    },
  ]);

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

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];

    async function registerEvents() {
      try {
        const registrations = await Promise.all([
          listen<SessionStatusPayload>("session:status", ({ payload }) => {
            setSessionState(payload.state);
            setSessionMessage(payload.message ?? "");
            if (payload.state === "loading" || payload.state === "running") {
              setRunning(true);
            } else {
              setRunning(false);
            }
          }),
          listen<SubtitlePartialPayload>("subtitle:partial", ({ payload }) => {
            setSubtitle({ stableJa: payload.stableJa, unstableJa: payload.unstableJa });
            setLatencyMs(payload.decodeMs);
          }),
          listen<SubtitleCommitPayload>("subtitle:commit", ({ payload }) => {
            setTranslation("等待翻译");
            setHistory((entries) => {
              const next = {
                time: formatTimestamp(payload.endMs),
                japanese: payload.textJa,
                chinese: "等待翻译",
                segmentId: payload.segmentId,
              };
              const existing = entries.findIndex((entry) => entry.segmentId === payload.segmentId);
              if (existing < 0) return [next, ...entries].slice(0, 20);
              return entries.map((entry, index) => index === existing ? next : entry);
            });
          }),
          listen<TranslationPayload>("subtitle:translation", ({ payload }) => {
            setTranslation(payload.textZh);
            setHistory((entries) => entries.map((entry) => (
              entry.segmentId === payload.segmentId
                ? { ...entry, chinese: payload.textZh }
                : entry
            )));
          }),
        ]);

        if (disposed) {
          registrations.forEach((remove) => remove());
        } else {
          unlisten.push(...registrations);
        }
      } catch {
        // Native event IPC is unavailable in browser preview mode.
      }
    }

    void registerEvents();
    return () => {
      disposed = true;
      unlisten.forEach((remove) => remove());
    };
  }, []);

  const ready = status.modelReady && status.vadReady && status.whisperReady;

  async function toggleRecognition() {
    setSessionMessage("");
    try {
      if (running) {
        await invoke("stop_session");
        return;
      }
      if (source === "test") {
        await invoke("start_test_session");
      } else {
        throw new Error("当前阶段只有本地测试视频接入了实时识别");
      }
    } catch (error) {
      setRunning(false);
      setSessionState("error");
      setSessionMessage(error instanceof Error ? error.message : String(error));
    }
  }

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
          <span className={`status-light ${ready ? "is-ready" : ""} ${sessionState === "running" ? "is-listening" : ""}`} />
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
              <option value="test">本地测试视频</option>
              <option value="browser">浏览器音频</option>
              <option value="system">系统音频</option>
              <option value="microphone">麦克风</option>
            </select>
          </label>

          <button
            className={`icon-button ${running ? "is-active" : ""}`}
            type="button"
            title={running ? "暂停识别" : "开始识别"}
            aria-label={running ? "暂停识别" : "开始识别"}
            data-testid="recognition-toggle"
            onClick={toggleRecognition}
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
        <p className="subtitle chinese">{translation}</p>
      </section>

      <footer className="statusbar" title={sessionMessage || undefined}>
        <span>{sessionLabel(sessionState, running)}</span>
        <span className="status-divider" />
        <span>large-v3-turbo q5</span>
        <span className="status-divider" />
        <span>RTF 0.11</span>
        <span className="spacer" />
        <span className="latency">{latencyMs === null ? "--" : `${latencyMs}ms`}</span>
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
              <option value="deepl" disabled>DeepL（待接入）</option>
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
          {history.map((entry) => (
            <div className="history-entry" key={`${entry.segmentId}-${entry.time}`}>
              <div className="history-time">{entry.time}</div>
              <div>
                <p lang="ja">{entry.japanese}</p>
                <p lang="zh-CN">{entry.chinese}</p>
              </div>
            </div>
          ))}
        </aside>
      )}
    </main>
  );
}

function formatTimestamp(milliseconds: number) {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return [hours, minutes, seconds].map((value) => String(value).padStart(2, "0")).join(":");
}

function sessionLabel(state: SessionStatusPayload["state"], running: boolean) {
  if (state === "loading") return "加载模型";
  if (state === "running") return "识别中";
  if (state === "error") return "识别错误";
  return running ? "识别中" : "待机";
}

export default App;
