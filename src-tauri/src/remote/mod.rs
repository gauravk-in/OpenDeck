//! Remote configuration server.
//!
//! An opt-in HTTP and WebSocket server that lets another device on the network (typically a phone) configure OpenDeck.
//! It mirrors the Tauri commands used by the desktop frontend, streams frontend events, proxies property inspectors,
//! and serves the mobile web app from `remote-ui/`.
//!
//! Routes:
//! - `GET  /api/info`         public; name, version and API version, and whether the caller is authorised
//! - `POST /api/rpc`          `{ "cmd": "<command>", "args": { ... } }`, answers `{ "ok": value }` or `{ "error": "..." }`
//! - `GET  /api/events`       WebSocket of `{ "event": "<name>", "payload": ... }`
//! - `GET  /pi/<path>`        files from the plugin webserver, with property inspectors adapted to run remotely
//! - `GET  /pi-socket`        WebSocket bridged to the plugin and property inspector WebSocket server
//! - `GET  /opendeck/<file>`  built-in images referenced as `opendeck/...` by action states
//! - `GET  /*`                the mobile web app
//!
//! Every route but `/api/info` and the web app requires the pairing token, given as an `Authorization: Bearer` header,
//! an `od_remote_token` cookie or a `token` query parameter. The token and settings live in `<config dir>/remote.json`.

mod rpc;

use crate::shared::config_dir;

use std::collections::HashMap;
use std::sync::LazyLock;

use axum::Router;
use axum::body::Body;
use axum::extract::ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, Request};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use futures::{SinkExt, StreamExt};
use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};
use tauri::Listener;
use tokio::sync::{RwLock, broadcast};

static WEB_APP: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../remote-ui/dist");

/// Bumped when the remote API changes incompatibly.
pub const API_VERSION: u32 = 1;
const DEFAULT_PORT: u16 = 57200;

/// Frontend events forwarded to remote clients.
const FORWARDED_EVENTS: &[&str] = &[
	"devices",
	"switch_profile",
	"rerender_images",
	"update_state",
	"key_moved",
	"show_alert",
	"show_ok",
	"plugin_reloaded",
	"device_brightness",
	"replicated_slots",
];

#[derive(Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
	#[serde(default)]
	pub enabled: bool,
	#[serde(default = "default_bind")]
	pub bind: String,
	#[serde(default = "default_port")]
	pub port: u16,
	#[serde(default)]
	pub token: String,
}

fn default_bind() -> String {
	"0.0.0.0".to_owned()
}
fn default_port() -> u16 {
	DEFAULT_PORT
}

fn new_token() -> String {
	format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Read `remote.json`, creating it with a fresh token if needed.
pub fn load_config() -> Result<RemoteConfig, anyhow::Error> {
	let path = config_dir().join("remote.json");
	let mut config: RemoteConfig = match std::fs::read(&path) {
		Ok(contents) => serde_json::from_slice(&contents)?,
		Err(_) => RemoteConfig {
			enabled: false,
			bind: default_bind(),
			port: DEFAULT_PORT,
			token: String::new(),
		},
	};
	if config.token.len() < 16 || !path.exists() {
		if config.token.len() < 16 {
			config.token = new_token();
		}
		std::fs::create_dir_all(config_dir())?;
		std::fs::write(&path, serde_json::to_string_pretty(&config)?)?;
		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
		}
	}
	Ok(config)
}

static CONFIG: LazyLock<RwLock<Option<RemoteConfig>>> = LazyLock::new(|| RwLock::new(None));
static EVENTS: LazyLock<broadcast::Sender<String>> = LazyLock::new(|| broadcast::channel(256).0);
/// The latest image sent to each key of each device, as `device -> "Controller.position" -> data URL`.
static IMAGES: LazyLock<RwLock<HashMap<String, HashMap<String, String>>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Send an event to every connected remote client.
pub fn broadcast(event: &str, payload: serde_json::Value) {
	let _ = EVENTS.send(serde_json::json!({ "event": event, "payload": payload }).to_string());
}

