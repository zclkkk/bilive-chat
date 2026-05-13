const container = document.getElementById("chat-container");
const params = new URLSearchParams(location.search);

function intParam(name, fallback, min, max) {
    const parsed = parseInt(params.get(name), 10);
    return clamp(Number.isFinite(parsed) ? parsed : fallback, min, max);
}

function clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
}

const MAX_ITEMS = intParam("max_items", 50, 1, 200);
const LIFETIME = intParam("lifetime", 300, 1, 3600) * 1000;
const AVATAR_PARAM = params.get("show_avatar");
const SHOW_AVATAR = AVATAR_PARAM === null ? true : AVATAR_PARAM !== "false" && AVATAR_PARAM !== "0";
const FONT_SIZE = intParam("font_size", 14, 8, 48);
const FADE_MS = 300;

document.documentElement.style.fontSize = FONT_SIZE + "px";

function removeItem(el) {
    el.classList.add("fade-out");
    setTimeout(() => el.remove(), FADE_MS);
}

function addItem(element) {
    const timerId = setTimeout(() => removeItem(element), LIFETIME);
    element.dataset.timerId = String(timerId);
    container.appendChild(element);

    while (container.children.length > MAX_ITEMS) {
        const oldest = container.firstElementChild;
        if (oldest.dataset.timerId) {
            clearTimeout(Number(oldest.dataset.timerId));
        }
        oldest.remove();
    }
}

function span(className, text) {
    const el = document.createElement("span");
    el.className = className;
    el.textContent = text;
    return el;
}

function buildItem(type, data, contentSpans) {
    const item = document.createElement("div");
    item.className = `chat-item ${type}`;

    if (SHOW_AVATAR) {
        const avatar = document.createElement("div");
        avatar.className = "avatar";
        avatar.style.background = data.avatar_color || "#666";
        avatar.textContent = (data.sender || "?")[0].toUpperCase();
        item.appendChild(avatar);
    }

    const body = document.createElement("div");
    body.className = "body";

    const sender = span("sender", data.sender);
    sender.style.color = data.avatar_color || "#fff";
    body.appendChild(sender);

    for (const s of contentSpans) {
        body.appendChild(s);
    }

    item.appendChild(body);
    return item;
}

function renderNormal(data) {
    addItem(buildItem("normal", data, [span("text", data.text)]));
}

function renderGift(data) {
    addItem(buildItem("gift", data, [span("text", `sent ${data.gift_name} x${data.count}`)]));
}

function renderSuperChat(data) {
    addItem(buildItem("super_chat", data, [
        span("amount", `${data.currency} ${data.amount}`),
        span("text", data.text),
    ]));
}

function renderGuard(data) {
    addItem(buildItem("guard", data, [span("text", `joined as ${data.guard_name} x${data.count}`)]));
}

function renderSystem(data) {
    const item = document.createElement("div");
    item.className = "chat-item system";
    item.textContent = data.text;
    addItem(item);
}

const renderers = {
    normal: renderNormal,
    gift: renderGift,
    super_chat: renderSuperChat,
    guard: renderGuard,
    system: renderSystem,
};

function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws/overlay`);

    ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        const renderer = renderers[data.type];
        if (renderer) {
            renderer(data);
        }
    };

    ws.onclose = () => {
        setTimeout(connect, 2000);
    };

    ws.onerror = () => {
        ws.close();
    };
}

connect();
