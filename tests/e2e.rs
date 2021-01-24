use std::error::Error;
use tokio;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use web_terminal::run_server;

#[test]
fn simple_e2e_test() -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on( async { e2e_test().await })?;
    assert!(false, "force test failure");
    Ok(())
}

async fn e2e_test() -> Result<(), String> {
    let (server_stop_tx, server_stop_rx) = oneshot::channel::<()>();
    let (server_msg_tx, mut server_msg_rx) = mpsc::channel(1);

    let server = tokio::spawn(run_server(server_stop_rx, server_msg_tx));

    let mut server_done = false;
    let sleep = tokio::time::sleep(tokio::time::Duration::from_millis(500));
    tokio::pin!(sleep);
    tokio::pin!(server);
    while !server_done {
        tokio::select! {
            _ = &mut sleep => {
                println!("test timed out");
                break
            }
            Some(msg) = server_msg_rx.recv() => {
                println!("server: {}", msg);
                server_done = true;
            }
        }
    }

    ensure_task_stopped("server", server, server_stop_tx).await?;
    
    Ok(())
}

async fn ensure_task_stopped<T,E>(
    task_name: &str,
    task_jh: std::pin::Pin<&mut JoinHandle<Result<T,E>>>,
    stop_tx: oneshot::Sender<()>,
)
-> Result<(), String>
where
    E: std::fmt::Display + Error
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
