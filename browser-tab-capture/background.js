// MV3 service worker: bridges the tab-capture permission into an offscreen
// document (MV3 requires tabCapture.getMediaStreamId to be called from the
// service worker, while getUserMedia must run in an offscreen document).

let capturedTabId = null;

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

async function startCapture() {
  const tab = await activeTab();
  const streamId = await new Promise((resolve, reject) => {
    chrome.tabCapture.getMediaStreamId({ targetTabId: tab.id }, (id) => {
      const error = chrome.runtime.lastError;
      if (error) return reject(new Error(error.message));
      if (!id) return reject(new Error("tabCapture returned no stream id"));
      resolve(id);
    });
  });

  await ensureOffscreenDocument();

  await new Promise((resolve, reject) => {
    chrome.runtime.sendMessage(
      { type: "start", streamId, tabId: tab.id },
      (response) => {
        const error = chrome.runtime.lastError;
        if (error) return reject(new Error(error.message));
        if (response && response.error) return reject(new Error(response.error));
        resolve();
      }
    );
  });

  capturedTabId = tab.id;
  await chrome.action.setBadgeText({ text: "ON" });
}

async function stopCapture() {
  await new Promise((resolve) => {
    chrome.runtime.sendMessage({ type: "stop" }, () => resolve());
  });
  capturedTabId = null;
  await chrome.action.setBadgeText({ text: "" });
}

chrome.action.onClicked.addListener(() => {
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
