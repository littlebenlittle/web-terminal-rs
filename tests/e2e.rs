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
)
-> Result<(), Error>
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
