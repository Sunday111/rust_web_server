use std::sync::Arc;

fn main() {
    let threads_count = 20;
    let http_server = web_server::run_server(threads_count, "127.0.0.1:7878".to_string()).unwrap();
    web_server::join_server(Arc::clone(&http_server)).unwrap();
}