use std::env;
use std::error::Error;
use std::fmt::{self, Write as _};
use std::future::pending;
use std::io::{self, Write};
use std::net::{Ipv4Addr, SocketAddr};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use chrono::Local;
use http_body::Body;
use http_body_util::{BodyExt, Full};
use hyper::header::{CONTENT_TYPE, HeaderValue};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use tokio::net::TcpListener;

const MAX_LOGGED_BODY: u64 = 64 * 1024;

struct Config {
    port: u16,
    body: String,
    content_type: HeaderValue,
    status: StatusCode,
}

#[derive(Debug)]
enum ConfigError {
    NotUnicode(&'static str),
    InvalidPort(String),
    InvalidStatus(String),
    InvalidContentType(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUnicode(name) => write!(f, "{name} is not valid unicode"),
            Self::InvalidPort(value) => write!(f, "PORT must be a u16, got {value:?}"),
            Self::InvalidStatus(value) => {
                write!(
                    f,
                    "RESPONSE_CODE must be an HTTP status code, got {value:?}"
                )
            }
            Self::InvalidContentType(value) => {
                write!(
                    f,
                    "RESPONSE_TYPE must be a valid header value, got {value:?}"
                )
            }
        }
    }
}

impl Error for ConfigError {}

impl Config {
    fn from_env() -> Result<Self, ConfigError> {
        let port = read_env("PORT")?;
        let body = read_env("RESPONSE_BODY")?;
        let content_type = read_env("RESPONSE_TYPE")?;
        let status = read_env("RESPONSE_CODE")?;
        Self::from_values(
            port.as_deref(),
            body.as_deref(),
            content_type.as_deref(),
            status.as_deref(),
        )
    }

    fn from_values(
        port: Option<&str>,
        body: Option<&str>,
        content_type: Option<&str>,
        status: Option<&str>,
    ) -> Result<Self, ConfigError> {
        let port = match port {
            Some(value) => value
                .parse()
                .map_err(|_| ConfigError::InvalidPort(value.to_owned()))?,
            None => 80,
        };
        let status = match status {
            Some(value) => {
                let code: u16 = value
                    .parse()
                    .map_err(|_| ConfigError::InvalidStatus(value.to_owned()))?;
                StatusCode::from_u16(code)
                    .map_err(|_| ConfigError::InvalidStatus(value.to_owned()))?
            }
            None => StatusCode::OK,
        };
        let content_type = content_type.unwrap_or("text/plain; charset=utf-8");
        let content_type = HeaderValue::from_str(content_type)
            .map_err(|_| ConfigError::InvalidContentType(content_type.to_owned()))?;

        Ok(Self {
            port,
            body: body.unwrap_or("OK").to_owned(),
            content_type,
            status,
        })
    }
}

fn read_env(key: &'static str) -> Result<Option<String>, ConfigError> {
    match env::var(key) {
        Ok(value) => Ok(Some(value)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::NotUnicode(key)),
    }
}

struct App {
    body: Bytes,
    content_type: HeaderValue,
    status: StatusCode,
    next_id: AtomicU64,
}

impl App {
    fn new(config: Config) -> Self {
        Self {
            body: Bytes::from(config.body),
            content_type: config.content_type,
            status: config.status,
            next_id: AtomicU64::new(0),
        }
    }

    async fn serve<B>(&self, request: Request<B>) -> Response<Full<Bytes>>
    where
        B: Body<Data = Bytes> + Send,
        B::Error: fmt::Display,
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let log = format_request(id, request).await;
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(log.as_bytes());

        let mut response = Response::new(Full::new(self.body.clone()));
        *response.status_mut() = self.status;
        response
            .headers_mut()
            .insert(CONTENT_TYPE, self.content_type.clone());
        response
    }
}

