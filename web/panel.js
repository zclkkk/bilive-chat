const status = document.getElementById("status");
const messages = document.getElementById("messages");

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
