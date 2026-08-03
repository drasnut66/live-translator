// MV3 service worker: bridges the tab-capture permission into an offscreen
// document (MV3 requires tabCapture.getMediaStreamId to be called from the
// service worker, while getUserMedia must run in an offscreen document).
//
// capturedTabId lives in chrome.storage.session so it survives service-worker
// suspension, which otherwise resets it to null while Chrome still holds the
// capture and getMediaStreamId then fails with "Cannot capture a tab with an
// active stream".

const SESSION_KEY = "capturedTabId";

async function getCapturedTabId() {
  const data = await chrome.storage.session.get(SESSION_KEY);
  return data[SESSION_KEY] ?? null;
}

async function setCapturedTabId(tabId) {
  if (tabId === null) {
    await chrome.storage.session.remove(SESSION_KEY);
  } else {
    await chrome.storage.session.set({ [SESSION_KEY]: tabId });
  }
}

function activeTab() {
  return new Promise((resolve, reject) => {
    chrome.tabs.query({ active: true, currentWindow: true }, (tabs) => {
      const error = chrome.runtime.lastError;
      if (error) return reject(new Error(error.message));
      const tab = tabs && tabs[0];
      if (!tab) return reject(new Error("no active tab"));
      resolve(tab);
    });
  });
}

function ensureOffscreenDocument() {
  return chrome.offscreen.hasDocument().then((hasDocument) => {
    if (hasDocument) return;
    return chrome.offscreen.createDocument({
      url: "offscreen.html",
      reasons: ["USER_MEDIA"],
      justification: "Capture the active tab's audio and stream it to Live Translator",
    });
  });
}

// Chrome can drop the offscreen document while a capture is active; if so the
// capture stream is orphaned and getMediaStreamId reports "active stream".
// Recreate the offscreen document and stop the orphaned capture before retrying.
async function stopOrphanedCapture() {
  const hasDocument = await chrome.offscreen.hasDocument();
  if (hasDocument) {
    await new Promise((resolve) => {
      chrome.runtime.sendMessage({ type: "stop" }, () => resolve());
    });
  } else {
    await ensureOffscreenDocument();
  }
}

async function startCapture() {
  let capturedTabId = await getCapturedTabId();
  if (capturedTabId !== null) {
    await stopCapture();
    capturedTabId = null;
  }
  const tab = await activeTab();
  console.log("capturing tab", tab.id, tab.url);

  let streamId;
  try {
    streamId = await new Promise((resolve, reject) => {
      chrome.tabCapture.getMediaStreamId({ targetTabId: tab.id }, (id) => {
        const error = chrome.runtime.lastError;
        if (error) return reject(new Error(error.message));
        if (!id) return reject(new Error("tabCapture returned no stream id"));
        resolve(id);
      });
    });
  } catch (error) {
    // Likely "Cannot capture a tab with an active stream" left over from an
    // orphaned capture. Clear it and retry once.
    if (String(error).includes("active stream")) {
      await stopOrphanedCapture();
      streamId = await new Promise((resolve, reject) => {
        chrome.tabCapture.getMediaStreamId({ targetTabId: tab.id }, (id) => {
          const error = chrome.runtime.lastError;
          if (error) return reject(new Error(error.message));
          if (!id) return reject(new Error("tabCapture returned no stream id"));
          resolve(id);
        });
      });
    } else {
      throw error;
    }
  }

  await ensureOffscreenDocument();

  const response = await new Promise((resolve) => {
    chrome.runtime.sendMessage(
      { type: "start", streamId, tabId: tab.id },
      (message) => {
        const error = chrome.runtime.lastError;
        if (error) return resolve({ error: error.message });
        resolve(message || {});
      }
    );
  });
  if (response && response.error) {
    throw new Error(`offscreen failed: ${response.error}`);
  }

  await setCapturedTabId(tab.id);
  await chrome.action.setBadgeText({ text: "ON" });
  console.log("capture started");
}

async function stopCapture() {
  await new Promise((resolve) => {
    chrome.runtime.sendMessage({ type: "stop" }, () => resolve());
  });
  await setCapturedTabId(null);
  await chrome.action.setBadgeText({ text: "" });
}

// Restore the badge when the service worker wakes up.
chrome.runtime.onStartup.addListener(async () => {
  const capturedTabId = await getCapturedTabId();
  await chrome.action.setBadgeText({ text: capturedTabId === null ? "" : "ON" });
});
chrome.runtime.onInstalled.addListener(async () => {
  const capturedTabId = await getCapturedTabId();
  await chrome.action.setBadgeText({ text: capturedTabId === null ? "" : "ON" });
});

chrome.action.onClicked.addListener(async () => {
  const capturedTabId = await getCapturedTabId();
  if (capturedTabId !== null) {
    stopCapture().catch(console.error);
  } else {
    startCapture().catch((error) => {
      console.error("capture failed:", error);
      chrome.action.setBadgeText({ text: "ERR" });
      setTimeout(() => chrome.action.setBadgeText({ text: "" }), 4000);
    });
  }
});
