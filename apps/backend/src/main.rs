#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    if let Err(error) = musubi_backend::verify_backend_startup().await {
        eprintln!("musubi backend startup check failed: {error}");
        std::process::exit(1);
    }

    musubi_backend::run(musubi_backend::new_state()).await;
}
