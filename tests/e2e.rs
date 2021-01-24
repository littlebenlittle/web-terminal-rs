use tokio;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration};
use tokio::task::JoinHandle;
use thiserror::Error;

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
fn test_websocket_connect() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    let (client_connected, server_connected) = rt.block_on(websocket_connect_harness(
        Duration::from_millis(1000),
    ))?;
    assert!(client_connected, "connection should have been established");
    assert!(server_connected, "connection should have been established");
    Ok(())
}

#[test]
fn test_websocket_connect_failed() -> Result<(), Box<dyn std::error::Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    let (client_connected, server_connected) = rt.block_on(websocket_connect_harness(
        Duration::from_millis(1000),
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
    if !stop_tx.is_closed() {
        if let Err(_) = stop_tx.send(()) {
            return Err(format!("failed to send {} stop signal", task_name))
        } else {
            match task_jh.await {
                Ok(v) => match v {
                    Ok(_)  => println!("{} exited OK", task_name),
                    Err(e) => println!("{} exited Err: {}", task_name, e)
                }
                Err(e) => return Err(format!("failed to join with {} task: {}", task_name, e))
            }
        }
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
    #[error("tokio oneshot recv error")]
    TokioOneshotRecv(#[from] tokio::sync::oneshot::error::RecvError),
}

async fn websocket_connect_harness(
    timeout_duration: Duration,  // time before test times out
) -> Result<(bool, bool), String> {
    let (client_stop_tx, client_stop_rx) = oneshot::channel::<()>();
    let (client_connect_tx, mut client_connect_rx) = mpsc::channel::<String>(1);
    let (client_msg_tx, mut client_msg_rx) = mpsc::channel(1);
    let client = tokio::spawn(run_websocket_client(
        client_stop_rx,
        client_connect_tx,
        client_msg_tx,
    ));

    let (server_stop_tx, server_stop_rx) = oneshot::channel::<()>();
    let (server_connect_tx, mut server_connect_rx) = mpsc::channel::<String>(1);
    let (server_msg_tx, mut server_msg_rx) = mpsc::channel(1);
    let server = tokio::spawn(run_websocket_server(
        server_stop_rx,
        server_connect_tx,
        server_msg_tx,
    ));

    let (mut client_done, mut server_done) = (false, false);
    let (mut client_connected, mut server_connected) = (false, false);
    let sleep = tokio::time::sleep(timeout_duration);
    tokio::pin!(sleep);
    tokio::pin!(client);
    tokio::pin!(server);
    while !(client_done && server_done) {
        tokio::select! {
            _ = &mut sleep => {
                panic!("test timed out");
            }
            Some(addr) = client_connect_rx.recv() => {
                println!("client connected: {}", addr);
                client_connected = true;
            }
            Some(addr) = server_connect_rx.recv() => {
                println!("server connected: {}", addr);
                server_connected = true;
            }
            Some(msg) = client_msg_rx.recv() => {
                println!("client received: {}", msg);
                client_done = true;
            }
            Some(msg) = server_msg_rx.recv() => {
                println!("server received: {}", msg);
                server_done = true;
            }
        }
    }

    ensure_task_stopped("client", client, client_stop_tx).await?;
    ensure_task_stopped("server", server, server_stop_tx).await?;
    
    Ok((client_connected, server_connected))
}

async fn run_websocket_client(
    stop_rx: oneshot::Receiver<()>,
    connect_tx: mpsc::Sender<String>,
    msg_tx: mpsc::Sender<String>,
) -> Result<(), Error> {
    Ok(())
}

async fn run_websocket_server(
    stop_rx: oneshot::Receiver<()>,
    connect_tx: mpsc::Sender<String>,
    msg_tx: mpsc::Sender<String>,
) -> Result<(), Error> {
    Ok(())
}