/// Record an image rendered by the frontend for a device key, so remote clients can show what the device shows.
pub async fn record_image(context: &crate::shared::Context, image: Option<&str>) {
	let key = format!("{}.{}", context.controller, context.position);
	{
		let mut images = IMAGES.write().await;
		let device = images.entry(context.device.clone()).or_default();
		match image {
			Some(image) => device.insert(key.clone(), image.to_owned()),
			None => device.remove(&key),
		};
	}
	broadcast("key_image", serde_json::json!({ "device": context.device, "profile": context.profile, "slot": key, "image": image }));
}

pub async fn rendered_images(device: &str) -> HashMap<String, String> {
	IMAGES.read().await.get(device).cloned().unwrap_or_default()
}

/// Start the remote server if enabled in `remote.json` or by the `--remote` argument.
pub async fn init() {
	let mut config = match load_config() {
		Ok(config) => config,
		Err(error) => {
			log::error!("Failed to load remote configuration: {error}");
			return;
		}
	};
	if std::env::args().any(|arg| arg == "--remote") {
		config.enabled = true;
	}
	if !config.enabled {
		return;
	}
	*CONFIG.write().await = Some(config.clone());

	let app = crate::APP_HANDLE.get().unwrap();
	for name in FORWARDED_EVENTS {
		app.listen_any(*name, move |event| {
			let payload = serde_json::from_str(event.payload()).unwrap_or(serde_json::Value::Null);
			broadcast(name, payload);
		});
	}

	let router = Router::new()
		.route("/api/info", get(info))
		.route("/api/rpc", any(rpc_handler))
		.route("/api/events", get(events_handler))
		.route("/pi-socket", get(pi_socket_handler))
		.route("/pi/{*path}", get(pi_handler))
		.route("/opendeck/{file}", get(builtin_image))
		.fallback(web_app);

	let address = format!("{}:{}", config.bind, config.port);
	let listener = match tokio::net::TcpListener::bind(&address).await {
		Ok(listener) => listener,
		Err(error) => {
			log::error!("Failed to bind remote server to {address}: {error}");
			return;
		}
	};
	let host = sysinfo::System::host_name().map(|name| format!("{name}.local")).unwrap_or_else(|| config.bind.clone());
	log::info!("Remote configuration listening on {address}; pair a phone at http://{host}:{}/#token={}", config.port, config.token);
	if let Err(error) = axum::serve(listener, router).await {
		log::error!("Remote server stopped: {error}");
	}
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
	a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Find the token presented by a request, from the Authorization header, the cookie, or the query string.
fn presented_token(headers: &HeaderMap, query: &HashMap<String, String>) -> Option<String> {
	if let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok())
		&& let Some(token) = value.strip_prefix("Bearer ")
	{
		return Some(token.trim().to_owned());
	}
	if let Some(cookies) = headers.get(header::COOKIE).and_then(|v| v.to_str().ok()) {
		for cookie in cookies.split(';') {
			if let Some(token) = cookie.trim().strip_prefix("od_remote_token=") {
				return Some(token.to_owned());
			}
		}
	}
	query.get("token").cloned()
}

async fn authorised(headers: &HeaderMap, query: &HashMap<String, String>) -> bool {
	let config = CONFIG.read().await;
	let Some(config) = config.as_ref() else { return false };
	presented_token(headers, query).is_some_and(|token| constant_time_eq(token.as_bytes(), config.token.as_bytes()))
}

fn cors(mut response: Response) -> Response {
	let headers = response.headers_mut();
	headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
	headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("authorization, content-type"));
	headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, OPTIONS"));
	response
}

async fn info(headers: HeaderMap, Query(query): Query<HashMap<String, String>>) -> Response {
	let authorised = authorised(&headers, &query).await;
	cors(
		axum::Json(serde_json::json!({
			"name": crate::shared::PRODUCT_NAME,
			"version": crate::built_info::PKG_VERSION,
			"api": API_VERSION,
			"authorised": authorised,
		}))
		.into_response(),
	)
}

#[derive(Deserialize)]
struct RpcRequest {
	cmd: String,
	#[serde(default)]
	args: serde_json::Value,
}

