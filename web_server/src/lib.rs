use std::{
    cell::RefCell,
    fmt::Display,
    io::{BufRead, BufReader, Write as IO_Write},
    net::{TcpListener, TcpStream},
    sync::{atomic::AtomicBool, Mutex},
    thread::JoinHandle,
};

mod thread_pool;
use std::sync::Arc;
use thread_pool::ThreadPool;

#[derive(Debug)]
pub struct WebServerError(String);

type Result<T> = std::result::Result<T, WebServerError>;

pub trait ConvertibleToResult<T> {
    fn to_web_server_result(self) -> Result<T>;
}

enum HttpRequest {
    GET(HttpGetRequest),
}

struct HttpGetRequest {
    path: String,
}

pub struct HttpServer {
    started: AtomicBool,
    stopped: AtomicBool,
    thread: Option<JoinHandle<Result<()>>>,
}

pub fn join_server(server: Arc<Mutex<HttpServer>>) -> Result<()> {
    // Wait until server start
    loop {
        let server = server.lock().to_web_server_result()?;
        if server.stopped.load(std::sync::atomic::Ordering::Relaxed)
        {
            break;
        }
    }

    {
        let mut server = server.lock().to_web_server_result()?;
        if let Some(handle) = server.thread.take() {
            handle.join().unwrap()?;
        }
    }

    Ok(())
}

pub fn run_server(threads_count: usize, address: String) -> Result<Arc<Mutex<HttpServer>>> {
    let server = Arc::new(Mutex::new(HttpServer {
        started: false.into(),
        stopped: false.into(),
        thread: None,
    }));

    let src = Arc::clone(&server);

    let thread = Some(std::thread::spawn(move || {
        // Wait until server start
        loop {
            let server = src.lock().to_web_server_result()?;
            if server.started.load(std::sync::atomic::Ordering::Relaxed)
                || server.stopped.load(std::sync::atomic::Ordering::Relaxed)
            {
                break;
            }
        }

        let thread_pool = ThreadPool::new(threads_count)?;
        let tcp_listener = TcpListener::bind(address).to_web_server_result()?;
        for stream in tcp_listener.incoming() {
            {
                let server = src.lock().to_web_server_result()?;
                if server.stopped.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
            }

            let stream = stream.to_web_server_result()?;
            thread_pool.execute(move || {
                let r = handle_connection(stream);
                if let Err(error) = r {
                    println!("Request failed with an error: {}", error.0);
                }
            });
        }

        {
            let server = src.lock().to_web_server_result()?;
            server
                .stopped
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }

        Ok(())
    }));

    {
        let mut server = server.lock().unwrap();
        server.thread = thread;
        server
            .started
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    Ok(server)
}

fn read_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let lines = {
        let reader = BufReader::new(stream);
        let mut lines = Vec::new();
        for result in reader.lines() {
            let line = result.to_web_server_result()?;
            if line.is_empty() {
                break;
            }
            lines.push(line);
        }

        lines
    };

    let mut tokens_iter = lines[0].split(' ');

    let method = tokens_iter
        .next()
        .ok_or("Invalid request format")
        .to_web_server_result()?;

    if method == "GET" {
        // read path for get request
        let path = tokens_iter
            .next()
            .ok_or("Invalid request format")
            .to_web_server_result()?;

        let http_ver = tokens_iter
            .next()
            .ok_or("Invalid request format")
            .to_web_server_result()?;
        if http_ver != "HTTP/1.1" {
            return Err(WebServerError(format!(
                "Expected HTTP/1.1, got {}",
                http_ver
            )));
        }

        Ok(HttpRequest::GET(HttpGetRequest {
            path: path.to_string(),
        }))
    } else {
        Err(WebServerError(format!(
            "Unsupported (or invalid) method {}",
            method
        )))
    }
}

fn get_absolute_path(path_from_request: &str) -> Result<String> {
    let cwd = std::env::current_dir().to_web_server_result()?;
    let content_dir = cwd.join("content");
    let rel_path = path_from_request
        .strip_prefix("/")
        .ok_or(WebServerError("Failed to strip prefix".to_string()))?;
    let abs_path = content_dir.join(rel_path);
    let abs_str = abs_path.to_str().ok_or(WebServerError(
        "Failed to convert path to string".to_string(),
    ))?;
    Ok(abs_str.to_string())
}

fn html_error_code_to_str(value: i32) -> Result<&'static str> {
    match value {
        200 => Ok("OK"),
        404 => Ok("NOT FOUND"),
        500 => Ok("INTERNAL SERVER ERROR"),
        _ => Err(WebServerError(format!("Unknown response conde {}", value))),
    }
}

fn handle_get_request(request: HttpGetRequest) -> Result<Vec<u8>> {
    let path = get_absolute_path(&request.path)?;

    let mut error_code = 200;
    let mut maybe_content: Option<Vec<u8>> = None;

    if !std::path::Path::new(&path).exists() {
        error_code = 404;
    } else {
        println!("Reading path: {}", path);
        maybe_content = Some(std::fs::read(path).to_web_server_result()?);
    }

    let mut response_head = Vec::new();
    write!(
        &mut response_head,
        "HTTP/1.1 {} {}\r\n",
        error_code,
        html_error_code_to_str(error_code)?
    )
    .to_web_server_result()?;

    if let Some(content) = &maybe_content {
        write!(&mut response_head, "Content-Length: {}\r\n", content.len())
            .to_web_server_result()?;
    }
    write!(&mut response_head, "\r\n").to_web_server_result()?;

    if let Some(content) = &maybe_content {
        response_head.extend(content);
    }

    // std::thread::sleep(std::time::Duration::from_secs(5));

    Ok(response_head)
}

fn handle_connection(mut stream: TcpStream) -> Result<()> {
    let response = match read_request(&mut stream)? {
        HttpRequest::GET(get_request) => match handle_get_request(get_request) {
            Ok(response) => response,
            Err(error) => {
                println!("Internal server error: {}", error.0);

                let mut body = Vec::new();
                write!(&mut body, "HTTP/1.1 500 Internal server error\r\n")
                    .to_web_server_result()?;
                body
            }
        },
    };

    stream.write_all(&response).to_web_server_result()
}

impl<T, SomeError> ConvertibleToResult<T> for std::result::Result<T, SomeError>
where
    SomeError: Display,
{
    fn to_web_server_result(self) -> Result<T> {
        match self {
            Ok(value) => Ok(value),
            Err(an_error) => Err(WebServerError(an_error.to_string())),
        }
    }
}
