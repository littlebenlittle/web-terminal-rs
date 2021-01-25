use std::net::SocketAddr;
use tokio;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration};
use tokio::task::JoinHandle;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::tokio as tokio_tungstenite;
use thiserror::Error;
use futures_util::{SinkExt, StreamExt};
use url::Url;

#[test]
fn e2e_test_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    let timed_out = rt.block_on(timeout_harness(
        Duration::from_millis(100),
        Duration::from_millis(50),
    ))?;
    assert!(timed_out, "test should time out");
    Ok(())
}

#[test]
fn e2e_test_no_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    let timed_out = rt.block_on(timeout_harness(
        Duration::from_millis(50),
        Duration::from_millis(100),
    ))?;
    assert!(timed_out == false, "test should not time out");
    Ok(())
}

#[test]
fn test_websocket_connection() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    let port = random_port();
    let addr = format!("127.0.0.1:{}", port);
    let (client_connected, server_connected) = rt.block_on(websocket_connect_harness(
        Duration::from_millis(100),
        format!("ws://{}", addr).parse().unwrap(),
        addr.parse()?,
    ))?;
    assert!(client_connected, "connection should have been established");
    assert!(server_connected, "connection should have been established");
    Ok(())
}

#[test]
fn test_failed_websocket_connection() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    let mut port;
    loop {
        port = random_port();
        if port != 12345 {
            break
        }
    }
    let addr = format!("127.0.0.1:{}", port).parse()?;
    let (client_connected, server_connected) = rt.block_on(websocket_connect_harness(
        Duration::from_millis(250),
        "ws://127.0.0.1:12345".parse().unwrap(),
        addr,
    ))?;
    assert!(!client_connected, "connection should not have been established");
    assert!(!server_connected, "connection should not have been established");
    Ok(())
}

/// Ok(true) if test times out, Ok(false) if not
async fn timeout_harness(
    pause_duration:   Duration,  // time before task sends message
    timeout_duration: Duration,  // time before test times out
) -> Result<bool, String> {
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let (msg_tx, mut msg_rx) = mpsc::channel(1);

    let task = tokio::spawn(timeout_test_task(
        stop_rx,
        msg_tx,
        pause_duration,
    ));

    let mut task_done = false;
    let mut timed_out = false;
    let sleep = tokio::time::sleep(timeout_duration);
    tokio::pin!(sleep);
    tokio::pin!(task);
    while !task_done {
        tokio::select! {
            _ = &mut sleep => {
                println!("test timed out");
                timed_out = true;
                break
            }
            Some(msg) = msg_rx.recv() => {
                println!("task: {}", msg);
                task_done = true;
            }
        }
    }

    ensure_task_stopped("task", task, stop_tx).await?;
    
    Ok(timed_out)
}

async fn ensure_task_stopped<T,E>(
    task_name: &str,
    task_jh: std::pin::Pin<&mut JoinHandle<Result<T,E>>>,
    stop_tx: oneshot::Sender<()>,
) -> Result<(), String>
where
    E: std::fmt::Display + std::error::Error
{
    println!("stopping task {}", task_name);
    if !stop_tx.is_closed() {
        if let Err(_) = stop_tx.send(()) {
            return Err(format!("failed to send {} stop signal", task_name))
        }
    }
    match task_jh.await {
        Ok(v) => match v {
            Ok(_)  => println!("{} exited OK", task_name),
            Err(e) => println!("{} exited Err: {}", task_name, e)
        }
        Err(e) => return Err(format!("failed to join with {} task: {}", task_name, e))
    }
    Ok(())
}

