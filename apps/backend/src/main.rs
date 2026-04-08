#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    musubi_backend::run(musubi_backend::new_state()).await;
}