async fn rpc_handler(method: Method, headers: HeaderMap, Query(query): Query<HashMap<String, String>>, body: axum::body::Bytes) -> Response {
	if method == Method::OPTIONS {
		return cors(StatusCode::NO_CONTENT.into_response());
	}
	if method != Method::POST {
		return cors(StatusCode::METHOD_NOT_ALLOWED.into_response());
	}
	if !authorised(&headers, &query).await {
		return cors((StatusCode::UNAUTHORIZED, axum::Json(serde_json::json!({ "error": "unauthorised" }))).into_response());
	}
	let request: RpcRequest = match serde_json::from_slice(&body) {
		Ok(request) => request,
		Err(error) => return cors((StatusCode::BAD_REQUEST, axum::Json(serde_json::json!({ "error": error.to_string() }))).into_response()),
	};
	let app = crate::APP_HANDLE.get().unwrap().clone();
	let args = if request.args.is_null() { serde_json::json!({}) } else { request.args };
	cors(match rpc::dispatch(&app, &request.cmd, args).await {
		Ok(value) => axum::Json(serde_json::json!({ "ok": value })).into_response(),
		Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, axum::Json(serde_json::json!({ "error": error }))).into_response(),
	})
}

async fn events_handler(ws: WebSocketUpgrade, headers: HeaderMap, Query(query): Query<HashMap<String, String>>) -> Response {
	if !authorised(&headers, &query).await {
		return StatusCode::UNAUTHORIZED.into_response();
	}
	ws.on_upgrade(|socket: WebSocket| async move {
		let (mut sender, mut receiver) = socket.split();
		let mut events = EVENTS.subscribe();
		let forward = async {
			loop {
				match events.recv().await {
					Ok(text) => {
						if sender.send(AxumMessage::Text(text.into())).await.is_err() {
							break;
						}
					}
					// The client fell behind; tell it to reload its state.
					Err(broadcast::error::RecvError::Lagged(_)) => {
						let text = serde_json::json!({ "event": "resync", "payload": null }).to_string();
						if sender.send(AxumMessage::Text(text.into())).await.is_err() {
							break;
						}
					}
					Err(broadcast::error::RecvError::Closed) => break,
				}
			}
		};
		// Drain incoming messages (pings, close) so the connection is kept alive and closes cleanly.
		let drain = async {
			while let Some(Ok(message)) = receiver.next().await {
				if matches!(message, AxumMessage::Close(_)) {
					break;
				}
			}
		};
		tokio::select! {
			_ = forward => {},
			_ = drain => {},
		}
	})
}

/// Bridge a property inspector's WebSocket to OpenDeck's plugin WebSocket server.
async fn pi_socket_handler(ws: WebSocketUpgrade, headers: HeaderMap, Query(query): Query<HashMap<String, String>>) -> Response {
	if !authorised(&headers, &query).await {
		return StatusCode::UNAUTHORIZED.into_response();
	}
	ws.on_upgrade(|socket: WebSocket| async move {
		use tokio_tungstenite::tungstenite::Message as TMessage;
		let url = format!("ws://127.0.0.1:{}", *crate::plugins::PORT_BASE);
		let Ok((upstream, _)) = tokio_tungstenite::connect_async(url).await else { return };
		let (mut up_tx, mut up_rx) = upstream.split();
		let (mut down_tx, mut down_rx) = socket.split();
		let to_upstream = async {
			while let Some(Ok(message)) = down_rx.next().await {
				let message = match message {
					AxumMessage::Text(text) => TMessage::Text(text.as_str().into()),
					AxumMessage::Binary(data) => TMessage::Binary(data),
					AxumMessage::Close(_) => break,
					_ => continue,
				};
				if up_tx.send(message).await.is_err() {
					break;
				}
			}
			let _ = up_tx.close().await;
		};
		let to_downstream = async {
			while let Some(Ok(message)) = up_rx.next().await {
				let message = match message {
					TMessage::Text(text) => AxumMessage::Text(text.as_str().into()),
					TMessage::Binary(data) => AxumMessage::Binary(data),
					TMessage::Close(_) => break,
					_ => continue,
				};
				if down_tx.send(message).await.is_err() {
					break;
				}
			}
			let _ = down_tx.close().await;
		};
		tokio::select! {
			_ = to_upstream => {},
			_ = to_downstream => {},
		}
	})
}