async fn timeout_test_task(
    stop_rx: oneshot::Receiver<()>,
    msg_tx:  mpsc::Sender<String>,
    pause_duration: Duration,
) -> Result<(), Error>
{
    let timeout = sleep(pause_duration);
    tokio::pin!(timeout);
    tokio::select! {
        _ = &mut timeout => { }
        result = stop_rx => {
            result?;
            println!("task received stop signal");
            return Ok(())
        }
    }
    msg_tx.send("done".to_string()).await?;
    println!("task sent message \"done\"");
    Ok(())
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("tokio mpsc send string error")]
    TokioMpscSendString(#[from] mpsc::error::SendError<std::string::String>),
    #[error("tokio mpsc send string error")]
    TokioMpscSendSocketAddr(#[from] mpsc::error::SendError<std::net::SocketAddr>),
    #[error("tokio oneshot recv error")]
    TokioOneshotRecv(#[from] oneshot::error::RecvError),
    #[error("failed to establish websocket connection")]
    WebSocketConnect(#[from] async_tungstenite::tungstenite::Error),
    #[error("failed to join a tokio task's join handle")]
    TokioTaskJoinError(#[from] tokio::task::JoinError),
}

async fn websocket_connect_harness(
    timeout_duration: Duration,  // time before test times out
    client_addr: Url,
    server_addr: SocketAddr,
) -> Result<(bool, bool), String> {
    let (client_connect_tx, mut client_connect_rx) = mpsc::channel::<Url>(1);
    let (client_msg_tx, _) = mpsc::channel(1);
    let client = tokio::spawn(run_websocket_client(
        client_addr,
        client_connect_tx,
        client_msg_tx,
        Duration::from_secs(2),
    ));

    let (server_connect_tx, mut server_connect_rx) = mpsc::channel::<SocketAddr>(1);
    let (server_msg_tx, _) = mpsc::channel(1);
    let server = tokio::spawn(run_websocket_server(
        server_addr,
        server_connect_tx,
        server_msg_tx,
    ));

    let (mut client_connected, mut server_connected) = (false, false);
    let sleep = tokio::time::sleep(timeout_duration);
    tokio::pin!(sleep);
    tokio::pin!(client);
    tokio::pin!(server);
    while !(client_connected && server_connected) {
        tokio::select! {
            _ = &mut sleep => {
                println!("test timed out");
                break
            }
            Some(addr) = client_connect_rx.recv() => {
                println!("client connected: {}", addr);
                client_connected = true;
            }
            Some(addr) = server_connect_rx.recv() => {
                println!("server connected: {}", addr);
                server_connected = true;
            }
        }
    }

    Ok((client_connected, server_connected))
}

async fn run_websocket_client(
    addr: Url,
    connect_tx: mpsc::Sender<Url>,
    msg_tx: mpsc::Sender<String>,
    give_up_after: Duration,
) -> Result<(), Error> {
    println!("client dialing {}", addr);
    let stream;
    let timeout = sleep(give_up_after);
    tokio::pin!(timeout);
    loop {
        tokio::select! {
            _ = &mut timeout => {
                println!("timed out waiting for connection after {:?}", give_up_after);
            }
            result = tokio_tungstenite::connect_async(addr.clone()) => {
                match result {
                    Ok((s, _)) => {
                        stream = s;
                        break
                    }
                    Err(e) => return Err(e.into())
                }
            }
        }
        sleep(Duration::from_millis(200)).await
    }
    
    let (mut tx, mut rx) = stream.split();
    tx.send(Message::text("test message".to_owned()))
        .await
        .expect("client failed to send test message");
    connect_tx.send(addr.clone())
        .await
        .expect("client failed to send msg");
    loop {
        match rx.next().await {
            Some(msg) => {
                match msg.expect("client could not load message") {
                    Message::Text(s) => {
                        if msg_tx.is_closed() {
                            println!("client channel to main thread is already closed -- that may be OK");
                            break
                        }
                        match msg_tx.send(s).await {
                            Ok(_) => println!("client passed msg to main thread"),
                            Err(e) => panic!("client failed to pass msg to main thread: {}", e)
                        }
                    }
                    Message::Close(f) => {
                        let mut m = "".to_string();
                        match f {
                            Some(v) => { m = format!(": {} {}", v.code, v.reason) }
                            None => {}
                        }
                        println!("connection from {} closed{}", addr, m);
                        break
                    }
                    _ => { println!("non-text message") }
                }
            }
            None => {
                println!("no message retrived from stream");
                break
            }
        }
    }
    Ok(())
}

async fn run_websocket_server(
    addr: SocketAddr,
    connect_tx: mpsc::Sender<SocketAddr>,
    msg_tx: mpsc::Sender<String>,
) -> Result<(), Error> {
    println!("server listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("server failed to bind");
    let (stream, _) = listener.accept()
        .await
        .expect("server failed to accept connection");
    let peer = stream
        .peer_addr()
        .expect("server: connected streams should have a peer address");
    println!("server: new connection from {}", peer);
    connect_tx.send(peer).await.unwrap();
    let websocket = tokio_tungstenite::accept_async(stream)
        .await
        .expect("server: error during websocket handshake");

    let (mut tx, mut rx) = websocket.split();
    tx.send(Message::text("test message".to_owned()))
        .await
        .expect("server failed to send message");
    loop {
        match rx.next().await {
            Some(msg) => {
                match msg.expect("could not load message") {
                    Message::Text(s) => {
                        if msg_tx.is_closed() {
                            println!("server channel to main thread is already closed -- that may be OK");
                            break
                        }
                        match msg_tx.send(s).await {
                            Ok(_) => println!("server passed msg to main thread"),
                            Err(e) => panic!("server failed to pass msg to main thread: {}", e)
                        }
                    }
                    Message::Close(f) => {
                        let mut m = "".to_string();
                        match f {
                            Some(v) => { m = format!(": {} {}", v.code, v.reason) }
                            None => {}
                        }
                        println!("connection from {} closed{}", peer, m);
                        break
                    }
                    _ => { println!("non-text message") }
                }
            }
            None => {
                println!("no message retrived from stream");
                break
            }
        }
    }
    Ok(())
}

fn random_port() -> u16 {
    let addr = "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap();
    let sock = std::net::UdpSocket::bind(addr).unwrap();
    let local_addr = sock.local_addr().unwrap();
    local_addr.port()
}
