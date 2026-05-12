const container = document.getElementById("chat-container");

function connect() {
    const ws = new WebSocket(`ws://${location.host}/ws/overlay`);

    ws.onopen = () => {
        container.textContent = "";
    };

    ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        const div = document.createElement("div");
        div.className = `chat-item ${data.kind || ""}`;
        div.textContent = data.text || JSON.stringify(data);
        container.appendChild(div);
        while (container.children.length > 30) {
            container.firstChild.remove();
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
