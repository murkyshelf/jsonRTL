use std::net::{Ipv4Addr, SocketAddr};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, 3000));
    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, jsonrtl_api::RouterBuilder::new().finish()).await
}
