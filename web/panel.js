const status = document.getElementById("status");
const messages = document.getElementById("messages");

// WebSocket
function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws/panel`);

    ws.onopen = () => {
        status.textContent = "connected";
    };

    ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        const div = document.createElement("div");
        div.textContent = `[${data.type}] ${data.message || data.status || JSON.stringify(data)}`;
        messages.prepend(div);
        while (messages.children.length > 50) {
            messages.lastChild.remove();
        }
    };

    ws.onclose = () => {
        status.textContent = "disconnected — reconnecting...";
        setTimeout(connect, 2000);
    };

    ws.onerror = () => {
        ws.close();
    };
}

connect();

// Config
const configForm = document.getElementById("config-form");
const configStatus = document.getElementById("config-status");

async function loadConfig() {
    const resp = await fetch("/api/config");
    const cfg = await resp.json();
    document.getElementById("cfg-host").value = cfg.host || "";
    document.getElementById("cfg-port").value = cfg.port || 7792;
    document.getElementById("cfg-room").value = cfg.room_id || 0;
}

configForm.addEventListener("submit", async (e) => {
    e.preventDefault();
    configStatus.textContent = "saving...";
    const body = {
        host: document.getElementById("cfg-host").value,
        port: parseInt(document.getElementById("cfg-port").value, 10),
        room_id: parseInt(document.getElementById("cfg-room").value, 10),
        overlay: {},
        filter: {},
    };
    const resp = await fetch("/api/config", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
    });
    configStatus.textContent = resp.ok ? "saved" : "error";
    setTimeout(() => { configStatus.textContent = ""; }, 2000);
});

loadConfig();

// Login state
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
    loginStatus.textContent = resp.ok ? "saved" : "error";
    setTimeout(() => { loginStatus.textContent = ""; }, 2000);
});

document.getElementById("delete-cookie").addEventListener("click", async () => {
    loginStatus.textContent = "deleting...";
    const resp = await fetch("/api/bilibili/login-state", { method: "DELETE" });
    if (resp.ok) {
        document.getElementById("login-cookie").value = "";
        loginStatus.textContent = "deleted";
    } else {
        loginStatus.textContent = "error";
    }
    setTimeout(() => { loginStatus.textContent = ""; }, 2000);
});
