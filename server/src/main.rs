mod server;
mod request;

use clap::Parser;
use rusqlite::Connection;

#[derive(Parser)]
struct Cli {
    path: std::path::PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let mut db_conn = match Connection::open(args.path) {
        Ok(conn) => conn,
        Err(e) => panic!("Encountered error {:?}", e),
    };

    let server = server::Server::new(None, db_conn);

    server.run().await;

    return Ok(());
}
