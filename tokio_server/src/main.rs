mod connections;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut shutdown = false;
    // create connections
    let mut connection_set = connections::ConnectionSet::new();

    loop {
        // poll for events

        // if got connection see what it is

        // perform search/lock and update

        // release lock

        if shutdown {
            break;
        }
    }
    Ok(())
}

