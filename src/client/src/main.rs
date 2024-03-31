use arboard::Clipboard;
use notify::{
	event::{AccessKind, AccessMode},
	Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
};
use reqwest::{
	blocking::Client,
	header::{HeaderMap, HeaderValue},
};
use std::path::Path;

fn main() {
	tracing_subscriber::fmt().pretty().without_time().init();

	let path = std::env::args().nth(1).expect("Argument 1 needs to be a path");

	let instance_url = std::env::args()
		.nth(2)
		.expect("Argument 2 needs to be the URL of your serv instance");
	let api_key = std::env::args()
		.nth(3)
		.expect("Argument 3 needs to be the api key for your serv instance");

	tracing::info!("Watching {path}");

	if let Err(error) = watch(path, &instance_url, &api_key) {
		tracing::error!("Error: {error:?}");
	}
}
fn watch<P: AsRef<Path>>(path: P, instance_url: &str, api_key: &str) -> notify::Result<()> {
	let (tx, rx) = std::sync::mpsc::channel();

	let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
	watcher.watch(path.as_ref(), RecursiveMode::NonRecursive)?;

	let mut headers = HeaderMap::new();
	headers.insert(
		"Authorization",
		HeaderValue::from_str(api_key).expect("api key is invalid"),
	);
	let client = Client::builder().default_headers(headers).build().unwrap();
	let mut clipboard = Clipboard::new().unwrap();

	for res in rx {
		match res {
			Ok(Event {
				kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
				paths,
				..
			}) => {
				for path in paths {
					tracing::info!("File uploaded {path:?}");
					let content = std::fs::read(path)?;
					match client.post(format!("{instance_url}/upload")).body(content).send() {
						Ok(res) => {
							if res.status().is_success() {
								tracing::info!("File uploaded");
								let bytes = res.bytes().expect("oom");
								let url = String::from_utf8_lossy(&bytes);
								let _ = clipboard.set_text(url);
							} else {
								tracing::info!("Error uploading file: {}", res.status());
							}
						}
						Err(err) => tracing::error!("Error: {err:?}"),
					}
				}
			}
			Ok(_) => {}
			Err(error) => tracing::error!("Error: {error:?}"),
		}
	}

	Ok(())
}
