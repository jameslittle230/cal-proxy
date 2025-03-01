use axum::{
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use chrono::{NaiveDate, NaiveDateTime};
use hyper::{Body, Client, Request};
use hyper_tls::HttpsConnector;
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

// Query parameters struct using Serde for extraction
#[derive(Deserialize)]
struct Params {
    url: Option<String>,
}

// Configuration for the proxy
struct ProxyConfig {
    default_url: String,
}

// Generate the static HTML page
fn generate_html() -> String {
    include_str!("./index.html").to_string()
}

async fn handle_request(
    Query(params): Query<Params>,
    config: Arc<Mutex<ProxyConfig>>,
) -> impl IntoResponse {
    // If no URL is provided, return the static HTML page
    if params.url.is_none() {
        return Html(generate_html()).into_response();
    }

    // Get the target URL from the query parameter or config
    let target_url = if let Some(url) = params.url {
        url
    } else {
        // This shouldn't happen due to earlier check, but just in case
        let config = config.lock().await;
        config.default_url.clone()
    };

    // Validate the URL
    if !target_url.starts_with("http://") && !target_url.starts_with("https://") {
        return (
            StatusCode::BAD_REQUEST,
            "Invalid URL. Must start with http:// or https://",
        )
            .into_response();
    }

    // Set up HTTPS client
    let https = HttpsConnector::new();
    let client = Client::builder().build::<_, hyper::Body>(https);

    // Create the request to forward
    let forward_req = match Request::builder().uri(&target_url).body(Body::empty()) {
        Ok(req) => req,
        Err(_) => {
            return (StatusCode::BAD_REQUEST, "Invalid URL format").into_response();
        }
    };

    // Forward the request
    match client.request(forward_req).await {
        Ok(res) => {
            // Extract status and headers
            let status = res.status();
            let headers = res.headers().clone();

            // Check if the response is calendar data
            let is_calendar = headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map_or(false, |s| s.contains("text/calendar"));

            if is_calendar {
                // Get the response body
                match hyper::body::to_bytes(res.into_body()).await {
                    Ok(body_bytes) => {
                        let body_str = String::from_utf8_lossy(&body_bytes).to_string();

                        // Modify the calendar data
                        let modified_calendar = modify_icalendar(&body_str);

                        // Create response
                        let mut response = Response::builder().status(status);

                        // Copy headers
                        for (name, value) in headers.iter() {
                            if name == "content-length" {
                                continue;
                            }
                            response = response.header(name, value);
                        }

                        match response.body(Body::from(modified_calendar)) {
                            Ok(resp) => resp.into_response(),
                            Err(_) => (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                "Failed to create response",
                            )
                                .into_response(),
                        }
                    }
                    Err(_) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to read response body",
                    )
                        .into_response(),
                }
            } else {
                // For non-calendar responses, convert to an axum response
                let (parts, body) = res.into_parts();
                let mut response = Response::builder().status(parts.status);

                // Copy headers
                for (name, value) in parts.headers.iter() {
                    response = response.header(name, value);
                }

                match response.body(Body::from(body)) {
                    Ok(resp) => resp.into_response(),
                    Err(_) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to create response",
                    )
                        .into_response(),
                }
            }
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to proxy request").into_response(),
    }
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
                return Some(date.and_hms(0, 0, 0));
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
    let default_url =
        std::env::var("DEFAULT_URL").unwrap_or_else(|_| "https://example.com/calendar".to_string());

    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(3000);

    // Create the configuration
    let config = Arc::new(Mutex::new(ProxyConfig { default_url }));

    // Create a clone for the router
    let app_config = config.clone();

    // Build our application with the route
    let app = Router::new().route(
        "/",
        get(move |params| handle_request(params, app_config.clone())),
    );

    // Set up the server
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("iCalendar proxy server running on http://{}", addr);
    println!("Default URL: {}", config.lock().await.default_url);
    println!("Use ?url=<calendar-url> to proxy a different calendar");

    // Start the server
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
