mod trie;

use async_stream::stream;
use axum::{
    Error, Router,
    body::{Body, Bytes},
    extract,
    http::StatusCode,
    middleware::{self, Next},
    response::{Html, Response},
    routing::get,
};
use futures::StreamExt;
use std::{cmp::min, sync::Arc, time::Duration};
use tokio::{sync::RwLock, time::sleep};
use trie::Trie;

struct File {
    data: Vec<u8>,
    done: bool,
}

struct AppState {
    files: RwLock<Trie<Arc<RwLock<File>>>>,
}

impl AppState {
    fn new() -> Arc<Self> {
        let files = RwLock::new(Trie::new());
        return Arc::new(Self { files });
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
    let files_guard = state.files.read().await;
    if let Some(file) = files_guard.get(&path) {
        let stream = stream! {
            // ffmpeg seems to be sending chunks in the 4KB to 8KB range
            let max_chunk_size = 2 << 14;
            let mut bytes_sent = 0;
            let mut miss_counter = 0;
            loop {
                let file_guard = file.read().await;
                let bytes_to_send = min(file_guard.data.len() - bytes_sent, max_chunk_size);
                let chunk = Bytes::copy_from_slice(&file_guard.data[bytes_sent..(bytes_sent + bytes_to_send)]);
                bytes_sent += bytes_to_send;
                yield Ok::<_, Error>(chunk);
                if bytes_to_send == 0 {
                    // This is to handle the scenario where we finish streaming
                    // the file out before we receive all of it.
                    // For some reason, incomming streams are not being reliably terminated.
                    if file_guard.done || miss_counter > 10 {
                        break;
                    } else {
                        miss_counter += 1;
                        sleep(Duration::from_millis(10)).await;
                    }
                } else {
                    miss_counter = 0;
                }
            }
        };
        let body = Body::from_stream(stream);
        return Ok(Response::new(body));
    } else {
        println!("GET [404] {}", path);
        return Err(StatusCode::NOT_FOUND);
    }
}

#[axum::debug_handler]
async fn handle_put(
    extract::State(state): extract::State<Arc<AppState>>,
    extract::Path(path): extract::Path<String>,
    body: Body,
) -> StatusCode {
    println!("PUT {}", path);

    let file = Arc::new(RwLock::new(File {
        done: false,
        data: Vec::new(),
    }));
    let mut files_guard = state.files.write().await;
    let overwritten = files_guard.insert(&path, file.clone()).is_some();
    drop(files_guard);

    println!("PUT {} streaming start", path);
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                let mut file_guard = file.write().await;
                file_guard.data.extend_from_slice(&bytes);
            }
            Err(e) => {
                eprintln!("Error reading chunk: {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    }
    println!("PUT {} streaming end", path);
    let mut file_guard = file.write().await;
    file_guard.done = true;

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
                let url = window.location.hash.substr(1) || "/stream/1/main.mpd";
                let player = dashjs.MediaPlayer().create();
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
