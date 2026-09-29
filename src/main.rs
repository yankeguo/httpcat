use bytes::Bytes;
use chrono::prelude::*;
use http_body::Body;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::HeaderValue;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use std::convert::Infallible;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

struct State {
    body: String,
    content_type: Option<HeaderValue>,
    code: StatusCode,
    id: Mutex<i32>,
}

async fn serve_request(
    state: Arc<State>,
    req: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    // request id
    let mut request_id = state.id.lock().await;
    *request_id += 1;
    let request_id = request_id.to_string();

    // date
    let now = Local::now();

    // request logging
    println!("================ {} #{} ================", now, request_id);
    println!("{:?} {} {}", req.version(), req.method(), req.uri());
    req.headers().iter().for_each(|(k, v)| {
        println!("{}: {}", k, v.to_str().unwrap_or("invalid utf-8"));
    });
    println!();
    let upper = req.body().size_hint().upper().unwrap_or(u64::MAX);
    if upper > 1024 * 64 {
        println!("Body: {} bytes", upper);
    } else {
        if let Ok(full_body) = req.into_body().collect().await {
            println!("{}", String::from_utf8_lossy(&full_body.to_bytes()));
        }
    }
    println!("=======================================================================");

    // response
    let mut res = Response::new(Full::new(Bytes::from(state.body.clone())));
    *res.status_mut() = state.code;
    match &state.content_type {
        None => {}
        Some(content_type) => {
            res.headers_mut()
                .insert("Content-Type", content_type.clone());
        }
    }

    Ok(res)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let port: u16 = env::var("PORT").unwrap_or("80".to_string()).parse()?;

    let state = Arc::new(State {
        body: env::var("RESPONSE_BODY").unwrap_or("OK".to_string()),
        content_type: HeaderValue::from_str(
            env::var("RESPONSE_TYPE")
                .unwrap_or("text/plain; charset=utf-8".to_string())
                .as_str(),
        )
        .ok(),
        code: StatusCode::from_u16(
            env::var("RESPONSE_CODE")
                .unwrap_or("200".to_string())
                .parse::<u16>()?,
        )?,
        id: Mutex::new(0),
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("listening at {}", port);

    let listener = TcpListener::bind(addr).await?;
    let builder = Builder::new(TokioExecutor::new());

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let state = state.clone();
        let builder = builder.clone();

        tokio::spawn(async move {
            let service = service_fn(move |req| {
                let state = state.clone();
                serve_request(state, req)
            });

            if let Err(e) = builder.serve_connection(io, service).await {
                eprintln!("server error: {}", e);
            }
        });
    }
}
