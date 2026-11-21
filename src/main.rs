use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use uuid::Uuid;
use warp::ws::{Message, WebSocket};
use warp::Filter;

#[derive(Clone, Serialize, Deserialize)]
struct Player {
    id: String,
    name: String,
    x: f32,
    y: f32,
    hp: i32,
    kills: i32,
    color: String,
}

#[derive(Clone, Serialize)]
struct Bullet {
    owner_id: String,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
}

#[derive(Clone, Serialize)]
struct GameState {
    players: HashMap<String, Player>,
    bullets: Vec<Bullet>,
}

#[derive(Deserialize)]
struct ClientInput {
    action: String,
    name: Option<String>,
    dx: Option<f32>,
    dy: Option<f32>,
    angle: Option<f32>,
}

type SharedState = Arc<Mutex<GameState>>;

#[tokio::main]
async fn main() {
    let state: SharedState = Arc::new(Mutex::new(GameState {
        players: HashMap::new(),
        bullets: Vec::new(),
    }));

    let (tx, _rx) = broadcast::channel::<String>(100);

    // --- ИГРОВОЙ ЦИКЛ RUST (30 FPS) ---
    let loop_state = state.clone();
    let loop_tx = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(33));
        loop {
            interval.tick().await;
            let snapshot = {
                let mut game = loop_state.lock().unwrap();
                
                // 1. Двигаем пули
                for b in &mut game.bullets {
                    b.x += b.dx;
                    b.y += b.dy;
                }

                // 2. Проверяем попадания по индексам (чтобы не конфликтовать с Borrow Checker)
                let mut dead_bullets = Vec::new();
                let mut killers = Vec::new();
                let bullets_len = game.bullets.len();

                for b_idx in 0..bullets_len {
                    let bx = game.bullets[b_idx].x;
                    let by = game.bullets[b_idx].y;
                    let b_owner = game.bullets[b_idx].owner_id.clone();

                    if bx < 0.0 || bx > 800.0 || by < 0.0 || by > 600.0 {
                        dead_bullets.push(b_idx);
                        continue;
                    }

                    for (p_id, p) in game.players.iter_mut() {
                        if *p_id != b_owner {
                            let dist = ((p.x - bx).powi(2) + (p.y - by).powi(2)).sqrt();
                            if dist < 20.0 {
                                p.hp -= 20;
                                dead_bullets.push(b_idx);
                                if p.hp <= 0 {
                                    p.hp = 100;
                                    p.x = 400.0;
                                    p.y = 300.0;
                                    killers.push(b_owner);
                                }
                                break;
                            }
                        }
                    }
                }

                // 3. Начисляем фраги победителям
                for k_id in killers {
                    if let Some(killer) = game.players.get_mut(&k_id) {
                        killer.kills += 1;
                    }
                }

                // 4. Удаляем попавшие пули
                dead_bullets.sort_unstable();
                dead_bullets.dedup();
                for idx in dead_bullets.into_iter().rev() {
                    if idx < game.bullets.len() {
                        game.bullets.remove(idx);
                    }
                }

                serde_json::to_string(&*game).unwrap()
            };

            let _ = loop_tx.send(snapshot);
        }
    });

    let state_filter = warp::any().map(move || state.clone());
    let tx_filter = warp::any().map(move || tx.clone());

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(state_filter)
        .and(tx_filter)
        .map(|ws: warp::ws::Ws, state, tx| {
            ws.on_upgrade(move |socket| handle_player(socket, state, tx))
        });

    let static_files = warp::fs::dir("static");
    let routes = ws_route.or(static_files);

    println!("🦀 Сервер RustArena запущен: http://localhost:3030");
    warp::serve(routes).run(([0, 0, 0, 0], 3030)).await;
}

async fn handle_player(ws: WebSocket, state: SharedState, tx: broadcast::Sender<String>) {
    let (mut user_ws_tx, mut user_ws_rx) = ws.split();
    let mut rx = tx.subscribe();
    let player_id = Uuid::new_v4().to_string();

    let init_msg = format!(r#"{{"type":"init","id":"{}"}}"#, player_id);
    let _ = user_ws_tx.send(Message::text(init_msg)).await;

    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if user_ws_tx.send(Message::text(msg)).await.is_err() {
                break;
            }
        }
    });

    let p_id_clone = player_id.clone();
    let state_clone = state.clone();
    while let Some(Ok(msg)) = user_ws_rx.next().await {
        if let Ok(text) = msg.to_str() {
            if let Ok(input) = serde_json::from_str::<ClientInput>(text) {
                let mut game = state_clone.lock().unwrap();
                match input.action.as_str() {
                    "join" => {
                        // Заранее считаем количество игроков перед вставкой
                        let count = game.players.len();
                        let colors = ["#00f2fe", "#ff0844", "#00e676", "#ffea00", "#d500f9"];
                        let color = colors[count % colors.len()].to_string();
                        let start_x = 100.0 + (count as f32 * 80.0);

                        game.players.insert(
                            p_id_clone.clone(),
                            Player {
                                id: p_id_clone.clone(),
                                name: input.name.unwrap_or_else(|| "Боец".to_string()),
                                x: start_x,
                                y: 200.0,
                                hp: 100,
                                kills: 0,
                                color,
                            },
                        );
                    }
                    "move" => {
                        if let Some(p) = game.players.get_mut(&p_id_clone) {
                            p.x = (p.x + input.dx.unwrap_or(0.0) * 5.0).clamp(20.0, 780.0);
                            p.y = (p.y + input.dy.unwrap_or(0.0) * 5.0).clamp(20.0, 580.0);
                        }
                    }
                    "shoot" => {
                        if let (Some(p), Some(angle)) = (game.players.get(&p_id_clone), input.angle) {
                            let b = Bullet {
                                owner_id: p_id_clone.clone(),
                                x: p.x,
                                y: p.y,
                                dx: angle.cos() * 12.0,
                                dy: angle.sin() * 12.0,
                            };
                            game.bullets.push(b);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    state.lock().unwrap().players.remove(&player_id);
    send_task.abort();
}
