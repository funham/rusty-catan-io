mod game;
mod protocol;
mod session;
mod ws;

use axum::{Router, routing::get};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/ws", get(ws::ws_handler));

    println!("Server running on ws://localhost:8080/ws");

    let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
