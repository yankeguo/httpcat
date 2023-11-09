use chrono::prelude::*;
use hyper::body::HttpBody;
use hyper::header::HeaderValue;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::env;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

struct State {
    body: String,
    content_type: Option<HeaderValue>,
    code: StatusCode,
    id: Mutex<i32>,
}

async fn serve_request(state: Arc<State>, req: Request<Body>) -> Result<Response<Body>, Infallible> {
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
        let full_body = hyper::body::to_bytes(req.into_body()).await;
        match full_body {
            Ok(full_body) => {
                println!("{}", String::from_utf8_lossy(&full_body));
            }
            Err(_) => {}
        }
    }
    println!(
        "======================================================================="
    );

    // response
    let mut res: Response<Body> = Response::new(Body::from(state.body.clone()));
    *res.status_mut() = state.code;
    match &state.content_type {
        None => {}
        Some(content_type) => {
            res.headers_mut()
                .insert("Content-Type", content_type.clone());
        }
    }

    Ok::<_, Infallible>(res)
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

    let make_svc = make_service_fn(move |_conn| {
        let state = state.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                let state = state.clone();

                async move {
                    return serve_request(state, req).await;
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }

    Ok(())
}
