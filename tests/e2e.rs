use std::error::Error;
use tokio;
use tokio::time::{sleep, Duration};

#[test]
fn simple_e2e_test() -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on( async { e2e_test().await })?;
    assert!(false, "force test failure");
    Ok(())
}

async fn e2e_test() -> Result<(), String> {
    let (server_stop_tx, stop_server_rx) = tokio::sync::oneshot::channel::<()>();
    let (server_msg_tx, mut server_msg_rx) = tokio::sync::mpsc::channel(1);

    let server = tokio::spawn( async move {
        let timeout = sleep(Duration::from_millis(750));
        tokio::pin!(timeout);
        tokio::select! {
            _ = &mut timeout => { }
            result = stop_server_rx => {
                match result {
                    Ok(_)  => {
                        println!("server received stop signal");
                        return Ok(())
                    }
                    Err(e) => return Err(format!("failed to recv from server stop channel: {}", e))
                }
            }
        }
        match server_msg_tx.send("done".to_string()).await {
            Ok(_)  => println!("server sent message \"done\""),
            Err(e) => return Err(format!("failed to send server message: {}", e))
        }
        Ok(())
    });

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
