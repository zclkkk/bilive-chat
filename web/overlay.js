const container = document.getElementById("chat-container");
const params = new URLSearchParams(location.search);

const MAX_ITEMS = parseInt(params.get("max_items"), 10) || 50;
const LIFETIME = (parseInt(params.get("lifetime"), 10) || 300) * 1000;
const AVATAR_PARAM = params.get("show_avatar");
const SHOW_AVATAR = AVATAR_PARAM === null ? true : AVATAR_PARAM !== "false" && AVATAR_PARAM !== "0";
const FONT_SIZE = parseInt(params.get("font_size"), 10) || 14;

document.documentElement.style.fontSize = FONT_SIZE + "px";

function addItem(element) {
    const timerId = setTimeout(() => {
        element.dataset.timerId = "";
        element.remove();
    }, LIFETIME);
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

function renderNormal(data) {
    const item = document.createElement("div");
    item.className = "chat-item normal";

    if (SHOW_AVATAR) {
        const avatar = document.createElement("div");
        avatar.className = "avatar";
        avatar.style.background = data.avatar_color || "#666";
        avatar.textContent = (data.sender || "?")[0].toUpperCase();
        item.appendChild(avatar);
    }

    const body = document.createElement("div");
    body.className = "body";

    const sender = document.createElement("span");
    sender.className = "sender";
    sender.style.color = data.avatar_color || "#fff";
    sender.textContent = data.sender;

    const text = document.createElement("span");
    text.className = "text";
    text.textContent = data.text;

    body.appendChild(sender);
    body.appendChild(text);
    item.appendChild(body);
    addItem(item);
}

function renderGift(data) {
    const item = document.createElement("div");
    item.className = "chat-item gift";

    if (SHOW_AVATAR) {
        const avatar = document.createElement("div");
        avatar.className = "avatar";
        avatar.style.background = data.avatar_color || "#666";
        avatar.textContent = (data.sender || "?")[0].toUpperCase();
        item.appendChild(avatar);
    }

    const body = document.createElement("div");
    body.className = "body";

    const sender = document.createElement("span");
    sender.className = "sender";
    sender.style.color = data.avatar_color || "#fff";
    sender.textContent = data.sender;

    const text = document.createElement("span");
    text.className = "text";
    text.textContent = `sent ${data.gift_name} x${data.count}`;

    body.appendChild(sender);
    body.appendChild(text);
    item.appendChild(body);
    addItem(item);
}

function renderSuperChat(data) {
    const item = document.createElement("div");
    item.className = "chat-item super_chat";

    if (SHOW_AVATAR) {
        const avatar = document.createElement("div");
        avatar.className = "avatar";
        avatar.style.background = data.avatar_color || "#666";
        avatar.textContent = (data.sender || "?")[0].toUpperCase();
        item.appendChild(avatar);
    }

    const body = document.createElement("div");
    body.className = "body";

    const sender = document.createElement("span");
    sender.className = "sender";
    sender.style.color = data.avatar_color || "#fff";
    sender.textContent = data.sender;

    const amount = document.createElement("span");
    amount.className = "amount";
    amount.textContent = `${data.currency} ${data.amount}`;

    const text = document.createElement("span");
    text.className = "text";
    text.textContent = data.text;

    body.appendChild(sender);
    body.appendChild(amount);
    body.appendChild(text);
    item.appendChild(body);
    addItem(item);
}

function renderGuard(data) {
    const item = document.createElement("div");
    item.className = "chat-item guard";

    if (SHOW_AVATAR) {
        const avatar = document.createElement("div");
        avatar.className = "avatar";
        avatar.style.background = data.avatar_color || "#666";
        avatar.textContent = (data.sender || "?")[0].toUpperCase();
        item.appendChild(avatar);
    }

    const body = document.createElement("div");
    body.className = "body";

    const sender = document.createElement("span");
    sender.className = "sender";
    sender.style.color = data.avatar_color || "#fff";
    sender.textContent = data.sender;

    const text = document.createElement("span");
    text.className = "text";
    text.textContent = `joined as ${data.guard_name} x${data.count}`;

    body.appendChild(sender);
    body.appendChild(text);
    item.appendChild(body);
    addItem(item);
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
