#!/usr/bin/env -S cargo +nightly -Zscript
//! Quick test: can tokio-tungstenite connect to the Cloudflare WS endpoint?
//!
//! ```cargo
//! [dependencies]
//! tokio = { version = "1", features = ["full"] }
//! tokio-tungstenite = { version = "0.26", features = ["rustls-tls-native-roots"] }
//! ```

#[tokio::main]
async fn main() {
    let url = "wss://dev.arlyon.dev/festivals/gala2026/ws";
    eprintln!("connecting to {url}...");

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio_tungstenite::connect_async(url),
    )
    .await;

    match result {
        Ok(Ok((mut ws, response))) => {
            eprintln!("connected! status={}", response.status());

            // Send subscribe + catchup
            use tokio_tungstenite::tungstenite::Message;
            use futures_util::{SinkExt, StreamExt};

            let (mut sink, mut stream) = ws.split();

            let sub = r#"{"type":"subscribe","topics":["festival/gala2026/state"]}"#;
            sink.send(Message::Text(sub.into())).await.unwrap();
            eprintln!("sent subscribe");

            let catchup = r#"{"type":"catchup","topic":"festival/gala2026/state","sinceSeq":0}"#;
            sink.send(Message::Text(catchup.into())).await.unwrap();
            eprintln!("sent catchup");

            // Read a few messages
            for _ in 0..5 {
                match tokio::time::timeout(std::time::Duration::from_secs(5), stream.next()).await {
                    Ok(Some(Ok(msg))) => {
                        let text = msg.to_text().unwrap_or("<binary>");
                        eprintln!("recv ({} bytes): {}",
                            text.len(),
                            if text.len() > 200 { &text[..200] } else { text }
                        );
                    }
                    Ok(Some(Err(e))) => { eprintln!("error: {e}"); break; }
                    Ok(None) => { eprintln!("stream closed"); break; }
                    Err(_) => { eprintln!("read timed out after 5s"); break; }
                }
            }
        }
        Ok(Err(e)) => eprintln!("connect error: {e}"),
        Err(_) => eprintln!("TIMED OUT after 10s — tokio-tungstenite cannot complete WS handshake"),
    }
}
