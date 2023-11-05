use crossbeam_channel::{bounded, Receiver};
use threadpool::ThreadPool;
use std::{
	future::Future, 
	task::{Poll, Context}, 
	pin::Pin,
	net::SocketAddr
};
use axum::{
	extract::State,
	routing::get,
	response::Json,
	Router,
};

type Headers = std::collections::HashMap<String, String>;

#[cfg(not(target_os = "darwin"))]
compile_error!("This only works on mac, as it requires calling into private apple APIs with objc");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	#[cfg(feature = "server")]
	return start_server().await;

	#[cfg(not(feature = "server"))]
	{
		let data = AnisetteGenerator {
			data_rx: None,
			pool: threadpool::ThreadPool::new(1)
		}.await;
		println!("{}", base64::encode(data));
		Ok(())
	}
}

#[cfg(feature = "server")]
async fn start_server() -> Result<(), Box<dyn std::error::Error>> {
	let port: u16 = std::env::args()
		.nth(1)
		.and_then(|p| p.parse().ok())
		.unwrap_or(4321);

	// 20 threads? that sound good?
	let pool = threadpool::ThreadPool::new(20);

	let app = Router::new()
		.route("/", get(anisette_req))
		.with_state(pool);

	axum::Server::bind(&SocketAddr::from(([127, 0, 0, 1], port)))
		.serve(app.into_make_service())
		.await?;

	Ok(())
}

fn generate_anisette() -> Headers {
	omnisette::aos_kit::AOSKitAnisetteProvider::new()
		.expect("This is completely useless if we can't load AOSKit")
		.get_anisette_headers(false)
}

async fn anisette_req(State(pool): State<ThreadPool>) -> Json<Headers> {
	Json(AnisetteGenerator {
		data_rx: None,
		pool
	}.await)
}

struct AnisetteGenerator {
	data_rx: Option<Receiver<<Self as Future>::Output>>,
	pool: ThreadPool
}

impl Future for AnisetteGenerator {
	type Output = Headers; 
	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		if let Some(ref rc) = self.data_rx {
			return rc.try_recv()
				.map_or(Poll::Pending, Poll::Ready)
		}

		let (tx, rx) = bounded(1);
		self.data_rx = Some(rx);

		let waker = cx.waker().clone();
		self.pool.execute(move || {
			tx.send(generate_anisette()).expect("No way to fix this future if sending fails");
			waker.wake();
		});

		Poll::Pending
	}
}
