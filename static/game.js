const canvas = document.getElementById('arena');
const ctx = canvas.getContext('2d');
const socket = new WebSocket(`ws://${location.host}/ws`);

let myId = null;
let gameState = { players: {}, bullets: [] };
const keys = {};

socket.onmessage = (event) => {
    const data = JSON.parse(event.data);
    if (data.type === 'init') {
        myId = data.id;
    } else if (data.players) {
        gameState = data;
        const me = gameState.players[myId];
        if (me) {
            document.getElementById('my-hp').innerText = me.hp;
            document.getElementById('my-kills').innerText = me.kills;
        }
    }
};

function joinArena() {
    const name = document.getElementById('nickname').value.trim() || 'Игрок';
    document.getElementById('join-box').style.display = 'none';
    document.getElementById('game-area').style.display = 'block';
    socket.send(JSON.stringify({ action: 'join', name }));
}

window.addEventListener('keydown', (e) => keys[e.code] = true);
window.addEventListener('keyup', (e) => keys[e.code] = false);

// Стрельба по клику мыши
canvas.addEventListener('mousedown', (e) => {
    const me = gameState.players[myId];
    if (!me) return;
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const angle = Math.atan2(my - me.y, mx - me.x);
    socket.send(JSON.stringify({ action: 'shoot', angle }));
});

// Отправка движения 30 раз в секунду
setInterval(() => {
    if (!myId) return;
    let dx = 0, dy = 0;
    if (keys['KeyW'] || keys['ArrowUp']) dy -= 1;
    if (keys['KeyS'] || keys['ArrowDown']) dy += 1;
    if (keys['KeyA'] || keys['ArrowLeft']) dx -= 1;
    if (keys['KeyD'] || keys['ArrowRight']) dx += 1;
    if (dx !== 0 || dy !== 0) {
        socket.send(JSON.stringify({ action: 'move', dx, dy }));
    }
}, 33);

// Отрисовка кадров на Canvas
function render() {
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // Сетка арены
    ctx.strokeStyle = '#1e293b';
    for (let x = 0; x < 800; x += 40) { ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, 600); ctx.stroke(); }
    for (let y = 0; y < 600; y += 40) { ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(800, y); ctx.stroke(); }

    // Пули
    ctx.fillStyle = '#facc15';
    gameState.bullets.forEach(b => {
        ctx.beginPath();
        ctx.arc(b.x, b.y, 4, 0, Math.PI * 2);
        ctx.fill();
    });

    // Игроки
    for (let id in gameState.players) {
        const p = gameState.players[id];
        ctx.fillStyle = p.color;
        ctx.beginPath();
        ctx.arc(p.x, p.y, 18, 0, Math.PI * 2);
        ctx.fill();

        // Ник и полоска HP
        ctx.fillStyle = 'white';
        ctx.font = '12px sans-serif';
        ctx.textAlign = 'center';
        ctx.fillText(`${p.name} [${p.kills}]`, p.x, p.y - 26);
    }

    requestAnimationFrame(render);
}
render();
