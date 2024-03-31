use std::path::Path;

use axum::{
	async_trait,
	body::Bytes,
	extract::FromRequestParts,
	extract::Path as AxumPath,
	http::{
		header::{AUTHORIZATION, CONTENT_TYPE},
		request::Parts,
		HeaderMap, HeaderValue, StatusCode,
	},
	routing::{get, post},
	Router,
};
use miette::{IntoDiagnostic, Result};
use rand::{distributions::Alphanumeric, Rng};
use tokio::{net::TcpListener, signal};

lazy_static::lazy_static! {
	static ref BIND_ADDR: String = std::env::var("BIND_ADDR").unwrap_or("127.0.0.1:8080".into());
	static ref API_KEY: String = std::env::var("API_KEY").unwrap_or("wawawa".into());
	static ref DATA_DIR: String = std::env::var("DATA_DIR").unwrap_or("./data/".into());
	static ref PUBLIC_ENDPOINT: String = std::env::var("PUBLIC_ENDPOINT").unwrap_or("http://localhost:8080".into());
}

struct APIKey(String);

#[async_trait]
impl<S> FromRequestParts<S> for APIKey
where
	S: Send + Sync,
{
	type Rejection = (StatusCode, String);

	async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
		if let Some(header) = parts.headers.get(AUTHORIZATION) {
			Ok(APIKey(
				header
					.to_str()
					.map_err(|v| (StatusCode::BAD_REQUEST, format!("{}", v)))?
					.to_string(),
			))
		} else {
			Err((StatusCode::BAD_REQUEST, "`Authorization` header is missing".to_string()))
		}
	}
}

async fn index() -> &'static str {
	"trans rights"
}

#[derive(Debug)]
enum FileType {
	Jpg,
	Png,
}

impl FileType {
	pub fn ext(&self) -> &'static str {
		match self {
			FileType::Jpg => "jpeg",
			FileType::Png => "png",
		}
	}
}

fn validate_file(bytes: &Bytes) -> Option<FileType> {
	const JPG_HEADER: &[u8] = &[0xFF, 0xD8, 0xFF];
	const JPG_FOOTER: &[u8] = &[0xFF, 0xD9];
	const PNG_HEADER: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0xD, 0xA, 0x1A, 0xA];

	if bytes[0..JPG_HEADER.len()].eq(JPG_HEADER) && bytes[bytes.len() - JPG_FOOTER.len()..].eq(JPG_FOOTER) {
		return Some(FileType::Jpg);
	}

	if bytes[0..PNG_HEADER.len()].eq(PNG_HEADER) {
		return Some(FileType::Png);
	}

	tracing::debug!(first_bytes = ?bytes[0..5], last_bytes = ?bytes[bytes.len() - 5..]);
	None
}

async fn upload(APIKey(key): APIKey, body: Bytes) -> miette::Result<(StatusCode, String), StatusCode> {
	if !API_KEY.eq(&key) {
		return Ok((StatusCode::UNAUTHORIZED, "Invalid API key".to_string()));
	}

	let Some(file_type) = validate_file(&body) else {
		return Ok((StatusCode::PRECONDITION_FAILED, "Invalid image.".to_string()));
	};

	let file_name: String = rand::thread_rng()
		.sample_iter(&Alphanumeric)
		.take(7)
		.map(char::from)
		.collect();
	let file_name = format!("{file_name}.{}", file_type.ext());

	tokio::fs::write(Path::new(&*DATA_DIR).join(&file_name), body)
		.await
		.map_err(|err| {
			tracing::error!(?err);
			StatusCode::INTERNAL_SERVER_ERROR
		})?;

	Ok((StatusCode::OK, format!("{}/{file_name}", *PUBLIC_ENDPOINT)))
}

async fn get_file(AxumPath(value): AxumPath<String>) -> miette::Result<(HeaderMap, Vec<u8>), StatusCode> {
	let file = Path::new(&*DATA_DIR).join(value);

	match tokio::fs::try_exists(&file).await {
		Ok(true) => {}
		Ok(false) => return Err(StatusCode::NOT_FOUND),
		Err(err) => {
			tracing::error!(?err);
			return Err(StatusCode::INTERNAL_SERVER_ERROR);
		}
	}

	let bytes = tokio::fs::read(&file).await.map_err(|err| {
		tracing::error!(?err);
		StatusCode::INTERNAL_SERVER_ERROR
	})?;

	let mut headers = HeaderMap::new();
	headers.append(
		CONTENT_TYPE,
		HeaderValue::from_str(&format!(
			"image/{}",
			file.extension().expect("unreachable").to_string_lossy()
		))
		.map_err(|err| {
			tracing::error!(?err);
			StatusCode::INTERNAL_SERVER_ERROR
		})?,
	);

	Ok((headers, bytes))
}

async fn shutdown_signal() {
	let ctrl_c = async {
		signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
	};

	#[cfg(unix)]
	let terminate = async {
		signal::unix::signal(signal::unix::SignalKind::terminate())
			.expect("failed to install signal handler")
			.recv()
			.await;
	};

	#[cfg(not(unix))]
	let terminate = std::future::pending::<()>();

	tokio::select! {
		_ = ctrl_c => {},
		_ = terminate => {},
	}
}

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt().pretty().without_time().init();

	let app = Router::new()
		.route("/", get(index))
		.route("/upload", post(upload))
		.route("/:path", get(get_file));

	tracing::info!("listening @ http://{}", *BIND_ADDR);
	axum::serve(
		TcpListener::bind(&*BIND_ADDR).await.into_diagnostic()?,
		app.into_make_service(),
	)
	.with_graceful_shutdown(shutdown_signal())
	.await
	.into_diagnostic()?;
	Ok(())
}
