use std::error::Error;
use tokio;
use web_terminal::run_server;

#[test]
fn simple_e2e_test() -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on( async { e2e_test().await })?;
    assert!(false, "force test failure");
    Ok(())
}

async fn e2e_test() -> Result<(), String> {
    let (server_stop_tx, server_stop_rx) = tokio::sync::oneshot::channel::<()>();
    let (server_msg_tx, mut server_msg_rx) = tokio::sync::mpsc::channel(1);

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

    if !server_stop_tx.is_closed() {
        if let Err(_) = server_stop_tx.send(()) {
            return Err(format!("failed to send server stop signal"))
        } else {
            match server.await {
                Ok(v) => match v {
                    Ok(_)  => println!("server exited OK"),
                    Err(e) => println!("server exited Err: {}", e)
                }
                Err(e) => return Err(format!("failed to join with server task: {}", e))
            }
        }
    }
    
    Ok(())
}
