use chrono::{NaiveDate, NaiveDateTime};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Client, Request, Response, Server};
use hyper_tls::HttpsConnector;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

// Configuration for the proxy
struct ProxyConfig {
    target_url: String,
}

async fn proxy_handler(
    req: Request<Body>,
    config: Arc<Mutex<ProxyConfig>>,
) -> Result<Response<Body>, hyper::Error> {
    // Get the target URL from the configuration
    let config = config.lock().await;
    let target_url = &config.target_url;

    // Create a new request to forward to the target
    let uri = format!(
        "{}{}",
        target_url,
        req.uri().path_and_query().map_or("", |p| p.as_str())
    );

    // Set up HTTPS client
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    // Create the forwarded request
    let (parts, body) = req.into_parts();
    let mut builder = Request::builder().method(parts.method).uri(uri);

    // Copy headers from original request
    for (name, value) in parts.headers {
        if let Some(name) = name {
            if name == "host" {
                continue;
            }
            builder = builder.header(name, value);
        }
    }

    // Forward the request
    let forward_req = builder.body(body).unwrap();
    println!("{:?}", &forward_req);
    let res = client.request(forward_req).await?;

    // Extract status and headers
    let status = res.status();
    let headers = res.headers().clone();

    // Check if the response is calendar data
    if headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map_or(false, |s| s.contains("text/calendar"))
    {
        // Get the response body
        let body_bytes = hyper::body::to_bytes(res.into_body()).await?;
        let body_str = String::from_utf8_lossy(&body_bytes).to_string();
        println!("{}", body_str);

        // Modify the calendar data
        let modified_calendar = modify_icalendar(&body_str);

        // Return the modified calendar
        let mut response_builder = Response::builder().status(status);

        // Copy headers
        for (name, value) in headers.iter() {
            if name == "content-length" {
                continue;
            }
            response_builder = response_builder.header(name, value);
        }

        return Ok(response_builder
            .body(Body::from(modified_calendar))
            .unwrap());
    }

    // For non-calendar responses, just return the original response
    Ok(res)
}

fn modify_icalendar(ical_str: &str) -> String {
    // Use line-by-line processing which works better for iCalendar format
    let mut result = String::new();
    let mut in_event = false;
    let mut event_lines = Vec::new();
    let mut dtstart_line_idx = None;
    let mut dtend_line_idx = None;
    let mut dtstart_value = String::new();
    let mut dtend_value = String::new();

    for line in ical_str.lines() {
        if line.starts_with("BEGIN:VEVENT") {
            in_event = true;
            event_lines.clear();
            event_lines.push(line.to_string());
            dtstart_line_idx = None;
            dtend_line_idx = None;
        } else if line.starts_with("END:VEVENT") {
            in_event = false;
            event_lines.push(line.to_string());

            // Check if this is a multi-day event that needs conversion
            if let (Some(start_idx), Some(end_idx)) = (dtstart_line_idx, dtend_line_idx) {
                let start_dt = parse_ical_datetime(&dtstart_value);
                let end_dt = parse_ical_datetime(&dtend_value);

                if let (Some(start), Some(end)) = (start_dt, end_dt) {
                    if start.date() != end.date() {
                        // Convert to all-day event
                        let start_date_str = start.date().format("%Y%m%d").to_string();
                        let end_date_str = end.date().format("%Y%m%d").to_string();

                        event_lines[start_idx] = format!("DTSTART;VALUE=DATE:{}", start_date_str);
                        event_lines[end_idx] = format!("DTEND;VALUE=DATE:{}", end_date_str);
                    }
                }
            }

            // Add the event to the result
            for line in &event_lines {
                result.push_str(line);
                result.push_str("\r\n");
            }
        } else if in_event {
            if line.starts_with("DTSTART") {
                dtstart_line_idx = Some(event_lines.len());
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    dtstart_value = parts[1].to_string();
                }
            } else if line.starts_with("DTEND") {
                dtend_line_idx = Some(event_lines.len());
                let parts: Vec<&str> = line.split(':').collect();
                if parts.len() >= 2 {
                    dtend_value = parts[1].to_string();
                }
            }

            event_lines.push(line.to_string());
        } else {
            // Outside an event, just add the line
            result.push_str(line);
            result.push_str("\r\n");
        }
    }

    result
}

fn parse_ical_datetime(dt_str: &str) -> Option<NaiveDateTime> {
    if dt_str.len() >= 8 {
        // Date-only format: YYYYMMDD
        if dt_str.len() == 8 {
            if let Ok(date) = NaiveDate::parse_from_str(dt_str, "%Y%m%d") {
                return Some(date.and_hms_opt(0, 0, 0).expect(""));
            }
        }
        // Date-time format: YYYYMMDDTHHMMSSz?
        else if dt_str.contains('T') && dt_str.len() >= 15 {
            let format = if dt_str.ends_with('Z') {
                "%Y%m%dT%H%M%SZ"
            } else {
                "%Y%m%dT%H%M%S"
            };

            if let Ok(datetime) = NaiveDateTime::parse_from_str(dt_str, format) {
                return Some(datetime);
            }
        }
    }

    None
}

#[tokio::main]
async fn main() {
    // Get configuration from environment or use defaults
    let target_url = std::env::var("TARGET_URL")
        .unwrap_or_else(|_| "https://example.com/calendar.ics".to_string());

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(8060);

    // Create the configuration
    let config = Arc::new(Mutex::new(ProxyConfig { target_url }));

    // Set up the server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let config_clone = config.clone();
    let make_svc = make_service_fn(move |_conn| {
        let config = config_clone.clone();
        async move { Ok::<_, Infallible>(service_fn(move |req| proxy_handler(req, config.clone()))) }
    });

    let server = Server::bind(&addr).serve(make_svc);

    println!("iCalendar proxy server running on http://{}", addr);
    println!("Forwarding requests to {}", config.lock().await.target_url);
    println!("Converting multi-day events to all-day events");

    if let Err(e) = server.await {
        eprintln!("server error: {}", e);
    }
}

