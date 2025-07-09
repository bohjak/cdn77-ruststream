mod trie;

use axum::{
    Router,
    body::{Body, Bytes},
    extract,
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, Response},
    routing::get,
};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use trie::Trie;

static BROADCAST_ENABLED: bool = false;

struct AppState {
    files: RwLock<Trie<Arc<RwLock<Vec<u8>>>>>,
    broadcasts: RwLock<Trie<Arc<broadcast::Sender<Bytes>>>>,
}

impl AppState {
    fn new() -> Arc<Self> {
        let files = RwLock::new(Trie::new());
        let broadcasts = RwLock::new(Trie::new());
        return Arc::new(Self { files, broadcasts });
    }
}

async fn handle_index(extract::State(state): extract::State<Arc<AppState>>) -> Html<String> {
    let files = state.files.read().await;
    return Html(
        files
            .keys()
            .iter()
            .map(|s| format!(r#"<a href="{0}">{0}</a><br>"#, s))
            .collect::<Vec<String>>()
            .join("\n"),
    );
}

async fn handle_get(
    extract::State(state): extract::State<Arc<AppState>>,
    extract::Path(path): extract::Path<String>,
) -> Result<Response, StatusCode> {
    let broadcasts_guard = state.broadcasts.read().await;
    if let Some(sender) = broadcasts_guard.get(&path) {
        let receiver = sender.subscribe();
        drop(broadcasts_guard);
        let stream = BroadcastStream::new(receiver);
        let body = Body::from_stream(stream);
        Ok(Response::new(body))
    } else {
        let files_guard = state.files.read().await;
        if let Some(file) = files_guard.get(&path) {
            println!("GET [200] {}", path);
            let file_guard = file.read().await;
            let body = Body::from(file_guard.clone());
            return Ok(Response::new(body));
        } else {
            println!("GET [404] {}", path);
            return Err(StatusCode::NOT_FOUND);
        }
    }
}

#[axum::debug_handler]
async fn handle_put(
    extract::State(state): extract::State<Arc<AppState>>,
    extract::Path(path): extract::Path<String>,
    body: Body,
) -> StatusCode {
    println!("PUT {}", path);

    let (sender, _receiver) = broadcast::channel::<Bytes>(1024);
    let sender_ptr = Arc::new(sender);
    if BROADCAST_ENABLED {
        let mut broadcasts_guard = state.broadcasts.write().await;
        broadcasts_guard.insert(&path, sender_ptr.clone());
        drop(broadcasts_guard);
    }

    let file = Arc::new(RwLock::new(Vec::new()));
    let mut files_guard = state.files.write().await;
    let overwritten = files_guard.insert(&path, file.clone()).is_some();
    drop(files_guard);

    println!("PUT {} streaming start", path);
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let mut file_guard = file.write().await;
                file_guard.extend_from_slice(&bytes);
                if BROADCAST_ENABLED {
                    if let Err(e) = sender_ptr.send(bytes) {
                        eprintln!("Error broadcasting new chunk: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading chunk: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    }
    println!("PUT {} streaming end", path);

    let mut broadcasts_guard = state.broadcasts.write().await;
    broadcasts_guard.remove(&path);

    if overwritten {
        return StatusCode::OK;
    } else {
        return StatusCode::CREATED;
    }
}

async fn handle_delete(
    extract::State(state): extract::State<Arc<AppState>>,
    extract::Path(path): extract::Path<String>,
) -> StatusCode {
    println!("DELETE {}", path);

    let mut files_guard = state.files.write().await;
    files_guard.remove(&path);
    return StatusCode::OK;
}

async fn handle_player() -> Html<&'static str> {
    return Html(
        r##"
        <!doctype html>
        <html>
        <head>
            <title>dash.js Rocks</title>
            <style>
                video {
                    width: 640px;
                    height: 360px;
                }
            </style>
        </head>
        <body>
        <div>
            <video id="videoPlayer" controls></video>
        </div>
        <script src="https://cdn.dashjs.org/latest/modern/umd/dash.all.min.js"></script>
        <script>
            (function () {
                // var url = "https://livesim2.dashif.org/livesim2/testpic_2s/Manifest.mpd";
                var url = "/foo/bar/main.mpd"
                var player = dashjs.MediaPlayer().create();
                player.initialize(document.querySelector("#videoPlayer"), url, true);
            })();
        </script>
        </body>
        </html>
        "##,
    );
}

async fn handle_list(state: Arc<AppState>, request: extract::Request) -> Response {
    let uri = request.uri();
    let path = uri.path().trim_start_matches('/').to_string();
    println!("LIST {}", path);
    let files = state.files.read().await;
    let keys = files
        .keys_by_prefix(&path)
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<&str>>()
        .join("\n");
    let body = Body::from(keys);
    return Response::builder()
        .status(StatusCode::OK)
        .body(Body::from(body))
        .unwrap();
}

async fn custom_method_middleware(
    extract::State(state): extract::State<Arc<AppState>>,
    request: extract::Request,
    next: Next,
) -> Response {
    let method = request.method();
    let response = match method.as_str() {
        "LIST" => handle_list(state, request).await,
        _ => next.run(request).await,
    };
    return response;
}

#[tokio::main]
async fn main() {
    let state = AppState::new();

    let app = Router::new()
        .route("/", get(handle_index))
        .route("/player", get(handle_player))
        .route(
            "/{*path}",
            get(handle_get).put(handle_put).delete(handle_delete),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            custom_method_middleware,
        ))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
