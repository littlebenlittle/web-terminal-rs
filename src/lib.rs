
use tokio::sync::{oneshot, mpsc};
use tokio::net::TcpListener;
use tokio;
use tokio::time::{sleep, Duration};

use async_tungstenite::tokio::accept_async;
use async_tungstenite::tungstenite::Message;
use thiserror::Error;
use futures::TryStreamExt;

pub async fn run_server(
    stop_rx: oneshot::Receiver<()>,
    msg_tx:  mpsc::Sender<String>
)
-> Result<(), Error>
{
    let timeout = sleep(Duration::from_millis(750));
    tokio::pin!(timeout);
    tokio::select! {
        _ = &mut timeout => { }
        result = stop_rx => {
            result?;
            println!("server received stop signal");
            return Ok(())
        }
    }
    msg_tx.send("done".to_string()).await?;
    println!("server sent message \"done\"");
    Ok(())
    
    // let listen_addr = "127.0.0.1:9001";
    // println!("listening on {}", listen_addr);
    // let server = TcpListener::bind(listen_addr).await?;
    // let (stream, addr) = server.accept().await?;
    // println!("new connection from {}", addr);
    // let mut websocket = accept_async(stream).await?;
    // loop {
    //     let msg = websocket.try_next().await?.unwrap();
    //     match msg {
    //         Message::Text(s) => msg_ch.send(s).await?,
    //         Message::Close(f) => {
    //             let mut m = "".to_string();
    //             match f {
    //                 Some(v) => { m = format!(": {} {}", v.code, v.reason) }
    //                 None => {}
    //             }
    //             println!("connection from {} closed{}", addr, m);
    //             break
    //         }
    //         _ => { println!("non-text message") }
    //     }
    // }
    // Ok(())
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("std error")]
    Std(#[from] std::io::Error),
    #[error("tungstenite error")]
    Tungstenite(#[from] async_tungstenite::tungstenite::Error),
    #[error("tokio mpsc send string error")]
    TokioSend(#[from] mpsc::error::SendError<std::string::String>),
    #[error("tokio oneshot recv error")]
    TokioRecv(#[from] tokio::sync::oneshot::error::RecvError),
}