/// Injected at the top of property inspectors so their WebSocket, which targets 127.0.0.1, reaches this server instead.
const PROPERTY_INSPECTOR_SHIM: &str = r#"<script>
(() => {
	const NativeWebSocket = window.WebSocket;
	const bridge = (location.protocol === "https:" ? "wss://" : "ws://") + location.host + "/pi-socket";
	function RemoteWebSocket(url, protocols) {
		try {
			const parsed = new URL(url, location.href);
			if (parsed.hostname === "127.0.0.1" || parsed.hostname === "localhost") url = bridge;
		} catch {}
		return protocols === undefined ? new NativeWebSocket(url) : new NativeWebSocket(url, protocols);
	}
	RemoteWebSocket.prototype = NativeWebSocket.prototype;
	for (const key of ["CONNECTING", "OPEN", "CLOSING", "CLOSED"]) RemoteWebSocket[key] = NativeWebSocket[key];
	window.WebSocket = RemoteWebSocket;
	document.documentElement.classList.add("opendeck-remote");
})();
</script>
"#;

/// Proxy a file from the plugin webserver. Property inspector pages get the WebSocket shim.
async fn pi_handler(Path(path): Path<String>, headers: HeaderMap, Query(query): Query<HashMap<String, String>>, request: Request) -> Response {
	if !authorised(&headers, &query).await {
		return StatusCode::UNAUTHORIZED.into_response();
	}
	// Keep the raw (still percent-encoded) path so the plugin webserver decodes it exactly as it would for the desktop.
	let raw = request.uri().path().strip_prefix("/pi/").unwrap_or(&path).to_owned();
	let url = format!("http://127.0.0.1:{}/{}", *crate::plugins::PORT_BASE + 2, raw);
	let response = match reqwest::get(&url).await {
		Ok(response) => response,
		Err(error) => return (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
	};
	let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
	let content_type = response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_owned);
	let Ok(bytes) = response.bytes().await else {
		return StatusCode::BAD_GATEWAY.into_response();
	};

	let decoded = urlencoding::decode(&raw).map(|s| s.into_owned()).unwrap_or_default();
	let is_inspector = decoded.ends_with("|opendeck_property_inspector") || decoded.ends_with("|opendeck_property_inspector_child");
	let body = if is_inspector && status.is_success() {
		let mut html = PROPERTY_INSPECTOR_SHIM.as_bytes().to_vec();
		html.extend_from_slice(&bytes);
		Body::from(html)
	} else {
		Body::from(bytes)
	};

	let mut builder = Response::builder().status(status).header(header::CACHE_CONTROL, "no-cache");
	if let Some(content_type) = content_type {
		builder = builder.header(header::CONTENT_TYPE, content_type);
	}
	builder.body(body).unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn builtin_image(Path(file): Path<String>) -> Response {
	let bytes: &'static [u8] = match file.as_str() {
		"alert.png" => include_bytes!("../../../static/alert.png"),
		"cube.png" => include_bytes!("../../../static/cube.png"),
		"multi-action.png" => include_bytes!("../../../static/multi-action.png"),
		"ok.png" => include_bytes!("../../../static/ok.png"),
		"toggle-action.png" => include_bytes!("../../../static/toggle-action.png"),
		_ => return StatusCode::NOT_FOUND.into_response(),
	};
	([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "max-age=86400")], bytes).into_response()
}

fn mime(path: &str) -> &'static str {
	match path.rsplit('.').next().unwrap_or("") {
		"html" => "text/html; charset=utf-8",
		"js" | "mjs" => "text/javascript",
		"css" => "text/css",
		"json" => "application/json",
		"webmanifest" => "application/manifest+json",
		"svg" => "image/svg+xml",
		"png" => "image/png",
		"ico" => "image/x-icon",
		"woff2" => "font/woff2",
		_ => "application/octet-stream",
	}
}

/// Serve the mobile web app, falling back to `index.html` for client-side routes.
async fn web_app(request: Request) -> Response {
	let path = request.uri().path().trim_start_matches('/');
	let path = if path.is_empty() { "index.html" } else { path };
	let (path, file) = match WEB_APP.get_file(path) {
		Some(file) => (path, file),
		None => match WEB_APP.get_file("index.html") {
			Some(file) => ("index.html", file),
			None => return (StatusCode::NOT_FOUND, "The remote web app was not built into this copy of OpenDeck.").into_response(),
		},
	};
	let cache = if path == "index.html" || path == "sw.js" { "no-cache" } else { "max-age=31536000, immutable" };
	([(header::CONTENT_TYPE, mime(path)), (header::CACHE_CONTROL, cache)], file.contents()).into_response()
}
