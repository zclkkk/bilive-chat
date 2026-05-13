const statusBadge = document.getElementById("status");
const connInfo = document.getElementById("conn-info");
const connError = document.getElementById("conn-error");
const btnStart = document.getElementById("btn-start");
const btnStop = document.getElementById("btn-stop");
const messages = document.getElementById("messages");

function formatSocketStatus(status) {
    if (!status || typeof status !== "object") return String(status);
    const type = status.type;
    if (type === "connected") return "connected";
    if (type === "connecting") return "connecting";
    if (type === "disconnected") {
        return status.error ? "disconnected: " + status.error : "disconnected";
    }
    return type || "unknown";
}

function setLiveStatus(text, state) {
    statusBadge.textContent = text;
    statusBadge.className = "status-badge";
    if (state) {
        statusBadge.classList.add(state);
    }
}

function updateConnectionUI(socketStatus) {
    const label = formatSocketStatus(socketStatus);
    connInfo.textContent = label;
    const type = socketStatus && socketStatus.type;
    if (type === "connected") {
        setLiveStatus("connected", "connected");
        btnStart.disabled = true;
        btnStop.disabled = false;
    } else if (type === "connecting") {
        setLiveStatus("connecting", "connecting");
        btnStart.disabled = true;
        btnStop.disabled = false;
    } else {
        setLiveStatus("disconnected", "error");
        btnStart.disabled = false;
        btnStop.disabled = true;
    }
}

function flashHint(el, text, ms) {
    el.textContent = text;
    setTimeout(() => { el.textContent = ""; }, ms || 2000);
}

function logEvent(tag, body) {
    const div = document.createElement("div");
    div.textContent = `[${tag}] ${body}`;
    messages.prepend(div);
    while (messages.children.length > 50) {
        messages.lastChild.remove();
    }
}

function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws/panel`);

    ws.onopen = () => {
        logEvent("panel", "ws connected");
    };

    ws.onmessage = (event) => {
        const data = JSON.parse(event.data);

        if (data.type === "status" && data.status && typeof data.status === "object" && typeof data.status.type === "string") {
            updateConnectionUI(data.status);
        }

        const tag = data.type || "?";
        const body = data.message || formatSocketStatus(data.status) || JSON.stringify(data);
        logEvent(tag, body);
    };

    ws.onclose = () => {
        logEvent("panel", "ws disconnected — reconnecting...");
        setTimeout(connect, 2000);
    };

    ws.onerror = () => {
        ws.close();
    };
}

connect();

async function fetchStatus() {
    try {
        const resp = await fetch("/api/bilibili/status");
        if (!resp.ok) {
            logEvent("status", "fetch failed: HTTP " + resp.status);
            return null;
        }
        const status = await resp.json();
        updateConnectionUI(status);
        return status;
    } catch (e) {
        logEvent("status", "fetch error: " + (e.message || e));
        return null;
    }
}

fetchStatus();

btnStart.addEventListener("click", async () => {
    updateConnectionUI({ type: "connecting" });
    connError.textContent = "";
    logEvent("panel", "start requested");
    try {
        const resp = await fetch("/api/bilibili/start", { method: "POST" });
        if (resp.ok) {
            logEvent("panel", "start ok");
        } else {
            const data = await resp.json().catch(() => ({}));
            const error = data.error || "start failed";
            connError.textContent = error;
            logEvent("panel", "start failed: " + error);
        }
    } catch (e) {
        const error = e.message || "network error";
        connError.textContent = error;
        logEvent("panel", "start failed: " + error);
    }
    await fetchStatus();
});

btnStop.addEventListener("click", async () => {
    connError.textContent = "";
    logEvent("panel", "stop requested");
    try {
        const resp = await fetch("/api/bilibili/stop", { method: "POST" });
        if (resp.ok) {
            logEvent("panel", "stop ok");
        } else {
            const data = await resp.json().catch(() => ({}));
            const error = data.error || "stop failed";
            connError.textContent = error;
            logEvent("panel", "stop failed: " + error);
        }
    } catch (e) {
        const error = e.message || "network error";
        connError.textContent = error;
        logEvent("panel", "stop failed: " + error);
    }
    await fetchStatus();
});

async function loadOverlayUrl() {
    const resp = await fetch("/api/overlay-url");
    const data = await resp.json();
    document.getElementById("obs-url").textContent = data.url;
}

document.getElementById("copy-url").addEventListener("click", async () => {
    const url = document.getElementById("obs-url").textContent;
    await navigator.clipboard.writeText(url);
    flashHint(document.getElementById("copy-status"), "copied!");
});

loadOverlayUrl();

const configForm = document.getElementById("config-form");
const configStatus = document.getElementById("config-status");
let currentConfig = {};

async function loadConfig() {
    const resp = await fetch("/api/config");
    currentConfig = await resp.json();
    document.getElementById("cfg-room").value = currentConfig.room_id || 0;
}

configForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    configStatus.textContent = "saving...";
    const body = {
        ...currentConfig,
        room_id: parseInt(document.getElementById("cfg-room").value, 10),
    };
    const resp = await fetch("/api/config", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
    });
    flashHint(configStatus, resp.ok ? "saved" : "error");
    loadOverlayUrl();
});

loadConfig();

const loginForm = document.getElementById("login-form");
const loginStatus = document.getElementById("login-status");

loginForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    loginStatus.textContent = "saving...";
    const cookie = document.getElementById("login-cookie").value;
    const resp = await fetch("/api/bilibili/login-state", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ cookie }),
    });
    flashHint(loginStatus, resp.ok ? "saved" : "error");
});

document.getElementById("delete-cookie").addEventListener("click", async () => {
    loginStatus.textContent = "deleting...";
    const resp = await fetch("/api/bilibili/login-state", { method: "DELETE" });
    if (resp.ok) {
        document.getElementById("login-cookie").value = "";
        flashHint(loginStatus, "deleted");
    } else {
        flashHint(loginStatus, "error");
    }
});