async fn format_request<B>(id: u64, request: Request<B>) -> String
where
    B: Body<Data = Bytes> + Send,
    B::Error: fmt::Display,
{
    let mut log = String::new();
    writeln!(
        log,
        "================ {} #{id} ================",
        Local::now()
    )
    .expect("write to string");
    writeln!(
        log,
        "{:?} {} {}",
        request.version(),
        request.method(),
        request.uri()
    )
    .expect("write to string");
    for (name, value) in request.headers() {
        let value = value.to_str().unwrap_or("invalid utf-8");
        writeln!(log, "{name}: {value}").expect("write to string");
    }
    writeln!(log).expect("write to string");

    // A missing length is treated as unbounded so a chunked body is not fully buffered.
    let upper = request.body().size_hint().upper().unwrap_or(u64::MAX);
    if upper > MAX_LOGGED_BODY {
        writeln!(log, "Body: {upper} bytes").expect("write to string");
    } else if let Ok(collected) = request.into_body().collect().await {
        let bytes = collected.to_bytes();
        let text = String::from_utf8_lossy(&bytes);
        writeln!(log, "{text}").expect("write to string");
    }
    writeln!(
        log,
        "======================================================================="
    )
    .expect("write to string");
    log
}

async fn run(app: Arc<App>, listener: TcpListener) -> io::Result<()> {
    let builder = Builder::new(TokioExecutor::new());
    let mut shutdown = pin!(shutdown_signal());

    loop {
        let (stream, _) = tokio::select! {
            () = &mut shutdown => {
                eprintln!("shutting down");
                return Ok(());
            }
            accepted = listener.accept() => accepted?,
        };

        let io = TokioIo::new(stream);
        let app = Arc::clone(&app);
        let builder = builder.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                let app = Arc::clone(&app);
                async move { Ok::<_, std::convert::Infallible>(app.serve(request).await) }
            });
            if let Err(err) = builder.serve_connection(io, service).await {
                eprintln!("server error: {err}");
            }
        });
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        } else {
            pending::<()>().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    let port = config.port;
    let app = Arc::new(App::new(config));
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).await?;
    println!("listening at {port}");
    run(app, listener).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = Config::from_values(None, None, None, None).unwrap();
        assert_eq!(config.port, 80);
        assert_eq!(config.body, "OK");
        assert_eq!(config.status, StatusCode::OK);
        assert_eq!(config.content_type, "text/plain; charset=utf-8");
    }

    #[test]
    fn custom_config() {
        let config =
            Config::from_values(Some("8080"), Some("pong"), Some("text/plain"), Some("201"))
                .unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.body, "pong");
        assert_eq!(config.status, StatusCode::CREATED);
        assert_eq!(config.content_type, "text/plain");
    }

    #[test]
    fn rejects_invalid_config() {
        assert!(Config::from_values(Some("nope"), None, None, None).is_err());
        assert!(Config::from_values(None, None, None, Some("99")).is_err());
        assert!(Config::from_values(None, None, Some("bad\r\nvalue"), None).is_err());
    }

    #[tokio::test]
    async fn responds_with_configured_body() {
        let app = App::new(
            Config::from_values(Some("8080"), Some("pong"), Some("text/plain"), Some("201"))
                .unwrap(),
        );
        let request = Request::builder()
            .method("POST")
            .uri("/path?q=1")
            .header("x-debug", "yes")
            .body(Full::new(Bytes::from_static(b"hello")))
            .unwrap();

        let response = app.serve(request).await;
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(response.headers().get(CONTENT_TYPE).unwrap(), "text/plain");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.as_ref(), b"pong");
    }

    #[tokio::test]
    async fn logs_a_small_request_body() {
        let request = Request::builder()
            .method("POST")
            .uri("/path?q=1")
            .header("x-debug", "yes")
            .body(Full::new(Bytes::from_static(b"hello body")))
            .unwrap();

        let log = format_request(1, request).await;
        assert!(log.contains("#1"));
        assert!(log.contains("HTTP/1.1 POST /path?q=1"));
        assert!(log.contains("x-debug: yes"));
        assert!(log.contains("hello body"));
    }

    #[tokio::test]
    async fn omits_a_body_above_the_log_limit() {
        let payload = vec![b'a'; usize::try_from(MAX_LOGGED_BODY).unwrap() + 1];
        let request = Request::builder()
            .body(Full::new(Bytes::from(payload)))
            .unwrap();

        let log = format_request(2, request).await;
        assert!(log.contains(&format!("Body: {} bytes", MAX_LOGGED_BODY + 1)));
        assert!(!log.contains("aaa"));
    }
}
