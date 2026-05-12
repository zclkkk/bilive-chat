const statusBadge = document.getElementById("status");
const connInfo = document.getElementById("conn-info");
const messages = document.getElementById("messages");

function setStatus(text, state) {
    statusBadge.textContent = text;
    statusBadge.className = "status-badge";
    if (state) {
        statusBadge.classList.add(state);
    }
}

function flashHint(el, text, ms) {
    el.textContent = text;
    setTimeout(() => { el.textContent = ""; }, ms || 2000);
}

function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws/panel`);

    ws.onopen = () => {
        setStatus("connected", "connected");
        connInfo.textContent = "WebSocket connected";
    };

    ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        const div = document.createElement("div");

        if (data.type === "status") {
            connInfo.textContent = data.message || data.status || "connected";
        }

        const tag = data.type || "?";
        const body = data.message || data.status || JSON.stringify(data);
        div.textContent = `[${tag}] ${body}`;
        messages.prepend(div);
        while (messages.children.length > 50) {
            messages.lastChild.remove();
        }
    };

    ws.onclose = () => {
        setStatus("disconnected", "error");
        connInfo.textContent = "Disconnected — reconnecting...";
        setTimeout(connect, 2000);
    };

    ws.onerror = () => {
        ws.close();
    };
}

connect();

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
